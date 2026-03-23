// PDF page canvas with click-to-place text handling.

use iced::widget::canvas;
use iced::widget::image::Handle;

use crate::app::Message;
use crate::coordinate::{ConversionParams, overlay_bounding_box, pdf_to_screen};
use crate::overlay::{PdfPosition, TextOverlay};

/// State for the PDF canvas view (persistent, lives in App).
pub struct CanvasState {
    pub zoom: f32,
    pub active_overlay: Option<usize>,
    pub editing: bool,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            active_overlay: None,
            editing: false,
        }
    }
}

/// The canvas::Program implementor that borrows App state for rendering and event handling.
pub struct PdfCanvasProgram<'a> {
    pub page_image: Option<&'a Handle>,
    pub page_dimensions: Option<(f32, f32)>,
    pub overlays: &'a [TextOverlay],
    pub current_page: u32,
    pub zoom: f32,
    pub dpi: f32,
    pub active_overlay: Option<usize>,
    pub editing: bool,
    pub overlay_color: [f32; 4],
}

/// Widget-local mutable state managed by Iced's canvas infrastructure.
#[derive(Default)]
pub struct ProgramState {
    pub cursor_position: Option<iced::Point>,
    pub drag: Option<LocalDragState>,
}

/// Tracks an in-progress overlay drag within the canvas widget.
pub struct LocalDragState {
    pub overlay_index: usize,
    pub initial_pdf_position: PdfPosition,
    pub grab_offset_x: f32,
    pub grab_offset_y: f32,
}

impl<'a> canvas::Program<Message> for PdfCanvasProgram<'a> {
    type State = ProgramState;

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let _ = (renderer, bounds);
        vec![]
    }
}

/// Compute the Rectangle where the PDF page image should be drawn, centered within the canvas.
pub fn page_image_bounds(
    page_dims: (f32, f32),
    zoom: f32,
    dpi: f32,
    canvas_bounds: iced::Rectangle,
) -> iced::Rectangle {
    let rendered_width = page_dims.0 * zoom * dpi / 72.0;
    let rendered_height = page_dims.1 * zoom * dpi / 72.0;
    let offset_x = (canvas_bounds.width - rendered_width) / 2.0;
    let offset_y = (canvas_bounds.height - rendered_height) / 2.0;
    iced::Rectangle {
        x: canvas_bounds.x + offset_x,
        y: canvas_bounds.y + offset_y,
        width: rendered_width,
        height: rendered_height,
    }
}

/// Convert a decoded image to an Iced image Handle.
pub fn image_to_handle(img: image::DynamicImage) -> Handle {
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Handle::from_rgba(width, height, rgba.into_raw())
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
    }

    // --- ProgramState tests ---

    #[test]
    fn program_state_default_has_no_cursor_or_drag() {
        let state = ProgramState::default();
        assert!(state.cursor_position.is_none());
        assert!(state.drag.is_none());
    }

    // --- LocalDragState tests ---

    #[test]
    fn local_drag_state_construction() {
        let drag = LocalDragState {
            overlay_index: 3,
            initial_pdf_position: PdfPosition { x: 100.0, y: 500.0 },
            grab_offset_x: 5.0,
            grab_offset_y: 10.0,
        };
        assert_eq!(drag.overlay_index, 3);
        assert!((drag.initial_pdf_position.x - 100.0).abs() < f32::EPSILON);
        assert!((drag.initial_pdf_position.y - 500.0).abs() < f32::EPSILON);
        assert!((drag.grab_offset_x - 5.0).abs() < f32::EPSILON);
        assert!((drag.grab_offset_y - 10.0).abs() < f32::EPSILON);
    }

    // --- PdfCanvasProgram tests ---

    #[test]
    fn pdf_canvas_program_construction_with_no_document() {
        let overlays: Vec<TextOverlay> = vec![];
        let program = PdfCanvasProgram {
            page_image: None,
            page_dimensions: None,
            overlays: &overlays,
            current_page: 0,
            zoom: 1.0,
            dpi: 150.0,
            active_overlay: None,
            editing: false,
            overlay_color: [0.0, 0.0, 1.0, 1.0],
        };
        assert!(program.page_image.is_none());
        assert!(program.page_dimensions.is_none());
        assert_eq!(program.overlays.len(), 0);
    }

    #[test]
    fn pdf_canvas_program_construction_with_document() {
        let handle = iced::widget::image::Handle::from_rgba(1, 1, vec![0u8; 4]);
        let overlays = vec![overlay_at(72.0, 720.0, "Test")];
        let program = PdfCanvasProgram {
            page_image: Some(&handle),
            page_dimensions: Some((612.0, 792.0)),
            overlays: &overlays,
            current_page: 1,
            zoom: 1.5,
            dpi: 150.0,
            active_overlay: Some(0),
            editing: true,
            overlay_color: [0.26, 0.53, 0.96, 1.0],
        };
        assert!(program.page_image.is_some());
        assert_eq!(program.page_dimensions, Some((612.0, 792.0)));
        assert_eq!(program.overlays.len(), 1);
        assert_eq!(program.current_page, 1);
        assert!((program.zoom - 1.5).abs() < f32::EPSILON);
        assert!(program.editing);
    }

    // --- page_image_bounds tests ---

    #[test]
    fn page_image_bounds_centers_within_canvas() {
        // US Letter at zoom=1.0, dpi=72 → 612x792 pixels
        // Canvas is 1000x1000
        let bounds = page_image_bounds(
            (612.0, 792.0),
            1.0,
            72.0,
            iced::Rectangle {
                x: 0.0,
                y: 0.0,
                width: 1000.0,
                height: 1000.0,
            },
        );
        // Image is 612x792, centered in 1000x1000
        assert!((bounds.width - 612.0).abs() < 0.1);
        assert!((bounds.height - 792.0).abs() < 0.1);
        // Centered horizontally: (1000 - 612) / 2 = 194
        assert!((bounds.x - 194.0).abs() < 0.1);
        // Centered vertically: (1000 - 792) / 2 = 104
        assert!((bounds.y - 104.0).abs() < 0.1);
    }

    #[test]
    fn page_image_bounds_scales_with_zoom() {
        // US Letter at zoom=2.0, dpi=72 → 1224x1584 pixels
        let bounds = page_image_bounds(
            (612.0, 792.0),
            2.0,
            72.0,
            iced::Rectangle {
                x: 0.0,
                y: 0.0,
                width: 2000.0,
                height: 2000.0,
            },
        );
        assert!((bounds.width - 1224.0).abs() < 0.1);
        assert!((bounds.height - 1584.0).abs() < 0.1);
    }

    #[test]
    fn page_image_bounds_scales_with_dpi() {
        // US Letter at zoom=1.0, dpi=150 → 612*150/72 = 1275 wide, 792*150/72 = 1650 tall
        let bounds = page_image_bounds(
            (612.0, 792.0),
            1.0,
            150.0,
            iced::Rectangle {
                x: 0.0,
                y: 0.0,
                width: 2000.0,
                height: 2000.0,
            },
        );
        assert!((bounds.width - 1275.0).abs() < 0.1);
        assert!((bounds.height - 1650.0).abs() < 0.1);
    }

    #[test]
    fn page_image_bounds_accounts_for_canvas_offset() {
        // Canvas bounds start at (50, 30)
        let bounds = page_image_bounds(
            (612.0, 792.0),
            1.0,
            72.0,
            iced::Rectangle {
                x: 50.0,
                y: 30.0,
                width: 1000.0,
                height: 1000.0,
            },
        );
        // Image is 612x792, centered in 1000x1000 starting at (50, 30)
        assert!((bounds.x - (50.0 + 194.0)).abs() < 0.1);
        assert!((bounds.y - (30.0 + 104.0)).abs() < 0.1);
    }

    // --- image_to_handle tests ---

    #[test]
    fn image_to_handle_converts_rgba_image() {
        // Create a 2x2 red image
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([255, 0, 0, 255]),
        ));
        let _handle = image_to_handle(img);
        // If we get here without panic, conversion succeeded
    }

    #[test]
    fn image_to_handle_converts_rgb_image() {
        // Create an RGB image (no alpha channel) — should still convert
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            3,
            3,
            image::Rgb([0, 128, 255]),
        ));
        let _handle = image_to_handle(img);
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
