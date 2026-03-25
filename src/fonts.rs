// Unified font model: FontId, PdfEmbedding, WidthTable.

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

#[cfg(test)]
mod tests {
    use super::*;

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
