// PDF page canvas with click-to-place text handling.

mod layout;
mod overlays;
mod pages;
mod text_metrics;
mod zoom;

pub use layout::*;
pub use overlays::*;
pub use pages::*;
pub use zoom::*;

pub(crate) use text_metrics::canvas_text_width;

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
/// Opacity for the background tint behind committed overlay text. High enough
/// that the tint reads as a highlight against white paper without hovering.
pub(crate) const OVERLAY_TINT_ALPHA: f32 = 0.30;
/// Opacity for the background tint when hovering over an overlay.
pub(crate) const OVERLAY_TINT_HOVER_ALPHA: f32 = 0.45;
/// Opacity for the border drawn around a hovered overlay.
pub(crate) const OVERLAY_TINT_HOVER_BORDER_ALPHA: f32 = 0.7;
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
    /// `Some(undo_stack.len())` recorded just before a freshly placed
    /// overlay's `PlaceOverlay` command was pushed, while that overlay is
    /// still being edited for the first time. Abandoning it truncates the
    /// undo stack back to this length, discarding the placement and any
    /// style commands recorded during the edit. `None` once the edit session
    /// ends for any reason: committed with text, cancelled, or superseded by
    /// another overlay becoming active.
    pub fresh_placement: Option<usize>,
    /// Counter incremented on each zoom change; used to debounce re-renders.
    pub zoom_generation: u64,
    /// The `zoom_generation` the currently cached `page_images` are being
    /// re-rendered for. Bumped when the debounced re-render actually starts
    /// (clears the stale cache), not when zoom changes — so it lags
    /// `zoom_generation` for the whole debounce window. `is_render_idle`
    /// compares the two to tell a fresh-but-not-yet-rendered zoom apart from
    /// a genuinely idle one (spe-d3m).
    pub rendered_generation: u64,
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
            fresh_placement: None,
            zoom_generation: 0,
            rendered_generation: 0,
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

/// Identifies the overlay a drag grabbed, independently of its list index.
///
/// Widget-local drag state lives in the canvas program and survives view
/// rebuilds, so nothing tells it when an IPC delete or an undo reorders or
/// shortens the overlay list mid-drag (spe-01a). The page and position the
/// drag grabbed are what the grab offsets were measured against, so they —
/// not the index — identify the overlay for the rest of the drag.
#[derive(Clone, Copy, PartialEq)]
pub struct OverlayAnchor {
    pub page: u32,
    pub position: PdfPosition,
    /// The overlay's wrap width, so a drag can only ever resolve onto a box of
    /// the same shape. Without it a resize could land on a single-line overlay
    /// and convert it into a wrapped one, or replay an `old_width` that the
    /// overlay it lands on never had.
    pub width: Option<f32>,
}

impl OverlayAnchor {
    pub fn of(overlay: &TextOverlay) -> Self {
        Self {
            page: overlay.page,
            position: overlay.position,
            width: overlay.width,
        }
    }

    /// The current index of an overlay matching this anchor, or `None` if none
    /// does. `last_known` is checked first so the common case — nothing changed
    /// — costs one comparison.
    ///
    /// The guarantee is that the result is an overlay of the same shape sitting
    /// where the drag grabbed, not that it is the same overlay: several
    /// overlays can share a page, position and width, and nothing in the model
    /// tells them apart. Stacked that way they are drawn on top of each other,
    /// so the search runs back to front to return the one on top — the same one
    /// `hit_test` would report, and therefore the one the drag started on.
    pub fn resolve(&self, overlays: &[TextOverlay], last_known: usize) -> Option<usize> {
        if overlays
            .get(last_known)
            .is_some_and(|overlay| Self::of(overlay) == *self)
        {
            return Some(last_known);
        }
        overlays
            .iter()
            .rposition(|overlay| Self::of(overlay) == *self)
    }
}

/// Tracks an in-progress resize drag on a multi-line overlay.
pub struct ResizeDragState {
    pub overlay_index: usize,
    pub anchor: OverlayAnchor,
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
    pub anchor: OverlayAnchor,
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

/// Whether to draw the selection box and resize handle for a given overlay.
/// Returns false while the overlay is being edited, because the floating text
/// widget draws its own border and a second box would render on top of it.
pub(crate) fn should_draw_selection_box(
    editing: bool,
    active_overlay: Option<usize>,
    index: usize,
) -> bool {
    active_overlay == Some(index) && !editing
}

/// Screen-space top edge of text sitting on the baseline `screen_y`.
///
/// Canvas text is anchored by its top edge, so text rendering and the tint
/// geometry behind it must both derive their position from here — computing
/// the anchor separately is how the tint drifted off the text (spe-ner).
pub(crate) fn text_top(screen_y: f32, scaled_font_size: f32) -> f32 {
    screen_y - scaled_font_size
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
        position: iced::Point::new(screen_x, text_top(screen_y, scaled_font_size)),
        color,
        size: iced::Pixels(scaled_font_size),
        font,
        ..canvas::Text::default()
    };
    frame.fill_text(text);
}

/// Line height iced applies to canvas text, as a multiple of the font size
/// (`canvas::Text` defaults to `LineHeight::Relative(1.2)`).
pub(crate) const TEXT_LINE_HEIGHT_RATIO: f32 = 1.2;

/// Line height for the floating edit widget. Deliberately not iced's widget
/// default (`Relative(1.3)`): the canvas and the saved PDF's text leading both
/// lay lines out on `TEXT_LINE_HEIGHT_RATIO`, so leaving the editor on its own
/// default made text jump vertically on entering edit mode (spe-m66).
pub(crate) const TEXT_LINE_HEIGHT: iced::widget::text::LineHeight =
    iced::widget::text::LineHeight::Relative(TEXT_LINE_HEIGHT_RATIO);

/// Opacity of the background tint behind an overlay, deepened while hovered.
pub(crate) fn tint_alpha(hovered: bool) -> f32 {
    if hovered {
        OVERLAY_TINT_HOVER_ALPHA
    } else {
        OVERLAY_TINT_ALPHA
    }
}

/// The screen-space rectangle an overlay's rendered text occupies.
///
/// `draw_overlay_text` anchors the text's top edge one font size above the PDF
/// baseline and lays out lines downward, so the box starts there and extends
/// one line height per line of text. Multi-line overlays are as wide as the box
/// the user dragged; single-line overlays are as wide as the canvas actually
/// shapes their text — the PDF's own width tables describe a different face
/// and leave trailing glyphs outside the box (spe-x2z).
pub(crate) fn overlay_text_box(
    overlay: &TextOverlay,
    screen_x: f32,
    screen_y: f32,
    scale: f32,
    registry: &FontRegistry,
) -> iced::Rectangle {
    let scaled_font_size = overlay.font_size * scale;
    let width = match overlay.width {
        Some(width_pts) => width_pts * scale,
        None => canvas_text_width(&overlay.text, overlay.font, scaled_font_size, registry),
    };
    let line_count = overlay.text.lines().count().max(1) as f32;
    iced::Rectangle {
        x: screen_x,
        y: text_top(screen_y, scaled_font_size),
        width,
        height: line_count * scaled_font_size * TEXT_LINE_HEIGHT_RATIO,
    }
}

/// The selection border rectangle that frames an overlay's text box.
///
/// The border and the tint outline the same content, so the border is the
/// text box grown by the padding rather than geometry computed on its own —
/// computing it separately is how the two drifted apart (spe-x2z).
pub(crate) fn selection_box_rect(text_box: iced::Rectangle) -> iced::Rectangle {
    iced::Rectangle {
        x: text_box.x - SELECTION_BOX_PADDING,
        y: text_box.y - SELECTION_BOX_PADDING,
        width: text_box.width + 2.0 * SELECTION_BOX_PADDING,
        height: text_box.height + 2.0 * SELECTION_BOX_PADDING,
    }
}

/// Whether a PDF-space point falls inside an overlay's rendered text box.
///
/// Evaluates [`overlay_text_box`] in an unscaled frame anchored at the
/// overlay's own baseline (`screen_x = 0`, `screen_y = 0`, `scale = 1`) and
/// moves the probe into that frame instead of moving the box: PDF y grows
/// upward while screen y grows downward, so the probe's vertical offset from
/// the baseline is negated. Because `overlay_text_box` is affine — `scale` a
/// pure multiplier, the anchor a pure translation — testing here and testing
/// the screen box against a screen probe are the same inequality multiplied
/// through by a positive `scale`, half-open bounds included.
///
/// Deriving the hit box from the drawing function rather than restating its
/// geometry is what stops the clickable area drifting away from the tint the
/// user can see.
pub(crate) fn overlay_text_box_contains_pdf(
    overlay: &TextOverlay,
    pdf_x: f32,
    pdf_y: f32,
    registry: &FontRegistry,
) -> bool {
    let text_box = overlay_text_box(overlay, 0.0, 0.0, 1.0, registry);
    let probe = iced::Point::new(pdf_x - overlay.position.x, -(pdf_y - overlay.position.y));
    text_box.contains(probe)
}

/// Half-width of the resize handle hit area in screen pixels.
pub(crate) const RESIZE_HANDLE_HIT_RADIUS: f32 = 4.0;

/// Return true if a screen-space click lands on the resize handle of a multi-line overlay.
///
/// The handle runs down the whole right edge of the overlay's text box, so the
/// hit area is derived from the same box the handle is drawn against.
pub(crate) fn resize_handle_hit(
    screen_x: f32,
    screen_y: f32,
    overlay: &TextOverlay,
    params: &ConversionParams,
    registry: &FontRegistry,
) -> bool {
    let (sx, sy) = pdf_to_screen(overlay.position.x, overlay.position.y, params);
    let text_box = overlay_text_box(overlay, sx, sy, params.scale(), registry);
    let handle_x = text_box.x + text_box.width;
    (screen_x - handle_x).abs() <= RESIZE_HANDLE_HIT_RADIUS
        && screen_y >= text_box.y
        && screen_y <= text_box.y + text_box.height
}

/// Minimum drag distance in pixels to initiate a resize. Clicks below this distance are treated as single-line overlays.
pub(crate) const MIN_DRAG_DISTANCE: f32 = 10.0;

#[cfg(test)]
mod tests;
