// Unified font model: FontId, PdfEmbedding, WidthTable, FontEntry, FontRegistry.

use crate::coordinate::BoundingBox;
use crate::pdf::win_ansi;
use skrifa::attribute::Style;
use skrifa::instance::Size;
use skrifa::{FontRef, MetadataProvider};

const GREAT_VIBES_BYTES: &[u8] = include_bytes!("../assets/fonts/great-vibes.ttf");
const DANCING_SCRIPT_BYTES: &[u8] = include_bytes!("../assets/fonts/dancing-script.ttf");
const PINYON_SCRIPT_BYTES: &[u8] = include_bytes!("../assets/fonts/pinyon-script.ttf");
const PACIFICO_BYTES: &[u8] = include_bytes!("../assets/fonts/pacifico.ttf");

/// Lightweight font identifier. Stored in overlays, messages, undo commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FontId(pub(crate) u16);

/// How the PDF writer should handle this font.
#[derive(Debug)]
pub enum PdfEmbedding {
    /// Standard 14 font — reference by name, no embedding needed.
    BuiltIn,
    /// Bundled TrueType — embed full font program in PDF.
    TrueType { bytes: &'static [u8] },
}

/// Per-character width data for text measurement.
/// Widths are in units per 1000em (standard AFM/TTF convention).
///
/// Stores the full 256-entry width table inline to avoid indirection;
/// the 1KB size cost is acceptable for performance.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum WidthTable {
    /// All characters have the same width (e.g., Courier).
    Monospaced(f32),
    /// Per-character lookup table indexed by WinAnsiEncoding code (0-255) —
    /// the same code space the PDF writer uses for `/Widths`. Characters
    /// WinAnsiEncoding cannot represent use the default width.
    Proportional { widths: [f32; 256], default: f32 },
}

impl WidthTable {
    /// Look up the width of a character in 1000em units.
    ///
    /// Resolves `c` through [`win_ansi::encode_char`] before indexing, so a
    /// character's width always comes from the same code the PDF writer will
    /// use to show it — including the 27 codes (euro sign, curly quotes,
    /// dashes, etc.) where WinAnsiEncoding diverges from raw Unicode scalars.
    pub fn char_width(&self, c: char) -> f32 {
        match self {
            Self::Monospaced(w) => *w,
            Self::Proportional { widths, default } => match win_ansi::encode_char(c) {
                Some(code) => widths[usize::from(code)],
                None => *default,
            },
        }
    }
}

/// Font descriptor values extracted from a TrueType font for PDF embedding.
/// Units are in 1000em (PDF convention), except italic_angle (degrees).
#[derive(Debug)]
pub struct FontDescriptorInfo {
    pub ascent: i64,
    pub descent: i64,
    pub cap_height: i64,
    pub italic_angle: f32,
    /// PDF font flags (32 = Nonsymbolic, 64 = Italic).
    pub flags: i64,
    /// [xMin, yMin, xMax, yMax] in 1000em units.
    pub bbox: [i64; 4],
    /// Approximate dominant vertical stem width.
    pub stem_v: i64,
}

/// All data needed to use a font for display, measurement, and PDF output.
#[derive(Debug)]
pub struct FontEntry {
    pub id: FontId,
    /// Human-readable name shown in the UI (e.g. "Helvetica Bold").
    pub display_name: &'static str,
    /// Name used in the PDF content stream (e.g. "Helvetica-Bold").
    pub pdf_name: &'static str,
    /// Iced font descriptor for rendering in the canvas.
    pub iced_font: iced::Font,
    /// How the PDF writer should handle this font.
    pub embedding: PdfEmbedding,
    /// Per-character widths for text measurement.
    pub widths: WidthTable,
    /// Font descriptor extracted from TTF, used by the PDF writer.
    /// None for Standard 14 fonts (built-in, no embedding).
    pub descriptor: Option<FontDescriptorInfo>,
    /// Height of the lowercase letters as a fraction of the em, read from the
    /// face itself. None for the Standard 14, which render through whatever
    /// face the system resolves — the face we could measure is not the face
    /// that reaches the screen.
    pub x_height_ratio: Option<f32>,
}

/// Holds all known fonts. The Standard 14 are always present.
#[derive(Debug)]
pub struct FontRegistry {
    fonts: Vec<FontEntry>,
}

impl FontRegistry {
    /// Build a registry pre-populated with the 14 Standard PDF fonts and 4 bundled cursive fonts.
    pub fn new() -> Self {
        let mut registry = Self {
            fonts: standard_14_fonts(),
        };

        let bundled: &[(&'static str, &'static str, &'static [u8])] = &[
            ("Great Vibes", "GreatVibes-Regular", GREAT_VIBES_BYTES),
            (
                "Dancing Script",
                "DancingScript-Regular",
                DANCING_SCRIPT_BYTES,
            ),
            ("Pinyon Script", "PinyonScript-Regular", PINYON_SCRIPT_BYTES),
            ("Pacifico", "Pacifico-Regular", PACIFICO_BYTES),
        ];

        for &(display, pdf, bytes) in bundled {
            registry.add_entry(FontEntry {
                id: FontId::default(),
                display_name: display,
                pdf_name: pdf,
                iced_font: iced::Font {
                    family: iced::font::Family::Name(display),
                    weight: iced::font::Weight::Normal,
                    stretch: iced::font::Stretch::Normal,
                    style: iced::font::Style::Normal,
                },
                embedding: PdfEmbedding::TrueType { bytes },
                widths: build_ttf_width_table(bytes),
                descriptor: Some(extract_font_descriptor(bytes)),
                x_height_ratio: extract_x_height_ratio(bytes),
            });
        }

        registry
    }

    /// All registered fonts in order.
    pub fn all(&self) -> &[FontEntry] {
        &self.fonts
    }

    /// Look up a font by id. Panics if the id is not in the registry.
    pub fn get(&self, id: FontId) -> &FontEntry {
        self.fonts
            .iter()
            .find(|e| e.id == id)
            .expect("FontId not found in registry")
    }

    /// The id of the default font (Helvetica).
    pub fn default_font(&self) -> FontId {
        self.fonts[0].id
    }

    /// Compute the bounding box of text using a font in the registry.
    /// Width is computed from per-character widths in the font's WidthTable.
    /// Height is the font size.
    pub fn overlay_bounding_box(&self, text: &str, font_id: FontId, font_size: f32) -> BoundingBox {
        let entry = self.get(font_id);
        let width: f32 = text
            .chars()
            .map(|c| entry.widths.char_width(c) * font_size / 1000.0)
            .sum();
        BoundingBox {
            width,
            height: font_size,
        }
    }

    /// Wrap text to fit within a maximum width, breaking at word boundaries.
    /// Respects explicit newlines. Words wider than max_width are kept intact (no mid-word break).
    /// Returns one line per logical line of wrapped output.
    pub fn word_wrap(
        &self,
        text: &str,
        font_id: FontId,
        font_size: f32,
        max_width: f32,
    ) -> Vec<String> {
        let entry = self.get(font_id);
        let mut lines = Vec::new();

        for paragraph in text.split('\n') {
            if paragraph.is_empty() {
                lines.push(String::new());
                continue;
            }

            let words: Vec<&str> = paragraph.split_whitespace().collect();
            if words.is_empty() {
                lines.push(String::new());
                continue;
            }

            let mut current_line = String::new();
            let mut current_width = 0.0_f32;
            let space_width = entry.widths.char_width(' ') * font_size / 1000.0;

            for word in &words {
                let word_width: f32 = word
                    .chars()
                    .map(|c| entry.widths.char_width(c) * font_size / 1000.0)
                    .sum();

                if current_line.is_empty() {
                    current_line.push_str(word);
                    current_width = word_width;
                } else if current_width + space_width + word_width <= max_width {
                    current_line.push(' ');
                    current_line.push_str(word);
                    current_width += space_width + word_width;
                } else {
                    lines.push(current_line);
                    current_line = word.to_string();
                    current_width = word_width;
                }
            }
            lines.push(current_line);
        }

        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }

    /// Register an additional font entry. Returns its `FontId`.
    /// The entry's `id` field is overwritten with a freshly assigned id.
    pub fn add_entry(&mut self, mut entry: FontEntry) -> FontId {
        let next = self.fonts.iter().map(|e| e.id.0).max().unwrap_or(0) + 1;
        let id = FontId(next);
        entry.id = id;
        self.fonts.push(entry);
        id
    }

    /// Find a font by display name or PDF name. Returns None if not found.
    pub fn find_by_name(&self, name: &str) -> Option<FontId> {
        self.fonts
            .iter()
            .find(|e| e.display_name == name || e.pdf_name == name)
            .map(|e| e.id)
    }
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract per-character width data from a TrueType font's glyph metrics.
///
/// Widths are normalised to 1000em units (standard PDF/AFM convention) and
/// indexed by WinAnsiEncoding code, not by Unicode scalar — the 27 codes
/// where WinAnsiEncoding diverges from Latin-1 (0x80-0x9F: euro sign, curly
/// quotes, dashes, etc.) name Unicode scalars far outside 0-255, so indexing
/// by scalar would silently drop their advances even when the font has the
/// glyph. Codes WinAnsiEncoding leaves undefined, or that the font has no
/// glyph for, use the default width.
fn build_ttf_width_table(font_bytes: &[u8]) -> WidthTable {
    let font = FontRef::new(font_bytes).expect("valid TTF");
    let units_per_em = font.metrics(Size::unscaled(), &[][..]).units_per_em as f32;
    let charmap = font.charmap();
    let glyph_metrics = font.glyph_metrics(Size::unscaled(), &[][..]);
    let mut widths = [0.0_f32; 256];
    let mut has_glyph = [false; 256];

    for code in 0u8..=255 {
        if let Some(c) = win_ansi::decode(code)
            && let Some(glyph_id) = charmap.map(c)
        {
            let advance = glyph_metrics.advance_width(glyph_id).unwrap_or(0.0);
            let index = usize::from(code);
            widths[index] = advance / units_per_em * 1000.0;
            has_glyph[index] = true;
        }
    }
    let default = widths[usize::from(b' ')].max(500.0);
    for code in 0..widths.len() {
        if !has_glyph[code] {
            widths[code] = default;
        }
    }
    WidthTable::Proportional { widths, default }
}

/// PDF font descriptor flag bits (PDF 32000-1 Table 123).
const FLAG_NONSYMBOLIC: i64 = 32;
const FLAG_ITALIC: i64 = 64;

/// The `/Flags` value for a text font. Nonsymbolic is always set: these fonts
/// are read through `/Encoding`, and a font a reader takes for symbolic is
/// read through its own built-in encoding instead.
fn descriptor_flags(is_italic: bool) -> i64 {
    if is_italic {
        FLAG_NONSYMBOLIC | FLAG_ITALIC
    } else {
        FLAG_NONSYMBOLIC
    }
}

/// Extract font descriptor values from a TrueType font for PDF embedding.
///
/// All metric values are normalised to 1000em units (PDF convention).
fn extract_font_descriptor(font_bytes: &[u8]) -> FontDescriptorInfo {
    let font = FontRef::new(font_bytes).expect("valid TTF");
    let metrics = font.metrics(Size::unscaled(), &[][..]);
    let units_per_em = metrics.units_per_em as f32;
    let scale = |v: f32| -> i64 { (v / units_per_em * 1000.0).round() as i64 };
    let bounds = metrics.bounds.unwrap_or_default();
    // Mirrors ttf-parser's is_italic(): the OS/2 fsSelection ITALIC bit, or a
    // non-zero post-table italic angle.
    let is_italic = font.attributes().style == Style::Italic || metrics.italic_angle != 0.0;
    FontDescriptorInfo {
        ascent: scale(metrics.ascent),
        descent: scale(metrics.descent),
        cap_height: scale(metrics.cap_height.unwrap_or(metrics.ascent)),
        italic_angle: metrics.italic_angle,
        flags: descriptor_flags(is_italic),
        bbox: [
            scale(bounds.x_min),
            scale(bounds.y_min),
            scale(bounds.x_max),
            scale(bounds.y_max),
        ],
        stem_v: 80,
    }
}

/// The face's x-height as a fraction of its em, or None when the face does
/// not declare one. Two faces set at the same point size look the same size
/// when their x-heights match, so this is what a preview normalizes on.
fn extract_x_height_ratio(font_bytes: &[u8]) -> Option<f32> {
    let font = FontRef::new(font_bytes).expect("valid TTF");
    let metrics = font.metrics(Size::unscaled(), &[][..]);
    let x_height = metrics.x_height?;
    Some(x_height / metrics.units_per_em as f32)
}

fn standard_14_fonts() -> Vec<FontEntry> {
    use iced::font::{Family, Style, Weight};

    vec![
        FontEntry {
            id: FontId(0),
            display_name: "Helvetica",
            pdf_name: "Helvetica",
            iced_font: iced::Font {
                family: Family::SansSerif,
                weight: Weight::Normal,
                style: Style::Normal,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: helvetica_widths(),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(1),
            display_name: "Helvetica Bold",
            pdf_name: "Helvetica-Bold",
            iced_font: iced::Font {
                family: Family::SansSerif,
                weight: Weight::Bold,
                style: Style::Normal,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: helvetica_bold_widths(),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(2),
            display_name: "Helvetica Oblique",
            pdf_name: "Helvetica-Oblique",
            iced_font: iced::Font {
                family: Family::SansSerif,
                weight: Weight::Normal,
                style: Style::Oblique,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: helvetica_widths(),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(3),
            display_name: "Helvetica Bold Oblique",
            pdf_name: "Helvetica-BoldOblique",
            iced_font: iced::Font {
                family: Family::SansSerif,
                weight: Weight::Bold,
                style: Style::Oblique,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: helvetica_bold_widths(),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(4),
            display_name: "Times Roman",
            pdf_name: "Times-Roman",
            iced_font: iced::Font {
                family: Family::Serif,
                weight: Weight::Normal,
                style: Style::Normal,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: times_roman_widths(),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(5),
            display_name: "Times Bold",
            pdf_name: "Times-Bold",
            iced_font: iced::Font {
                family: Family::Serif,
                weight: Weight::Bold,
                style: Style::Normal,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: times_bold_widths(),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(6),
            display_name: "Times Italic",
            pdf_name: "Times-Italic",
            iced_font: iced::Font {
                family: Family::Serif,
                weight: Weight::Normal,
                style: Style::Italic,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: times_roman_widths(),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(7),
            display_name: "Times Bold Italic",
            pdf_name: "Times-BoldItalic",
            iced_font: iced::Font {
                family: Family::Serif,
                weight: Weight::Bold,
                style: Style::Italic,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: times_bold_widths(),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(8),
            display_name: "Courier",
            pdf_name: "Courier",
            iced_font: iced::Font {
                family: Family::Monospace,
                weight: Weight::Normal,
                style: Style::Normal,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: WidthTable::Monospaced(600.0),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(9),
            display_name: "Courier Bold",
            pdf_name: "Courier-Bold",
            iced_font: iced::Font {
                family: Family::Monospace,
                weight: Weight::Bold,
                style: Style::Normal,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: WidthTable::Monospaced(600.0),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(10),
            display_name: "Courier Oblique",
            pdf_name: "Courier-Oblique",
            iced_font: iced::Font {
                family: Family::Monospace,
                weight: Weight::Normal,
                style: Style::Oblique,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: WidthTable::Monospaced(600.0),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(11),
            display_name: "Courier Bold Oblique",
            pdf_name: "Courier-BoldOblique",
            iced_font: iced::Font {
                family: Family::Monospace,
                weight: Weight::Bold,
                style: Style::Oblique,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: WidthTable::Monospaced(600.0),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(12),
            display_name: "Symbol",
            pdf_name: "Symbol",
            iced_font: iced::Font {
                family: Family::SansSerif,
                weight: Weight::Normal,
                style: Style::Normal,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: WidthTable::Monospaced(600.0),
            descriptor: None,
            x_height_ratio: None,
        },
        FontEntry {
            id: FontId(13),
            display_name: "Zapf Dingbats",
            pdf_name: "ZapfDingbats",
            iced_font: iced::Font {
                family: Family::SansSerif,
                weight: Weight::Normal,
                style: Style::Normal,
                ..iced::Font::DEFAULT
            },
            embedding: PdfEmbedding::BuiltIn,
            widths: WidthTable::Monospaced(600.0),
            descriptor: None,
            x_height_ratio: None,
        },
    ]
}

/// Build a proportional width table from an array of (WinAnsiEncoding code,
/// width) pairs. Codes not listed get the `default` width.
fn build_proportional_width_table(entries: &[(u8, f32)], default: f32) -> WidthTable {
    let mut widths = [default; 256];
    for &(i, v) in entries {
        widths[i as usize] = v;
    }
    WidthTable::Proportional { widths, default }
}

/// Build the Helvetica / Helvetica-Oblique AFM width table.
/// Source: Adobe AFM data. Fallback (unknown glyphs) = 556.
fn helvetica_widths() -> WidthTable {
    build_proportional_width_table(
        &[
            (32, 278.0),
            (33, 278.0),
            (34, 355.0),
            (35, 556.0),
            (36, 556.0),
            (37, 889.0),
            (38, 667.0),
            (39, 191.0),
            (40, 333.0),
            (41, 333.0),
            (42, 389.0),
            (43, 584.0),
            (44, 278.0),
            (45, 333.0),
            (46, 278.0),
            (47, 278.0),
            (48, 556.0),
            (49, 556.0),
            (50, 556.0),
            (51, 556.0),
            (52, 556.0),
            (53, 556.0),
            (54, 556.0),
            (55, 556.0),
            (56, 556.0),
            (57, 556.0),
            (58, 278.0),
            (59, 278.0),
            (60, 584.0),
            (61, 584.0),
            (62, 584.0),
            (63, 556.0),
            (64, 1015.0),
            (65, 667.0),
            (66, 667.0),
            (67, 722.0),
            (68, 722.0),
            (69, 667.0),
            (70, 611.0),
            (71, 778.0),
            (72, 722.0),
            (73, 278.0),
            (74, 500.0),
            (75, 667.0),
            (76, 556.0),
            (77, 833.0),
            (78, 722.0),
            (79, 778.0),
            (80, 667.0),
            (81, 778.0),
            (82, 722.0),
            (83, 667.0),
            (84, 611.0),
            (85, 722.0),
            (86, 667.0),
            (87, 944.0),
            (88, 667.0),
            (89, 667.0),
            (90, 611.0),
            (91, 278.0),
            (92, 278.0),
            (93, 278.0),
            (94, 469.0),
            (95, 556.0),
            (96, 333.0),
            (97, 556.0),
            (98, 556.0),
            (99, 500.0),
            (100, 556.0),
            (101, 556.0),
            (102, 278.0),
            (103, 556.0),
            (104, 556.0),
            (105, 222.0),
            (106, 222.0),
            (107, 500.0),
            (108, 222.0),
            (109, 833.0),
            (110, 556.0),
            (111, 556.0),
            (112, 556.0),
            (113, 556.0),
            (114, 333.0),
            (115, 500.0),
            (116, 278.0),
            (117, 556.0),
            (118, 500.0),
            (119, 722.0),
            (120, 500.0),
            (121, 500.0),
            (122, 500.0),
            (123, 334.0),
            (124, 260.0),
            (125, 334.0),
            (126, 584.0),
        ],
        556.0,
    )
}

/// Build the Helvetica-Bold / Helvetica-BoldOblique AFM width table.
/// Source: Adobe AFM data. Fallback = 556.
fn helvetica_bold_widths() -> WidthTable {
    build_proportional_width_table(
        &[
            (32, 278.0),
            (33, 333.0),
            (34, 474.0),
            (35, 556.0),
            (36, 556.0),
            (37, 889.0),
            (38, 722.0),
            (39, 238.0),
            (40, 333.0),
            (41, 333.0),
            (42, 389.0),
            (43, 584.0),
            (44, 278.0),
            (45, 333.0),
            (46, 278.0),
            (47, 278.0),
            (48, 556.0),
            (49, 556.0),
            (50, 556.0),
            (51, 556.0),
            (52, 556.0),
            (53, 556.0),
            (54, 556.0),
            (55, 556.0),
            (56, 556.0),
            (57, 556.0),
            (58, 333.0),
            (59, 333.0),
            (60, 584.0),
            (61, 584.0),
            (62, 584.0),
            (63, 611.0),
            (64, 975.0),
            (65, 722.0),
            (66, 722.0),
            (67, 722.0),
            (68, 722.0),
            (69, 667.0),
            (70, 611.0),
            (71, 778.0),
            (72, 722.0),
            (73, 278.0),
            (74, 556.0),
            (75, 722.0),
            (76, 611.0),
            (77, 833.0),
            (78, 722.0),
            (79, 778.0),
            (80, 667.0),
            (81, 778.0),
            (82, 722.0),
            (83, 667.0),
            (84, 611.0),
            (85, 722.0),
            (86, 667.0),
            (87, 944.0),
            (88, 667.0),
            (89, 667.0),
            (90, 611.0),
            (91, 333.0),
            (92, 278.0),
            (93, 333.0),
            (94, 584.0),
            (95, 556.0),
            (96, 333.0),
            (97, 556.0),
            (98, 611.0),
            (99, 556.0),
            (100, 611.0),
            (101, 556.0),
            (102, 333.0),
            (103, 611.0),
            (104, 611.0),
            (105, 278.0),
            (106, 278.0),
            (107, 556.0),
            (108, 278.0),
            (109, 889.0),
            (110, 611.0),
            (111, 611.0),
            (112, 611.0),
            (113, 611.0),
            (114, 389.0),
            (115, 556.0),
            (116, 333.0),
            (117, 611.0),
            (118, 556.0),
            (119, 778.0),
            (120, 556.0),
            (121, 556.0),
            (122, 500.0),
            (123, 389.0),
            (124, 280.0),
            (125, 389.0),
            (126, 584.0),
        ],
        556.0,
    )
}

/// Build the Times-Roman / Times-Italic AFM width table.
/// Source: Adobe AFM data. Fallback = 500.
fn times_roman_widths() -> WidthTable {
    build_proportional_width_table(
        &[
            (32, 250.0),
            (33, 333.0),
            (34, 408.0),
            (35, 500.0),
            (36, 500.0),
            (37, 833.0),
            (38, 778.0),
            (39, 180.0),
            (40, 333.0),
            (41, 333.0),
            (42, 500.0),
            (43, 564.0),
            (44, 250.0),
            (45, 333.0),
            (46, 250.0),
            (47, 278.0),
            (48, 500.0),
            (49, 500.0),
            (50, 500.0),
            (51, 500.0),
            (52, 500.0),
            (53, 500.0),
            (54, 500.0),
            (55, 500.0),
            (56, 500.0),
            (57, 500.0),
            (58, 278.0),
            (59, 278.0),
            (60, 564.0),
            (61, 564.0),
            (62, 564.0),
            (63, 444.0),
            (64, 921.0),
            (65, 722.0),
            (66, 667.0),
            (67, 667.0),
            (68, 722.0),
            (69, 611.0),
            (70, 556.0),
            (71, 722.0),
            (72, 722.0),
            (73, 333.0),
            (74, 389.0),
            (75, 722.0),
            (76, 611.0),
            (77, 889.0),
            (78, 722.0),
            (79, 722.0),
            (80, 556.0),
            (81, 722.0),
            (82, 667.0),
            (83, 556.0),
            (84, 611.0),
            (85, 722.0),
            (86, 722.0),
            (87, 944.0),
            (88, 722.0),
            (89, 722.0),
            (90, 611.0),
            (91, 333.0),
            (92, 278.0),
            (93, 333.0),
            (94, 469.0),
            (95, 500.0),
            (96, 333.0),
            (97, 444.0),
            (98, 500.0),
            (99, 444.0),
            (100, 500.0),
            (101, 444.0),
            (102, 333.0),
            (103, 500.0),
            (104, 500.0),
            (105, 278.0),
            (106, 278.0),
            (107, 500.0),
            (108, 278.0),
            (109, 778.0),
            (110, 500.0),
            (111, 500.0),
            (112, 500.0),
            (113, 500.0),
            (114, 333.0),
            (115, 389.0),
            (116, 278.0),
            (117, 500.0),
            (118, 500.0),
            (119, 722.0),
            (120, 500.0),
            (121, 500.0),
            (122, 444.0),
            (123, 480.0),
            (124, 200.0),
            (125, 480.0),
            (126, 541.0),
        ],
        500.0,
    )
}

/// Build the Times-Bold / Times-BoldItalic AFM width table.
/// Source: Adobe AFM data. Fallback = 500.
fn times_bold_widths() -> WidthTable {
    build_proportional_width_table(
        &[
            (32, 250.0),
            (33, 333.0),
            (34, 555.0),
            (35, 500.0),
            (36, 500.0),
            (37, 1000.0),
            (38, 833.0),
            (39, 278.0),
            (40, 333.0),
            (41, 333.0),
            (42, 500.0),
            (43, 570.0),
            (44, 250.0),
            (45, 333.0),
            (46, 250.0),
            (47, 278.0),
            (48, 500.0),
            (49, 500.0),
            (50, 500.0),
            (51, 500.0),
            (52, 500.0),
            (53, 500.0),
            (54, 500.0),
            (55, 500.0),
            (56, 500.0),
            (57, 500.0),
            (58, 333.0),
            (59, 333.0),
            (60, 570.0),
            (61, 570.0),
            (62, 570.0),
            (63, 500.0),
            (64, 930.0),
            (65, 722.0),
            (66, 667.0),
            (67, 722.0),
            (68, 722.0),
            (69, 667.0),
            (70, 611.0),
            (71, 778.0),
            (72, 778.0),
            (73, 389.0),
            (74, 500.0),
            (75, 778.0),
            (76, 667.0),
            (77, 944.0),
            (78, 722.0),
            (79, 778.0),
            (80, 611.0),
            (81, 778.0),
            (82, 722.0),
            (83, 556.0),
            (84, 667.0),
            (85, 722.0),
            (86, 722.0),
            (87, 1000.0),
            (88, 722.0),
            (89, 722.0),
            (90, 667.0),
            (91, 333.0),
            (92, 278.0),
            (93, 333.0),
            (94, 581.0),
            (95, 500.0),
            (96, 333.0),
            (97, 500.0),
            (98, 556.0),
            (99, 444.0),
            (100, 556.0),
            (101, 444.0),
            (102, 333.0),
            (103, 500.0),
            (104, 556.0),
            (105, 278.0),
            (106, 333.0),
            (107, 556.0),
            (108, 278.0),
            (109, 833.0),
            (110, 556.0),
            (111, 500.0),
            (112, 556.0),
            (113, 556.0),
            (114, 444.0),
            (115, 389.0),
            (116, 333.0),
            (117, 556.0),
            (118, 500.0),
            (119, 722.0),
            (120, 500.0),
            (121, 500.0),
            (122, 444.0),
            (123, 394.0),
            (124, 220.0),
            (125, 394.0),
            (126, 520.0),
        ],
        500.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_standard_fonts() {
        let registry = FontRegistry::new();
        // 14 Standard 14 + 4 bundled cursive fonts.
        assert_eq!(registry.all().len(), 18);
    }

    #[test]
    fn registry_lookup_by_id() {
        let registry = FontRegistry::new();
        let helvetica = registry.all()[0].id;
        let entry = registry.get(helvetica);
        assert_eq!(entry.display_name, "Helvetica");
    }

    #[test]
    fn registry_standard14_have_builtin_embedding() {
        let registry = FontRegistry::new();
        let standard_names = [
            "Helvetica",
            "Helvetica Bold",
            "Helvetica Oblique",
            "Helvetica Bold Oblique",
            "Times Roman",
            "Times Bold",
            "Times Italic",
            "Times Bold Italic",
            "Courier",
            "Courier Bold",
            "Courier Oblique",
            "Courier Bold Oblique",
            "Symbol",
            "Zapf Dingbats",
        ];
        for name in standard_names {
            let id = registry.find_by_name(name).unwrap();
            let entry = registry.get(id);
            assert!(
                matches!(entry.embedding, PdfEmbedding::BuiltIn),
                "{name} should have BuiltIn embedding"
            );
        }
    }

    #[test]
    fn registry_bundled_fonts_have_truetype_embedding() {
        let registry = FontRegistry::new();
        for name in ["Great Vibes", "Dancing Script", "Pinyon Script", "Pacifico"] {
            let id = registry.find_by_name(name).unwrap();
            let entry = registry.get(id);
            assert!(
                matches!(entry.embedding, PdfEmbedding::TrueType { .. }),
                "{name} should have TrueType embedding"
            );
        }
    }

    #[test]
    fn bundled_fonts_report_the_x_height_their_face_declares() {
        let registry = FontRegistry::new();
        for name in ["Great Vibes", "Dancing Script", "Pinyon Script", "Pacifico"] {
            let id = registry.find_by_name(name).unwrap();
            let ratio = registry
                .get(id)
                .x_height_ratio
                .unwrap_or_else(|| panic!("{name} embeds its face, so its x-height is knowable"));
            assert!(
                (0.0..1.0).contains(&ratio),
                "{name}'s x-height is {ratio} of the em, which no face has"
            );
        }
    }

    #[test]
    fn a_cursive_face_has_a_smaller_x_height_than_a_rounder_one() {
        // Reads a real per-face metric rather than a constant: Great Vibes is
        // the most extreme of the bundled scripts, Pacifico the least.
        let registry = FontRegistry::new();
        let ratio = |name: &str| {
            registry
                .get(registry.find_by_name(name).unwrap())
                .x_height_ratio
                .unwrap()
        };
        assert!(
            ratio("Great Vibes") < ratio("Pacifico"),
            "Great Vibes ({}) should sit lower than Pacifico ({})",
            ratio("Great Vibes"),
            ratio("Pacifico")
        );
    }

    #[test]
    fn standard_14_fonts_report_no_x_height() {
        // They render through whatever face the system resolves, so the face
        // we could measure is not the face that reaches the screen.
        let registry = FontRegistry::new();
        for name in ["Helvetica", "Times Roman", "Courier", "Symbol"] {
            let id = registry.find_by_name(name).unwrap();
            assert!(
                registry.get(id).x_height_ratio.is_none(),
                "{name} is not embedded, so its rendered x-height is unknown"
            );
        }
    }

    #[test]
    fn registry_helvetica_pdf_name() {
        let registry = FontRegistry::new();
        let entry = &registry.all()[0];
        assert_eq!(entry.pdf_name, "Helvetica");
    }

    #[test]
    fn registry_courier_is_monospaced() {
        let registry = FontRegistry::new();
        let courier = registry
            .all()
            .iter()
            .find(|e| e.display_name == "Courier")
            .unwrap();
        assert!((courier.widths.char_width('A') - 600.0).abs() < f32::EPSILON);
        assert!((courier.widths.char_width('z') - 600.0).abs() < f32::EPSILON);
    }

    #[test]
    fn registry_helvetica_is_proportional() {
        let registry = FontRegistry::new();
        let helv = registry
            .all()
            .iter()
            .find(|e| e.display_name == "Helvetica")
            .unwrap();
        let a_width = helv.widths.char_width('A');
        let i_width = helv.widths.char_width('i');
        assert!(
            a_width > i_width,
            "A ({a_width}) should be wider than i ({i_width})"
        );
    }

    #[test]
    fn registry_default_font_is_helvetica() {
        let registry = FontRegistry::new();
        let entry = registry.get(registry.default_font());
        assert_eq!(entry.display_name, "Helvetica");
        assert_eq!(registry.all()[0].id, FontId::default());
    }

    #[test]
    fn add_entry_assigns_next_id_and_is_retrievable() {
        let mut registry = FontRegistry::new();
        assert_eq!(registry.all().len(), 18);
        let entry = FontEntry {
            id: FontId::default(),
            display_name: "TestFont",
            pdf_name: "TestFont-Regular",
            iced_font: iced::Font::default(),
            embedding: PdfEmbedding::BuiltIn,
            widths: WidthTable::Monospaced(500.0),
            descriptor: None,
            x_height_ratio: None,
        };
        let id = registry.add_entry(entry);
        assert_eq!(registry.all().len(), 19);
        let retrieved = registry.get(id);
        assert_eq!(retrieved.display_name, "TestFont");
        assert_eq!(retrieved.id, id);
    }

    #[test]
    fn font_id_is_copy_and_eq() {
        let a = FontId(0);
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn font_id_can_be_hashed() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(FontId(0));
        set.insert(FontId(1));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn pdf_embedding_built_in_variant() {
        let e = PdfEmbedding::BuiltIn;
        assert!(matches!(e, PdfEmbedding::BuiltIn));
    }

    #[test]
    fn pdf_embedding_truetype_variant() {
        let bytes: &[u8] = &[0, 1, 0, 0];
        let e = PdfEmbedding::TrueType { bytes };
        assert!(matches!(e, PdfEmbedding::TrueType { .. }));
    }

    #[test]
    fn monospaced_width_table_returns_constant() {
        let table = WidthTable::Monospaced(600.0);
        assert!((table.char_width('A') - 600.0).abs() < f32::EPSILON);
        assert!((table.char_width('z') - 600.0).abs() < f32::EPSILON);
    }

    #[test]
    fn proportional_width_table_returns_per_char() {
        let mut widths = [500.0_f32; 256];
        widths[b'A' as usize] = 667.0;
        widths[b'i' as usize] = 222.0;
        let table = WidthTable::Proportional {
            widths,
            default: 500.0,
        };
        assert!((table.char_width('A') - 667.0).abs() < f32::EPSILON);
        assert!((table.char_width('i') - 222.0).abs() < f32::EPSILON);
    }

    #[test]
    fn proportional_width_table_uses_default_for_non_latin1() {
        let widths = [500.0_f32; 256];
        let table = WidthTable::Proportional {
            widths,
            default: 750.0,
        };
        assert!((table.char_width('\u{1F600}') - 750.0).abs() < f32::EPSILON);
    }

    #[test]
    fn registry_bounding_box_courier_monospaced() {
        let registry = FontRegistry::new();
        let courier = registry
            .all()
            .iter()
            .find(|e| e.display_name == "Courier")
            .unwrap();
        let bbox = registry.overlay_bounding_box("Hello", courier.id, 12.0);
        let expected = 5.0 * 600.0 * 12.0 / 1000.0; // 36.0
        assert!((bbox.width - expected).abs() < f32::EPSILON);
        assert!((bbox.height - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn registry_bounding_box_helvetica_proportional() {
        let registry = FontRegistry::new();
        let helv = registry
            .all()
            .iter()
            .find(|e| e.display_name == "Helvetica")
            .unwrap();
        let bbox = registry.overlay_bounding_box("Hello", helv.id, 12.0);
        assert!(bbox.width > 0.0);
        assert!((bbox.height - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn registry_word_wrap_splits_long_text() {
        let registry = FontRegistry::new();
        let courier = registry
            .all()
            .iter()
            .find(|e| e.display_name == "Courier")
            .unwrap();
        let lines = registry.word_wrap("Hello World", courier.id, 12.0, 50.0);
        assert!(lines.len() > 1, "Should wrap at 50pt with Courier 12pt");
    }

    #[test]
    fn registry_word_wrap_no_split_when_fits() {
        let registry = FontRegistry::new();
        let courier = registry
            .all()
            .iter()
            .find(|e| e.display_name == "Courier")
            .unwrap();
        let lines = registry.word_wrap("Hi", courier.id, 12.0, 200.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "Hi");
    }

    #[test]
    fn registry_word_wrap_empty_text() {
        let registry = FontRegistry::new();
        let courier = registry
            .all()
            .iter()
            .find(|e| e.display_name == "Courier")
            .unwrap();
        let lines = registry.word_wrap("", courier.id, 12.0, 200.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "");
    }

    #[test]
    fn registry_word_wrap_respects_explicit_newlines() {
        let registry = FontRegistry::new();
        let courier = registry
            .all()
            .iter()
            .find(|e| e.display_name == "Courier")
            .unwrap();
        let lines = registry.word_wrap("Hello\nWorld", courier.id, 12.0, 200.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "Hello");
        assert_eq!(lines[1], "World");
    }

    #[test]
    fn registry_word_wrap_keeps_wide_word_intact() {
        let registry = FontRegistry::new();
        let courier = registry
            .all()
            .iter()
            .find(|e| e.display_name == "Courier")
            .unwrap();
        // Courier at 12pt: each char = 600 * 12 / 1000 = 7.2pt
        // "ABCDEFGHIJ" = 10 chars = 72pt, wider than max_width of 50pt
        let lines = registry.word_wrap("ABCDEFGHIJ", courier.id, 12.0, 50.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "ABCDEFGHIJ");
    }

    #[test]
    fn word_wrap_keeps_word_that_fits_exactly() {
        let registry = FontRegistry::new();
        let courier = registry.find_by_name("Courier").unwrap();
        // "AA BB" is exactly 30.0 wide at size 10: fits on one line.
        let lines = registry.word_wrap("AA BB", courier, 10.0, 30.0);
        assert_eq!(lines, vec!["AA BB".to_string()]);
    }

    #[test]
    fn word_wrap_breaks_word_just_over_the_limit() {
        let registry = FontRegistry::new();
        let courier = registry.find_by_name("Courier").unwrap();
        // 29.9 < 30.0: "BB" no longer fits after "AA ".
        let lines = registry.word_wrap("AA BB", courier, 10.0, 29.9);
        assert_eq!(lines, vec!["AA".to_string(), "BB".to_string()]);
    }

    #[test]
    fn word_wrap_accumulates_line_width_across_words() {
        let registry = FontRegistry::new();
        let courier = registry.find_by_name("Courier").unwrap();
        // "AA BB CC" = 48.0 exactly; "AA BB CC DD" adds " DD" (18.0) -> 66.0.
        // At max 48.0 the break must fall after CC -- anywhere else means the
        // accumulator (fonts.rs:220) drifted.
        let lines = registry.word_wrap("AA BB CC DD", courier, 10.0, 48.0);
        assert_eq!(lines, vec!["AA BB CC".to_string(), "DD".to_string()]);
    }

    #[test]
    fn find_by_name_display_name() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Helvetica").unwrap();
        assert_eq!(registry.get(id).display_name, "Helvetica");
    }

    #[test]
    fn find_by_name_pdf_name() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Helvetica-Bold").unwrap();
        assert_eq!(registry.get(id).display_name, "Helvetica Bold");
    }

    #[test]
    fn find_by_name_returns_none_for_unknown() {
        let registry = FontRegistry::new();
        assert!(registry.find_by_name("Comic Sans").is_none());
    }

    #[test]
    fn find_by_name_all_standard14_resolvable() {
        let registry = FontRegistry::new();
        let names = [
            "Helvetica",
            "Helvetica Bold",
            "Helvetica Oblique",
            "Helvetica Bold Oblique",
            "Times Roman",
            "Times Bold",
            "Times Italic",
            "Times Bold Italic",
            "Courier",
            "Courier Bold",
            "Courier Oblique",
            "Courier Bold Oblique",
            "Symbol",
            "Zapf Dingbats",
        ];
        for name in names {
            assert!(
                registry.find_by_name(name).is_some(),
                "Failed to find: {name}"
            );
        }
    }

    #[test]
    fn registry_has_18_fonts_with_bundled() {
        let registry = FontRegistry::new();
        assert_eq!(registry.all().len(), 18);
    }

    #[test]
    fn registry_has_all_four_bundled() {
        let registry = FontRegistry::new();
        for name in ["Great Vibes", "Dancing Script", "Pinyon Script", "Pacifico"] {
            assert!(registry.find_by_name(name).is_some(), "Missing: {name}");
        }
    }

    #[test]
    fn registry_has_great_vibes_as_truetype() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Great Vibes").unwrap();
        let entry = registry.get(id);
        assert!(matches!(entry.embedding, PdfEmbedding::TrueType { .. }));
    }

    #[test]
    fn bundled_font_has_proportional_widths() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Great Vibes").unwrap();
        let entry = registry.get(id);
        let a_width = entry.widths.char_width('A');
        let i_width = entry.widths.char_width('i');
        assert!(a_width > 0.0);
        assert!(i_width > 0.0);
    }

    #[test]
    fn bundled_font_has_descriptor() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Great Vibes").unwrap();
        let entry = registry.get(id);
        assert!(entry.descriptor.is_some());
        let desc = entry.descriptor.as_ref().unwrap();
        assert!(desc.ascent > 0);
        assert!(desc.descent < 0);
    }

    /// Golden-master values below were captured from the pre-migration
    /// ttf-parser implementation of `extract_font_descriptor` /
    /// `build_ttf_width_table`. They pin the exact descriptor and glyph
    /// width outputs for the bundled TrueType fonts so the skrifa migration
    /// cannot silently change what gets baked into saved PDFs.
    ///
    /// Width tolerance is 0.01 units-per-1000em: well under the 1-unit
    /// rounding the PDF writer applies (`w.round() as i64` in
    /// `pdf/writer.rs`), so any difference here is invisible in output PDFs.
    fn assert_width_close(actual: f32, expected: f32, label: &str) {
        assert!(
            (actual - expected).abs() < 0.01,
            "{label}: expected {expected}, got {actual}"
        );
    }

    fn assert_descriptor_matches(
        desc: &FontDescriptorInfo,
        ascent: i64,
        descent: i64,
        cap_height: i64,
        italic_angle: f32,
        flags: i64,
        bbox: [i64; 4],
    ) {
        assert_eq!(desc.ascent, ascent, "ascent");
        assert_eq!(desc.descent, descent, "descent");
        assert_eq!(desc.cap_height, cap_height, "cap_height");
        assert!(
            (desc.italic_angle - italic_angle).abs() < f32::EPSILON,
            "italic_angle: expected {italic_angle}, got {}",
            desc.italic_angle
        );
        assert_eq!(desc.flags, flags, "flags");
        assert_eq!(desc.bbox, bbox, "bbox");
    }

    #[test]
    fn upright_font_flags_mark_it_nonsymbolic() {
        assert_eq!(descriptor_flags(false), 32);
    }

    /// An italic font is still a text font read through /Encoding. Dropping
    /// Nonsymbolic lets a reader treat it as symbolic and use the font's own
    /// built-in encoding instead, showing the wrong glyphs for our bytes.
    #[test]
    fn italic_font_flags_keep_nonsymbolic_alongside_italic() {
        assert_eq!(descriptor_flags(true), 96);
    }

    #[test]
    fn every_descriptor_flag_value_marks_the_font_nonsymbolic() {
        for is_italic in [false, true] {
            assert_eq!(
                descriptor_flags(is_italic) & 32,
                32,
                "Nonsymbolic must be set (is_italic = {is_italic})"
            );
        }
    }

    #[test]
    fn great_vibes_descriptor_matches_golden_values() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Great Vibes").unwrap();
        let desc = registry.get(id).descriptor.as_ref().unwrap();
        assert_descriptor_matches(desc, 851, -401, 750, 0.0, 32, [-491, -556, 2022, 1153]);
    }

    #[test]
    fn great_vibes_widths_match_golden_values() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Great Vibes").unwrap();
        let widths = &registry.get(id).widths;
        assert_width_close(widths.char_width('A'), 716.0, "A");
        assert_width_close(widths.char_width('a'), 358.0, "a");
        assert_width_close(widths.char_width('i'), 176.0, "i");
        assert_width_close(widths.char_width('W'), 1351.0, "W");
        assert_width_close(widths.char_width('z'), 343.0, "z");
        assert_width_close(widths.char_width(' '), 171.0, " ");
        assert_width_close(widths.char_width('0'), 458.0, "0");
    }

    #[test]
    fn dancing_script_descriptor_matches_golden_values() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Dancing Script").unwrap();
        let desc = registry.get(id).descriptor.as_ref().unwrap();
        assert_descriptor_matches(desc, 920, -280, 720, 0.0, 32, [-239, -284, 1298, 1095]);
    }

    #[test]
    fn dancing_script_widths_match_golden_values() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Dancing Script").unwrap();
        let widths = &registry.get(id).widths;
        assert_width_close(widths.char_width('A'), 592.0, "A");
        assert_width_close(widths.char_width('a'), 436.0, "a");
        assert_width_close(widths.char_width('i'), 241.0, "i");
        assert_width_close(widths.char_width('W'), 906.0, "W");
        assert_width_close(widths.char_width('z'), 317.0, "z");
        assert_width_close(widths.char_width(' '), 260.0, " ");
        assert_width_close(widths.char_width('0'), 630.0, "0");
    }

    #[test]
    fn pinyon_script_descriptor_matches_golden_values() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Pinyon Script").unwrap();
        let desc = registry.get(id).descriptor.as_ref().unwrap();
        assert_descriptor_matches(desc, 863, -384, 678, 0.0, 32, [-562, -470, 1696, 1048]);
    }

    #[test]
    fn pinyon_script_widths_match_golden_values() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Pinyon Script").unwrap();
        let widths = &registry.get(id).widths;
        assert_width_close(widths.char_width('A'), 794.9219, "A");
        assert_width_close(widths.char_width('a'), 417.968_75, "a");
        assert_width_close(widths.char_width('i'), 221.679_69, "i");
        assert_width_close(widths.char_width('W'), 840.8203, "W");
        assert_width_close(widths.char_width('z'), 338.8672, "z");
        assert_width_close(widths.char_width(' '), 245.605_47, " ");
        assert_width_close(widths.char_width('0'), 562.9883, "0");
    }

    #[test]
    fn pacifico_descriptor_matches_golden_values() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Pacifico").unwrap();
        let desc = registry.get(id).descriptor.as_ref().unwrap();
        assert_descriptor_matches(desc, 1303, -453, 840, 0.0, 32, [-593, -457, 1660, 1478]);
    }

    /// `char_width` for the euro sign must resolve to Great Vibes' real glyph
    /// advance, not the proportional table's fallback width. Ground truth is
    /// computed directly from the font's charmap/glyph metrics, independent of
    /// `build_ttf_width_table`, so a bug in that function can't pass by
    /// agreeing with itself.
    #[test]
    fn great_vibes_euro_width_is_the_real_glyph_advance_not_the_fallback() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Great Vibes").unwrap();
        let widths = &registry.get(id).widths;

        let font = FontRef::new(GREAT_VIBES_BYTES).expect("valid TTF");
        let units_per_em = font.metrics(Size::unscaled(), &[][..]).units_per_em as f32;
        let glyph_id = font
            .charmap()
            .map('\u{20AC}')
            .expect("Great Vibes has a euro glyph");
        let glyph_metrics = font.glyph_metrics(Size::unscaled(), &[][..]);
        let expected = glyph_metrics.advance_width(glyph_id).unwrap() / units_per_em * 1000.0;

        // The fallback (space width) is far narrower than a real euro glyph
        // advance in a cursive script font; this also guards against the fix
        // accidentally leaving the fallback in place.
        assert_width_close(widths.char_width('\u{20AC}'), expected, "€");
    }

    #[test]
    fn pacifico_widths_match_golden_values() {
        let registry = FontRegistry::new();
        let id = registry.find_by_name("Pacifico").unwrap();
        let widths = &registry.get(id).widths;
        assert_width_close(widths.char_width('A'), 786.0, "A");
        assert_width_close(widths.char_width('a'), 470.0, "a");
        assert_width_close(widths.char_width('i'), 246.0, "i");
        assert_width_close(widths.char_width('W'), 1217.0, "W");
        assert_width_close(widths.char_width('z'), 445.0, "z");
        assert_width_close(widths.char_width(' '), 265.0, " ");
        assert_width_close(widths.char_width('0'), 543.0, "0");
    }
}
