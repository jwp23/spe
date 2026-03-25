// Unified font model: FontId, PdfEmbedding, WidthTable, FontEntry, FontRegistry.

/// Lightweight font identifier. Stored in overlays, messages, undo commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Per-character lookup table for the Latin-1 range (0-255).
    /// Characters outside this range use the default width.
    Proportional { widths: [f32; 256], default: f32 },
}

impl WidthTable {
    /// Look up the width of a character in 1000em units.
    pub fn char_width(&self, c: char) -> f32 {
        match self {
            Self::Monospaced(w) => *w,
            Self::Proportional { widths, default } => {
                let code = c as u32;
                if code < 256 {
                    widths[code as usize]
                } else {
                    *default
                }
            }
        }
    }
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
}

/// Holds all known fonts. The Standard 14 are always present.
#[derive(Debug)]
pub struct FontRegistry {
    fonts: Vec<FontEntry>,
}

impl FontRegistry {
    /// Build a registry pre-populated with the 14 Standard PDF fonts.
    pub fn new() -> Self {
        Self {
            fonts: standard_14_fonts(),
        }
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
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
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
        },
    ]
}

/// Build the Helvetica / Helvetica-Oblique AFM width table.
/// Source: Adobe AFM data. Fallback (unknown glyphs) = 556.
fn helvetica_widths() -> WidthTable {
    let mut w = [556.0_f32; 256];
    let entries: &[(usize, f32)] = &[
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
    ];
    for &(i, v) in entries {
        w[i] = v;
    }
    WidthTable::Proportional {
        widths: w,
        default: 556.0,
    }
}

/// Build the Helvetica-Bold / Helvetica-BoldOblique AFM width table.
/// Source: Adobe AFM data. Fallback = 556.
fn helvetica_bold_widths() -> WidthTable {
    let mut w = [556.0_f32; 256];
    let entries: &[(usize, f32)] = &[
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
    ];
    for &(i, v) in entries {
        w[i] = v;
    }
    WidthTable::Proportional {
        widths: w,
        default: 556.0,
    }
}

/// Build the Times-Roman / Times-Italic AFM width table.
/// Source: Adobe AFM data. Fallback = 500.
fn times_roman_widths() -> WidthTable {
    let mut w = [500.0_f32; 256];
    let entries: &[(usize, f32)] = &[
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
    ];
    for &(i, v) in entries {
        w[i] = v;
    }
    WidthTable::Proportional {
        widths: w,
        default: 500.0,
    }
}

/// Build the Times-Bold / Times-BoldItalic AFM width table.
/// Source: Adobe AFM data. Fallback = 500.
fn times_bold_widths() -> WidthTable {
    let mut w = [500.0_f32; 256];
    let entries: &[(usize, f32)] = &[
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
    ];
    for &(i, v) in entries {
        w[i] = v;
    }
    WidthTable::Proportional {
        widths: w,
        default: 500.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_14_standard_fonts() {
        let registry = FontRegistry::new();
        assert_eq!(registry.all().len(), 14);
    }

    #[test]
    fn registry_lookup_by_id() {
        let registry = FontRegistry::new();
        let helvetica = registry.all()[0].id;
        let entry = registry.get(helvetica);
        assert_eq!(entry.display_name, "Helvetica");
    }

    #[test]
    fn registry_all_have_builtin_embedding() {
        let registry = FontRegistry::new();
        for entry in registry.all() {
            assert!(matches!(entry.embedding, PdfEmbedding::BuiltIn));
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
}
