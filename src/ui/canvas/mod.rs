// PDF page canvas with click-to-place text handling.

mod layout;
mod pages;
mod zoom;

pub use layout::*;
pub use pages::*;
pub use zoom::*;

use iced::mouse;
use iced::widget::canvas;
use iced::widget::image::Handle;

use crate::app::Message;
use crate::coordinate::{
    ConversionParams, overlay_bounding_box, pdf_to_screen, render_scale, screen_to_pdf,
};
use crate::overlay::{PdfPosition, Standard14Font, TextOverlay};

/// Time window for double-click detection (milliseconds).
const DOUBLE_CLICK_TIMEOUT_MS: u128 = 500;
/// Maximum distance for double-click detection (pixels).
const DOUBLE_CLICK_DISTANCE_PX: f32 = 5.0;
/// Blue used for selection boxes, resize handles, and text input borders.
pub const SELECTION_COLOR: iced::Color = iced::Color::from_rgb(0.2, 0.5, 1.0);
/// Opacity for the background tint behind committed overlay text.
pub(crate) const OVERLAY_TINT_ALPHA: f32 = 0.15;
/// Opacity for the background tint when hovering over an overlay.
pub(crate) const OVERLAY_TINT_HOVER_ALPHA: f32 = 0.25;
/// Opacity for the border drawn around a hovered overlay.
pub(crate) const OVERLAY_TINT_HOVER_BORDER_ALPHA: f32 = 0.5;
/// Padding around the selection box border (screen pixels).
const SELECTION_BOX_PADDING: f32 = 2.0;
/// Stroke width for selection-style borders drawn via `draw_image` strips.
const SELECTION_BORDER_WIDTH: f32 = 1.5;
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

/// The canvas::Program implementor that borrows App state for rendering and event handling.
pub struct PdfCanvasProgram<'a> {
    pub page_images: &'a std::collections::HashMap<u32, Handle>,
    pub page_layout: PageLayout,
    pub page_dimensions: &'a std::collections::HashMap<u32, (f32, f32)>,
    pub page_count: u32,
    pub scroll_y: f32,
    pub viewport_height: f32,
    pub overlays: &'a [TextOverlay],
    pub zoom: f32,
    pub dpi: f32,
    pub active_overlay: Option<usize>,
    pub editing: bool,
    pub overlay_color: [f32; 4],
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

impl PdfCanvasProgram<'_> {
    /// Build ConversionParams for a specific page using its canvas-space rectangle.
    fn conversion_params_for_page(
        &self,
        page: u32,
        page_rect: &iced::Rectangle,
    ) -> Option<ConversionParams> {
        let (_, h) = self.page_dimensions.get(&page)?;
        Some(ConversionParams {
            zoom: self.zoom,
            dpi: self.dpi,
            page_height: *h,
            offset_x: page_rect.x,
            offset_y: page_rect.y,
        })
    }

    /// Find which page a canvas-relative y-coordinate falls on,
    /// and return (page_number, page_rect_in_canvas).
    fn page_at_canvas_y(&self, canvas_y: f32, canvas_width: f32) -> Option<(u32, iced::Rectangle)> {
        let page = page_at_y(&self.page_layout, canvas_y)?;
        let rect = page_rect_in_canvas(&self.page_layout, page, canvas_width);
        Some((page, rect))
    }
}

impl<'a> canvas::Program<Message> for PdfCanvasProgram<'a> {
    type State = ProgramState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let cursor_pos = cursor.position()?;
                if !bounds.contains(cursor_pos) {
                    return None;
                }

                // Any left click while editing commits the current text first
                if self.editing {
                    return Some(canvas::Action::publish(Message::CommitText).and_capture());
                }

                // Determine which page was clicked (canvas-relative y)
                let canvas_y = cursor_pos.y - bounds.y;
                let canvas_x = cursor_pos.x - bounds.x;

                if let Some((page, page_rect)) = self.page_at_canvas_y(canvas_y, bounds.width) {
                    // Convert cursor to page-local coordinates for hit testing
                    let page_screen_rect = to_screen_rect(page_rect, &bounds);
                    let params = self.conversion_params_for_page(page, &page_screen_rect)?;

                    // Check if we hit the resize handle of the selected multi-line overlay first.
                    if let Some(active_idx) = self.active_overlay
                        && let Some(overlay) = self.overlays.get(active_idx)
                        && overlay.page == page
                        && let Some(width_pts) = overlay.width
                        && resize_handle_hit(
                            cursor_pos.x,
                            cursor_pos.y,
                            overlay,
                            width_pts,
                            &params,
                        )
                    {
                        state.resize_drag = Some(ResizeDragState {
                            overlay_index: active_idx,
                            initial_width: width_pts,
                        });
                        return Some(canvas::Action::capture());
                    }

                    // Check if we hit an existing overlay on this page
                    if let Some(idx) =
                        hit_test(cursor_pos.x, cursor_pos.y, self.overlays, page, &params)
                    {
                        let is_double_click =
                            state.last_click.as_ref().is_some_and(|(time, pos)| {
                                time.elapsed().as_millis() < DOUBLE_CLICK_TIMEOUT_MS
                                    && (pos.x - cursor_pos.x).abs() < DOUBLE_CLICK_DISTANCE_PX
                                    && (pos.y - cursor_pos.y).abs() < DOUBLE_CLICK_DISTANCE_PX
                            });
                        state.last_click = Some((std::time::Instant::now(), cursor_pos));

                        if is_double_click {
                            return Some(
                                canvas::Action::publish(Message::EditOverlay(idx)).and_capture(),
                            );
                        }

                        let (overlay_sx, overlay_sy) = pdf_to_screen(
                            self.overlays[idx].position.x,
                            self.overlays[idx].position.y,
                            &params,
                        );
                        state.drag = Some(LocalDragState {
                            overlay_index: idx,
                            initial_pdf_position: self.overlays[idx].position,
                            grab_offset_x: cursor_pos.x - overlay_sx,
                            grab_offset_y: cursor_pos.y - overlay_sy,
                        });
                        Some(canvas::Action::publish(Message::SelectOverlay(idx)).and_capture())
                    } else if page_rect.contains(iced::Point::new(canvas_x, canvas_y)) {
                        state.last_click = None;
                        state.placement_drag = Some(PlacementDragState {
                            start_screen: cursor_pos,
                            page,
                            page_screen_rect: to_screen_rect(page_rect, &bounds),
                        });
                        Some(canvas::Action::capture())
                    } else {
                        state.last_click = None;
                        Some(canvas::Action::publish(Message::DeselectOverlay).and_capture())
                    }
                } else {
                    // Click in gap or outside pages
                    state.last_click = None;
                    Some(canvas::Action::publish(Message::DeselectOverlay).and_capture())
                }
            }

            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                // Use cursor.position() (scroll-adjusted) instead of the raw event
                // position, so state.cursor_position matches the coordinate space
                // used by cursor.position() in press/release handlers.
                state.cursor_position = cursor.position();

                if state.drag.is_some()
                    || state.placement_drag.is_some()
                    || state.resize_drag.is_some()
                {
                    return Some(canvas::Action::request_redraw());
                }

                // Hover tracking: hit-test the cursor position against overlays.
                let new_hover = cursor.position().and_then(|cursor_pos| {
                    if !bounds.contains(cursor_pos) {
                        return None;
                    }
                    let canvas_y = cursor_pos.y - bounds.y;
                    let (page, page_rect) = self.page_at_canvas_y(canvas_y, bounds.width)?;
                    let page_screen_rect = to_screen_rect(page_rect, &bounds);
                    let params = self.conversion_params_for_page(page, &page_screen_rect)?;
                    hit_test(cursor_pos.x, cursor_pos.y, self.overlays, page, &params)
                });

                if new_hover != state.hovered_overlay {
                    state.hovered_overlay = new_hover;
                    Some(canvas::Action::request_redraw())
                } else {
                    None
                }
            }

            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let cursor_pos = cursor.position()?;

                // Check for placement drag first (click-to-place or drag-to-size)
                if let Some(placement) = state.placement_drag.take() {
                    let dx = cursor_pos.x - placement.start_screen.x;
                    let dy = cursor_pos.y - placement.start_screen.y;
                    let distance = (dx * dx + dy * dy).sqrt();

                    let params = self
                        .conversion_params_for_page(placement.page, &placement.page_screen_rect)?;

                    if distance < MIN_DRAG_DISTANCE {
                        // Short drag / click: single-line overlay
                        let (pdf_x, pdf_y) = screen_to_pdf(
                            placement.start_screen.x,
                            placement.start_screen.y,
                            &params,
                        );
                        return Some(
                            canvas::Action::publish(Message::PlaceOverlay {
                                page: placement.page,
                                position: PdfPosition { x: pdf_x, y: pdf_y },
                                width: None,
                            })
                            .and_capture(),
                        );
                    } else {
                        // Drag: multi-line overlay — width defined by horizontal drag distance
                        let (start_pdf_x, start_pdf_y) = screen_to_pdf(
                            placement.start_screen.x,
                            placement.start_screen.y,
                            &params,
                        );
                        let (end_pdf_x, _) = screen_to_pdf(cursor_pos.x, cursor_pos.y, &params);
                        let width_pts = (end_pdf_x - start_pdf_x).abs();
                        let pdf_x = start_pdf_x.min(end_pdf_x);
                        return Some(
                            canvas::Action::publish(Message::PlaceOverlay {
                                page: placement.page,
                                position: PdfPosition {
                                    x: pdf_x,
                                    y: start_pdf_y,
                                },
                                width: Some(width_pts),
                            })
                            .and_capture(),
                        );
                    }
                }

                // Handle resize drag end before overlay move drag
                if let Some(resize) = state.resize_drag.take() {
                    let overlay = self.overlays.get(resize.overlay_index)?;
                    let page_rect =
                        page_rect_in_canvas(&self.page_layout, overlay.page, bounds.width);
                    let page_screen_rect = to_screen_rect(page_rect, &bounds);
                    let params =
                        self.conversion_params_for_page(overlay.page, &page_screen_rect)?;
                    let (cursor_pdf_x, _) = screen_to_pdf(cursor_pos.x, cursor_pos.y, &params);
                    let new_width = (cursor_pdf_x - overlay.position.x).max(20.0);
                    if (new_width - resize.initial_width).abs() > 0.1 {
                        return Some(
                            canvas::Action::publish(Message::ResizeOverlay {
                                index: resize.overlay_index,
                                old_width: resize.initial_width,
                                new_width,
                            })
                            .and_capture(),
                        );
                    }
                    return Some(canvas::Action::capture());
                }

                let drag = state.drag.take()?;

                // Use the overlay's page for coordinate conversion
                let overlay = self.overlays.get(drag.overlay_index)?;
                let page_rect = page_rect_in_canvas(&self.page_layout, overlay.page, bounds.width);
                let page_screen_rect = to_screen_rect(page_rect, &bounds);
                let params = self.conversion_params_for_page(overlay.page, &page_screen_rect)?;

                let overlay_screen_x = cursor_pos.x - drag.grab_offset_x;
                let overlay_screen_y = cursor_pos.y - drag.grab_offset_y;
                let (new_pdf_x, new_pdf_y) =
                    screen_to_pdf(overlay_screen_x, overlay_screen_y, &params);

                let moved = (new_pdf_x - drag.initial_pdf_position.x).abs() > 0.1
                    || (new_pdf_y - drag.initial_pdf_position.y).abs() > 0.1;

                if moved {
                    Some(
                        canvas::Action::publish(Message::MoveOverlay(
                            drag.overlay_index,
                            PdfPosition {
                                x: new_pdf_x,
                                y: new_pdf_y,
                            },
                        ))
                        .and_capture(),
                    )
                } else {
                    Some(canvas::Action::capture())
                }
            }

            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if state.keyboard_modifiers.command() {
                    let y = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => *y,
                        mouse::ScrollDelta::Pixels { y, .. } => *y,
                    };
                    let msg = if y > 0.0 {
                        Message::ZoomIn
                    } else {
                        Message::ZoomOut
                    };
                    Some(canvas::Action::publish(msg).and_capture())
                } else {
                    None
                }
            }

            canvas::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
                state.keyboard_modifiers = *modifiers;
                None
            }

            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // Gray background
        frame.fill_rectangle(iced::Point::ORIGIN, bounds.size(), CANVAS_BACKGROUND);

        if self.page_layout.page_tops.is_empty() {
            return vec![frame.into_geometry()];
        }

        let scale = render_scale(self.zoom, self.dpi);
        let overlay_color = iced::Color::from_rgba(
            self.overlay_color[0],
            self.overlay_color[1],
            self.overlay_color[2],
            self.overlay_color[3],
        );

        // Determine visible pages
        let (first, last) = visible_pages(&self.page_layout, self.scroll_y, self.viewport_height);

        // Draw each visible page
        for page in first..=last {
            let page_rect = page_rect_in_canvas(&self.page_layout, page, bounds.width);

            // Draw white page background
            frame.fill_rectangle(
                iced::Point::new(page_rect.x, page_rect.y),
                iced::Size::new(page_rect.width, page_rect.height),
                iced::Color::WHITE,
            );

            // Draw page image if cached
            if let Some(handle) = self.page_images.get(&page) {
                frame.draw_image(page_rect, handle);
            }

            // Build conversion params for this page (frame-local coordinates)
            let Some((_, page_h)) = self.page_dimensions.get(&page) else {
                continue;
            };
            let local_params = ConversionParams {
                zoom: self.zoom,
                dpi: self.dpi,
                page_height: *page_h,
                offset_x: page_rect.x,
                offset_y: page_rect.y,
            };

            // Draw overlays on this page
            for (i, overlay) in self.overlays.iter().enumerate() {
                if overlay.page != page {
                    continue;
                }
                let is_dragging = state.drag.as_ref().is_some_and(|d| d.overlay_index == i);
                if is_dragging {
                    continue;
                }

                let (sx, sy) = pdf_to_screen(overlay.position.x, overlay.position.y, &local_params);
                let scaled_size = overlay.font_size * scale;

                if should_draw_overlay_text(self.editing, self.active_overlay, i) {
                    let is_hovered = state.hovered_overlay == Some(i);
                    let tint_alpha = if is_hovered {
                        OVERLAY_TINT_HOVER_ALPHA
                    } else {
                        OVERLAY_TINT_ALPHA
                    };
                    draw_overlay_tint(
                        &mut frame,
                        overlay,
                        sx,
                        sy,
                        scale,
                        overlay_color,
                        tint_alpha,
                    );
                    if is_hovered {
                        draw_overlay_hover_border(
                            &mut frame,
                            overlay,
                            sx,
                            sy,
                            scale,
                            overlay_color,
                        );
                    }
                    draw_overlay_text(
                        &mut frame,
                        &overlay.text,
                        sx,
                        sy,
                        scaled_size,
                        iced::Color::BLACK,
                    );
                }

                if self.active_overlay == Some(i) {
                    draw_selection_box(
                        &mut frame,
                        &overlay.text,
                        overlay.font,
                        overlay.font_size,
                        sx,
                        sy,
                        scale,
                    );
                    if let Some(width_pts) = overlay.width {
                        draw_resize_handle(&mut frame, sx, sy, width_pts, scale, overlay.font_size);
                    }
                }
            }
        }

        // Drag preview
        if let (Some(drag), Some(cursor_pos)) = (&state.drag, state.cursor_position)
            && let Some(overlay) = self.overlays.get(drag.overlay_index)
        {
            let preview_screen_x = cursor_pos.x - drag.grab_offset_x - bounds.x;
            let preview_screen_y = cursor_pos.y - drag.grab_offset_y - bounds.y;
            let scaled_size = overlay.font_size * scale;
            draw_overlay_text(
                &mut frame,
                &overlay.text,
                preview_screen_x,
                preview_screen_y,
                scaled_size,
                iced::Color::BLACK,
            );
        }

        // Placement drag preview (rectangle from start to cursor)
        if let Some(placement) = &state.placement_drag
            && let Some(cursor_pos) = state.cursor_position
        {
            let dx = cursor_pos.x - placement.start_screen.x;
            let dy = cursor_pos.y - placement.start_screen.y;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance < MIN_DRAG_DISTANCE {
                return vec![frame.into_geometry()];
            }

            let start_canvas = iced::Point::new(
                placement.start_screen.x - bounds.x,
                placement.start_screen.y - bounds.y,
            );
            let end_canvas = iced::Point::new(cursor_pos.x - bounds.x, cursor_pos.y - bounds.y);
            let rect_x = start_canvas.x.min(end_canvas.x);
            let rect_y = start_canvas.y.min(end_canvas.y);
            let rect_w = (end_canvas.x - start_canvas.x).abs();
            let rect_h = (end_canvas.y - start_canvas.y).abs();

            draw_image_border(
                &mut frame,
                iced::Rectangle::new(
                    iced::Point::new(rect_x, rect_y),
                    iced::Size::new(rect_w, rect_h),
                ),
                SELECTION_BORDER_WIDTH,
                SELECTION_COLOR,
                1.0,
            );
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.drag.is_some() {
            return mouse::Interaction::Grabbing;
        }

        if state.resize_drag.is_some() {
            return mouse::Interaction::ResizingHorizontally;
        }

        let Some(cursor_pos) = cursor.position() else {
            return mouse::Interaction::default();
        };

        if !bounds.contains(cursor_pos) {
            return mouse::Interaction::default();
        }

        let canvas_y = cursor_pos.y - bounds.y;
        let Some((page, page_rect)) = self.page_at_canvas_y(canvas_y, bounds.width) else {
            return mouse::Interaction::default();
        };

        let page_screen_rect = to_screen_rect(page_rect, &bounds);
        let Some(params) = self.conversion_params_for_page(page, &page_screen_rect) else {
            return mouse::Interaction::default();
        };

        // Show resize cursor when hovering the resize handle of the selected multi-line overlay.
        if let Some(active_idx) = self.active_overlay
            && let Some(overlay) = self.overlays.get(active_idx)
            && overlay.page == page
            && let Some(width_pts) = overlay.width
            && resize_handle_hit(cursor_pos.x, cursor_pos.y, overlay, width_pts, &params)
        {
            return mouse::Interaction::ResizingHorizontally;
        }

        if hit_test(cursor_pos.x, cursor_pos.y, self.overlays, page, &params).is_some() {
            return mouse::Interaction::Pointer;
        }

        let canvas_x = cursor_pos.x - bounds.x;
        if page_rect.contains(iced::Point::new(canvas_x, canvas_y)) {
            return mouse::Interaction::Crosshair;
        }

        mouse::Interaction::default()
    }
}

fn to_screen_rect(page_rect: iced::Rectangle, bounds: &iced::Rectangle) -> iced::Rectangle {
    iced::Rectangle {
        x: page_rect.x + bounds.x,
        y: page_rect.y + bounds.y,
        ..page_rect
    }
}

/// Whether to draw overlay text on the canvas for a given overlay.
/// Returns false when the overlay is being actively edited via the floating widget.
fn should_draw_overlay_text(editing: bool, active_overlay: Option<usize>, index: usize) -> bool {
    !(editing && active_overlay == Some(index))
}

/// Draw overlay text at a screen position on the canvas frame.
fn draw_overlay_text(
    frame: &mut canvas::Frame,
    content: &str,
    screen_x: f32,
    screen_y: f32,
    scaled_font_size: f32,
    color: iced::Color,
) {
    let text = canvas::Text {
        content: content.to_string(),
        position: iced::Point::new(screen_x, screen_y - scaled_font_size),
        color,
        size: iced::Pixels(scaled_font_size),
        font: iced::Font::default(),
        ..canvas::Text::default()
    };
    frame.fill_text(text);
}

/// Compute the screen-space (width, height) of the tint rectangle for an overlay.
/// For multi-line overlays (width=Some), uses the specified width and line count.
/// For single-line overlays, uses the bounding box of the text.
pub(crate) fn tint_size_for_overlay(overlay: &TextOverlay, scale: f32) -> (f32, f32) {
    if let Some(width_pts) = overlay.width {
        let scaled_width = width_pts * scale;
        let line_count = overlay.text.lines().count().max(1) as f32;
        let scaled_height = overlay.font_size * scale * line_count;
        (scaled_width, scaled_height)
    } else {
        let bbox = overlay_bounding_box(&overlay.text, overlay.font, overlay.font_size);
        (bbox.width * scale, bbox.height * scale)
    }
}

/// Create a 1x1 pixel image handle with the given color.
///
/// Iced's wgpu canvas renders fill/stroke primitives before images,
/// so `fill_rectangle` is always hidden behind page images regardless
/// of draw order. Using `draw_image` with a stretched 1x1 pixel image
/// ensures the tint renders on top of page content.
fn color_image(color: iced::Color, alpha: f32) -> Handle {
    let pixels = vec![
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8,
        (alpha * 255.0) as u8,
    ];
    Handle::from_rgba(1, 1, pixels)
}

/// Draw a filled rectangle on top of page images.
///
/// Uses `draw_image` with a stretched 1x1 pixel image instead of
/// `fill_rectangle` because Iced's wgpu canvas always renders images
/// on top of tessellated geometry (spe-4ha).
fn draw_image_rect(
    frame: &mut canvas::Frame,
    rect: iced::Rectangle,
    color: iced::Color,
    alpha: f32,
) {
    let handle = color_image(color, alpha);
    frame.draw_image(rect, &handle);
}

/// Draw a rectangular border on top of page images.
///
/// Uses four thin `draw_image` strips instead of `stroke_rectangle`
/// because Iced's wgpu canvas always renders images on top of
/// tessellated geometry (spe-4ha).
fn draw_image_border(
    frame: &mut canvas::Frame,
    rect: iced::Rectangle,
    border_width: f32,
    color: iced::Color,
    alpha: f32,
) {
    let handle = color_image(color, alpha);
    let bw = border_width;
    let x = rect.x;
    let y = rect.y;
    let w = rect.width;
    let h = rect.height;
    // Top edge
    frame.draw_image(
        iced::Rectangle::new(iced::Point::new(x, y), iced::Size::new(w, bw)),
        &handle,
    );
    // Bottom edge
    frame.draw_image(
        iced::Rectangle::new(iced::Point::new(x, y + h - bw), iced::Size::new(w, bw)),
        &handle,
    );
    // Left edge
    frame.draw_image(
        iced::Rectangle::new(iced::Point::new(x, y), iced::Size::new(bw, h)),
        &handle,
    );
    // Right edge
    frame.draw_image(
        iced::Rectangle::new(iced::Point::new(x + w - bw, y), iced::Size::new(bw, h)),
        &handle,
    );
}

/// Draw a semi-transparent tint rectangle behind overlay text.
fn draw_overlay_tint(
    frame: &mut canvas::Frame,
    overlay: &TextOverlay,
    screen_x: f32,
    screen_y: f32,
    scale: f32,
    tint_color: iced::Color,
    alpha: f32,
) {
    let (w, h) = tint_size_for_overlay(overlay, scale);
    draw_image_rect(
        frame,
        iced::Rectangle::new(
            iced::Point::new(screen_x, screen_y - h),
            iced::Size::new(w, h),
        ),
        tint_color,
        alpha,
    );
}

/// Draw a thin border around a hovered overlay.
fn draw_overlay_hover_border(
    frame: &mut canvas::Frame,
    overlay: &TextOverlay,
    screen_x: f32,
    screen_y: f32,
    scale: f32,
    border_color: iced::Color,
) {
    let (w, h) = tint_size_for_overlay(overlay, scale);
    draw_image_border(
        frame,
        iced::Rectangle::new(
            iced::Point::new(screen_x, screen_y - h),
            iced::Size::new(w, h),
        ),
        1.0,
        border_color,
        OVERLAY_TINT_HOVER_BORDER_ALPHA,
    );
}

/// Draw a selection bounding box around an overlay at a screen position.
fn draw_selection_box(
    frame: &mut canvas::Frame,
    text: &str,
    font: Standard14Font,
    font_size: f32,
    screen_x: f32,
    screen_y: f32,
    scale: f32,
) {
    let bbox = overlay_bounding_box(text, font, font_size);
    let w = bbox.width * scale + 2.0 * SELECTION_BOX_PADDING;
    let h = bbox.height * scale + 2.0 * SELECTION_BOX_PADDING;
    draw_image_border(
        frame,
        iced::Rectangle::new(
            iced::Point::new(
                screen_x - SELECTION_BOX_PADDING,
                screen_y - bbox.height * scale - SELECTION_BOX_PADDING,
            ),
            iced::Size::new(w, h),
        ),
        SELECTION_BORDER_WIDTH,
        SELECTION_COLOR,
        1.0,
    );
}

/// Half-width of the resize handle hit area in screen pixels.
const RESIZE_HANDLE_HIT_RADIUS: f32 = 4.0;

/// Draw a vertical bar resize handle on the right edge of a multi-line overlay.
fn draw_resize_handle(
    frame: &mut canvas::Frame,
    overlay_sx: f32,
    overlay_sy: f32,
    width_pts: f32,
    scale: f32,
    font_size: f32,
) {
    let handle_x = overlay_sx + width_pts * scale;
    let scaled_size = font_size * scale;
    draw_image_rect(
        frame,
        iced::Rectangle::new(
            iced::Point::new(handle_x - 2.0, overlay_sy - scaled_size),
            iced::Size::new(4.0, scaled_size),
        ),
        SELECTION_COLOR,
        1.0,
    );
}

/// Return true if a screen-space click lands on the resize handle of a multi-line overlay.
fn resize_handle_hit(
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
const MIN_DRAG_DISTANCE: f32 = 10.0;

#[cfg(test)]
mod tests;
