// Phosphor icon font integration: subsetted TTF and glyph constants.

use iced::Font;
use iced::font::Family;

/// Subsetted Phosphor Icons Regular font (~12 glyphs, < 20KB).
const PHOSPHOR_BYTES: &[u8] = include_bytes!("../../assets/icons/phosphor-subset.ttf");

/// Iced `Font` descriptor for the Phosphor icon font.
pub const ICON_FONT: Font = Font {
    family: Family::Name("Phosphor"),
    weight: iced::font::Weight::Normal,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};

/// Returns the raw font bytes for loading into Iced's font system.
pub fn font_bytes() -> &'static [u8] {
    PHOSPHOR_BYTES
}

// Icon glyph constants (Phosphor Regular codepoints).
pub const ARROW_CLOCKWISE: char = '\u{E036}';
pub const ARROW_COUNTER_CLOCKWISE: char = '\u{E038}';
pub const ARROW_U_UP_LEFT: char = '\u{E08A}';
pub const CARET_LEFT: char = '\u{E138}';
pub const CARET_RIGHT: char = '\u{E13A}';
pub const FLOPPY_DISK: char = '\u{E248}';
pub const FOLDER_OPEN: char = '\u{E256}';
pub const MAGNIFYING_GLASS: char = '\u{E30C}';
pub const MAGNIFYING_GLASS_MINUS: char = '\u{E30E}';
pub const MAGNIFYING_GLASS_PLUS: char = '\u{E310}';
pub const SIDEBAR: char = '\u{EAB6}';
pub const TRASH: char = '\u{E4A6}';

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_bytes_is_not_empty() {
        assert!(!font_bytes().is_empty());
    }

    #[test]
    fn font_bytes_starts_with_ttf_magic() {
        let bytes = font_bytes();
        // TrueType fonts start with 0x00010000
        assert!(bytes.len() > 4);
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x01);
        assert_eq!(bytes[2], 0x00);
        assert_eq!(bytes[3], 0x00);
    }

    #[test]
    fn subsetted_font_under_20kb() {
        assert!(
            font_bytes().len() < 20_000,
            "Font is {} bytes, expected < 20KB",
            font_bytes().len()
        );
    }

    #[test]
    fn icon_constants_are_in_private_use_area() {
        let icons = [
            ARROW_CLOCKWISE,
            ARROW_COUNTER_CLOCKWISE,
            ARROW_U_UP_LEFT,
            CARET_LEFT,
            CARET_RIGHT,
            FLOPPY_DISK,
            FOLDER_OPEN,
            MAGNIFYING_GLASS,
            MAGNIFYING_GLASS_MINUS,
            MAGNIFYING_GLASS_PLUS,
            SIDEBAR,
            TRASH,
        ];
        for icon in icons {
            let code = icon as u32;
            assert!(
                (0xE000..=0xF8FF).contains(&code),
                "Icon U+{:04X} is not in the Private Use Area",
                code
            );
        }
    }

    #[test]
    fn all_twelve_icons_are_unique() {
        let icons = [
            ARROW_CLOCKWISE,
            ARROW_COUNTER_CLOCKWISE,
            ARROW_U_UP_LEFT,
            CARET_LEFT,
            CARET_RIGHT,
            FLOPPY_DISK,
            FOLDER_OPEN,
            MAGNIFYING_GLASS,
            MAGNIFYING_GLASS_MINUS,
            MAGNIFYING_GLASS_PLUS,
            SIDEBAR,
            TRASH,
        ];
        let mut seen = std::collections::HashSet::new();
        for icon in icons {
            assert!(seen.insert(icon), "Duplicate icon: U+{:04X}", icon as u32);
        }
        assert_eq!(seen.len(), 12);
    }

    #[test]
    fn icon_font_descriptor_uses_phosphor_family() {
        assert!(matches!(ICON_FONT.family, Family::Name("Phosphor")));
    }
}
