// View rendering: layout, floating text widgets, overlay styling.

use super::*;

use crate::coordinate::{ConversionParams, overlay_bounding_box, pdf_to_screen, render_scale};
use crate::ui::canvas::{self, PdfCanvasProgram};
use crate::ui::toolbar::{self, ToolbarContext};

impl App {
    pub fn view(&self) -> iced::Element<'_, Message> {
        let toolbar_ctx = ToolbarContext {
            has_document: self.document.is_some(),
            can_undo: !self.undo_stack.is_empty(),
            can_redo: !self.redo_stack.is_empty(),
            has_selection: self.canvas.active_overlay.is_some(),
            current_page: self.document.as_ref().map_or(0, |d| d.current_page),
            page_count: self.document.as_ref().map_or(0, |d| d.page_count),
            zoom_percent: canvas::zoom_percent(self.canvas.zoom),
            sidebar_visible: self.sidebar.visible,
        };
        let toolbar = toolbar::toolbar_view(&self.toolbar, &toolbar_ctx).map(Message::Toolbar);

        let content: iced::Element<Message> = if let Some(doc) = &self.document {
            let dpi = canvas::effective_dpi(self.canvas.zoom);
            let layout =
                canvas::page_layout(&doc.page_dimensions, doc.page_count, self.canvas.zoom, dpi);

            let program = PdfCanvasProgram {
                page_images: &doc.page_images,
                page_layout: layout.clone(),
                page_dimensions: &doc.page_dimensions,
                page_count: doc.page_count,
                scroll_y: self.canvas.scroll_y,
                viewport_height: self.canvas.viewport_height,
                overlays: &doc.overlays,
                zoom: self.canvas.zoom,
                dpi,
                active_overlay: self.canvas.active_overlay,
                editing: self.canvas.editing,
                overlay_color: self.config.overlay_color,
            };

            let (canvas_width, canvas_height) = self.canvas_dimensions(doc);

            let canvas_area: iced::Element<Message> = iced::widget::canvas(program)
                .width(canvas_width)
                .height(canvas_height)
                .into();

            let scrollable_canvas: iced::Element<Message> = iced::widget::scrollable(canvas_area)
                .direction(iced::widget::scrollable::Direction::Both {
                    vertical: iced::widget::scrollable::Scrollbar::default(),
                    horizontal: iced::widget::scrollable::Scrollbar::default(),
                })
                .id(self.scrollable_id.clone())
                .on_scroll(|viewport| {
                    Message::CanvasScrolled(viewport.absolute_offset().y, viewport.bounds().height)
                })
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .into();

            let canvas_area_element: iced::Element<Message> =
                self.floating_text_input(doc, &layout, scrollable_canvas);

            if self.sidebar.visible {
                let sidebar = crate::ui::sidebar::sidebar_view(
                    &self.sidebar,
                    doc.page_count,
                    doc.current_page,
                    &doc.page_dimensions,
                    &doc.overlays,
                    self.config.overlay_color,
                );

                let handle_strip = iced::widget::container(
                    iced::widget::Space::new()
                        .width(4)
                        .height(iced::Length::Fill),
                )
                .style(|_theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(SIDEBAR_HANDLE_BACKGROUND)),
                    ..Default::default()
                })
                .width(4)
                .height(iced::Length::Fill);

                let handle = iced::widget::mouse_area(handle_strip)
                    .on_press(Message::SidebarDragStart(0.0))
                    .interaction(iced::mouse::Interaction::ResizingHorizontally);

                iced::widget::row![sidebar, handle, canvas_area_element].into()
            } else {
                canvas_area_element
            }
        } else {
            iced::widget::center(iced::widget::text("Open a PDF to get started").size(20)).into()
        };

        let mut main_column = iced::widget::column![toolbar];

        if let Some((msg, _)) = &self.status_message {
            let toast = iced::widget::container(iced::widget::text(msg.as_str()).size(14))
                .padding(8)
                .width(iced::Length::Fill)
                .style(|_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(TOAST_BACKGROUND)),
                    text_color: Some(iced::Color::WHITE),
                    ..Default::default()
                });
            main_column = main_column.push(toast);
        }

        main_column = main_column.push(content);

        main_column.into()
    }

    /// Compute canvas widget dimensions for multi-page layout.
    /// Width: max page width or viewport, whichever is larger.
    /// Height: total layout height (all pages + gaps) or viewport, whichever is larger.
    pub(super) fn canvas_dimensions(&self, doc: &DocumentState) -> (iced::Length, iced::Length) {
        const TOOLBAR_HEIGHT_ESTIMATE: f32 = 40.0;

        let dpi = canvas::effective_dpi(self.canvas.zoom);
        let layout =
            canvas::page_layout(&doc.page_dimensions, doc.page_count, self.canvas.zoom, dpi);

        if layout.page_tops.is_empty() {
            return (iced::Length::Fill, iced::Length::Fill);
        }

        match self.window_size {
            Some(win) => {
                let available_w =
                    (win.width - self.effective_sidebar_width() - SCROLLBAR_MARGIN).max(1.0);
                let available_h =
                    (win.height - TOOLBAR_HEIGHT_ESTIMATE - SCROLLBAR_MARGIN).max(1.0);
                (
                    iced::Length::Fixed(layout.max_width.max(available_w)),
                    iced::Length::Fixed(layout.total_height.max(available_h)),
                )
            }
            None => (iced::Length::Fill, iced::Length::Fill),
        }
    }

    /// Wrap the scrollable canvas in a stack with a floating text widget when editing an overlay.
    /// Wrap the scrollable canvas in a 2-child Stack. The second child is
    /// either the editing widget (text_input or text_editor) or an invisible
    /// placeholder. The child count MUST stay constant so Iced's widget tree
    /// reconciliation preserves the canvas ProgramState across editing transitions.
    fn floating_text_input<'a>(
        &'a self,
        doc: &'a DocumentState,
        layout: &canvas::PageLayout,
        scrollable_canvas: iced::Element<'a, Message>,
    ) -> iced::Element<'a, Message> {
        let overlay_child = self.stack_overlay_element(doc, layout);
        iced::widget::stack![scrollable_canvas, overlay_child].into()
    }

    /// Returns the second child for the floating text Stack.
    /// Always returns an element to keep the Stack child count consistent across
    /// editing state changes, preventing Iced from resetting the canvas ProgramState.
    pub(super) fn stack_overlay_element<'a>(
        &'a self,
        doc: &'a DocumentState,
        layout: &canvas::PageLayout,
    ) -> iced::Element<'a, Message> {
        self.build_editing_widget(doc, layout)
            .unwrap_or_else(|| iced::widget::Space::new().into())
    }

    /// Build the positioned floating text widget if currently editing an overlay.
    /// Returns None when not editing or when required state is unavailable.
    fn build_editing_widget<'a>(
        &'a self,
        doc: &'a DocumentState,
        layout: &canvas::PageLayout,
    ) -> Option<iced::Element<'a, Message>> {
        let idx = self.canvas.active_overlay?;
        if !self.canvas.editing {
            return None;
        }
        let overlay = doc.overlays.get(idx)?;

        let canvas_w = self
            .window_size
            .map(|s| (s.width - self.effective_sidebar_width() - SCROLLBAR_MARGIN).max(1.0))
            .unwrap_or(800.0);

        let dpi = canvas::effective_dpi(self.canvas.zoom);

        if layout.page_tops.is_empty() {
            return None;
        }

        let page_rect = canvas::page_rect_in_canvas(layout, overlay.page, canvas_w);
        let (_, page_h) = doc.page_dimensions.get(&overlay.page)?;

        let params = ConversionParams {
            zoom: self.canvas.zoom,
            dpi,
            page_height: *page_h,
            offset_x: page_rect.x,
            offset_y: page_rect.y,
        };

        let (screen_x, screen_y) = pdf_to_screen(overlay.position.x, overlay.position.y, &params);

        let scale = render_scale(self.canvas.zoom, dpi);
        let scaled_font_size = overlay.font_size * scale;
        let top_offset = (screen_y - self.canvas.scroll_y - scaled_font_size).max(0.0);
        let left_offset = screen_x.max(0.0);

        let widget: iced::Element<Message> = if let Some(pdf_width) = overlay.width {
            let content = self.editor_content.as_ref()?;
            let screen_width = pdf_width * scale;
            iced::widget::text_editor(content)
                .on_action(Message::TextEditorAction)
                .id(self.text_input_id.clone())
                .size(iced::Pixels(scaled_font_size))
                .padding(iced::Padding::ZERO)
                .width(screen_width)
                .style(overlay_text_editor_style)
                .into()
        } else {
            let text_width =
                overlay_bounding_box(&overlay.text, overlay.font, overlay.font_size).width * scale;
            let buffer = scaled_font_size * 2.0;
            let input_width = (scaled_font_size * 6.0).max(text_width + buffer);
            iced::widget::text_input("", &overlay.text)
                .id(self.text_input_id.clone())
                .on_input(Message::UpdateOverlayText)
                .on_submit(Message::CommitText)
                .size(iced::Pixels(scaled_font_size))
                .padding(iced::Padding::ZERO)
                .width(input_width)
                .style(overlay_text_input_style)
                .into()
        };

        let positioned: iced::Element<Message> = iced::widget::container(widget)
            .padding(iced::Padding::ZERO.top(top_offset).left(left_offset))
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .align_x(iced::alignment::Horizontal::Left)
            .align_y(iced::alignment::Vertical::Top)
            .into();

        Some(positioned)
    }
}

/// Transparent background with blue outline for the floating text input overlay.
const OVERLAY_BORDER: iced::Border = iced::Border {
    color: canvas::SELECTION_COLOR,
    width: 1.5,
    radius: iced::border::Radius {
        top_left: 2.0,
        top_right: 2.0,
        bottom_right: 2.0,
        bottom_left: 2.0,
    },
};
const OVERLAY_PLACEHOLDER: iced::Color = iced::Color::from_rgba(0.0, 0.0, 0.0, 0.4);
const OVERLAY_SELECTION: iced::Color = iced::Color::from_rgba(0.2, 0.5, 1.0, 0.3);
/// Background color for the sidebar resize handle strip.
const SIDEBAR_HANDLE_BACKGROUND: iced::Color = iced::Color::from_rgb(0.2, 0.2, 0.3);
/// Background color for the status/toast message bar.
const TOAST_BACKGROUND: iced::Color = iced::Color::from_rgb(0.15, 0.15, 0.2);

fn overlay_text_input_style(
    _theme: &iced::Theme,
    _status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    iced::widget::text_input::Style {
        background: iced::Background::Color(iced::Color::TRANSPARENT),
        border: OVERLAY_BORDER,
        icon: iced::Color::BLACK,
        placeholder: OVERLAY_PLACEHOLDER,
        value: iced::Color::BLACK,
        selection: OVERLAY_SELECTION,
    }
}

fn overlay_text_editor_style(
    _theme: &iced::Theme,
    _status: iced::widget::text_editor::Status,
) -> iced::widget::text_editor::Style {
    iced::widget::text_editor::Style {
        background: iced::Background::Color(iced::Color::TRANSPARENT),
        border: OVERLAY_BORDER,
        placeholder: OVERLAY_PLACEHOLDER,
        value: iced::Color::BLACK,
        selection: OVERLAY_SELECTION,
    }
}
