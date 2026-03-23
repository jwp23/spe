// PDF page canvas with click-to-place text handling.

use crate::coordinate::{ConversionParams, overlay_bounding_box, pdf_to_screen};
use crate::overlay::{PdfPosition, TextOverlay};

/// State for the PDF canvas view.
pub struct CanvasState {
    pub zoom: f32,
    pub active_overlay: Option<usize>,
    pub editing: bool,
    pub dragging: Option<DragState>,
}

/// Tracks an in-progress overlay drag operation.
pub struct DragState {
    pub overlay_index: usize,
    pub initial_position: PdfPosition,
    pub grab_offset: (f32, f32),
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            active_overlay: None,
            editing: false,
            dragging: None,
        }
    }
}

/// Test whether a screen-space click hits any overlay on the current page.
/// Returns the index of the topmost (last-placed) overlay hit, or None.
pub fn hit_test(
    screen_x: f32,
    screen_y: f32,
    overlays: &[TextOverlay],
    current_page: u32,
    params: &ConversionParams,
) -> Option<usize> {
    // Test in reverse order so topmost (last-placed) overlay wins
    for (i, overlay) in overlays.iter().enumerate().rev() {
        if overlay.page != current_page {
            continue;
        }
        let (sx, sy) = pdf_to_screen(overlay.position.x, overlay.position.y, params);
        let bbox = overlay_bounding_box(&overlay.text, overlay.font, overlay.font_size);
        let scale = params.zoom * (params.dpi / 72.0);
        let w = bbox.width * scale;
        let h = bbox.height * scale;
        // In screen space, the overlay baseline is at sy.
        // Text extends upward from baseline, so the hit box is [sy - h, sy].
        if screen_x >= sx && screen_x <= sx + w && screen_y >= sy - h && screen_y <= sy {
            return Some(i);
        }
    }
    None
}

/// Compute the effective DPI for the current zoom level.
/// Base rendering DPI is 150; zoom scales from there.
pub fn effective_dpi(zoom: f32) -> f32 {
    150.0 * zoom
}

/// Zoom percentage for display (100% = zoom 1.0).
pub fn zoom_percent(zoom: f32) -> u32 {
    (zoom * 100.0).round() as u32
}

/// Zoom steps: 25%, 50%, 75%, 100%, 125%, 150%, 200%.
const ZOOM_STEPS: [f32; 7] = [0.25, 0.50, 0.75, 1.0, 1.25, 1.50, 2.0];

/// Next zoom level up from current, or current if already at max.
pub fn zoom_in(current: f32) -> f32 {
    for &step in &ZOOM_STEPS {
        if step > current + 0.001 {
            return step;
        }
    }
    *ZOOM_STEPS.last().unwrap()
}

/// Next zoom level down from current, or current if already at min.
pub fn zoom_out(current: f32) -> f32 {
    for &step in ZOOM_STEPS.iter().rev() {
        if step < current - 0.001 {
            return step;
        }
    }
    ZOOM_STEPS[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinate::ConversionParams;
    use crate::overlay::{PdfPosition, Standard14Font, TextOverlay};

    fn default_params() -> ConversionParams {
        ConversionParams {
            zoom: 1.0,
            dpi: 72.0,
            page_height: 792.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }

    fn overlay_at(x: f32, y: f32, text: &str) -> TextOverlay {
        TextOverlay {
            page: 1,
            position: PdfPosition { x, y },
            text: text.to_string(),
            font: Standard14Font::Courier,
            font_size: 12.0,
        }
    }

    #[test]
    fn default_canvas_state() {
        let state = CanvasState::default();
        assert!((state.zoom - 1.0).abs() < f32::EPSILON);
        assert!(state.active_overlay.is_none());
        assert!(!state.editing);
        assert!(state.dragging.is_none());
    }

    #[test]
    fn hit_test_returns_none_for_empty_overlays() {
        let params = default_params();
        assert!(hit_test(100.0, 100.0, &[], 1, &params).is_none());
    }

    #[test]
    fn hit_test_finds_overlay_at_position() {
        let params = default_params();
        // Courier at 12pt: each char is 600/1000 * 12 = 7.2 px wide
        // "Hello" = 5 * 7.2 = 36px wide, 12px tall
        // Overlay at PDF (72, 720) → screen (72, 72) at zoom=1, dpi=72
        // Hit box: x=[72, 108], y=[60, 72]
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let result = hit_test(80.0, 65.0, &overlays, 1, &params);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn hit_test_returns_none_for_miss() {
        let params = default_params();
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        // Click far away from overlay
        let result = hit_test(500.0, 500.0, &overlays, 1, &params);
        assert!(result.is_none());
    }

    #[test]
    fn hit_test_returns_topmost_for_overlapping() {
        let params = default_params();
        let overlays = vec![
            overlay_at(72.0, 720.0, "First"),
            overlay_at(72.0, 720.0, "Second"),
        ];
        // Both at same position, should return index 1 (topmost/last)
        let result = hit_test(80.0, 65.0, &overlays, 1, &params);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn hit_test_ignores_overlays_on_other_pages() {
        let params = default_params();
        let overlays = vec![TextOverlay {
            page: 2,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "On page 2".to_string(),
            font: Standard14Font::Courier,
            font_size: 12.0,
        }];
        let result = hit_test(80.0, 65.0, &overlays, 1, &params);
        assert!(result.is_none());
    }

    #[test]
    fn zoom_in_steps_up() {
        assert!((zoom_in(1.0) - 1.25).abs() < 0.01);
        assert!((zoom_in(0.5) - 0.75).abs() < 0.01);
    }

    #[test]
    fn zoom_out_steps_down() {
        assert!((zoom_out(1.0) - 0.75).abs() < 0.01);
        assert!((zoom_out(0.5) - 0.25).abs() < 0.01);
    }

    #[test]
    fn zoom_in_caps_at_max() {
        assert!((zoom_in(2.0) - 2.0).abs() < 0.01);
    }

    #[test]
    fn zoom_out_caps_at_min() {
        assert!((zoom_out(0.25) - 0.25).abs() < 0.01);
    }

    #[test]
    fn zoom_percent_at_default() {
        assert_eq!(zoom_percent(1.0), 100);
    }

    #[test]
    fn zoom_percent_at_150() {
        assert_eq!(zoom_percent(1.5), 150);
    }

    #[test]
    fn effective_dpi_at_default_zoom() {
        assert!((effective_dpi(1.0) - 150.0).abs() < 0.01);
    }

    #[test]
    fn effective_dpi_at_double_zoom() {
        assert!((effective_dpi(2.0) - 300.0).abs() < 0.01);
    }
}
