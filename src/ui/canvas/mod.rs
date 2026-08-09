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

pub(crate) use text_metrics::{canvas_line_count, canvas_text_width};

use iced::widget::canvas;

use crate::coordinate::{ConversionParams, pdf_to_screen};
use crate::fonts::FontRegistry;
use crate::overlay::{OverlayBox, PdfPosition, TextOverlay};

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
    /// Undo/redo history of the edits made inside the open edit session, so
    /// an edit in progress can be stepped back through without disturbing the
    /// document history. Empty whenever no session is open.
    pub session_history: crate::command::SessionHistory,
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
            session_history: crate::command::SessionHistory::default(),
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
    /// Which handle was grabbed, and so which dimensions the drag changes.
    pub edge: ResizeEdge,
    pub initial_box: OverlayBox,
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

/// Draw overlay text at a screen position on the canvas frame, wrapping it
/// within `max_width` screen pixels.
///
/// A wrapping overlay must be drawn inside the box it was given: left
/// unbounded, its committed text ran off the right of its own box in one line,
/// agreeing neither with the box the user drew nor with the wrapped lines the
/// PDF writer emits.
pub(crate) fn draw_overlay_text(
    frame: &mut canvas::Frame,
    overlay: &TextOverlay,
    baseline: iced::Point,
    scale: f32,
    registry: &FontRegistry,
) {
    let scaled_font_size = overlay.font_size * scale;
    let text = canvas::Text {
        content: overlay.text.clone(),
        position: iced::Point::new(baseline.x, text_top(baseline.y, scaled_font_size)),
        max_width: overlay_wrap_width(overlay, scale),
        color: iced::Color::BLACK,
        size: iced::Pixels(scaled_font_size),
        font: registry.get(overlay.font).iced_font,
        ..canvas::Text::default()
    };
    frame.fill_text(text);
}

/// How wide `overlay`'s text may run on screen before it wraps. A single-line
/// overlay has no box to stay inside, so it never wraps.
pub(crate) fn overlay_wrap_width(overlay: &TextOverlay, scale: f32) -> f32 {
    overlay
        .width
        .map_or(f32::INFINITY, |width_pts| width_pts * scale)
}

/// Line height iced applies to canvas text, as a multiple of the font size
/// (`canvas::Text` defaults to `LineHeight::Relative(1.2)`). Defined in
/// `overlay` — the shared data model — so `pdf::writer` can match it without
/// depending on this presentation module.
pub(crate) use crate::overlay::TEXT_LINE_HEIGHT_RATIO;

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
///
/// A multi-line overlay's text wraps inside that width, so its lines are
/// counted by the text engine rather than by counting `\n`s: text the user
/// never broke still lays out on several lines, and a box that missed them
/// framed only the first (spe-0hl). The box never shrinks below the overlay's
/// own [`TextOverlay::min_height`], so a box dragged taller than its text keeps
/// the whitespace the user asked for (spe-x9e), and never below its text
/// either, so typing past the bottom grows it (spe-b2h).
///
/// The line count also floors to what [`FontRegistry::word_wrap`] — the same
/// wrap the PDF writer uses — would need at this width (spe-y63). The canvas
/// and the writer wrap with different per-character widths (system-font
/// shaping vs. AFM/embedded-font metrics), so they can break a line in
/// different places; deliberately so, since making the canvas *wrap* by the
/// writer's metrics risks text overflowing its own box on screen (measured
/// up to ~25% wider for a Standard 14 serif face — see the investigation
/// recorded against spe-y63). But whichever wraps to *more* lines is the one
/// the box must fit, or a box sized only for the canvas's shorter wrap would
/// undersell the vertical space the saved artifact actually uses.
pub(crate) fn overlay_text_box(
    overlay: &TextOverlay,
    screen_x: f32,
    screen_y: f32,
    scale: f32,
    registry: &FontRegistry,
) -> iced::Rectangle {
    let scaled_font_size = overlay.font_size * scale;
    let (width, line_count) = match overlay.width {
        Some(width_pts) => {
            let canvas_lines = canvas_line_count(
                &overlay.text,
                overlay.font,
                wrap_ratio(width_pts, overlay.font_size),
                registry,
            );
            let writer_lines = registry
                .word_wrap(&overlay.text, overlay.font, overlay.font_size, width_pts)
                .len();
            (width_pts * scale, canvas_lines.max(writer_lines))
        }
        None => (
            canvas_text_width(&overlay.text, overlay.font, scaled_font_size, registry),
            overlay.text.lines().count().max(1),
        ),
    };
    let text_height = line_count as f32 * scaled_font_size * TEXT_LINE_HEIGHT_RATIO;
    iced::Rectangle {
        x: screen_x,
        y: text_top(screen_y, scaled_font_size),
        width,
        height: text_height.max(overlay.min_height.unwrap_or(0.0) * scale),
    }
}

/// A wrap width expressed in font sizes, which is what decides where lines
/// break. A degenerate font size would make the ratio meaningless, so it
/// reports a box too narrow to hold anything rather than a NaN.
fn wrap_ratio(width_pts: f32, font_size: f32) -> f32 {
    if font_size > 0.0 {
        width_pts / font_size
    } else {
        0.0
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

/// Which handle of an overlay's box a resize drag grabbed, and therefore which
/// dimensions the drag changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    /// The right edge: width only.
    Right,
    /// The bottom edge: height only.
    Bottom,
    /// The bottom-right corner: both at once.
    Corner,
}

impl ResizeEdge {
    /// Whether dragging this handle changes the box's width.
    pub fn resizes_width(self) -> bool {
        matches!(self, Self::Right | Self::Corner)
    }

    /// Whether dragging this handle changes the box's height.
    pub fn resizes_height(self) -> bool {
        matches!(self, Self::Bottom | Self::Corner)
    }

    /// The pointer shape that says which way this handle moves.
    pub fn mouse_interaction(self) -> iced::mouse::Interaction {
        match self {
            Self::Right => iced::mouse::Interaction::ResizingHorizontally,
            Self::Bottom => iced::mouse::Interaction::ResizingVertically,
            Self::Corner => iced::mouse::Interaction::ResizingDiagonallyDown,
        }
    }
}

/// Which resize handle of a multi-line overlay a screen-space click lands on,
/// if any.
///
/// The handles run down the right edge and along the bottom edge of the
/// overlay's text box, meeting at a corner that resizes both. The hit areas are
/// derived from the same box the handles are drawn against, and the corner is
/// tested first so the shared pixels resize diagonally rather than picking
/// whichever edge happened to be checked first.
pub(crate) fn resize_handle_hit(
    screen_x: f32,
    screen_y: f32,
    overlay: &TextOverlay,
    params: &ConversionParams,
    registry: &FontRegistry,
) -> Option<ResizeEdge> {
    overlay.width?;
    let (sx, sy) = pdf_to_screen(overlay.position.x, overlay.position.y, params);
    let text_box = overlay_text_box(overlay, sx, sy, params.scale(), registry);
    let on_right = (screen_x - (text_box.x + text_box.width)).abs() <= RESIZE_HANDLE_HIT_RADIUS;
    let on_bottom = (screen_y - (text_box.y + text_box.height)).abs() <= RESIZE_HANDLE_HIT_RADIUS;
    let within_rows = screen_y >= text_box.y
        && screen_y <= text_box.y + text_box.height + RESIZE_HANDLE_HIT_RADIUS;
    let within_columns = screen_x >= text_box.x
        && screen_x <= text_box.x + text_box.width + RESIZE_HANDLE_HIT_RADIUS;

    match (on_right && within_rows, on_bottom && within_columns) {
        (true, true) => Some(ResizeEdge::Corner),
        (true, false) => Some(ResizeEdge::Right),
        (false, true) => Some(ResizeEdge::Bottom),
        (false, false) => None,
    }
}

/// Smallest box a placement or resize drag can produce, in PDF points. A box
/// dragged to nothing would wrap one word per line and leave a handle too
/// narrow to grab again, so every gesture that sizes a box floors it here.
pub const MIN_BOX_DIMENSION: f32 = 20.0;

/// The box a drag between `from` and `to` draws: floored to
/// [`MIN_BOX_DIMENSION`] on both axes and lying entirely within `bounds`.
///
/// Size wins, position gives. A drag can clear the placement threshold on its
/// vertical extent alone, so the floor belongs on the box rather than on the
/// gesture — but flooring a drag that ended at the page edge widens the box
/// past that edge, which is the one place clamping the cursor cannot help.
/// The floored box is therefore translated back inside rather than trimmed:
/// near an edge the user gets a usable box nudged inward, never a sliver and
/// never a box hanging off the paper. A page always exceeds the minimum, so
/// the floored box always fits.
///
/// Pure rectangle geometry, so any caller can use it — but only in a space
/// where y grows *downward*, as screen space does, since that is the direction
/// a floored box grows. A PDF-space caller measures y from the page top and
/// converts back.
pub fn drag_box_within(
    from: iced::Point,
    to: iced::Point,
    bounds: iced::Rectangle,
) -> iced::Rectangle {
    let from = clamp_to_rect(from, &bounds);
    let to = clamp_to_rect(to, &bounds);
    let width = (to.x - from.x).abs().max(MIN_BOX_DIMENSION);
    let height = (to.y - from.y).abs().max(MIN_BOX_DIMENSION);
    iced::Rectangle {
        x: from
            .x
            .min(to.x)
            .min(bounds.x + bounds.width - width)
            .max(bounds.x),
        y: from
            .y
            .min(to.y)
            .min(bounds.y + bounds.height - height)
            .max(bounds.y),
        width,
        height,
    }
}

/// The box `overlay` would occupy if a resize of `edge` finished with the
/// cursor at the PDF-space point (`pdf_x`, `pdf_y`).
///
/// Shared by the release handler and the live preview so the rectangle drawn
/// during the drag is the rectangle the release commits.
pub(crate) fn resized_box(
    overlay: &TextOverlay,
    edge: ResizeEdge,
    pdf_x: f32,
    pdf_y: f32,
) -> Option<OverlayBox> {
    let current = OverlayBox::of(overlay)?;
    // The box's top edge sits one font size above the first baseline, matching
    // `overlay_text_box`, so a height dragged out here means the same height
    // the box draws.
    let box_top = overlay.position.y + overlay.font_size;
    Some(OverlayBox {
        width: if edge.resizes_width() {
            (pdf_x - overlay.position.x).max(MIN_BOX_DIMENSION)
        } else {
            current.width
        },
        min_height: if edge.resizes_height() {
            (box_top - pdf_y).max(MIN_BOX_DIMENSION)
        } else {
            current.min_height
        },
    })
}

/// Minimum drag distance in pixels to initiate a resize. Clicks below this distance are treated as single-line overlays.
pub(crate) const MIN_DRAG_DISTANCE: f32 = 10.0;

/// The point in `rect` nearest to `point`.
///
/// A drag that leaves the page still ends somewhere, and a box sized from a
/// cursor out in the margin would run off the paper it is written on.
pub(crate) fn clamp_to_rect(point: iced::Point, rect: &iced::Rectangle) -> iced::Point {
    iced::Point::new(
        point.x.clamp(rect.x, rect.x + rect.width),
        point.y.clamp(rect.y, rect.y + rect.height),
    )
}

#[cfg(test)]
mod tests;
