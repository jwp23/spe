// Text overlay data model: position, text content, font family, font size.
#![allow(dead_code)]

/// PDF Standard 14 built-in fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Standard14Font {
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    TimesRoman,
    TimesBold,
    TimesItalic,
    TimesBoldItalic,
    Courier,
    CourierBold,
    CourierOblique,
    CourierBoldOblique,
    Symbol,
    ZapfDingbats,
}

impl Standard14Font {
    /// Returns the PDF-internal name for this font.
    pub fn pdf_name(&self) -> &'static str {
        match self {
            Self::Helvetica => "Helvetica",
            Self::HelveticaBold => "Helvetica-Bold",
            Self::HelveticaOblique => "Helvetica-Oblique",
            Self::HelveticaBoldOblique => "Helvetica-BoldOblique",
            Self::TimesRoman => "Times-Roman",
            Self::TimesBold => "Times-Bold",
            Self::TimesItalic => "Times-Italic",
            Self::TimesBoldItalic => "Times-BoldItalic",
            Self::Courier => "Courier",
            Self::CourierBold => "Courier-Bold",
            Self::CourierOblique => "Courier-Oblique",
            Self::CourierBoldOblique => "Courier-BoldOblique",
            Self::Symbol => "Symbol",
            Self::ZapfDingbats => "ZapfDingbats",
        }
    }
}

/// A position on a PDF page in PDF coordinate space (points, origin bottom-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfPosition {
    pub x: f32,
    pub y: f32,
}

/// A text overlay to be placed on a PDF page.
#[derive(Debug, Clone, PartialEq)]
pub struct TextOverlay {
    pub page: u32,
    pub position: PdfPosition,
    pub text: String,
    pub font: Standard14Font,
    pub font_size: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard14font_pdf_names() {
        assert_eq!(Standard14Font::Helvetica.pdf_name(), "Helvetica");
        assert_eq!(Standard14Font::HelveticaBold.pdf_name(), "Helvetica-Bold");
        assert_eq!(
            Standard14Font::HelveticaOblique.pdf_name(),
            "Helvetica-Oblique"
        );
        assert_eq!(
            Standard14Font::HelveticaBoldOblique.pdf_name(),
            "Helvetica-BoldOblique"
        );
        assert_eq!(Standard14Font::TimesRoman.pdf_name(), "Times-Roman");
        assert_eq!(Standard14Font::TimesBold.pdf_name(), "Times-Bold");
        assert_eq!(Standard14Font::TimesItalic.pdf_name(), "Times-Italic");
        assert_eq!(
            Standard14Font::TimesBoldItalic.pdf_name(),
            "Times-BoldItalic"
        );
        assert_eq!(Standard14Font::Courier.pdf_name(), "Courier");
        assert_eq!(Standard14Font::CourierBold.pdf_name(), "Courier-Bold");
        assert_eq!(Standard14Font::CourierOblique.pdf_name(), "Courier-Oblique");
        assert_eq!(
            Standard14Font::CourierBoldOblique.pdf_name(),
            "Courier-BoldOblique"
        );
        assert_eq!(Standard14Font::Symbol.pdf_name(), "Symbol");
        assert_eq!(Standard14Font::ZapfDingbats.pdf_name(), "ZapfDingbats");
    }

    #[test]
    fn standard14font_is_copy() {
        let font = Standard14Font::Helvetica;
        let font2 = font;
        assert_eq!(font, font2);
    }

    #[test]
    fn pdf_position_construction() {
        let pos = PdfPosition { x: 100.0, y: 200.0 };
        assert_eq!(pos.x, 100.0);
        assert_eq!(pos.y, 200.0);
    }

    #[test]
    fn pdf_position_is_copy() {
        let pos = PdfPosition { x: 10.0, y: 20.0 };
        let pos2 = pos;
        assert_eq!(pos, pos2);
    }

    #[test]
    fn text_overlay_construction() {
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: Standard14Font::Helvetica,
            font_size: 12.0,
        };
        assert_eq!(overlay.page, 1);
        assert_eq!(overlay.position.x, 72.0);
        assert_eq!(overlay.position.y, 720.0);
        assert_eq!(overlay.text, "Hello");
        assert_eq!(overlay.font, Standard14Font::Helvetica);
        assert_eq!(overlay.font_size, 12.0);
    }

    #[test]
    fn text_overlay_clone() {
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: Standard14Font::Courier,
            font_size: 14.0,
        };
        let cloned = overlay.clone();
        assert_eq!(overlay, cloned);
    }
}
