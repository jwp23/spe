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

/// Number of lines a string breaks into, keyed by the font, the string, and
/// the wrap ratio. The ratio rather than the width because a box twice as wide
/// holding text twice as large breaks in the same places, so one entry serves
/// every zoom level. `f32` has no `Hash`, so the ratio is keyed by its bits —
/// exact equality is what a cache key wants anyway.
static LINE_COUNTS: LazyLock<RwLock<HashMap<LineCountKey, usize>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Number of lines [`FontRegistry::word_wrap`] — the PDF writer's own wrap —
/// breaks a string into, keyed the same way as [`LINE_COUNTS`]. The writer
/// sums AFM widths that scale linearly with font size just like the shaped
/// widths above, so the same ratio key and reference-size trick apply.
static WRITER_LINE_COUNTS: LazyLock<RwLock<HashMap<LineCountKey, usize>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Font, string, and wrap ratio bits — see [`LINE_COUNTS`].
type LineCountKey = (FontId, String, u32);

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

    let per_point =
        shaped_width(text, registry.get(font).iced_font, REFERENCE_SIZE) / REFERENCE_SIZE;
    insert_bounded(
        &mut WIDTHS.write().unwrap_or_else(|e| e.into_inner()),
        key,
        per_point,
    );
    per_point * font_size
}

/// Number of lines `text` breaks into when drawn in a box `wrap_ratio` times
/// the font size wide.
///
/// The count, not the height, because the caller already knows the line height
/// it lays out on and multiplying is exact where dividing a shaped height back
/// out is not.
pub(crate) fn canvas_line_count(
    text: &str,
    font: FontId,
    wrap_ratio: f32,
    registry: &FontRegistry,
) -> usize {
    let key = (font, text.to_string(), wrap_ratio.to_bits());
    if let Some(count) = LINE_COUNTS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
    {
        return *count;
    }

    let count = shaped_line_count(
        text,
        registry.get(font).iced_font,
        REFERENCE_SIZE,
        wrap_ratio * REFERENCE_SIZE,
    );
    insert_bounded(
        &mut LINE_COUNTS.write().unwrap_or_else(|e| e.into_inner()),
        key,
        count,
    );
    count
}

/// Number of lines the PDF writer's [`FontRegistry::word_wrap`] breaks `text`
/// into when wrapped at `wrap_ratio` times the font size — the same wrap the
/// saved PDF uses, so the canvas can floor a box's height to it without
/// re-wrapping on every frame.
pub(crate) fn writer_line_count(
    text: &str,
    font: FontId,
    wrap_ratio: f32,
    registry: &FontRegistry,
) -> usize {
    let key = (font, text.to_string(), wrap_ratio.to_bits());
    if let Some(count) = WRITER_LINE_COUNTS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
    {
        return *count;
    }

    let count = registry
        .word_wrap(text, font, REFERENCE_SIZE, wrap_ratio * REFERENCE_SIZE)
        .len();
    insert_bounded(
        &mut WRITER_LINE_COUNTS
            .write()
            .unwrap_or_else(|e| e.into_inner()),
        key,
        count,
    );
    count
}

/// Record a measurement, starting the cache over once it reaches its bound.
/// Dropping everything is enough: the strings still on screen are re-measured
/// on the next draw, and no eviction order is worth tracking for a cache this
/// cheap to refill.
fn insert_bounded<K: Eq + std::hash::Hash, V>(cache: &mut HashMap<K, V>, key: K, value: V) {
    if cache.len() >= MAX_CACHED_STRINGS {
        cache.clear();
    }
    cache.insert(key, value);
}

/// Shape `text` at `size` inside a box `max_width` wide.
///
/// The paragraph mirrors the one `canvas::Text` lays out when it draws, so the
/// measurement describes the glyphs that actually reach the screen.
fn shaped(text: &str, font: iced::Font, size: f32, max_width: f32) -> Paragraph {
    Paragraph::with_text(Text {
        content: text,
        bounds: iced::Size::new(max_width, f32::INFINITY),
        size: iced::Pixels(size),
        line_height: super::TEXT_LINE_HEIGHT,
        font,
        align_x: iced::advanced::text::Alignment::Default,
        align_y: iced::alignment::Vertical::Top,
        shaping: iced::advanced::text::Shaping::default(),
        wrapping: iced::advanced::text::Wrapping::default(),
    })
}

/// Shape `text` at `size` unconstrained and return its width.
fn shaped_width(text: &str, font: iced::Font, size: f32) -> f32 {
    shaped(text, font, size, f32::INFINITY).min_bounds().width
}

/// Shape `text` at `size` in a box `max_width` wide and return how many lines
/// it laid out on, counting both wraps and explicit line breaks.
///
/// The shaped height divided by the line height is the line count, because the
/// paragraph lays every line out on the same `TEXT_LINE_HEIGHT`. Empty text
/// shapes to nothing but still shows a caret on one line, hence the floor.
fn shaped_line_count(text: &str, font: iced::Font, size: f32, max_width: f32) -> usize {
    let height = shaped(text, font, size, max_width).min_bounds().height;
    let line_height = size * super::TEXT_LINE_HEIGHT_RATIO;
    ((height / line_height).round() as usize).max(1)
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
    fn a_scaled_cache_hit_matches_shaping_at_that_size() {
        // The cache keeps one measurement per string and scales it, which is
        // only sound because the engine's advances are linear in the text
        // size. Each size is compared against the engine measuring that size
        // directly — comparing two cache reads would only re-check the
        // multiplication.
        let registry = registry();
        let font = registry.default_font();
        let text = "Hello world";
        for size in [8.0_f32, 12.0, 36.0, 72.0] {
            let shaped = shaped_width(text, registry.get(font).iced_font, size);
            let cached = canvas_text_width(text, font, size, &registry);
            assert!(shaped > 0.0, "the engine measured nothing at {size}pt");
            assert!(
                (cached - shaped).abs() <= 0.01 * shaped,
                "at {size}pt the cache reports {cached} but the engine shapes {shaped}"
            );
        }
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
    fn empty_text_still_occupies_one_line() {
        let registry = registry();
        assert_eq!(
            canvas_line_count("", registry.default_font(), 10.0, &registry),
            1
        );
    }

    #[test]
    fn text_that_fits_the_wrap_width_occupies_one_line() {
        let registry = registry();
        let font = registry.find_by_name("Courier").unwrap();
        assert_eq!(
            canvas_line_count("mmmm mmmm mmmm mmmm", font, 100.0, &registry),
            1
        );
    }

    #[test]
    fn text_wider_than_the_wrap_width_occupies_several_lines() {
        // The same string the previous test fits on one line, given a box a
        // few characters wide, has to break across lines.
        let registry = registry();
        let font = registry.find_by_name("Courier").unwrap();
        let lines = canvas_line_count("mmmm mmmm mmmm mmmm", font, 6.0, &registry);
        assert!(lines > 1, "expected wrapping, got {lines} line(s)");
    }

    #[test]
    fn a_narrower_box_wraps_the_same_text_onto_more_lines() {
        let registry = registry();
        let font = registry.find_by_name("Courier").unwrap();
        let text = "mmmm mmmm mmmm mmmm mmmm mmmm";
        let wide = canvas_line_count(text, font, 12.0, &registry);
        let narrow = canvas_line_count(text, font, 6.0, &registry);
        assert!(narrow > wide, "{narrow} should exceed {wide}");
    }

    #[test]
    fn explicit_line_breaks_each_start_a_line() {
        let registry = registry();
        let font = registry.find_by_name("Courier").unwrap();
        assert_eq!(canvas_line_count("a\nb\nc", font, 100.0, &registry), 3);
    }

    #[test]
    fn the_wrap_ratio_describes_line_breaking_at_every_size() {
        // The count is cached per ratio rather than per (width, size) pair,
        // which is only sound because a box twice as wide holding text twice
        // as large breaks in the same places. Compared against the engine
        // shaping each size directly, not against another cache read.
        let registry = registry();
        let font = registry.find_by_name("Courier").unwrap();
        let text = "mmmm mmmm mmmm mmmm mmmm";
        let ratio = 8.0;
        let expected = canvas_line_count(text, font, ratio, &registry);
        for size in [8.0_f32, 24.0, 72.0] {
            let shaped = shaped_line_count(text, registry.get(font).iced_font, size, ratio * size);
            assert_eq!(shaped, expected, "line breaking drifted at {size}pt");
        }
    }

    #[test]
    fn writer_line_count_matches_a_direct_word_wrap_call() {
        let registry = registry();
        let font = registry.find_by_name("Courier").unwrap();
        let text = "mmmm mmmm mmmm mmmm";
        let font_size = 12.0;
        let max_width = 50.0;
        let expected = registry.word_wrap(text, font, font_size, max_width).len();
        let cached = writer_line_count(text, font, max_width / font_size, &registry);
        assert_eq!(cached, expected);
    }

    #[test]
    fn repeated_writer_line_count_queries_agree() {
        let registry = registry();
        let font = registry.find_by_name("Courier").unwrap();
        let text = "mmmm mmmm mmmm mmmm";
        let ratio = 50.0 / 12.0;
        let first = writer_line_count(text, font, ratio, &registry);
        let second = writer_line_count(text, font, ratio, &registry);
        assert_eq!(first, second);
    }

    #[test]
    fn writer_line_count_ratio_describes_wrapping_at_every_size() {
        // Same invariant as canvas_line_count: word_wrap's break points depend
        // only on max_width/font_size, since every AFM width it sums scales
        // linearly with font_size. Compared against the engine wrapping each
        // size directly, not against another cache read.
        let registry = registry();
        let font = registry.find_by_name("Courier").unwrap();
        let text = "mmmm mmmm mmmm mmmm mmmm";
        let ratio = 50.0 / 12.0;
        let expected = writer_line_count(text, font, ratio, &registry);
        for size in [8.0_f32, 24.0, 72.0] {
            let direct = registry.word_wrap(text, font, size, ratio * size).len();
            assert_eq!(direct, expected, "wrapping drifted at {size}pt");
        }
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
