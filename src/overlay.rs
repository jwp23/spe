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
    /// Minimum box height in PDF points: the height of the box the user
    /// dragged out or resized to, which is only meaningful alongside `width`.
    ///
    /// It is a *minimum*, not a height: the box always grows to hold its text,
    /// so this only ever adds whitespace below the last line. That makes it an
    /// editing affordance and nothing more — the saved PDF lays wrapped lines
    /// out downward from the first baseline and emits no operators for empty
    /// space, so a box dragged taller writes byte-for-byte the same document.
    pub min_height: Option<f32>,
}

/// The dimensions of a wrapping overlay's box, in PDF points.
///
/// `min_height` is stored flat rather than as an `Option` because a resize
/// always produces a definite height, and zero — "no more room than the text
/// needs" — is exactly what an absent minimum already means.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayBox {
    pub width: f32,
    pub min_height: f32,
}

impl OverlayBox {
    /// The box `overlay` currently occupies, or `None` for a single-line
    /// overlay, which has no box to resize.
    pub fn of(overlay: &TextOverlay) -> Option<Self> {
        Some(Self {
            width: overlay.width?,
            min_height: overlay.min_height.unwrap_or(0.0),
        })
    }

    /// Resize `overlay` to these dimensions.
    pub fn apply_to(&self, overlay: &mut TextOverlay) {
        overlay.width = Some(self.width);
        overlay.min_height = Some(self.min_height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::FontRegistry;

    #[test]
    fn a_single_line_overlay_has_no_box() {
        let registry = FontRegistry::new();
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
            min_height: None,
        };
        assert_eq!(OverlayBox::of(&overlay), None);
    }

    #[test]
    fn an_overlay_box_round_trips_through_an_overlay() {
        let registry = FontRegistry::new();
        let mut overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: Some(200.0),
            min_height: Some(90.0),
        };
        let original = OverlayBox::of(&overlay).unwrap();
        assert_eq!(
            original,
            OverlayBox {
                width: 200.0,
                min_height: 90.0
            }
        );

        OverlayBox {
            width: 300.0,
            min_height: 45.0,
        }
        .apply_to(&mut overlay);
        assert_eq!(overlay.width, Some(300.0));
        assert_eq!(overlay.min_height, Some(45.0));

        original.apply_to(&mut overlay);
        assert_eq!(OverlayBox::of(&overlay), Some(original));
    }

    #[test]
    fn an_absent_minimum_height_reads_as_no_extra_room() {
        let registry = FontRegistry::new();
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: Some(200.0),
            min_height: None,
        };
        assert_eq!(OverlayBox::of(&overlay).unwrap().min_height, 0.0);
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
        let registry = FontRegistry::new();
        let helvetica = registry.default_font();
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: helvetica,
            font_size: 12.0,
            width: None,
            min_height: None,
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
            min_height: None,
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
            min_height: None,
        };
        assert!(overlay.width.is_none());
    }

    #[test]
    fn text_overlay_min_height_defaults_to_none() {
        let registry = FontRegistry::new();
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
            min_height: None,
        };
        assert!(
            overlay.min_height.is_none(),
            "an overlay only has a minimum height once one is dragged out for it"
        );
    }

    #[test]
    fn text_overlay_remembers_a_dragged_minimum_height() {
        let registry = FontRegistry::new();
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: Some(200.0),
            min_height: Some(90.0),
        };
        assert!((overlay.min_height.unwrap() - 90.0).abs() < f32::EPSILON);
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
            min_height: None,
        };
        assert!((overlay.width.unwrap() - 200.0).abs() < f32::EPSILON);
    }
}
