// PDF page canvas with click-to-place text handling.

use iced::mouse;
use iced::widget::canvas;
use iced::widget::image::Handle;

use crate::app::Message;
use crate::coordinate::{ConversionParams, overlay_bounding_box, pdf_to_screen, screen_to_pdf};
use crate::overlay::{PdfPosition, Standard14Font, TextOverlay};

/// Time window for double-click detection (milliseconds).
const DOUBLE_CLICK_TIMEOUT_MS: u128 = 500;
/// Maximum distance for double-click detection (pixels).
const DOUBLE_CLICK_DISTANCE_PX: f32 = 5.0;

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
                    let page_screen_rect = iced::Rectangle {
                        x: page_rect.x + bounds.x,
                        y: page_rect.y + bounds.y,
                        ..page_rect
                    };
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
                            page_screen_rect: iced::Rectangle {
                                x: page_rect.x + bounds.x,
                                y: page_rect.y + bounds.y,
                                ..page_rect
                            },
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
                    let page_screen_rect = iced::Rectangle {
                        x: page_rect.x + bounds.x,
                        y: page_rect.y + bounds.y,
                        ..page_rect
                    };
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
                let page_screen_rect = iced::Rectangle {
                    x: page_rect.x + bounds.x,
                    y: page_rect.y + bounds.y,
                    ..page_rect
                };
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
        frame.fill_rectangle(
            iced::Point::ORIGIN,
            bounds.size(),
            iced::Color::from_rgb(0.85, 0.85, 0.85),
        );

        if self.page_layout.page_tops.is_empty() {
            return vec![frame.into_geometry()];
        }

        let overlay_color = iced::Color::from_rgba(
            self.overlay_color[0],
            self.overlay_color[1],
            self.overlay_color[2],
            self.overlay_color[3],
        );
        let scale = self.zoom * self.dpi / 72.0;

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

                draw_overlay_text(
                    &mut frame,
                    &overlay.text,
                    sx,
                    sy,
                    scaled_size,
                    overlay_color,
                );

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
            let preview_color = iced::Color::from_rgba(
                self.overlay_color[0],
                self.overlay_color[1],
                self.overlay_color[2],
                0.5,
            );
            draw_overlay_text(
                &mut frame,
                &overlay.text,
                preview_screen_x,
                preview_screen_y,
                scaled_size,
                preview_color,
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

            frame.stroke_rectangle(
                iced::Point::new(rect_x, rect_y),
                iced::Size::new(rect_w, rect_h),
                canvas::Stroke::default()
                    .with_color(iced::Color::from_rgb(0.2, 0.5, 1.0))
                    .with_width(1.5),
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

        let page_screen_rect = iced::Rectangle {
            x: page_rect.x + bounds.x,
            y: page_rect.y + bounds.y,
            ..page_rect
        };
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
    let w = bbox.width * scale;
    let h = bbox.height * scale;
    let padding = 2.0;

    frame.stroke_rectangle(
        iced::Point::new(screen_x - padding, screen_y - h - padding),
        iced::Size::new(w + 2.0 * padding, h + 2.0 * padding),
        canvas::Stroke::default()
            .with_color(iced::Color::from_rgb(0.2, 0.5, 1.0))
            .with_width(1.5),
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
    frame.fill_rectangle(
        iced::Point::new(handle_x - 2.0, overlay_sy - scaled_size),
        iced::Size::new(4.0, scaled_size),
        iced::Color::from_rgb(0.2, 0.5, 1.0),
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
    let scale = params.zoom * params.dpi / 72.0;
    let handle_x = sx + width_pts * scale;
    let scaled_size = overlay.font_size * scale;
    // Hit box: x within ±RESIZE_HANDLE_HIT_RADIUS of handle_x, y within [sy - scaled_size, sy]
    (screen_x - handle_x).abs() <= RESIZE_HANDLE_HIT_RADIUS
        && screen_y >= sy - scaled_size
        && screen_y <= sy
}

/// Gap between pages in continuous scrolling mode (pixels).
pub const PAGE_GAP: f32 = 16.0;

/// Minimum drag distance in pixels to initiate a resize. Clicks below this distance are treated as single-line overlays.
const MIN_DRAG_DISTANCE: f32 = 10.0;

/// Layout of all pages stacked vertically for continuous scrolling.
#[derive(Clone)]
pub struct PageLayout {
    /// Y-offset of each page's top edge in canvas space. Index 0 = page 1.
    pub page_tops: Vec<f32>,
    /// Rendered height of each page. Index 0 = page 1.
    pub page_heights: Vec<f32>,
    /// Rendered width of each page. Index 0 = page 1.
    pub page_widths: Vec<f32>,
    /// Total canvas height (all pages + gaps + top/bottom margins).
    pub total_height: f32,
    /// Maximum rendered page width.
    pub max_width: f32,
}

/// Compute the vertical layout of all pages at the current zoom/DPI.
pub fn page_layout(
    page_dimensions: &std::collections::HashMap<u32, (f32, f32)>,
    page_count: u32,
    zoom: f32,
    dpi: f32,
) -> PageLayout {
    let scale = zoom * dpi / 72.0;
    let mut page_tops = Vec::with_capacity(page_count as usize);
    let mut page_heights = Vec::with_capacity(page_count as usize);
    let mut page_widths = Vec::with_capacity(page_count as usize);
    let mut y = PAGE_GAP / 2.0;
    let mut max_width: f32 = 0.0;

    for page in 1..=page_count {
        let (w, h) = page_dimensions
            .get(&page)
            .copied()
            .unwrap_or((612.0, 792.0));
        let rendered_w = w * scale;
        let rendered_h = h * scale;
        page_tops.push(y);
        page_widths.push(rendered_w);
        page_heights.push(rendered_h);
        max_width = max_width.max(rendered_w);
        y += rendered_h + PAGE_GAP;
    }

    let total_height = y - PAGE_GAP / 2.0;

    PageLayout {
        page_tops,
        page_heights,
        page_widths,
        total_height,
        max_width,
    }
}

/// Return the 1-indexed page number at canvas y-coordinate, or None if in a gap or past end.
pub fn page_at_y(layout: &PageLayout, y: f32) -> Option<u32> {
    for (i, &top) in layout.page_tops.iter().enumerate() {
        let bottom = top + layout.page_heights[i];
        if y >= top && y < bottom {
            return Some(i as u32 + 1);
        }
        if y < top {
            return None; // in gap before this page
        }
    }
    None
}

/// Return (first, last) 1-indexed page range whose rendered area overlaps the viewport.
pub fn visible_pages(layout: &PageLayout, scroll_y: f32, viewport_h: f32) -> (u32, u32) {
    let view_top = scroll_y;
    let view_bottom = scroll_y + viewport_h;
    let page_count = layout.page_tops.len() as u32;

    let mut first = page_count;
    let mut last = 1;

    for (i, &top) in layout.page_tops.iter().enumerate() {
        let bottom = top + layout.page_heights[i];
        if bottom > view_top && top < view_bottom {
            let page = i as u32 + 1;
            first = first.min(page);
            last = last.max(page);
        }
    }

    if first > last {
        (1, 1)
    } else {
        (first.max(1), last.min(page_count))
    }
}

/// Return the 1-indexed page that occupies the most vertical space in the viewport.
pub fn dominant_page(layout: &PageLayout, scroll_y: f32, viewport_h: f32) -> u32 {
    let view_top = scroll_y;
    let view_bottom = scroll_y + viewport_h;
    let mut best_page = 1u32;
    let mut best_overlap = 0.0f32;

    for (i, &top) in layout.page_tops.iter().enumerate() {
        let bottom = top + layout.page_heights[i];
        let overlap_top = top.max(view_top);
        let overlap_bottom = bottom.min(view_bottom);
        let overlap = (overlap_bottom - overlap_top).max(0.0);
        if overlap > best_overlap {
            best_overlap = overlap;
            best_page = i as u32 + 1;
        }
    }

    best_page
}

/// Compute the Rectangle for a page in canvas coordinates (for drawing).
pub fn page_rect_in_canvas(layout: &PageLayout, page: u32, canvas_width: f32) -> iced::Rectangle {
    let idx = (page - 1) as usize;
    let w = layout.page_widths[idx];
    let h = layout.page_heights[idx];
    let x = (canvas_width - w) / 2.0;
    let y = layout.page_tops[idx];
    iced::Rectangle {
        x: x.max(0.0),
        y,
        width: w,
        height: h,
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

/// Compute zoom level that makes the rendered page width match the viewport width.
/// Returns a continuous value clamped to the valid zoom range.
pub fn fit_to_width_zoom(page_width_pts: f32, viewport_width: f32) -> f32 {
    if page_width_pts <= 0.0 || viewport_width <= 0.0 {
        return ZOOM_STEPS[0];
    }
    // rendered_width = page_width * zoom² * 150/72
    // Solving for zoom: zoom = sqrt(viewport_width * 72 / (page_width * 150))
    let zoom_sq = viewport_width * 72.0 / (page_width_pts * 150.0);
    let zoom = zoom_sq.sqrt();
    zoom.clamp(ZOOM_STEPS[0], *ZOOM_STEPS.last().unwrap())
}

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
    use iced::event;
    use std::collections::HashMap;

    // --- PageLayout tests ---

    fn uniform_page_dims(count: u32) -> HashMap<u32, (f32, f32)> {
        (1..=count).map(|p| (p, (612.0, 792.0))).collect()
    }

    #[test]
    fn page_layout_single_page() {
        let dims = uniform_page_dims(1);
        let layout = page_layout(&dims, 1, 1.0, 72.0);
        assert_eq!(layout.page_tops.len(), 1);
        assert_eq!(layout.page_heights.len(), 1);
        // At zoom=1, dpi=72: scale=1.0, rendered=612x792
        assert!((layout.page_widths[0] - 612.0).abs() < 0.1);
        assert!((layout.page_heights[0] - 792.0).abs() < 0.1);
        // First page starts at PAGE_GAP/2
        assert!((layout.page_tops[0] - PAGE_GAP / 2.0).abs() < 0.1);
        // Total height = GAP/2 + 792 + GAP/2 = 792 + GAP
        assert!((layout.total_height - (792.0 + PAGE_GAP)).abs() < 0.1);
        assert!((layout.max_width - 612.0).abs() < 0.1);
    }

    #[test]
    fn page_layout_two_uniform_pages() {
        let dims = uniform_page_dims(2);
        let layout = page_layout(&dims, 2, 1.0, 72.0);
        assert_eq!(layout.page_tops.len(), 2);
        // Page 1 starts at GAP/2
        assert!((layout.page_tops[0] - PAGE_GAP / 2.0).abs() < 0.1);
        // Page 2 starts at GAP/2 + 792 + GAP
        let expected_top2 = PAGE_GAP / 2.0 + 792.0 + PAGE_GAP;
        assert!((layout.page_tops[1] - expected_top2).abs() < 0.1);
        // Total = GAP/2 + 792 + GAP + 792 + GAP/2 = 2*792 + 2*GAP
        let expected_total = 2.0 * 792.0 + 2.0 * PAGE_GAP;
        assert!((layout.total_height - expected_total).abs() < 0.1);
    }

    #[test]
    fn page_layout_mixed_page_sizes() {
        let mut dims = HashMap::new();
        dims.insert(1, (612.0, 792.0)); // Letter
        dims.insert(2, (842.0, 595.0)); // A4 landscape
        let layout = page_layout(&dims, 2, 1.0, 72.0);
        assert!((layout.page_widths[0] - 612.0).abs() < 0.1);
        assert!((layout.page_widths[1] - 842.0).abs() < 0.1);
        assert!((layout.page_heights[0] - 792.0).abs() < 0.1);
        assert!((layout.page_heights[1] - 595.0).abs() < 0.1);
        assert!((layout.max_width - 842.0).abs() < 0.1);
    }

    #[test]
    fn page_layout_respects_zoom_and_dpi() {
        let dims = uniform_page_dims(1);
        // zoom=2.0, dpi=150 → scale = 2*150/72 ≈ 4.167
        let layout = page_layout(&dims, 1, 2.0, 150.0);
        let scale = 2.0 * 150.0 / 72.0;
        assert!((layout.page_widths[0] - 612.0 * scale).abs() < 0.1);
        assert!((layout.page_heights[0] - 792.0 * scale).abs() < 0.1);
    }

    // --- page_at_y tests ---

    #[test]
    fn page_at_y_hits_first_page() {
        let dims = uniform_page_dims(3);
        let layout = page_layout(&dims, 3, 1.0, 72.0);
        // Middle of first page
        let y = layout.page_tops[0] + layout.page_heights[0] / 2.0;
        assert_eq!(page_at_y(&layout, y), Some(1));
    }

    #[test]
    fn page_at_y_hits_second_page() {
        let dims = uniform_page_dims(3);
        let layout = page_layout(&dims, 3, 1.0, 72.0);
        let y = layout.page_tops[1] + 10.0;
        assert_eq!(page_at_y(&layout, y), Some(2));
    }

    #[test]
    fn page_at_y_in_gap_returns_none() {
        let dims = uniform_page_dims(2);
        let layout = page_layout(&dims, 2, 1.0, 72.0);
        // Gap is between page_tops[0]+page_heights[0] and page_tops[1]
        let gap_y = layout.page_tops[0] + layout.page_heights[0] + PAGE_GAP / 2.0;
        assert!(page_at_y(&layout, gap_y).is_none());
    }

    #[test]
    fn page_at_y_before_first_page_returns_none() {
        let dims = uniform_page_dims(1);
        let layout = page_layout(&dims, 1, 1.0, 72.0);
        assert!(page_at_y(&layout, 0.0).is_none());
    }

    #[test]
    fn page_at_y_past_last_page_returns_none() {
        let dims = uniform_page_dims(1);
        let layout = page_layout(&dims, 1, 1.0, 72.0);
        assert!(page_at_y(&layout, layout.total_height + 100.0).is_none());
    }

    // --- visible_pages tests ---

    #[test]
    fn visible_pages_all_visible() {
        let dims = uniform_page_dims(3);
        let layout = page_layout(&dims, 3, 1.0, 72.0);
        // Viewport tall enough to see everything
        let (first, last) = visible_pages(&layout, 0.0, layout.total_height + 100.0);
        assert_eq!(first, 1);
        assert_eq!(last, 3);
    }

    #[test]
    fn visible_pages_first_page_only() {
        let dims = uniform_page_dims(3);
        let layout = page_layout(&dims, 3, 1.0, 72.0);
        // Viewport covers only first page
        let (first, last) =
            visible_pages(&layout, 0.0, layout.page_tops[0] + layout.page_heights[0]);
        assert_eq!(first, 1);
        assert_eq!(last, 1);
    }

    #[test]
    fn visible_pages_at_boundary() {
        let dims = uniform_page_dims(3);
        let layout = page_layout(&dims, 3, 1.0, 72.0);
        // Scroll to where page 1 bottom and page 2 top are both visible
        let scroll_y = layout.page_tops[0] + layout.page_heights[0] - 50.0;
        let (first, last) = visible_pages(&layout, scroll_y, 100.0);
        assert_eq!(first, 1);
        assert_eq!(last, 2);
    }

    #[test]
    fn visible_pages_last_page_only() {
        let dims = uniform_page_dims(3);
        let layout = page_layout(&dims, 3, 1.0, 72.0);
        let scroll_y = layout.page_tops[2];
        let (first, last) = visible_pages(&layout, scroll_y, 800.0);
        assert_eq!(first, 3);
        assert_eq!(last, 3);
    }

    // --- dominant_page tests ---

    #[test]
    fn dominant_page_first_page_at_top() {
        let dims = uniform_page_dims(3);
        let layout = page_layout(&dims, 3, 1.0, 72.0);
        assert_eq!(dominant_page(&layout, 0.0, 800.0), 1);
    }

    #[test]
    fn dominant_page_at_boundary_picks_more_visible() {
        let dims = uniform_page_dims(2);
        let layout = page_layout(&dims, 2, 1.0, 72.0);
        // Scroll so page 2 has more visible area than page 1
        let scroll_y = layout.page_tops[1] - 100.0;
        let result = dominant_page(&layout, scroll_y, 800.0);
        assert_eq!(result, 2);
    }

    #[test]
    fn dominant_page_fully_on_page_2() {
        let dims = uniform_page_dims(3);
        let layout = page_layout(&dims, 3, 1.0, 72.0);
        let scroll_y = layout.page_tops[1] + 10.0;
        assert_eq!(dominant_page(&layout, scroll_y, 200.0), 2);
    }

    // --- page_rect_in_canvas tests ---

    #[test]
    fn page_rect_in_canvas_centers_horizontally() {
        let dims = uniform_page_dims(1);
        let layout = page_layout(&dims, 1, 1.0, 72.0);
        let rect = page_rect_in_canvas(&layout, 1, 1000.0);
        // Page is 612px wide in 1000px canvas → x = (1000-612)/2 = 194
        assert!((rect.x - 194.0).abs() < 0.1);
        assert!((rect.y - PAGE_GAP / 2.0).abs() < 0.1);
        assert!((rect.width - 612.0).abs() < 0.1);
        assert!((rect.height - 792.0).abs() < 0.1);
    }

    #[test]
    fn page_rect_in_canvas_second_page_position() {
        let dims = uniform_page_dims(2);
        let layout = page_layout(&dims, 2, 1.0, 72.0);
        let rect = page_rect_in_canvas(&layout, 2, 1000.0);
        let expected_y = PAGE_GAP / 2.0 + 792.0 + PAGE_GAP;
        assert!((rect.y - expected_y).abs() < 0.1);
    }
    use iced::widget::canvas::Program;

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
            width: None,
        }
    }

    /// Canvas bounds used in event handling tests: 1000x1000 starting at origin.
    fn test_canvas_bounds() -> iced::Rectangle {
        iced::Rectangle {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 1000.0,
        }
    }

    /// US Letter page at zoom=1, dpi=72 produces a 612x792 image.
    /// Centered in 1000x1000 canvas: offset_x=194, offset_y=104.
    const TEST_PAGE_DIMS: (f32, f32) = (612.0, 792.0);
    const TEST_ZOOM: f32 = 1.0;
    const TEST_DPI: f32 = 72.0;

    fn test_page_images() -> HashMap<u32, Handle> {
        HashMap::new()
    }

    fn test_page_dimensions() -> HashMap<u32, (f32, f32)> {
        let mut dims = HashMap::new();
        dims.insert(1, TEST_PAGE_DIMS);
        dims
    }

    /// Build a PdfCanvasProgram for event handling tests (single page).
    fn test_program<'a>(
        overlays: &'a [TextOverlay],
        page_images: &'a HashMap<u32, Handle>,
        page_dims: &'a HashMap<u32, (f32, f32)>,
    ) -> PdfCanvasProgram<'a> {
        let layout = page_layout(page_dims, 1, TEST_ZOOM, TEST_DPI);
        PdfCanvasProgram {
            page_images,
            page_layout: layout,
            page_dimensions: page_dims,
            page_count: 1,
            scroll_y: 0.0,
            viewport_height: 1000.0,
            overlays,
            zoom: TEST_ZOOM,
            dpi: TEST_DPI,
            active_overlay: None,
            editing: false,
            overlay_color: [0.0, 0.0, 1.0, 1.0],
        }
    }

    fn left_press_event() -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
    }

    fn left_release_event() -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
    }

    fn cursor_moved_event(x: f32, y: f32) -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::CursorMoved {
            position: iced::Point::new(x, y),
        })
    }

    fn cursor_at(x: f32, y: f32) -> mouse::Cursor {
        mouse::Cursor::Available(iced::Point::new(x, y))
    }

    /// Decompose an update() result into (message, event_status) for assertions.
    fn decompose(action: Option<canvas::Action<Message>>) -> (Option<Message>, event::Status) {
        match action {
            Some(a) => {
                let (msg, _redraw, status) = a.into_inner();
                (msg, status)
            }
            None => (None, event::Status::Ignored),
        }
    }

    #[test]
    fn default_canvas_state() {
        let state = CanvasState::default();
        assert!((state.zoom - 1.0).abs() < f32::EPSILON);
        assert!(state.active_overlay.is_none());
        assert!(!state.editing);
        assert_eq!(state.zoom_generation, 0);
    }

    // --- ProgramState tests ---

    #[test]
    fn program_state_default_has_no_cursor_or_drag() {
        let state = ProgramState::default();
        assert!(state.cursor_position.is_none());
        assert!(state.drag.is_none());
        assert!(state.placement_drag.is_none());
    }

    // --- PlacementDragState tests ---

    #[test]
    fn placement_drag_state_construction() {
        let state = PlacementDragState {
            start_screen: iced::Point::new(100.0, 200.0),
            page: 1,
            page_screen_rect: iced::Rectangle::new(
                iced::Point::ORIGIN,
                iced::Size::new(612.0, 792.0),
            ),
        };
        assert_eq!(state.page, 1);
        assert!((state.start_screen.x - 100.0).abs() < f32::EPSILON);
        assert!((state.start_screen.y - 200.0).abs() < f32::EPSILON);
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
        let empty_imgs: HashMap<u32, Handle> = HashMap::new();
        let empty_dims: HashMap<u32, (f32, f32)> = HashMap::new();
        let layout = page_layout(&empty_dims, 0, 1.0, 150.0);
        let program = PdfCanvasProgram {
            page_images: &empty_imgs,
            page_layout: layout,
            page_dimensions: &empty_dims,
            page_count: 0,
            scroll_y: 0.0,
            viewport_height: 1000.0,
            overlays: &overlays,
            zoom: 1.0,
            dpi: 150.0,
            active_overlay: None,
            editing: false,
            overlay_color: [0.0, 0.0, 1.0, 1.0],
        };
        assert!(program.page_images.is_empty());
        assert!(program.page_dimensions.is_empty());
        assert_eq!(program.overlays.len(), 0);
    }

    #[test]
    fn pdf_canvas_program_construction_with_document() {
        let mut page_images = HashMap::new();
        let handle = iced::widget::image::Handle::from_rgba(1, 1, vec![0u8; 4]);
        page_images.insert(1u32, handle);
        let overlays = vec![overlay_at(72.0, 720.0, "Test")];
        let dims = test_page_dimensions();
        let layout = page_layout(&dims, 1, 1.5, 150.0);
        let program = PdfCanvasProgram {
            page_images: &page_images,
            page_layout: layout,
            page_dimensions: &dims,
            page_count: 1,
            scroll_y: 0.0,
            viewport_height: 1000.0,
            overlays: &overlays,
            zoom: 1.5,
            dpi: 150.0,
            active_overlay: Some(0),
            editing: true,
            overlay_color: [0.26, 0.53, 0.96, 1.0],
        };
        assert_eq!(program.page_images.len(), 1);
        assert!(program.page_dimensions.contains_key(&1));
        assert_eq!(program.overlays.len(), 1);
        assert_eq!(program.page_count, 1);
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
            width: None,
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
    fn zoom_in_from_continuous_value() {
        // From a fit-to-width zoom of 0.885, zoom_in should jump to 1.0
        assert!((zoom_in(0.885) - 1.0).abs() < 0.01);
    }

    #[test]
    fn zoom_out_from_continuous_value() {
        // From a fit-to-width zoom of 0.885, zoom_out should jump to 0.75
        assert!((zoom_out(0.885) - 0.75).abs() < 0.01);
    }

    #[test]
    fn zoom_percent_continuous_value() {
        // 0.885 → 89%
        assert_eq!(zoom_percent(0.885), 89);
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

    // =====================================================================
    // update() tests — event handling
    // =====================================================================

    #[test]
    fn update_ignores_click_when_no_pages() {
        let overlays: Vec<TextOverlay> = vec![];
        let empty_imgs: HashMap<u32, Handle> = HashMap::new();
        let empty_dims: HashMap<u32, (f32, f32)> = HashMap::new();
        let layout = page_layout(&empty_dims, 0, TEST_ZOOM, TEST_DPI);
        let program = PdfCanvasProgram {
            page_images: &empty_imgs,
            page_layout: layout,
            page_dimensions: &empty_dims,
            page_count: 0,
            scroll_y: 0.0,
            viewport_height: 1000.0,
            overlays: &overlays,
            zoom: TEST_ZOOM,
            dpi: TEST_DPI,
            active_overlay: None,
            editing: false,
            overlay_color: [0.0, 0.0, 1.0, 1.0],
        };
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(500.0, 500.0);

        let action = program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        // Click in gap/outside pages → deselect
        assert!(matches!(msg, Some(Message::DeselectOverlay)));
        assert_eq!(status, event::Status::Captured);
    }

    #[test]
    fn update_ignores_click_when_cursor_unavailable() {
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = mouse::Cursor::Unavailable;

        let action = program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert!(msg.is_none());
        assert_eq!(status, event::Status::Ignored);
    }

    #[test]
    fn update_click_on_empty_page_records_placement_drag_on_press() {
        // On mouse-down over a blank page area, placement is deferred to mouse-up.
        // In multi-page mode, page 1 starts at y=PAGE_GAP/2=8, centered at x=194.
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(300.0, 200.0);

        let action = program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        // Capture event but do not emit a message yet
        assert_eq!(status, event::Status::Captured);
        assert!(msg.is_none());
        // Placement drag state must be recorded
        assert!(state.placement_drag.is_some());
        let pd = state.placement_drag.as_ref().unwrap();
        assert_eq!(pd.page, 1);
        assert!((pd.start_screen.x - 300.0).abs() < 0.5);
        assert!((pd.start_screen.y - 200.0).abs() < 0.5);
    }

    #[test]
    fn update_click_on_empty_page_places_single_line_overlay_on_release() {
        // In multi-page mode, page 1 starts at y=PAGE_GAP/2=8, centered at x=194
        // Page rect: (194, 8) to (806, 800)
        // Click at screen (300, 200):
        //   pdf_x = (300 - 194) / 1.0 = 106
        //   pdf_y = 792 - ((200 - 8) / 1.0) = 600
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(300.0, 200.0);

        // Press to start placement drag
        program.update(&mut state, &left_press_event(), bounds, cursor);
        assert!(state.placement_drag.is_some());

        // Release at same spot (distance < 10px) → single-line PlaceOverlay
        let action = program.update(&mut state, &left_release_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(state.placement_drag.is_none());
        match msg {
            Some(Message::PlaceOverlay {
                page,
                position,
                width,
            }) => {
                assert_eq!(page, 1);
                assert!((position.x - 106.0).abs() < 0.5);
                assert!((position.y - 600.0).abs() < 0.5);
                assert!(
                    width.is_none(),
                    "click should produce single-line (no width)"
                );
            }
            other => panic!("Expected PlaceOverlay, got {other:?}"),
        }
    }

    #[test]
    fn update_drag_on_empty_page_places_multi_line_overlay_on_release() {
        // Drag from screen (300, 200) to (450, 200) — 150px horizontal drag
        // At zoom=1, dpi=72 (scale=1.0): 150 screen px = 150 PDF pts
        // Start: pdf_x = 300 - 194 = 106, pdf_y = 792 - (200 - 8) = 600
        // End: pdf_x = 450 - 194 = 256
        // Width = |256 - 106| = 150 pts
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();

        // Press at (300, 200)
        let cursor_start = cursor_at(300.0, 200.0);
        program.update(&mut state, &left_press_event(), bounds, cursor_start);
        assert!(state.placement_drag.is_some());

        // Release at (450, 200) — 150px drag, well over the 10px threshold
        let cursor_end = cursor_at(450.0, 200.0);
        let action = program.update(&mut state, &left_release_event(), bounds, cursor_end);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(state.placement_drag.is_none());
        match msg {
            Some(Message::PlaceOverlay {
                page,
                position,
                width,
            }) => {
                assert_eq!(page, 1);
                assert!((position.x - 106.0).abs() < 1.0);
                assert!((position.y - 600.0).abs() < 1.0);
                let w = width.expect("drag should produce multi-line (width Some)");
                assert!((w - 150.0).abs() < 1.0, "expected width ~150, got {w}");
            }
            other => panic!("Expected PlaceOverlay, got {other:?}"),
        }
    }

    #[test]
    fn update_click_on_overlay_selects_it() {
        // Overlay at PDF (72, 720) → screen (266, 80) in multi-page mode
        // page at y=8, so screen_y = (792-720) + 8 = 80
        // Courier 12pt "Hello": hit box x=[266, 302], y=[68, 80]
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(270.0, 75.0);

        let action = program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(matches!(msg, Some(Message::SelectOverlay(0))));
    }

    #[test]
    fn update_click_on_overlay_starts_drag() {
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(270.0, 75.0);

        program.update(&mut state, &left_press_event(), bounds, cursor);
        assert!(state.drag.is_some());
        let drag = state.drag.as_ref().unwrap();
        assert_eq!(drag.overlay_index, 0);
        assert!((drag.initial_pdf_position.x - 72.0).abs() < 0.01);
        assert!((drag.initial_pdf_position.y - 720.0).abs() < 0.01);
    }

    #[test]
    fn update_click_outside_page_deselects() {
        // Page image bounds: (194, 104) to (806, 896).
        // Click at (50, 50) which is outside the page.
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(50.0, 50.0);

        let action = program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(matches!(msg, Some(Message::DeselectOverlay)));
    }

    #[test]
    fn update_click_while_editing_commits_text_first() {
        // Iced actions carry a single message, so clicking while editing
        // returns CommitText only. The place/select happens on the next click.
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = PdfCanvasProgram {
            editing: true,
            active_overlay: Some(0),
            ..test_program(&overlays, &imgs, &dims)
        };
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(300.0, 200.0); // inside page

        let action = program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(matches!(msg, Some(Message::CommitText)));
    }

    #[test]
    fn update_cursor_move_updates_state() {
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(400.0, 300.0);

        let action = program.update(
            &mut state,
            &cursor_moved_event(400.0, 300.0),
            bounds,
            cursor,
        );
        let (msg, _status) = decompose(action);
        assert!(msg.is_none());
        assert!(state.cursor_position.is_some());
        let pos = state.cursor_position.unwrap();
        assert!((pos.x - 400.0).abs() < 0.01);
        assert!((pos.y - 300.0).abs() < 0.01);
    }

    #[test]
    fn update_mouse_release_without_drag_is_ignored() {
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(300.0, 200.0);

        let action = program.update(&mut state, &left_release_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert!(msg.is_none());
        assert_eq!(status, event::Status::Ignored);
    }

    #[test]
    fn update_drag_and_release_publishes_move() {
        // 1) Click on overlay to start drag
        // 2) Move cursor
        // 3) Release → should publish MoveOverlay

        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();

        // Step 1: click on overlay at screen (270, 75) — overlay at PDF (72,720), page y-offset=8
        let cursor = cursor_at(270.0, 75.0);
        program.update(&mut state, &left_press_event(), bounds, cursor);
        assert!(state.drag.is_some());

        // Step 2: move cursor to (370, 175) — 100px right, 100px down
        let cursor = cursor_at(370.0, 175.0);
        program.update(
            &mut state,
            &cursor_moved_event(370.0, 175.0),
            bounds,
            cursor,
        );

        // Step 3: release at (370, 175)
        let action = program.update(&mut state, &left_release_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(state.drag.is_none());
        match msg {
            Some(Message::MoveOverlay(idx, pos)) => {
                assert_eq!(idx, 0);
                // The overlay moved. The new position should reflect 100px
                // shift in screen space converted to PDF space.
                // At zoom=1, dpi=72: scale=1.0, so 100 screen px = 100 PDF pts
                // Original PDF: (72, 720)
                // Shift: +100 x, -100 y (screen down = PDF y decrease)
                // Expected: (172, 620)
                assert!((pos.x - 172.0).abs() < 1.0);
                assert!((pos.y - 620.0).abs() < 1.0);
            }
            other => panic!("Expected MoveOverlay, got {other:?}"),
        }
    }

    #[test]
    fn update_drag_release_at_same_position_no_move_message() {
        // Click on overlay, don't move, release → no MoveOverlay needed
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();

        let cursor = cursor_at(270.0, 75.0);
        program.update(&mut state, &left_press_event(), bounds, cursor);
        assert!(state.drag.is_some());

        // Release at same position
        let action = program.update(&mut state, &left_release_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert!(state.drag.is_none());
        // No movement → captured but no message
        assert_eq!(status, event::Status::Captured);
        assert!(msg.is_none());
    }

    #[test]
    fn update_ignores_right_click() {
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(300.0, 200.0);

        let event = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
        let action = program.update(&mut state, &event, bounds, cursor);
        let (msg, status) = decompose(action);
        assert!(msg.is_none());
        assert_eq!(status, event::Status::Ignored);
    }

    // =====================================================================
    // mouse_interaction() tests
    // =====================================================================

    #[test]
    fn mouse_interaction_grabbing_during_drag() {
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        state.drag = Some(LocalDragState {
            overlay_index: 0,
            initial_pdf_position: PdfPosition { x: 72.0, y: 720.0 },
            grab_offset_x: 4.0,
            grab_offset_y: 6.0,
        });
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(300.0, 200.0);

        let interaction = program.mouse_interaction(&state, bounds, cursor);
        assert_eq!(interaction, mouse::Interaction::Grabbing);
    }

    #[test]
    fn mouse_interaction_pointer_over_overlay() {
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let state = ProgramState::default();
        let bounds = test_canvas_bounds();
        // Cursor over the overlay's hit box at screen (270, 75) — page y-offset=8
        let cursor = cursor_at(270.0, 75.0);

        let interaction = program.mouse_interaction(&state, bounds, cursor);
        assert_eq!(interaction, mouse::Interaction::Pointer);
    }

    #[test]
    fn mouse_interaction_crosshair_on_page() {
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let state = ProgramState::default();
        let bounds = test_canvas_bounds();
        // Cursor inside page image but not over any overlay
        let cursor = cursor_at(500.0, 500.0);

        let interaction = program.mouse_interaction(&state, bounds, cursor);
        assert_eq!(interaction, mouse::Interaction::Crosshair);
    }

    #[test]
    fn mouse_interaction_default_outside_page() {
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let state = ProgramState::default();
        let bounds = test_canvas_bounds();
        // Cursor outside page image (50, 50) but inside canvas bounds
        let cursor = cursor_at(50.0, 50.0);

        let interaction = program.mouse_interaction(&state, bounds, cursor);
        assert_eq!(interaction, mouse::Interaction::default());
    }

    #[test]
    fn mouse_interaction_default_when_no_page() {
        let overlays: Vec<TextOverlay> = vec![];
        let empty_imgs: HashMap<u32, Handle> = HashMap::new();
        let empty_dims: HashMap<u32, (f32, f32)> = HashMap::new();
        let layout = page_layout(&empty_dims, 0, TEST_ZOOM, TEST_DPI);
        let program = PdfCanvasProgram {
            page_images: &empty_imgs,
            page_layout: layout,
            page_dimensions: &empty_dims,
            page_count: 0,
            scroll_y: 0.0,
            viewport_height: 1000.0,
            overlays: &overlays,
            zoom: TEST_ZOOM,
            dpi: TEST_DPI,
            active_overlay: None,
            editing: false,
            overlay_color: [0.0, 0.0, 1.0, 1.0],
        };
        let state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(500.0, 500.0);

        let interaction = program.mouse_interaction(&state, bounds, cursor);
        assert_eq!(interaction, mouse::Interaction::default());
    }

    // =====================================================================
    // draw() logic tests — verify coordinate consistency
    // =====================================================================

    #[test]
    fn local_page_bounds_offsets_correctly_from_canvas_position() {
        // When canvas bounds start at (50, 30), page_image_bounds includes that offset.
        // The frame-local adjustment should subtract canvas origin.
        let canvas_bounds = iced::Rectangle {
            x: 50.0,
            y: 30.0,
            width: 1000.0,
            height: 1000.0,
        };
        let page_bounds = page_image_bounds(TEST_PAGE_DIMS, TEST_ZOOM, TEST_DPI, canvas_bounds);
        let local_page_bounds = iced::Rectangle {
            x: page_bounds.x - canvas_bounds.x,
            y: page_bounds.y - canvas_bounds.y,
            width: page_bounds.width,
            height: page_bounds.height,
        };
        // The local bounds should be the same as if canvas started at origin
        let origin_bounds = page_image_bounds(
            TEST_PAGE_DIMS,
            TEST_ZOOM,
            TEST_DPI,
            iced::Rectangle {
                x: 0.0,
                y: 0.0,
                width: 1000.0,
                height: 1000.0,
            },
        );
        assert!((local_page_bounds.x - origin_bounds.x).abs() < 0.1);
        assert!((local_page_bounds.y - origin_bounds.y).abs() < 0.1);
        assert!((local_page_bounds.width - origin_bounds.width).abs() < 0.1);
        assert!((local_page_bounds.height - origin_bounds.height).abs() < 0.1);
    }

    #[test]
    fn conversion_params_from_local_bounds_produce_valid_coordinates() {
        // Verify that using local (frame-relative) page bounds in ConversionParams
        // produces screen coordinates within the frame dimensions.
        let canvas_bounds = test_canvas_bounds();
        let page_bounds = page_image_bounds(TEST_PAGE_DIMS, TEST_ZOOM, TEST_DPI, canvas_bounds);
        let local_page_bounds = iced::Rectangle {
            x: page_bounds.x - canvas_bounds.x,
            y: page_bounds.y - canvas_bounds.y,
            width: page_bounds.width,
            height: page_bounds.height,
        };
        let params = ConversionParams {
            zoom: TEST_ZOOM,
            dpi: TEST_DPI,
            page_height: TEST_PAGE_DIMS.1,
            offset_x: local_page_bounds.x,
            offset_y: local_page_bounds.y,
        };
        // A point at the top-left of the PDF page (0, page_height) should map
        // to approximately local_page_bounds origin.
        let (sx, sy) = pdf_to_screen(0.0, TEST_PAGE_DIMS.1, &params);
        assert!(
            (sx - local_page_bounds.x).abs() < 0.1,
            "screen x ({sx}) should be near page left ({})",
            local_page_bounds.x
        );
        assert!(
            (sy - local_page_bounds.y).abs() < 0.1,
            "screen y ({sy}) should be near page top ({})",
            local_page_bounds.y
        );
    }

    // =====================================================================
    // fit_to_width_zoom tests
    // =====================================================================

    #[test]
    fn fit_to_width_zoom_us_letter_1000px() {
        // US Letter (612pt), viewport 1000px
        // zoom = sqrt(1000 * 72 / (612 * 150)) = sqrt(0.784) ≈ 0.885
        let zoom = fit_to_width_zoom(612.0, 1000.0);
        assert!((zoom - 0.885).abs() < 0.01, "zoom was {zoom}");
    }

    #[test]
    fn fit_to_width_zoom_us_letter_1920px() {
        // zoom = sqrt(1920 * 72 / (612 * 150)) ≈ 1.227
        let zoom = fit_to_width_zoom(612.0, 1920.0);
        assert!((zoom - 1.227).abs() < 0.01, "zoom was {zoom}");
    }

    #[test]
    fn fit_to_width_zoom_clamps_to_max() {
        // Very wide viewport should clamp to max zoom (2.0)
        let zoom = fit_to_width_zoom(100.0, 100000.0);
        assert!((zoom - 2.0).abs() < f32::EPSILON, "zoom was {zoom}");
    }

    #[test]
    fn fit_to_width_zoom_clamps_to_min() {
        // Very narrow viewport should clamp to min zoom (0.25)
        let zoom = fit_to_width_zoom(612.0, 1.0);
        assert!((zoom - 0.25).abs() < f32::EPSILON, "zoom was {zoom}");
    }

    #[test]
    fn fit_to_width_zoom_zero_page_width_returns_min() {
        let zoom = fit_to_width_zoom(0.0, 1000.0);
        assert!((zoom - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn fit_to_width_zoom_zero_viewport_returns_min() {
        let zoom = fit_to_width_zoom(612.0, 0.0);
        assert!((zoom - 0.25).abs() < f32::EPSILON);
    }

    // =====================================================================
    // Keyboard modifier tracking tests
    // =====================================================================

    fn modifiers_changed_event(modifiers: iced::keyboard::Modifiers) -> canvas::Event {
        canvas::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers))
    }

    #[test]
    fn modifiers_changed_updates_program_state() {
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(0.0, 0.0);

        assert!(!state.keyboard_modifiers.command());

        let event = modifiers_changed_event(iced::keyboard::Modifiers::COMMAND);
        let action = program.update(&mut state, &event, bounds, cursor);
        let (_msg, status) = decompose(action);
        assert_eq!(status, event::Status::Ignored);
        assert!(state.keyboard_modifiers.command());
    }

    // =====================================================================
    // Scroll wheel tests
    // =====================================================================

    fn scroll_event(delta_y: f32) -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: delta_y },
        })
    }

    #[test]
    fn ctrl_scroll_up_publishes_zoom_in() {
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        state.keyboard_modifiers = iced::keyboard::Modifiers::COMMAND;
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(500.0, 500.0);

        let action = program.update(&mut state, &scroll_event(1.0), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(matches!(msg, Some(Message::ZoomIn)));
    }

    #[test]
    fn ctrl_scroll_down_publishes_zoom_out() {
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        state.keyboard_modifiers = iced::keyboard::Modifiers::COMMAND;
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(500.0, 500.0);

        let action = program.update(&mut state, &scroll_event(-1.0), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(matches!(msg, Some(Message::ZoomOut)));
    }

    #[test]
    fn bare_scroll_is_not_captured() {
        let overlays: Vec<TextOverlay> = vec![];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        // No modifiers set
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(500.0, 500.0);

        let action = program.update(&mut state, &scroll_event(1.0), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Ignored);
        assert!(msg.is_none());
    }

    // =====================================================================
    // Double-click to re-edit tests
    // =====================================================================

    #[test]
    fn program_state_default_has_no_last_click() {
        let state = ProgramState::default();
        assert!(state.last_click.is_none());
    }

    #[test]
    fn single_click_on_overlay_emits_select_not_edit() {
        // First click on overlay — no previous click — should emit SelectOverlay.
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(270.0, 75.0);

        let action = program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(matches!(msg, Some(Message::SelectOverlay(0))));
        // last_click is recorded after a hit
        assert!(state.last_click.is_some());
    }

    #[test]
    fn double_click_on_overlay_emits_edit_overlay() {
        // Two rapid clicks at the same position on an overlay → EditOverlay.
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(270.0, 75.0);

        // First click: sets last_click, emits SelectOverlay
        let action = program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, _) = decompose(action);
        assert!(matches!(msg, Some(Message::SelectOverlay(0))));
        assert!(state.last_click.is_some());

        // Second click immediately: should emit EditOverlay
        let action = program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(matches!(msg, Some(Message::EditOverlay(0))));
    }

    #[test]
    fn double_click_too_far_away_does_not_edit() {
        // Two clicks where second is more than 5px away from first → still SelectOverlay.
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();

        // First click at (270, 75)
        let cursor1 = cursor_at(270.0, 75.0);
        let action = program.update(&mut state, &left_press_event(), bounds, cursor1);
        let (msg, _) = decompose(action);
        assert!(matches!(msg, Some(Message::SelectOverlay(0))));

        // Second click at (280, 75) — 10px away, beyond 5px threshold
        let cursor2 = cursor_at(280.0, 75.0);
        let action = program.update(&mut state, &left_press_event(), bounds, cursor2);
        let (msg, _) = decompose(action);
        assert!(matches!(msg, Some(Message::SelectOverlay(0))));
    }

    #[test]
    fn double_click_records_last_click_after_edit() {
        // After a double-click, last_click is updated with the second click's position.
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(270.0, 75.0);

        program.update(&mut state, &left_press_event(), bounds, cursor);
        program.update(&mut state, &left_press_event(), bounds, cursor);

        // last_click should reflect the position of the second (double) click
        let (_, pos) = state.last_click.as_ref().unwrap();
        assert!((pos.x - 270.0).abs() < 0.5);
        assert!((pos.y - 75.0).abs() < 0.5);
    }

    // =====================================================================
    // Resize handle tests
    // =====================================================================

    /// Build a multi-line overlay (width = Some) for resize handle tests.
    fn multiline_overlay_at(x: f32, y: f32, width: f32, text: &str) -> TextOverlay {
        TextOverlay {
            page: 1,
            position: PdfPosition { x, y },
            text: text.to_string(),
            font: Standard14Font::Courier,
            font_size: 12.0,
            width: Some(width),
        }
    }

    #[test]
    fn resize_drag_state_construction() {
        let state = ResizeDragState {
            overlay_index: 2,
            initial_width: 150.0,
        };
        assert_eq!(state.overlay_index, 2);
        assert!((state.initial_width - 150.0).abs() < f32::EPSILON);
    }

    #[test]
    fn program_state_default_has_no_resize_drag() {
        let state = ProgramState::default();
        assert!(state.resize_drag.is_none());
    }

    // --- Resize handle hit-test helpers ---
    // The handle occupies +-4px on the right edge of a multi-line overlay's width.
    // At zoom=1, dpi=72 (scale=1.0):
    //   overlay at PDF (72, 720), width=150pt → handle_screen_x = 194 + 72 + 150 = 416
    //   (page left x = (1000-612)/2 = 194, page top y = 8)
    //
    // Handle hit area: x in [412, 420], y in [sy-h, sy] (full overlay height vertically)

    #[test]
    fn click_on_resize_handle_starts_resize_drag() {
        // Multi-line overlay at PDF (72, 720), width=150pt.
        // At scale=1: handle_screen_x = 194 + 72 + 150 = 416
        // Overlay screen y = 792-720 + 8 = 80 (baseline). Height = 12*1 = 12.
        // Handle y range: [68, 80].
        // Click at (416, 75): should start resize_drag, not overlay move drag.
        let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = PdfCanvasProgram {
            active_overlay: Some(0),
            ..test_program(&overlays, &imgs, &dims)
        };
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(416.0, 75.0);

        let action = program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        // Should capture event. Resize drag started, no message on press.
        assert!(
            msg.is_none(),
            "expected None message on resize handle press, got {msg:?}"
        );
        assert!(state.resize_drag.is_some(), "resize_drag should be set");
        assert!(state.drag.is_none(), "overlay move drag should NOT be set");
        let rd = state.resize_drag.as_ref().unwrap();
        assert_eq!(rd.overlay_index, 0);
        assert!((rd.initial_width - 150.0).abs() < 0.5);
    }

    #[test]
    fn resize_drag_on_single_line_overlay_does_not_start() {
        // Single-line overlays (width=None) have no resize handle.
        // Click at the same x position should fall through to normal overlay hit test.
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")]; // width=None
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = PdfCanvasProgram {
            active_overlay: Some(0),
            ..test_program(&overlays, &imgs, &dims)
        };
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        // Click at x=416, y=75 — but overlay has no width, so no handle exists there
        let cursor = cursor_at(416.0, 75.0);

        program.update(&mut state, &left_press_event(), bounds, cursor);
        assert!(
            state.resize_drag.is_none(),
            "single-line overlay should have no resize drag"
        );
    }

    #[test]
    fn resize_drag_only_starts_on_selected_overlay() {
        // The resize handle only appears for the active (selected) overlay.
        // If no overlay is selected, clicking the handle position starts placement drag.
        let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        // active_overlay is None — not selected
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(416.0, 75.0);

        program.update(&mut state, &left_press_event(), bounds, cursor);
        assert!(
            state.resize_drag.is_none(),
            "resize drag should not start when overlay not selected"
        );
    }

    #[test]
    fn resize_drag_release_publishes_resize_overlay_message() {
        // Drag from handle at x=416 to x=516 (100px rightward) → new_width = 150 + 100 = 250pt
        let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = PdfCanvasProgram {
            active_overlay: Some(0),
            ..test_program(&overlays, &imgs, &dims)
        };
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();

        // Press on handle
        let cursor_press = cursor_at(416.0, 75.0);
        program.update(&mut state, &left_press_event(), bounds, cursor_press);
        assert!(state.resize_drag.is_some());

        // Release 100px to the right
        let cursor_release = cursor_at(516.0, 75.0);
        let action = program.update(&mut state, &left_release_event(), bounds, cursor_release);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(state.resize_drag.is_none());
        match msg {
            Some(Message::ResizeOverlay {
                index,
                old_width,
                new_width,
            }) => {
                assert_eq!(index, 0);
                assert!(
                    (old_width - 150.0).abs() < 1.0,
                    "old_width should be 150, got {old_width}"
                );
                // new_width: cursor_release pdf_x - overlay.position.x
                // cursor_release.x = 516, page_left = 194, scale=1 → pdf_x = 516-194 = 322
                // overlay.position.x = 72 → new_width = 322 - 72 = 250
                assert!(
                    (new_width - 250.0).abs() < 1.0,
                    "new_width should be ~250, got {new_width}"
                );
            }
            other => panic!("Expected ResizeOverlay message, got {other:?}"),
        }
    }

    #[test]
    fn resize_drag_release_enforces_minimum_width() {
        // Drag leftward past the overlay's left edge. Width clamped to 20pt.
        let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = PdfCanvasProgram {
            active_overlay: Some(0),
            ..test_program(&overlays, &imgs, &dims)
        };
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();

        // Press on handle
        let cursor_press = cursor_at(416.0, 75.0);
        program.update(&mut state, &left_press_event(), bounds, cursor_press);

        // Release far to the left (cursor_pdf_x < overlay_pdf_x + 20)
        // page_left=194, overlay.position.x=72 → at x=194+72+5=271 → pdf_x=77 → width=5 < 20
        let cursor_release = cursor_at(271.0, 75.0);
        let action = program.update(&mut state, &left_release_event(), bounds, cursor_release);
        let (msg, _status) = decompose(action);
        match msg {
            Some(Message::ResizeOverlay { new_width, .. }) => {
                assert!(
                    new_width >= 20.0,
                    "new_width should be at least 20, got {new_width}"
                );
            }
            other => panic!("Expected ResizeOverlay, got {other:?}"),
        }
    }

    #[test]
    fn resize_drag_release_at_same_position_emits_no_message() {
        // If the user presses and releases the handle without moving, no resize needed.
        let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = PdfCanvasProgram {
            active_overlay: Some(0),
            ..test_program(&overlays, &imgs, &dims)
        };
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();

        let cursor = cursor_at(416.0, 75.0);
        program.update(&mut state, &left_press_event(), bounds, cursor);
        assert!(state.resize_drag.is_some());

        let action = program.update(&mut state, &left_release_event(), bounds, cursor);
        let (msg, status) = decompose(action);
        assert_eq!(status, event::Status::Captured);
        assert!(state.resize_drag.is_none());
        assert!(
            msg.is_none(),
            "no change in width → no ResizeOverlay message, got {msg:?}"
        );
    }

    #[test]
    fn cursor_move_requests_redraw_during_resize_drag() {
        let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = PdfCanvasProgram {
            active_overlay: Some(0),
            ..test_program(&overlays, &imgs, &dims)
        };
        let mut state = ProgramState::default();
        state.resize_drag = Some(ResizeDragState {
            overlay_index: 0,
            initial_width: 150.0,
        });
        let bounds = test_canvas_bounds();
        let cursor = cursor_at(450.0, 75.0);

        let action = program.update(&mut state, &cursor_moved_event(450.0, 75.0), bounds, cursor);
        // Should request redraw
        assert!(
            action.is_some(),
            "cursor move during resize drag should return Some(action)"
        );
    }

    #[test]
    fn click_on_empty_page_between_overlay_clicks_prevents_double_click() {
        // Clicking blank page area clears last_click and prevents false-positive double-click.
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let program = test_program(&overlays, &imgs, &dims);
        let mut state = ProgramState::default();
        let bounds = test_canvas_bounds();

        // First: click on overlay to set last_click
        let cursor1 = cursor_at(270.0, 75.0);
        program.update(&mut state, &left_press_event(), bounds, cursor1);
        assert!(state.last_click.is_some());

        // Second: click on blank page area — should start placement drag, not edit
        // 300, 500 is well inside the page but away from the overlay
        let cursor2 = cursor_at(300.0, 500.0);
        let action = program.update(&mut state, &left_press_event(), bounds, cursor2);
        let (msg, _) = decompose(action);
        // Blank area click starts placement drag, no message on press
        assert!(msg.is_none());
        assert!(state.placement_drag.is_some());
    }

    #[test]
    fn drag_after_commit_preserves_overlay() {
        // Regression: overlay disappeared when drag-moving after text commit.
        // Simulates: editing overlay → commit → click overlay → drag → release.
        // The ProgramState persists across program changes (editing → not editing).
        let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
        let imgs = test_page_images();
        let dims = test_page_dimensions();
        let bounds = test_canvas_bounds();
        let mut state = ProgramState::default();

        // Phase 1: User is editing. Move cursor to overlay position to set cursor_position.
        let editing_program = PdfCanvasProgram {
            editing: true,
            active_overlay: Some(0),
            ..test_program(&overlays, &imgs, &dims)
        };
        let cursor = cursor_at(270.0, 75.0);
        editing_program.update(&mut state, &cursor_moved_event(270.0, 75.0), bounds, cursor);
        assert!(
            state.cursor_position.is_some(),
            "cursor_position must be set before commit"
        );

        // Phase 2: User commits text (Enter). Program is recreated with editing=false.
        // Click while editing produces CommitText.
        let action = editing_program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, _) = decompose(action);
        assert!(matches!(msg, Some(Message::CommitText)));

        // Phase 3: After commit, program changes to editing=false.
        // ProgramState MUST persist — cursor_position must survive.
        let committed_program = PdfCanvasProgram {
            editing: false,
            active_overlay: Some(0),
            ..test_program(&overlays, &imgs, &dims)
        };
        assert!(
            state.cursor_position.is_some(),
            "cursor_position must survive program change (commit)"
        );

        // Phase 4: Click on overlay to start drag.
        let action = committed_program.update(&mut state, &left_press_event(), bounds, cursor);
        let (msg, _) = decompose(action);
        assert!(
            matches!(msg, Some(Message::SelectOverlay(0))),
            "click on overlay after commit should select it"
        );
        assert!(state.drag.is_some(), "drag must start on overlay click");

        // Phase 5: Drag preview requires cursor_position to be Some.
        // If cursor_position is None, the overlay would be invisible during drag
        // (skipped from normal rendering AND no preview drawn).
        assert!(
            state.cursor_position.is_some(),
            "cursor_position must be Some during drag for preview to render"
        );

        // Phase 6: Move cursor and release to complete the drag.
        let new_cursor = cursor_at(370.0, 175.0);
        committed_program.update(
            &mut state,
            &cursor_moved_event(370.0, 175.0),
            bounds,
            new_cursor,
        );
        let action =
            committed_program.update(&mut state, &left_release_event(), bounds, new_cursor);
        let (msg, _) = decompose(action);
        match msg {
            Some(Message::MoveOverlay(idx, pos)) => {
                assert_eq!(idx, 0);
                assert!((pos.x - 172.0).abs() < 1.0);
                assert!((pos.y - 620.0).abs() < 1.0);
            }
            other => panic!("Expected MoveOverlay, got {other:?}"),
        }
    }
}
