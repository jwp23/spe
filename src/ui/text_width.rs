// Width of a string as the text engine actually shapes it, for any face.
//
// Widths come from the engine rather than a font's own width table because the
// face that reaches the screen is not always the face a table describes — the
// Standard 14 resolve to whatever the system offers (see `canvas::text_metrics`
// for how far the two diverge).

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{Paragraph as _, Text};

/// Size the cache measures at. Shaped advances scale linearly with the text
/// size, so one measurement per (face, string) serves every size.
const REFERENCE_SIZE: f32 = 100.0;

/// Entries kept before the cache is dropped and rebuilt, bounding it by the
/// strings a session actually measures rather than letting it grow forever.
const MAX_CACHED_STRINGS: usize = 4096;

/// Shaped width per point of text size, keyed by the face and the string.
static WIDTHS: LazyLock<RwLock<HashMap<(iced::Font, String), f32>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Width in pixels of `text` shaped in `font` at `size`.
pub fn shaped_text_width(text: &str, font: iced::Font, size: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }

    let key = (font, text.to_string());
    if let Some(per_point) = WIDTHS.read().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return per_point * size;
    }

    let per_point = shape(text, font, REFERENCE_SIZE).min_bounds().width / REFERENCE_SIZE;

    let mut cache = WIDTHS.write().unwrap_or_else(|e| e.into_inner());
    if cache.len() >= MAX_CACHED_STRINGS {
        cache.clear();
    }
    cache.insert(key, per_point);

    per_point * size
}

/// Shape `text` at `size` with no width to wrap against.
fn shape(text: &str, font: iced::Font, size: f32) -> Paragraph {
    Paragraph::with_text(Text {
        content: text,
        bounds: iced::Size::INFINITE,
        size: iced::Pixels(size),
        line_height: iced::widget::text::LineHeight::default(),
        font,
        align_x: iced::advanced::text::Alignment::Default,
        align_y: iced::alignment::Vertical::Top,
        shaping: iced::advanced::text::Shaping::default(),
        wrapping: iced::advanced::text::Wrapping::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_has_no_width() {
        let width = shaped_text_width("", iced::Font::DEFAULT, 16.0);
        assert!(width.abs() < f32::EPSILON, "got {width}");
    }

    #[test]
    fn longer_text_is_wider() {
        let short = shaped_text_width("Helvetica", iced::Font::DEFAULT, 16.0);
        let long = shaped_text_width("Helvetica Bold Oblique", iced::Font::DEFAULT, 16.0);
        assert!(
            long > short,
            "the longer name measured {long}, the shorter {short}"
        );
    }

    #[test]
    fn the_face_decides_the_width() {
        // The picker sizes itself from per-face measurements, so a face
        // argument that did not reach the engine would silently give every
        // family the default face's width.
        let text = "Helvetica Bold Oblique";
        let sans = shaped_text_width(text, iced::Font::DEFAULT, 16.0);
        let monospace = shaped_text_width(text, iced::Font::MONOSPACE, 16.0);
        assert!(
            (sans - monospace).abs() > 1.0,
            "sans measured {sans} and monospace {monospace} — the face was ignored"
        );
    }

    #[test]
    fn a_scaled_cache_hit_matches_shaping_at_that_size() {
        // One measurement per (face, string) is cached and scaled, which only
        // holds because the engine's advances are linear in the text size.
        // Each size is checked against the engine measuring it directly.
        let text = "Pinyon Script";
        for size in [8.0_f32, 16.0, 36.0, 72.0] {
            let shaped = shape(text, iced::Font::DEFAULT, size).min_bounds().width;
            let cached = shaped_text_width(text, iced::Font::DEFAULT, size);
            assert!(shaped > 0.0, "the engine measured nothing at {size}pt");
            assert!(
                (cached - shaped).abs() <= 0.01 * shaped,
                "at {size}pt the cache reports {cached} but the engine shapes {shaped}"
            );
        }
    }
}
