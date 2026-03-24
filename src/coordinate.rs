// Coordinate conversion between screen pixels and PDF points,
// and AFM-based text bounding box computation.

use crate::overlay::Standard14Font;

pub struct ConversionParams {
    pub zoom: f32,
    pub dpi: f32,
    pub page_height: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

pub struct BoundingBox {
    pub width: f32,
    pub height: f32,
}

/// Convert screen pixel coordinates to PDF point coordinates.
pub fn screen_to_pdf(screen_x: f32, screen_y: f32, params: &ConversionParams) -> (f32, f32) {
    let scale = params.zoom * (params.dpi / 72.0);
    let pdf_x = (screen_x - params.offset_x) / scale;
    let pdf_y = params.page_height - ((screen_y - params.offset_y) / scale);
    (pdf_x, pdf_y)
}

/// Convert PDF point coordinates to screen pixel coordinates.
pub fn pdf_to_screen(pdf_x: f32, pdf_y: f32, params: &ConversionParams) -> (f32, f32) {
    let scale = params.zoom * (params.dpi / 72.0);
    let screen_x = pdf_x * scale + params.offset_x;
    let screen_y = (params.page_height - pdf_y) * scale + params.offset_y;
    (screen_x, screen_y)
}

/// Compute the bounding box of text in a given font at a given size.
/// Width is computed from per-character AFM widths. Height is the font size.
pub fn overlay_bounding_box(text: &str, font: Standard14Font, font_size: f32) -> BoundingBox {
    let width: f32 = text
        .chars()
        .map(|c| char_width(font, c) * font_size / 1000.0)
        .sum();
    BoundingBox {
        width,
        height: font_size,
    }
}

fn char_width(font: Standard14Font, c: char) -> f32 {
    let code = c as u32;
    if code > 255 {
        return default_width(font);
    }
    let code = code as u8;
    match font {
        Standard14Font::Courier
        | Standard14Font::CourierBold
        | Standard14Font::CourierOblique
        | Standard14Font::CourierBoldOblique => 600.0,
        Standard14Font::Helvetica | Standard14Font::HelveticaOblique => helvetica_width(code),
        Standard14Font::HelveticaBold | Standard14Font::HelveticaBoldOblique => {
            helvetica_bold_width(code)
        }
        Standard14Font::TimesRoman | Standard14Font::TimesItalic => times_roman_width(code),
        Standard14Font::TimesBold | Standard14Font::TimesBoldItalic => times_bold_width(code),
        Standard14Font::Symbol | Standard14Font::ZapfDingbats => default_width(font),
    }
}

fn default_width(_font: Standard14Font) -> f32 {
    600.0
}

fn helvetica_width(code: u8) -> f32 {
    match code {
        32 => 278.0,      // space
        33 => 278.0,      // !
        34 => 355.0,      // "
        35 => 556.0,      // #
        36 => 556.0,      // $
        37 => 889.0,      // %
        38 => 667.0,      // &
        39 => 191.0,      // '
        40 => 333.0,      // (
        41 => 333.0,      // )
        42 => 389.0,      // *
        43 => 584.0,      // +
        44 => 278.0,      // ,
        45 => 333.0,      // -
        46 => 278.0,      // .
        47 => 278.0,      // /
        48..=57 => 556.0, // 0-9
        58 => 278.0,      // :
        59 => 278.0,      // ;
        60 => 584.0,      // <
        61 => 584.0,      // =
        62 => 584.0,      // >
        63 => 556.0,      // ?
        64 => 1015.0,     // @
        65 => 667.0,      // A
        66 => 667.0,      // B
        67 => 722.0,      // C
        68 => 722.0,      // D
        69 => 667.0,      // E
        70 => 611.0,      // F
        71 => 778.0,      // G
        72 => 722.0,      // H
        73 => 278.0,      // I
        74 => 500.0,      // J
        75 => 667.0,      // K
        76 => 556.0,      // L
        77 => 833.0,      // M
        78 => 722.0,      // N
        79 => 778.0,      // O
        80 => 667.0,      // P
        81 => 778.0,      // Q
        82 => 722.0,      // R
        83 => 667.0,      // S
        84 => 611.0,      // T
        85 => 722.0,      // U
        86 => 667.0,      // V
        87 => 944.0,      // W
        88 => 667.0,      // X
        89 => 667.0,      // Y
        90 => 611.0,      // Z
        91 => 278.0,      // [
        92 => 278.0,      // \
        93 => 278.0,      // ]
        94 => 469.0,      // ^
        95 => 556.0,      // _
        96 => 333.0,      // `
        97 => 556.0,      // a
        98 => 556.0,      // b
        99 => 500.0,      // c
        100 => 556.0,     // d
        101 => 556.0,     // e
        102 => 278.0,     // f
        103 => 556.0,     // g
        104 => 556.0,     // h
        105 => 222.0,     // i
        106 => 222.0,     // j
        107 => 500.0,     // k
        108 => 222.0,     // l
        109 => 833.0,     // m
        110 => 556.0,     // n
        111 => 556.0,     // o
        112 => 556.0,     // p
        113 => 556.0,     // q
        114 => 333.0,     // r
        115 => 500.0,     // s
        116 => 278.0,     // t
        117 => 556.0,     // u
        118 => 500.0,     // v
        119 => 722.0,     // w
        120 => 500.0,     // x
        121 => 500.0,     // y
        122 => 500.0,     // z
        123 => 334.0,     // {
        124 => 260.0,     // |
        125 => 334.0,     // }
        126 => 584.0,     // ~
        _ => 556.0,       // fallback
    }
}

fn helvetica_bold_width(code: u8) -> f32 {
    match code {
        32 => 278.0,
        33 => 333.0,
        34 => 474.0,
        35 => 556.0,
        36 => 556.0,
        37 => 889.0,
        38 => 722.0,
        39 => 238.0,
        40 => 333.0,
        41 => 333.0,
        42 => 389.0,
        43 => 584.0,
        44 => 278.0,
        45 => 333.0,
        46 => 278.0,
        47 => 278.0,
        48..=57 => 556.0,
        58 => 333.0,
        59 => 333.0,
        60 => 584.0,
        61 => 584.0,
        62 => 584.0,
        63 => 611.0,
        64 => 975.0,
        65 => 722.0,
        66 => 722.0,
        67 => 722.0,
        68 => 722.0,
        69 => 667.0,
        70 => 611.0,
        71 => 778.0,
        72 => 722.0,
        73 => 278.0,
        74 => 556.0,
        75 => 722.0,
        76 => 611.0,
        77 => 833.0,
        78 => 722.0,
        79 => 778.0,
        80 => 667.0,
        81 => 778.0,
        82 => 722.0,
        83 => 667.0,
        84 => 611.0,
        85 => 722.0,
        86 => 667.0,
        87 => 944.0,
        88 => 667.0,
        89 => 667.0,
        90 => 611.0,
        91 => 333.0,
        92 => 278.0,
        93 => 333.0,
        94 => 584.0,
        95 => 556.0,
        96 => 333.0,
        97 => 556.0,
        98 => 611.0,
        99 => 556.0,
        100 => 611.0,
        101 => 556.0,
        102 => 333.0,
        103 => 611.0,
        104 => 611.0,
        105 => 278.0,
        106 => 278.0,
        107 => 556.0,
        108 => 278.0,
        109 => 889.0,
        110 => 611.0,
        111 => 611.0,
        112 => 611.0,
        113 => 611.0,
        114 => 389.0,
        115 => 556.0,
        116 => 333.0,
        117 => 611.0,
        118 => 556.0,
        119 => 778.0,
        120 => 556.0,
        121 => 556.0,
        122 => 500.0,
        123 => 389.0,
        124 => 280.0,
        125 => 389.0,
        126 => 584.0,
        _ => 556.0,
    }
}

fn times_roman_width(code: u8) -> f32 {
    match code {
        32 => 250.0,
        33 => 333.0,
        34 => 408.0,
        35 => 500.0,
        36 => 500.0,
        37 => 833.0,
        38 => 778.0,
        39 => 180.0,
        40 => 333.0,
        41 => 333.0,
        42 => 500.0,
        43 => 564.0,
        44 => 250.0,
        45 => 333.0,
        46 => 250.0,
        47 => 278.0,
        48 => 500.0,
        49 => 500.0,
        50 => 500.0,
        51 => 500.0,
        52 => 500.0,
        53 => 500.0,
        54 => 500.0,
        55 => 500.0,
        56 => 500.0,
        57 => 500.0,
        58 => 278.0,
        59 => 278.0,
        60 => 564.0,
        61 => 564.0,
        62 => 564.0,
        63 => 444.0,
        64 => 921.0,
        65 => 722.0,
        66 => 667.0,
        67 => 667.0,
        68 => 722.0,
        69 => 611.0,
        70 => 556.0,
        71 => 722.0,
        72 => 722.0,
        73 => 333.0,
        74 => 389.0,
        75 => 722.0,
        76 => 611.0,
        77 => 889.0,
        78 => 722.0,
        79 => 722.0,
        80 => 556.0,
        81 => 722.0,
        82 => 667.0,
        83 => 556.0,
        84 => 611.0,
        85 => 722.0,
        86 => 722.0,
        87 => 944.0,
        88 => 722.0,
        89 => 722.0,
        90 => 611.0,
        91 => 333.0,
        92 => 278.0,
        93 => 333.0,
        94 => 469.0,
        95 => 500.0,
        96 => 333.0,
        97 => 444.0,
        98 => 500.0,
        99 => 444.0,
        100 => 500.0,
        101 => 444.0,
        102 => 333.0,
        103 => 500.0,
        104 => 500.0,
        105 => 278.0,
        106 => 278.0,
        107 => 500.0,
        108 => 278.0,
        109 => 778.0,
        110 => 500.0,
        111 => 500.0,
        112 => 500.0,
        113 => 500.0,
        114 => 333.0,
        115 => 389.0,
        116 => 278.0,
        117 => 500.0,
        118 => 500.0,
        119 => 722.0,
        120 => 500.0,
        121 => 500.0,
        122 => 444.0,
        123 => 480.0,
        124 => 200.0,
        125 => 480.0,
        126 => 541.0,
        _ => 500.0,
    }
}

fn times_bold_width(code: u8) -> f32 {
    match code {
        32 => 250.0,
        33 => 333.0,
        34 => 555.0,
        35 => 500.0,
        36 => 500.0,
        37 => 1000.0,
        38 => 833.0,
        39 => 278.0,
        40 => 333.0,
        41 => 333.0,
        42 => 500.0,
        43 => 570.0,
        44 => 250.0,
        45 => 333.0,
        46 => 250.0,
        47 => 278.0,
        48 => 500.0,
        49 => 500.0,
        50 => 500.0,
        51 => 500.0,
        52 => 500.0,
        53 => 500.0,
        54 => 500.0,
        55 => 500.0,
        56 => 500.0,
        57 => 500.0,
        58 => 333.0,
        59 => 333.0,
        60 => 570.0,
        61 => 570.0,
        62 => 570.0,
        63 => 500.0,
        64 => 930.0,
        65 => 722.0,
        66 => 667.0,
        67 => 722.0,
        68 => 722.0,
        69 => 667.0,
        70 => 611.0,
        71 => 778.0,
        72 => 778.0,
        73 => 389.0,
        74 => 500.0,
        75 => 778.0,
        76 => 667.0,
        77 => 944.0,
        78 => 722.0,
        79 => 778.0,
        80 => 611.0,
        81 => 778.0,
        82 => 722.0,
        83 => 556.0,
        84 => 667.0,
        85 => 722.0,
        86 => 722.0,
        87 => 1000.0,
        88 => 722.0,
        89 => 722.0,
        90 => 667.0,
        91 => 333.0,
        92 => 278.0,
        93 => 333.0,
        94 => 581.0,
        95 => 500.0,
        96 => 333.0,
        97 => 500.0,
        98 => 556.0,
        99 => 444.0,
        100 => 556.0,
        101 => 444.0,
        102 => 333.0,
        103 => 500.0,
        104 => 556.0,
        105 => 278.0,
        106 => 333.0,
        107 => 556.0,
        108 => 278.0,
        109 => 833.0,
        110 => 556.0,
        111 => 500.0,
        112 => 556.0,
        113 => 556.0,
        114 => 444.0,
        115 => 389.0,
        116 => 333.0,
        117 => 556.0,
        118 => 500.0,
        119 => 722.0,
        120 => 500.0,
        121 => 500.0,
        122 => 444.0,
        123 => 394.0,
        124 => 220.0,
        125 => 394.0,
        126 => 520.0,
        _ => 500.0,
    }
}

/// Wrap text into lines that fit within max_width (in PDF points).
/// Splits on explicit `\n` first, then wraps each line at word boundaries.
/// Words wider than max_width are kept intact (no mid-word break).
pub fn word_wrap(text: &str, font: Standard14Font, font_size: f32, max_width: f32) -> Vec<String> {
    let mut result = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            result.push(String::new());
            continue;
        }
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            result.push(String::new());
            continue;
        }
        let mut current_line = String::new();
        let mut current_width: f32 = 0.0;
        let space_width = char_width(font, ' ') * font_size / 1000.0;

        for word in &words {
            let word_width = overlay_bounding_box(word, font, font_size).width;
            if current_line.is_empty() {
                current_line.push_str(word);
                current_width = word_width;
            } else if current_width + space_width + word_width <= max_width {
                current_line.push(' ');
                current_line.push_str(word);
                current_width += space_width + word_width;
            } else {
                result.push(current_line);
                current_line = word.to_string();
                current_width = word_width;
            }
        }
        result.push(current_line);
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_to_pdf_at_default_zoom_and_dpi() {
        let params = ConversionParams {
            zoom: 1.0,
            dpi: 72.0,
            page_height: 792.0,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        let (pdf_x, pdf_y) = screen_to_pdf(72.0, 72.0, &params);
        assert!((pdf_x - 72.0).abs() < 0.01);
        assert!((pdf_y - 720.0).abs() < 0.01);
    }

    #[test]
    fn round_trip_preserves_position() {
        let params = ConversionParams {
            zoom: 1.5,
            dpi: 150.0,
            page_height: 842.0,
            offset_x: 20.0,
            offset_y: 10.0,
        };
        let (px, py) = screen_to_pdf(300.0, 400.0, &params);
        let (sx, sy) = pdf_to_screen(px, py, &params);
        assert!((sx - 300.0).abs() < 0.01);
        assert!((sy - 400.0).abs() < 0.01);
    }

    #[test]
    fn screen_to_pdf_at_200_percent_zoom() {
        let params = ConversionParams {
            zoom: 2.0,
            dpi: 72.0,
            page_height: 792.0,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        let (pdf_x, _pdf_y) = screen_to_pdf(144.0, 0.0, &params);
        assert!((pdf_x - 72.0).abs() < 0.01);
    }

    #[test]
    fn pdf_to_screen_y_axis_flip() {
        let params = ConversionParams {
            zoom: 1.0,
            dpi: 72.0,
            page_height: 792.0,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        // PDF origin is bottom-left. PDF y=0 should map to screen y=792
        let (_sx, sy) = pdf_to_screen(0.0, 0.0, &params);
        assert!((sy - 792.0).abs() < 0.01);
        // PDF y=792 should map to screen y=0
        let (_sx, sy) = pdf_to_screen(0.0, 792.0, &params);
        assert!(sy.abs() < 0.01);
    }

    #[test]
    fn screen_to_pdf_with_offset() {
        let params = ConversionParams {
            zoom: 1.0,
            dpi: 72.0,
            page_height: 792.0,
            offset_x: 50.0,
            offset_y: 30.0,
        };
        let (pdf_x, pdf_y) = screen_to_pdf(122.0, 102.0, &params);
        assert!((pdf_x - 72.0).abs() < 0.01);
        assert!((pdf_y - 720.0).abs() < 0.01);
    }

    #[test]
    fn round_trip_at_high_dpi() {
        let params = ConversionParams {
            zoom: 1.0,
            dpi: 300.0,
            page_height: 792.0,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        let (px, py) = screen_to_pdf(1000.0, 500.0, &params);
        let (sx, sy) = pdf_to_screen(px, py, &params);
        assert!((sx - 1000.0).abs() < 0.01);
        assert!((sy - 500.0).abs() < 0.01);
    }

    #[test]
    fn courier_bounding_box_monospaced() {
        let bbox = overlay_bounding_box("Hello", Standard14Font::Courier, 12.0);
        // 5 chars * (600/1000 * 12) = 5 * 7.2 = 36.0 width
        assert!((bbox.width - 36.0).abs() < 0.1);
        assert!((bbox.height - 12.0).abs() < 0.01);
    }

    #[test]
    fn helvetica_bounding_box_proportional() {
        // "il" should be narrower than "HH" in Helvetica
        let narrow = overlay_bounding_box("il", Standard14Font::Helvetica, 12.0);
        let wide = overlay_bounding_box("HH", Standard14Font::Helvetica, 12.0);
        assert!(
            narrow.width < wide.width,
            "narrow={} wide={}",
            narrow.width,
            wide.width
        );
    }

    #[test]
    fn empty_string_has_zero_width() {
        let bbox = overlay_bounding_box("", Standard14Font::Helvetica, 12.0);
        assert!(bbox.width.abs() < f32::EPSILON);
    }

    #[test]
    fn courier_bold_is_monospaced() {
        let bbox = overlay_bounding_box("iiiii", Standard14Font::CourierBold, 10.0);
        let bbox2 = overlay_bounding_box("MMMMM", Standard14Font::CourierBold, 10.0);
        assert!((bbox.width - bbox2.width).abs() < f32::EPSILON);
    }

    #[test]
    fn helvetica_oblique_same_widths_as_helvetica() {
        let regular = overlay_bounding_box("Hello World", Standard14Font::Helvetica, 12.0);
        let oblique = overlay_bounding_box("Hello World", Standard14Font::HelveticaOblique, 12.0);
        assert!((regular.width - oblique.width).abs() < f32::EPSILON);
    }

    #[test]
    fn times_roman_proportional() {
        let narrow = overlay_bounding_box("iii", Standard14Font::TimesRoman, 12.0);
        let wide = overlay_bounding_box("MMM", Standard14Font::TimesRoman, 12.0);
        assert!(narrow.width < wide.width);
    }

    #[test]
    fn font_size_scales_linearly() {
        let small = overlay_bounding_box("Hello", Standard14Font::Helvetica, 12.0);
        let large = overlay_bounding_box("Hello", Standard14Font::Helvetica, 24.0);
        assert!((large.width - small.width * 2.0).abs() < 0.01);
    }

    #[test]
    fn word_wrap_single_line_fits() {
        let lines = word_wrap("Hello", Standard14Font::Courier, 12.0, 200.0);
        assert_eq!(lines, vec!["Hello"]);
    }

    #[test]
    fn word_wrap_explicit_newline() {
        let lines = word_wrap("Line 1\nLine 2", Standard14Font::Courier, 12.0, 200.0);
        assert_eq!(lines, vec!["Line 1", "Line 2"]);
    }

    #[test]
    fn word_wrap_breaks_at_width() {
        // Courier 12pt: each char = 600/1000 * 12 = 7.2pt
        // "AAAA BBBB" = 9 chars. At width 40pt, "AAAA " = 5*7.2=36pt fits, "BBBB" wraps.
        let lines = word_wrap("AAAA BBBB", Standard14Font::Courier, 12.0, 40.0);
        assert_eq!(lines, vec!["AAAA", "BBBB"]);
    }

    #[test]
    fn word_wrap_empty_text() {
        let lines = word_wrap("", Standard14Font::Courier, 12.0, 200.0);
        assert_eq!(lines, vec![""]);
    }

    #[test]
    fn word_wrap_single_word_wider_than_width() {
        // "ABCDEFGHIJ" at Courier 12pt = 10*7.2=72pt, width=30pt
        // Word overflows — kept as-is on one line (no mid-word break)
        let lines = word_wrap("ABCDEFGHIJ", Standard14Font::Courier, 12.0, 30.0);
        assert_eq!(lines, vec!["ABCDEFGHIJ"]);
    }

    #[test]
    fn word_wrap_multiple_words_across_lines() {
        // Each word "AAA" = 3*7.2=21.6pt, space=600/1000*12=7.2pt (Courier space is 600)
        // "AAA AAA AAA" — width 55pt:
        // "AAA AAA" = 21.6+7.2+21.6=50.4 fits
        // "AAA AAA AAA" = 50.4+7.2+21.6=79.2 doesn't fit → wrap
        let lines = word_wrap("AAA AAA AAA", Standard14Font::Courier, 12.0, 55.0);
        assert_eq!(lines, vec!["AAA AAA", "AAA"]);
    }
}
