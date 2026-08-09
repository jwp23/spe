// Widths of overlay text as the canvas font system actually shapes it.
//
// The PDF's AFM/TrueType width tables are authoritative for the *saved*
// document, but the canvas draws with whatever font the system resolves for
// an overlay's family — the Standard 14 map onto generic families, so the two
// are never the same face. Measured across the registry the AFM width runs
// from 0.42x to 1.65x the shaped width, so the canvas has to ask the text
// engine rather than scale a table (spe-x2z).

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{Paragraph as _, Text};

use crate::fonts::{FontId, FontRegistry};

/// Size the cache measures at. Shaped advances scale linearly with the text
/// size, so one measurement per (font, string) serves every size and zoom.
const REFERENCE_SIZE: f32 = 100.0;

/// Entries kept before the cache is dropped and rebuilt. Every keystroke
/// caches another prefix, so an unbounded map would grow with the session
/// rather than with the document.
const MAX_CACHED_STRINGS: usize = 4096;

/// Shaped width per point of font size, keyed by the font and the string.
static WIDTHS: LazyLock<RwLock<HashMap<(FontId, String), f32>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Width in screen pixels of `text` drawn on the canvas at `font_size`.
pub(crate) fn canvas_text_width(
    text: &str,
    font: FontId,
    font_size: f32,
    registry: &FontRegistry,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }

    let key = (font, text.to_string());
    if let Some(per_point) = WIDTHS.read().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return per_point * font_size;
    }

    let per_point = shaped_width(text, registry.get(font).iced_font) / REFERENCE_SIZE;
    insert_bounded(
        &mut WIDTHS.write().unwrap_or_else(|e| e.into_inner()),
        key,
        per_point,
    );
    per_point * font_size
}

/// Record a measurement, starting the cache over once it reaches its bound.
/// Dropping everything is enough: the strings still on screen are re-measured
/// on the next draw, and no eviction order is worth tracking for a cache this
/// cheap to refill.
fn insert_bounded(
    cache: &mut HashMap<(FontId, String), f32>,
    key: (FontId, String),
    per_point: f32,
) {
    if cache.len() >= MAX_CACHED_STRINGS {
        cache.clear();
    }
    cache.insert(key, per_point);
}

/// Shape `text` at `REFERENCE_SIZE` and return its width.
///
/// The paragraph mirrors the one `canvas::Text` lays out when it draws, so the
/// measurement describes the glyphs that actually reach the screen.
fn shaped_width(text: &str, font: iced::Font) -> f32 {
    Paragraph::with_text(Text {
        content: text,
        bounds: iced::Size::new(f32::INFINITY, f32::INFINITY),
        size: iced::Pixels(REFERENCE_SIZE),
        line_height: super::TEXT_LINE_HEIGHT,
        font,
        align_x: iced::advanced::text::Alignment::Default,
        align_y: iced::alignment::Vertical::Top,
        shaping: iced::advanced::text::Shaping::default(),
        wrapping: iced::advanced::text::Wrapping::default(),
    })
    .min_bounds()
    .width
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> FontRegistry {
        FontRegistry::new()
    }

    #[test]
    fn empty_text_has_no_width() {
        let registry = registry();
        let width = canvas_text_width("", registry.default_font(), 12.0, &registry);
        assert!(width.abs() < f32::EPSILON, "got {width}");
    }

    #[test]
    fn width_scales_linearly_with_font_size() {
        let registry = registry();
        let font = registry.default_font();
        let small = canvas_text_width("Hello world", font, 12.0, &registry);
        let large = canvas_text_width("Hello world", font, 36.0, &registry);
        assert!(small > 0.0, "expected a measurable width, got {small}");
        assert!(
            (large - 3.0 * small).abs() < 0.01,
            "tripling the size should triple the width: {small} -> {large}"
        );
    }

    #[test]
    fn longer_text_is_wider() {
        let registry = registry();
        let font = registry.default_font();
        let short = canvas_text_width("Hi", font, 12.0, &registry);
        let long = canvas_text_width("Hi there, everyone", font, 12.0, &registry);
        assert!(long > short, "{long} should exceed {short}");
    }

    #[test]
    fn repeated_measurements_agree() {
        // The second call is served from the cache and must not drift.
        let registry = registry();
        let font = registry.default_font();
        let first = canvas_text_width("cached", font, 17.0, &registry);
        let second = canvas_text_width("cached", font, 17.0, &registry);
        assert!((first - second).abs() < f32::EPSILON, "{first} vs {second}");
    }

    #[test]
    fn proportional_fonts_do_not_measure_like_their_pdf_metrics() {
        // The whole point of this module: the canvas face is not the PDF face,
        // so a Standard 14 serif overlay is meaningfully wider on screen than
        // its AFM widths claim (spe-x2z).
        let registry = registry();
        let font = registry.find_by_name("Times Bold").unwrap();
        let text = "Hello world";
        let afm = registry.overlay_bounding_box(text, font, 36.0).width;
        let canvas = canvas_text_width(text, font, 36.0, &registry);
        assert!(
            (canvas - afm).abs() > 0.05 * afm,
            "expected the shaped width {canvas} to differ from the AFM width {afm}"
        );
    }

    #[test]
    fn a_full_cache_starts_over_rather_than_growing() {
        let font = registry().default_font();
        let mut cache = HashMap::new();
        for i in 0..MAX_CACHED_STRINGS {
            insert_bounded(&mut cache, (font, format!("entry {i}")), i as f32);
        }
        assert_eq!(cache.len(), MAX_CACHED_STRINGS);

        insert_bounded(&mut cache, (font, "one too many".to_string()), 1.0);
        assert_eq!(
            cache.len(),
            1,
            "reaching the bound should clear the cache and keep only the new entry"
        );
    }

    #[test]
    fn monospace_text_grows_one_advance_at_a_time() {
        let registry = registry();
        let font = registry.find_by_name("Courier").unwrap();
        let one = canvas_text_width("m", font, 24.0, &registry);
        let three = canvas_text_width("mmm", font, 24.0, &registry);
        assert!(
            (three - 3.0 * one).abs() < 0.01,
            "monospace advances should be uniform: {one} -> {three}"
        );
    }
}
