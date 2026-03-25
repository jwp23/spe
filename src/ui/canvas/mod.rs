// PDF page canvas with click-to-place text handling.

mod layout;
mod overlays;
mod pages;
mod zoom;

pub use layout::*;
pub use overlays::*;
pub use pages::*;
pub use zoom::*;

use iced::widget::canvas;

use crate::coordinate::{ConversionParams, pdf_to_screen};
use crate::fonts::FontRegistry;
use crate::overlay::{PdfPosition, TextOverlay};

/// Time window for double-click detection (milliseconds).
pub(crate) const DOUBLE_CLICK_TIMEOUT_MS: u128 = 500;
/// Maximum distance for double-click detection (pixels).
pub(crate) const DOUBLE_CLICK_DISTANCE_PX: f32 = 5.0;
/// Blue used for selection boxes, resize handles, and text input borders.
pub const SELECTION_COLOR: iced::Color = iced::Color::from_rgb(0.2, 0.5, 1.0);
/// Opacity for the background tint behind committed overlay text.
pub(crate) const OVERLAY_TINT_ALPHA: f32 = 0.15;
/// Opacity for the background tint when hovering over an overlay.
pub(crate) const OVERLAY_TINT_HOVER_ALPHA: f32 = 0.25;
/// Opacity for the border drawn around a hovered overlay.
pub(crate) const OVERLAY_TINT_HOVER_BORDER_ALPHA: f32 = 0.5;
/// Padding around the selection box border (screen pixels).
pub(crate) const SELECTION_BOX_PADDING: f32 = 2.0;
/// Stroke width for selection-style borders (selection box, placement preview).
pub(crate) const SELECTION_BORDER_WIDTH: f32 = 1.5;
/// Background color for the canvas area behind PDF pages.
const CANVAS_BACKGROUND: iced::Color = iced::Color::from_rgb(0.85, 0.85, 0.85);

/// State for the PDF canvas view (persistent, lives in App).
pub struct CanvasState {
    pub zoom: f32,
    pub active_overlay: Option<usize>,
    pub editing: bool,
    /// The overlay text at the start of an edit session, for undo support.
    pub edit_start_text: Option<String>,
    /// Counter incremented on each zoom change; used to debounce re-renders.
    pub zoom_generation: u64,
    /// Current vertical scroll offset in pixels.
    pub scroll_y: f32,
    /// Visible viewport height in pixels.
    pub viewport_height: f32,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            active_overlay: None,
            editing: false,
            edit_start_text: None,
            zoom_generation: 0,
            scroll_y: 0.0,
            viewport_height: 0.0,
        }
    }
}

/// Widget-local mutable state managed by Iced's canvas infrastructure.
pub struct ProgramState {
    pub cursor_position: Option<iced::Point>,
    pub drag: Option<LocalDragState>,
    pub placement_drag: Option<PlacementDragState>,
    pub resize_drag: Option<ResizeDragState>,
    pub keyboard_modifiers: iced::keyboard::Modifiers,
    /// Tracks the time and position of the last left-click for double-click detection.
    pub last_click: Option<(std::time::Instant, iced::Point)>,
    /// Tracks which overlay the cursor is currently over, if any.
    pub hovered_overlay: Option<usize>,
}

impl Default for ProgramState {
    fn default() -> Self {
        Self {
            cursor_position: None,
            drag: None,
            placement_drag: None,
            resize_drag: None,
            keyboard_modifiers: iced::keyboard::Modifiers::empty(),
            last_click: None,
            hovered_overlay: None,
        }
    }
}

/// Tracks an in-progress resize drag on a multi-line overlay.
pub struct ResizeDragState {
    pub overlay_index: usize,
    pub initial_width: f32,
}

/// Tracks an in-progress placement drag (click-and-drag to create a multi-line overlay).
pub struct PlacementDragState {
    pub start_screen: iced::Point,
    pub page: u32,
    pub page_screen_rect: iced::Rectangle,
}

/// Tracks an in-progress overlay drag within the canvas widget.
pub struct LocalDragState {
    pub overlay_index: usize,
    pub initial_pdf_position: PdfPosition,
    pub grab_offset_x: f32,
    pub grab_offset_y: f32,
}

pub(crate) fn to_screen_rect(
    page_rect: iced::Rectangle,
    bounds: &iced::Rectangle,
) -> iced::Rectangle {
    iced::Rectangle {
        x: page_rect.x + bounds.x,
        y: page_rect.y + bounds.y,
        ..page_rect
    }
}

/// Whether to draw overlay text on the canvas for a given overlay.
/// Returns false when the overlay is being actively edited via the floating widget.
pub(crate) fn should_draw_overlay_text(
    editing: bool,
    active_overlay: Option<usize>,
    index: usize,
) -> bool {
    !(editing && active_overlay == Some(index))
}

/// Draw overlay text at a screen position on the canvas frame.
pub(crate) fn draw_overlay_text(
    frame: &mut canvas::Frame,
    content: &str,
    screen_x: f32,
    screen_y: f32,
    scaled_font_size: f32,
    color: iced::Color,
    font: iced::Font,
) {
    let text = canvas::Text {
        content: content.to_string(),
        position: iced::Point::new(screen_x, screen_y - scaled_font_size),
        color,
        size: iced::Pixels(scaled_font_size),
        font,
        ..canvas::Text::default()
    };
    frame.fill_text(text);
}

/// Compute the screen-space (width, height) of the tint rectangle for an overlay.
/// For multi-line overlays (width=Some), uses the specified width and line count.
/// For single-line overlays, uses the bounding box of the text.
pub(crate) fn tint_size_for_overlay(
    overlay: &TextOverlay,
    scale: f32,
    registry: &FontRegistry,
) -> (f32, f32) {
    if let Some(width_pts) = overlay.width {
        let scaled_width = width_pts * scale;
        let line_count = overlay.text.lines().count().max(1) as f32;
        let scaled_height = overlay.font_size * scale * line_count;
        (scaled_width, scaled_height)
    } else {
        let bbox = registry.overlay_bounding_box(&overlay.text, overlay.font, overlay.font_size);
        (bbox.width * scale, bbox.height * scale)
    }
}

/// Half-width of the resize handle hit area in screen pixels.
pub(crate) const RESIZE_HANDLE_HIT_RADIUS: f32 = 4.0;

/// Return true if a screen-space click lands on the resize handle of a multi-line overlay.
pub(crate) fn resize_handle_hit(
    screen_x: f32,
    screen_y: f32,
    overlay: &TextOverlay,
    width_pts: f32,
    params: &ConversionParams,
) -> bool {
    let (sx, sy) = pdf_to_screen(overlay.position.x, overlay.position.y, params);
    let scale = params.scale();
    let handle_x = sx + width_pts * scale;
    let scaled_size = overlay.font_size * scale;
    // Hit box: x within ±RESIZE_HANDLE_HIT_RADIUS of handle_x, y within [sy - scaled_size, sy]
    (screen_x - handle_x).abs() <= RESIZE_HANDLE_HIT_RADIUS
        && screen_y >= sy - scaled_size
        && screen_y <= sy
}

/// Minimum drag distance in pixels to initiate a resize. Clicks below this distance are treated as single-line overlays.
pub(crate) const MIN_DRAG_DISTANCE: f32 = 10.0;

#[cfg(test)]
mod tests;
