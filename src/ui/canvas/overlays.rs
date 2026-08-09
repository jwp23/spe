// Overlay rendering program — draws text overlays using native fill/stroke on top of page images.

use iced::mouse;
use iced::widget::canvas;

use crate::app::Message;
use crate::coordinate::{ConversionParams, pdf_to_screen, render_scale, screen_to_pdf};
use crate::fonts::FontRegistry;
use crate::overlay::{OverlayBox, PdfPosition, TextOverlay};

use super::{
    DOUBLE_CLICK_DISTANCE_PX, DOUBLE_CLICK_TIMEOUT_MS, LocalDragState, MIN_DRAG_DISTANCE,
    OVERLAY_TINT_HOVER_BORDER_ALPHA, OverlayAnchor, PageLayout, PlacementDragState, ProgramState,
    ResizeDragState, ResizeEdge, SELECTION_BORDER_WIDTH, SELECTION_COLOR, clamp_to_rect,
    drag_box_within, draw_overlay_text, hit_test, overlay_text_box, page_rect_in_canvas,
    resize_handle_hit, resized_box, selection_box_rect, should_draw_overlay_text,
    should_draw_selection_box, tint_alpha, to_screen_rect, visible_pages,
};

/// Canvas program that renders text overlays using native Iced drawing primitives.
///
/// Intended for use as the top layer in a Stack widget, on top of PdfPagesProgram.
/// Because overlays live in a separate canvas layer, fill_rectangle and stroke_rectangle
/// render on top of page images (no draw_image workaround needed).
pub struct OverlayCanvasProgram<'a> {
    pub page_layout: PageLayout,
    pub page_dimensions: &'a std::collections::HashMap<u32, (f32, f32)>,
    pub scroll_y: f32,
    pub viewport_height: f32,
    pub overlays: &'a [TextOverlay],
    pub zoom: f32,
    pub dpi: f32,
    pub active_overlay: Option<usize>,
    pub editing: bool,
    pub overlay_color: [f32; 4],
    pub font_registry: &'a FontRegistry,
}

impl OverlayCanvasProgram<'_> {
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
        let page = super::page_at_y(&self.page_layout, canvas_y)?;
        let rect = page_rect_in_canvas(&self.page_layout, page, canvas_width);
        Some((page, rect))
    }

    /// Find the page under the cursor, if any.
    ///
    /// Returns None if the cursor is outside the canvas bounds or falls in a
    /// gap between pages. Distinct from [`Self::page_at_cursor`]: a caller
    /// that must tell "no page here" apart from "page here but its
    /// conversion params are unavailable" hit-tests with this first, then
    /// resolves params itself.
    fn page_hit(
        &self,
        cursor_pos: iced::Point,
        bounds: iced::Rectangle,
    ) -> Option<(u32, iced::Rectangle)> {
        if !bounds.contains(cursor_pos) {
            return None;
        }
        let canvas_y = cursor_pos.y - bounds.y;
        self.page_at_canvas_y(canvas_y, bounds.width)
    }

    /// Resolve the page under the cursor and its PDF conversion params.
    ///
    /// Returns None if the cursor is outside the canvas bounds, falls in a
    /// gap between pages, or lands on a page with unknown dimensions.
    fn page_at_cursor(
        &self,
        cursor_pos: iced::Point,
        bounds: iced::Rectangle,
    ) -> Option<(u32, iced::Rectangle, ConversionParams)> {
        let (page, page_rect) = self.page_hit(cursor_pos, bounds)?;
        let page_screen_rect = to_screen_rect(page_rect, &bounds);
        let params = self.conversion_params_for_page(page, &page_screen_rect)?;
        Some((page, page_rect, params))
    }

    /// Handle left mouse button press: resize handle, overlay hit test, placement, or deselect.
    fn handle_left_click(
        &self,
        state: &mut ProgramState,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let cursor_pos = cursor.position()?;
        if !bounds.contains(cursor_pos) {
            return None;
        }

        // Any left click while editing commits the current text first
        if self.editing {
            return Some(canvas::Action::publish(Message::CommitText).and_capture());
        }

        let canvas_y = cursor_pos.y - bounds.y;
        let canvas_x = cursor_pos.x - bounds.x;

        let Some((page, page_rect)) = self.page_hit(cursor_pos, bounds) else {
            // Click in gap or outside pages
            state.last_click = None;
            return Some(canvas::Action::publish(Message::DeselectOverlay).and_capture());
        };

        let page_screen_rect = to_screen_rect(page_rect, &bounds);
        let params = self.conversion_params_for_page(page, &page_screen_rect)?;

        if let Some(action) = self.try_resize_handle(state, cursor_pos, page, &params) {
            return Some(action);
        }

        if let Some(action) = self.try_hit_overlay(state, cursor_pos, page, &params) {
            return Some(action);
        }

        // No overlay hit — start placement drag or deselect
        state.last_click = None;
        if page_rect.contains(iced::Point::new(canvas_x, canvas_y)) {
            state.placement_drag = Some(PlacementDragState {
                start_screen: cursor_pos,
                page,
                page_screen_rect: to_screen_rect(page_rect, &bounds),
            });
            Some(canvas::Action::capture())
        } else {
            Some(canvas::Action::publish(Message::DeselectOverlay).and_capture())
        }
    }

    /// Check if the click hit the resize handle of the selected multi-line overlay.
    fn try_resize_handle(
        &self,
        state: &mut ProgramState,
        cursor_pos: iced::Point,
        page: u32,
        params: &ConversionParams,
    ) -> Option<canvas::Action<Message>> {
        let active_idx = self.active_overlay?;
        let overlay = self.overlays.get(active_idx)?;
        if overlay.page != page {
            return None;
        }
        let edge = resize_handle_hit(
            cursor_pos.x,
            cursor_pos.y,
            overlay,
            params,
            self.font_registry,
        )?;
        state.resize_drag = Some(ResizeDragState {
            overlay_index: active_idx,
            anchor: OverlayAnchor::of(overlay),
            edge,
            initial_box: OverlayBox::of(overlay)?,
        });
        Some(canvas::Action::capture())
    }

    /// Check if the click hit an existing overlay: handle double-click edit or start drag.
    fn try_hit_overlay(
        &self,
        state: &mut ProgramState,
        cursor_pos: iced::Point,
        page: u32,
        params: &ConversionParams,
    ) -> Option<canvas::Action<Message>> {
        let idx = hit_test(
            cursor_pos.x,
            cursor_pos.y,
            self.overlays,
            page,
            params,
            self.font_registry,
        )?;

        let is_double_click = state.last_click.as_ref().is_some_and(|(time, pos)| {
            time.elapsed().as_millis() < DOUBLE_CLICK_TIMEOUT_MS
                && (pos.x - cursor_pos.x).abs() < DOUBLE_CLICK_DISTANCE_PX
                && (pos.y - cursor_pos.y).abs() < DOUBLE_CLICK_DISTANCE_PX
        });
        state.last_click = Some((std::time::Instant::now(), cursor_pos));

        if is_double_click {
            return Some(canvas::Action::publish(Message::EditOverlay(idx)).and_capture());
        }

        let (overlay_sx, overlay_sy) = pdf_to_screen(
            self.overlays[idx].position.x,
            self.overlays[idx].position.y,
            params,
        );
        state.drag = Some(LocalDragState {
            overlay_index: idx,
            anchor: OverlayAnchor::of(&self.overlays[idx]),
            grab_offset_x: cursor_pos.x - overlay_sx,
            grab_offset_y: cursor_pos.y - overlay_sy,
        });
        Some(canvas::Action::publish(Message::SelectOverlay(idx)).and_capture())
    }

    /// Handle cursor movement: update position, redraw during drags, track hover state.
    fn handle_cursor_moved(
        &self,
        state: &mut ProgramState,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        // Use cursor.position() (scroll-adjusted) instead of the raw event
        // position, so state.cursor_position matches the coordinate space
        // used by cursor.position() in press/release handlers.
        state.cursor_position = cursor.position();

        if state.drag.is_some() || state.placement_drag.is_some() || state.resize_drag.is_some() {
            return Some(canvas::Action::request_redraw());
        }

        // Hover tracking: hit-test the cursor position against overlays.
        let new_hover = cursor.position().and_then(|cursor_pos| {
            let (page, _, params) = self.page_at_cursor(cursor_pos, bounds)?;
            hit_test(
                cursor_pos.x,
                cursor_pos.y,
                self.overlays,
                page,
                &params,
                self.font_registry,
            )
        });

        if new_hover != state.hovered_overlay {
            state.hovered_overlay = new_hover;
            Some(canvas::Action::request_redraw())
        } else {
            None
        }
    }

    /// Handle left mouse button release: finalize placement, resize, or drag move.
    fn handle_button_released(
        &self,
        state: &mut ProgramState,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let cursor_pos = cursor.position()?;

        if let Some(action) = self.handle_placement_release(state, cursor_pos) {
            return Some(action);
        }

        if let Some(action) = self.handle_resize_release(state, bounds, cursor_pos) {
            return Some(action);
        }

        self.handle_drag_release(state, bounds, cursor_pos)
    }

    /// Finalize a placement drag: click-to-place (single-line) or drag-to-size (multi-line).
    fn handle_placement_release(
        &self,
        state: &mut ProgramState,
        cursor_pos: iced::Point,
    ) -> Option<canvas::Action<Message>> {
        let placement = state.placement_drag.take()?;
        let dx = cursor_pos.x - placement.start_screen.x;
        let dy = cursor_pos.y - placement.start_screen.y;
        let distance = (dx * dx + dy * dy).sqrt();

        let params =
            self.conversion_params_for_page(placement.page, &placement.page_screen_rect)?;

        if distance < MIN_DRAG_DISTANCE {
            // Short drag / click: single-line overlay
            let (pdf_x, pdf_y) =
                screen_to_pdf(placement.start_screen.x, placement.start_screen.y, &params);
            Some(
                canvas::Action::publish(Message::PlaceOverlay {
                    page: placement.page,
                    position: PdfPosition { x: pdf_x, y: pdf_y },
                })
                .and_capture(),
            )
        } else {
            // Drag: a wrapping overlay filling the rectangle just drawn. The
            // whole rectangle travels, not just its horizontal extent, so the
            // box the editor opens is the box the user saw (spe-x9e).
            let box_screen = drag_box_within(
                placement.start_screen,
                cursor_pos,
                placement.page_screen_rect,
            );
            // The screen box's top-left corner is the box's first line; its
            // size converts straight across, the scale being a pure multiplier.
            let (pdf_x, pdf_y) = screen_to_pdf(box_screen.x, box_screen.y, &params);
            let scale = params.scale();
            Some(
                canvas::Action::publish(Message::PlaceTextBox {
                    page: placement.page,
                    top_left: PdfPosition { x: pdf_x, y: pdf_y },
                    width: box_screen.width / scale,
                    height: box_screen.height / scale,
                })
                .and_capture(),
            )
        }
    }

    /// Finalize a resize drag: compute new width from cursor position.
    fn handle_resize_release(
        &self,
        state: &mut ProgramState,
        bounds: iced::Rectangle,
        cursor_pos: iced::Point,
    ) -> Option<canvas::Action<Message>> {
        let resize = state.resize_drag.take()?;
        let index = resize.anchor.resolve(self.overlays, resize.overlay_index)?;
        let overlay = &self.overlays[index];
        let new_box = self.resize_target(overlay, resize.edge, cursor_pos, bounds)?;
        if new_box != resize.initial_box {
            Some(
                canvas::Action::publish(Message::ResizeOverlay {
                    index,
                    old_box: resize.initial_box,
                    new_box,
                })
                .and_capture(),
            )
        } else {
            Some(canvas::Action::capture())
        }
    }

    /// The box a resize of `edge` would give `overlay` with the cursor at
    /// `cursor_pos`, with the cursor held to the page so a box can never be
    /// dragged off the paper it is written on.
    fn resize_target(
        &self,
        overlay: &TextOverlay,
        edge: ResizeEdge,
        cursor_pos: iced::Point,
        bounds: iced::Rectangle,
    ) -> Option<OverlayBox> {
        let page_rect = page_rect_in_canvas(&self.page_layout, overlay.page, bounds.width);
        let page_screen_rect = to_screen_rect(page_rect, &bounds);
        let params = self.conversion_params_for_page(overlay.page, &page_screen_rect)?;
        let held = clamp_to_rect(cursor_pos, &page_screen_rect);
        let (pdf_x, pdf_y) = screen_to_pdf(held.x, held.y, &params);
        resized_box(overlay, edge, pdf_x, pdf_y)
    }

    /// Finalize a drag-move: compute new PDF position from cursor offset.
    fn handle_drag_release(
        &self,
        state: &mut ProgramState,
        bounds: iced::Rectangle,
        cursor_pos: iced::Point,
    ) -> Option<canvas::Action<Message>> {
        let drag = state.drag.take()?;
        let index = drag.anchor.resolve(self.overlays, drag.overlay_index)?;
        let overlay = &self.overlays[index];
        let page_rect = page_rect_in_canvas(&self.page_layout, overlay.page, bounds.width);
        let page_screen_rect = to_screen_rect(page_rect, &bounds);
        let params = self.conversion_params_for_page(overlay.page, &page_screen_rect)?;

        let overlay_screen_x = cursor_pos.x - drag.grab_offset_x;
        let overlay_screen_y = cursor_pos.y - drag.grab_offset_y;
        let (new_pdf_x, new_pdf_y) = screen_to_pdf(overlay_screen_x, overlay_screen_y, &params);

        let moved = (new_pdf_x - drag.anchor.position.x).abs() > 0.1
            || (new_pdf_y - drag.anchor.position.y).abs() > 0.1;

        if moved {
            Some(
                canvas::Action::publish(Message::MoveOverlay(
                    index,
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

    /// Draw a single overlay: tint, hover border, text, selection box, and
    /// resize handles. `resizing` is the overlay a resize drag has hold of,
    /// whose box is drawn as a live preview instead (see
    /// [`Self::draw_resize_preview`]).
    #[allow(clippy::too_many_arguments)]
    fn draw_single_overlay(
        &self,
        frame: &mut canvas::Frame,
        state: &ProgramState,
        overlay: &TextOverlay,
        index: usize,
        params: &ConversionParams,
        scale: f32,
        overlay_color: iced::Color,
        resizing: Option<usize>,
    ) {
        let (sx, sy) = pdf_to_screen(overlay.position.x, overlay.position.y, params);
        let text_box = overlay_text_box(overlay, sx, sy, scale, self.font_registry);

        if should_draw_overlay_text(self.editing, self.active_overlay, index) {
            let is_hovered = state.hovered_overlay == Some(index);
            draw_overlay_tint(frame, text_box, overlay_color, tint_alpha(is_hovered));
            if is_hovered {
                draw_overlay_hover_border(frame, text_box, overlay_color);
            }
            draw_overlay_text(
                frame,
                overlay,
                iced::Point::new(sx, sy),
                scale,
                self.font_registry,
            );
        }

        if resizing != Some(index)
            && should_draw_selection_box(self.editing, self.active_overlay, index)
        {
            draw_selection_box(frame, text_box);
            if overlay.width.is_some() {
                draw_resize_handles(frame, text_box);
            }
        }
    }

    /// Draw the overlay text at the cursor position during a drag move.
    fn draw_drag_preview(
        &self,
        frame: &mut canvas::Frame,
        state: &ProgramState,
        bounds: iced::Rectangle,
        scale: f32,
    ) {
        if let (Some(drag), Some(cursor_pos)) = (&state.drag, state.cursor_position)
            && let Some(index) = drag.anchor.resolve(self.overlays, drag.overlay_index)
        {
            let overlay = &self.overlays[index];
            let preview_screen_x = cursor_pos.x - drag.grab_offset_x - bounds.x;
            let preview_screen_y = cursor_pos.y - drag.grab_offset_y - bounds.y;
            draw_overlay_text(
                frame,
                overlay,
                iced::Point::new(preview_screen_x, preview_screen_y),
                scale,
                self.font_registry,
            );
        }
    }

    /// Draw the box a resize drag would commit if released now.
    ///
    /// Without it the box only changed on release, so the user resized blind
    /// (spe-qrj). The rectangle comes from the same `resized_box` the release
    /// publishes, so the preview cannot promise a box the release won't give.
    fn draw_resize_preview(
        &self,
        frame: &mut canvas::Frame,
        state: &ProgramState,
        bounds: iced::Rectangle,
        scale: f32,
    ) {
        let (Some(resize), Some(cursor_pos)) = (&state.resize_drag, state.cursor_position) else {
            return;
        };
        let Some(index) = resize.anchor.resolve(self.overlays, resize.overlay_index) else {
            return;
        };
        let overlay = &self.overlays[index];
        let Some(new_box) = self.resize_target(overlay, resize.edge, cursor_pos, bounds) else {
            return;
        };

        let page_rect = page_rect_in_canvas(&self.page_layout, overlay.page, bounds.width);
        let Some(params) = self.conversion_params_for_page(overlay.page, &page_rect) else {
            return;
        };
        let (sx, sy) = pdf_to_screen(overlay.position.x, overlay.position.y, &params);

        // A stand-in carrying the prospective dimensions, so the preview is
        // measured by the same geometry that will draw the committed box.
        let mut preview = overlay.clone();
        new_box.apply_to(&mut preview);
        let text_box = overlay_text_box(&preview, sx, sy, scale, self.font_registry);
        stroke_preview_rect(frame, selection_box_rect(text_box));
    }

    /// Draw the placement drag rectangle preview. Returns true if the frame should be
    /// finalized early (drag distance below threshold).
    fn draw_placement_preview(
        &self,
        frame: &mut canvas::Frame,
        state: &ProgramState,
        bounds: iced::Rectangle,
    ) -> bool {
        let Some(placement) = &state.placement_drag else {
            return false;
        };
        let Some(cursor_pos) = state.cursor_position else {
            return false;
        };

        let dx = cursor_pos.x - placement.start_screen.x;
        let dy = cursor_pos.y - placement.start_screen.y;
        let distance = (dx * dx + dy * dy).sqrt();
        if distance < MIN_DRAG_DISTANCE {
            return true;
        }

        // The very box the release will place, so the rectangle drawn here
        // cannot promise a box the release won't give.
        let box_screen = drag_box_within(
            placement.start_screen,
            cursor_pos,
            placement.page_screen_rect,
        );
        stroke_preview_rect(
            frame,
            iced::Rectangle {
                x: box_screen.x - bounds.x,
                y: box_screen.y - bounds.y,
                ..box_screen
            },
        );
        false
    }

    /// Handle Ctrl+scroll wheel for zoom in/out.
    fn handle_wheel_scrolled(
        &self,
        state: &ProgramState,
        delta: &mouse::ScrollDelta,
    ) -> Option<canvas::Action<Message>> {
        if !state.keyboard_modifiers.command() {
            return None;
        }
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
    }
}

impl<'a> canvas::Program<Message> for OverlayCanvasProgram<'a> {
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
                self.handle_left_click(state, bounds, cursor)
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                self.handle_cursor_moved(state, bounds, cursor)
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                self.handle_button_released(state, bounds, cursor)
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                self.handle_wheel_scrolled(state, delta)
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

        // The overlay being dragged is drawn at the cursor instead of in place.
        // Resolved once here rather than per overlay, which would scan the list
        // for every entry drawn.
        let dragged = state
            .drag
            .as_ref()
            .and_then(|drag| drag.anchor.resolve(self.overlays, drag.overlay_index));
        let resizing = state
            .resize_drag
            .as_ref()
            .and_then(|resize| resize.anchor.resolve(self.overlays, resize.overlay_index));

        // Draw overlays for each visible page
        for page in first..=last {
            let page_rect = page_rect_in_canvas(&self.page_layout, page, bounds.width);

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

            for (i, overlay) in self.overlays.iter().enumerate() {
                if overlay.page != page {
                    continue;
                }
                if dragged == Some(i) {
                    continue;
                }
                self.draw_single_overlay(
                    &mut frame,
                    state,
                    overlay,
                    i,
                    &local_params,
                    scale,
                    overlay_color,
                    resizing,
                );
            }
        }

        self.draw_drag_preview(&mut frame, state, bounds, scale);
        self.draw_resize_preview(&mut frame, state, bounds, scale);
        if self.draw_placement_preview(&mut frame, state, bounds) {
            return vec![frame.into_geometry()];
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

        if let Some(resize) = &state.resize_drag {
            return resize.edge.mouse_interaction();
        }

        let Some(cursor_pos) = cursor.position() else {
            return mouse::Interaction::default();
        };

        let Some((page, page_rect, params)) = self.page_at_cursor(cursor_pos, bounds) else {
            return mouse::Interaction::default();
        };

        // Show the matching resize cursor when hovering a handle of the
        // selected multi-line overlay.
        if let Some(active_idx) = self.active_overlay
            && let Some(overlay) = self.overlays.get(active_idx)
            && overlay.page == page
            && let Some(edge) = resize_handle_hit(
                cursor_pos.x,
                cursor_pos.y,
                overlay,
                &params,
                self.font_registry,
            )
        {
            return edge.mouse_interaction();
        }

        if hit_test(
            cursor_pos.x,
            cursor_pos.y,
            self.overlays,
            page,
            &params,
            self.font_registry,
        )
        .is_some()
        {
            return mouse::Interaction::Pointer;
        }

        let canvas_x = cursor_pos.x - bounds.x;
        let canvas_y = cursor_pos.y - bounds.y;
        if page_rect.contains(iced::Point::new(canvas_x, canvas_y)) {
            return mouse::Interaction::Crosshair;
        }

        mouse::Interaction::default()
    }
}

/// Draw a semi-transparent tint rectangle behind overlay text using native fill_rectangle.
fn draw_overlay_tint(
    frame: &mut canvas::Frame,
    text_box: iced::Rectangle,
    tint_color: iced::Color,
    alpha: f32,
) {
    let fill_color = iced::Color {
        a: alpha,
        ..tint_color
    };
    frame.fill_rectangle(text_box.position(), text_box.size(), fill_color);
}

/// Draw a thin border around a hovered overlay using native stroke_rectangle.
fn draw_overlay_hover_border(
    frame: &mut canvas::Frame,
    text_box: iced::Rectangle,
    border_color: iced::Color,
) {
    let stroke_color = iced::Color {
        a: OVERLAY_TINT_HOVER_BORDER_ALPHA,
        ..border_color
    };
    frame.stroke_rectangle(
        text_box.position(),
        text_box.size(),
        canvas::Stroke::default()
            .with_color(stroke_color)
            .with_width(1.0),
    );
}

/// Outline a prospective rectangle in the selection color — the box a
/// placement or resize drag would commit if released now.
fn stroke_preview_rect(frame: &mut canvas::Frame, rect: iced::Rectangle) {
    frame.stroke_rectangle(
        rect.position(),
        rect.size(),
        canvas::Stroke::default()
            .with_color(SELECTION_COLOR)
            .with_width(SELECTION_BORDER_WIDTH),
    );
}

/// Draw a selection bounding box framing an overlay's text box.
fn draw_selection_box(frame: &mut canvas::Frame, text_box: iced::Rectangle) {
    stroke_preview_rect(frame, selection_box_rect(text_box));
}

/// Thickness of the resize handle bars, in screen pixels.
const RESIZE_HANDLE_THICKNESS: f32 = 4.0;
/// Side length of the square corner handle, in screen pixels.
const RESIZE_CORNER_SIZE: f32 = 8.0;

/// Draw the resize handles of a multi-line overlay: a bar down the right edge
/// for width, a bar along the bottom edge for height, and a square where they
/// meet for both at once.
fn draw_resize_handles(frame: &mut canvas::Frame, text_box: iced::Rectangle) {
    let half = RESIZE_HANDLE_THICKNESS / 2.0;
    let right = text_box.x + text_box.width;
    let bottom = text_box.y + text_box.height;

    frame.fill_rectangle(
        iced::Point::new(right - half, text_box.y),
        iced::Size::new(RESIZE_HANDLE_THICKNESS, text_box.height),
        SELECTION_COLOR,
    );
    frame.fill_rectangle(
        iced::Point::new(text_box.x, bottom - half),
        iced::Size::new(text_box.width, RESIZE_HANDLE_THICKNESS),
        SELECTION_COLOR,
    );
    frame.fill_rectangle(
        iced::Point::new(
            right - RESIZE_CORNER_SIZE / 2.0,
            bottom - RESIZE_CORNER_SIZE / 2.0,
        ),
        iced::Size::new(RESIZE_CORNER_SIZE, RESIZE_CORNER_SIZE),
        SELECTION_COLOR,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn overlay_program_can_be_constructed() {
        let dims = HashMap::from([(1, (612.0, 792.0))]);
        let layout = super::super::page_layout(&dims, 1, 1.0, 72.0);
        let registry = crate::fonts::FontRegistry::new();
        let _program = OverlayCanvasProgram {
            page_layout: layout,
            page_dimensions: &dims,
            scroll_y: 0.0,
            viewport_height: 800.0,
            overlays: &[],
            zoom: 1.0,
            dpi: 72.0,
            active_overlay: None,
            editing: false,
            overlay_color: [0.0, 0.0, 1.0, 1.0],
            font_registry: &registry,
        };
    }
}
