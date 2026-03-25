// Text overlay data model: position, text content, font family, font size.

use crate::fonts::FontId;

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
    pub font: FontId,
    pub font_size: f32,
    /// Wrap width in PDF points. `None` = single-line, `Some(w)` = multi-line with wrapping.
    pub width: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::FontRegistry;

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
        let registry = FontRegistry::new();
        let helvetica = registry.default_font();
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: helvetica,
            font_size: 12.0,
            width: None,
        };
        assert_eq!(overlay.page, 1);
        assert_eq!(overlay.position.x, 72.0);
        assert_eq!(overlay.position.y, 720.0);
        assert_eq!(overlay.text, "Hello");
        assert_eq!(overlay.font, helvetica);
        assert_eq!(overlay.font_size, 12.0);
        assert!(overlay.width.is_none());
    }

    #[test]
    fn text_overlay_clone() {
        let registry = FontRegistry::new();
        let courier = registry.find_by_name("Courier").unwrap();
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: courier,
            font_size: 14.0,
            width: None,
        };
        let cloned = overlay.clone();
        assert_eq!(overlay, cloned);
    }

    #[test]
    fn text_overlay_width_none_by_default() {
        let registry = FontRegistry::new();
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
        };
        assert!(overlay.width.is_none());
    }

    #[test]
    fn text_overlay_width_some_for_multiline() {
        let registry = FontRegistry::new();
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: Some(200.0),
        };
        assert!((overlay.width.unwrap() - 200.0).abs() < f32::EPSILON);
    }
}
