// Font family and size selection controls.

use iced::widget::{button, pick_list, row, text, text_input};

use crate::overlay::Standard14Font;
use crate::ui::icons;

/// Persistent state for the toolbar that must survive between view calls.
pub struct ToolbarState {
    pub font: Standard14Font,
    pub font_size: f32,
    pub font_size_input: String,
    pub page_input: String,
}

impl Default for ToolbarState {
    fn default() -> Self {
        Self {
            font: Standard14Font::Helvetica,
            font_size: 12.0,
            font_size_input: "12".to_string(),
            page_input: "1".to_string(),
        }
    }
}

/// Messages emitted by the toolbar.
#[derive(Debug, Clone)]
pub enum Message {
    OpenFile,
    Save,
    SaveAs,
    Undo,
    Redo,
    FontSelected(Standard14Font),
    FontSizeInput(String),
    FontSizeSubmit,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ZoomFitWidth,
    PreviousPage,
    NextPage,
    PageInput(String),
    PageInputSubmit,
    ToggleSidebar,
    DeleteOverlay,
}

/// Parameters for rendering the toolbar, collected from app state.
pub struct ToolbarContext {
    pub has_document: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub has_selection: bool,
    pub current_page: u32,
    pub page_count: u32,
    pub zoom_percent: u32,
    pub sidebar_visible: bool,
}

/// Renders the application toolbar.
#[allow(clippy::too_many_lines)]
pub fn toolbar_view<'a>(state: &ToolbarState, ctx: &ToolbarContext) -> iced::Element<'a, Message> {
    let has_document = ctx.has_document;
    let can_undo = ctx.can_undo;
    let can_redo = ctx.can_redo;
    let has_selection = ctx.has_selection;
    let current_page = ctx.current_page;
    let page_count = ctx.page_count;
    let zoom_percent = ctx.zoom_percent;
    let separator = || {
        iced::widget::container(iced::widget::rule::vertical(1))
            .height(28)
            .padding([0, 4])
    };

    let doc_group = row![
        icon_button(icons::FOLDER_OPEN, Message::OpenFile, true),
        icon_button(icons::FLOPPY_DISK, Message::Save, has_document),
        icon_button(icons::ARROW_U_UP_LEFT, Message::SaveAs, has_document),
    ]
    .spacing(2);

    let history_group = row![
        icon_button(
            icons::ARROW_COUNTER_CLOCKWISE,
            Message::Undo,
            has_document && can_undo
        ),
        icon_button(
            icons::ARROW_CLOCKWISE,
            Message::Redo,
            has_document && can_redo
        ),
    ]
    .spacing(2);

    let font_group = {
        let font_pick: iced::Element<'a, Message> = pick_list(
            Standard14Font::ALL.as_slice(),
            Some(state.font),
            Message::FontSelected,
        )
        .into();

        let size_input: iced::Element<'a, Message> = if has_document {
            text_input("size", &state.font_size_input)
                .on_input(Message::FontSizeInput)
                .on_submit(Message::FontSizeSubmit)
                .width(48)
                .into()
        } else {
            text_input("size", &state.font_size_input).width(48).into()
        };

        row![font_pick, size_input].spacing(4)
    };

    let zoom_group = row![
        icon_button(
            icons::MAGNIFYING_GLASS_MINUS,
            Message::ZoomOut,
            has_document
        ),
        text(format!("{zoom_percent}%")).size(14),
        icon_button(icons::MAGNIFYING_GLASS_PLUS, Message::ZoomIn, has_document),
        icon_button(icons::MAGNIFYING_GLASS, Message::ZoomFitWidth, has_document,),
    ]
    .spacing(2)
    .align_y(iced::Alignment::Center);

    let page_group = {
        let prev_enabled = has_document && current_page > 1;
        let next_enabled = has_document && current_page < page_count;

        let page_input: iced::Element<'a, Message> = if has_document {
            text_input("page", &state.page_input)
                .on_input(Message::PageInput)
                .on_submit(Message::PageInputSubmit)
                .width(40)
                .into()
        } else {
            text_input("page", &state.page_input).width(40).into()
        };

        row![
            icon_button(icons::CARET_LEFT, Message::PreviousPage, prev_enabled),
            page_input,
            text(format!("/ {page_count}")).size(14),
            icon_button(icons::CARET_RIGHT, Message::NextPage, next_enabled),
        ]
        .spacing(2)
        .align_y(iced::Alignment::Center)
    };

    let delete_group = icon_button(icons::TRASH, Message::DeleteOverlay, has_selection);

    row![
        icon_button(icons::SIDEBAR, Message::ToggleSidebar, true),
        separator(),
        doc_group,
        separator(),
        history_group,
        separator(),
        font_group,
        separator(),
        zoom_group,
        separator(),
        page_group,
        separator(),
        delete_group,
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center)
    .padding(4)
    .into()
}

fn icon_button(icon: char, message: Message, enabled: bool) -> iced::Element<'static, Message> {
    let label = text(icon).font(icons::ICON_FONT).size(18);
    let btn = button(label).padding(4);
    if enabled {
        btn.on_press(message).into()
    } else {
        btn.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_state_defaults() {
        let state = ToolbarState::default();
        assert_eq!(state.font, Standard14Font::Helvetica);
        assert!((state.font_size - 12.0).abs() < f32::EPSILON);
        assert_eq!(state.font_size_input, "12");
        assert_eq!(state.page_input, "1");
    }

    #[test]
    fn message_variants_are_constructible() {
        let _ = Message::OpenFile;
        let _ = Message::Save;
        let _ = Message::FontSelected(Standard14Font::Courier);
        let _ = Message::FontSizeInput("14".to_string());
        let _ = Message::ZoomIn;
        let _ = Message::PreviousPage;
        let _ = Message::PageInput("5".to_string());
        let _ = Message::ToggleSidebar;
        let _ = Message::DeleteOverlay;
    }
}
