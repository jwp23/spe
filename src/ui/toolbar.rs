// Font family and size selection controls.

use iced::widget::{button, pick_list, row, text, text_input};

use crate::fonts::{FontId, FontRegistry};
use crate::ui::icons;

/// Lightweight wrapper for the font pick list. Holds a FontId and display name,
/// implementing Display for the Iced pick_list widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontOption {
    pub id: FontId,
    pub name: String,
}

impl std::fmt::Display for FontOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Build the list of FontOption values from a FontRegistry.
pub fn font_options(registry: &FontRegistry) -> Vec<FontOption> {
    registry
        .all()
        .iter()
        .map(|e| FontOption {
            id: e.id,
            name: e.display_name.to_string(),
        })
        .collect()
}

/// Amount the size stepper buttons/keys change the font size per press.
const FONT_SIZE_STEP: f32 = 1.0;
/// Smallest font size the stepper will produce (matches the `> 0.0` bound
/// enforced when a typed value is submitted).
const MIN_FONT_SIZE: f32 = 1.0;

/// Step the font size up by one increment.
pub fn increment_font_size(current: f32) -> f32 {
    current + FONT_SIZE_STEP
}

/// Step the font size down by one increment, floored at [`MIN_FONT_SIZE`].
pub fn decrement_font_size(current: f32) -> f32 {
    (current - FONT_SIZE_STEP).max(MIN_FONT_SIZE)
}

/// Persistent state for the toolbar that must survive between view calls.
pub struct ToolbarState {
    pub font: FontId,
    pub font_size: f32,
    pub font_size_input: String,
    pub page_input: String,
    /// Stable ID for the font-size text_input, used to query its focus
    /// state when handling ArrowUp/ArrowDown key presses.
    pub font_size_input_id: iced::widget::Id,
}

impl ToolbarState {
    pub fn new(default_font: FontId) -> Self {
        Self {
            font: default_font,
            font_size: 12.0,
            font_size_input: "12".to_string(),
            page_input: "1".to_string(),
            font_size_input_id: iced::widget::Id::unique(),
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
    FontSelected(FontOption),
    FontSizeInput(String),
    FontSizeSubmit,
    FontSizeIncrement,
    FontSizeDecrement,
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
pub fn toolbar_view<'a>(
    state: &ToolbarState,
    ctx: &ToolbarContext,
    options: &[FontOption],
) -> iced::Element<'a, Message> {
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
        let selected = options.iter().find(|o| o.id == state.font).cloned();
        let font_pick: iced::Element<'a, Message> =
            pick_list(options.to_vec(), selected, Message::FontSelected).into();

        let size_input: iced::Element<'a, Message> = if has_document {
            text_input("size", &state.font_size_input)
                .id(state.font_size_input_id.clone())
                .on_input(Message::FontSizeInput)
                .on_submit(Message::FontSizeSubmit)
                .width(40)
                .into()
        } else {
            text_input("size", &state.font_size_input)
                .id(state.font_size_input_id.clone())
                .width(40)
                .into()
        };

        let size_stepper = row![
            spinner_button('-', Message::FontSizeDecrement, has_document),
            size_input,
            spinner_button('+', Message::FontSizeIncrement, has_document),
        ]
        .spacing(2)
        .align_y(iced::Alignment::Center);

        row![font_pick, size_stepper].spacing(4)
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

/// A small `+`/`-` button flanking the font size input. Uses a plain glyph
/// (not the Phosphor icon font) because regenerating the icon subset
/// requires the original Phosphor.ttf, which isn't available in this repo
/// (see README.md's "Phosphor Icon Font" section for the regen process).
fn spinner_button(glyph: char, message: Message, enabled: bool) -> iced::Element<'static, Message> {
    let label = text(glyph).size(14);
    let btn = button(label).padding([0, 6]);
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
        let registry = FontRegistry::new();
        let state = ToolbarState::new(registry.default_font());
        assert_eq!(state.font, registry.default_font());
        assert!((state.font_size - 12.0).abs() < f32::EPSILON);
        assert_eq!(state.font_size_input, "12");
        assert_eq!(state.page_input, "1");
    }

    #[test]
    fn font_options_has_18_entries() {
        let registry = FontRegistry::new();
        let options = font_options(&registry);
        // 14 Standard 14 + 4 bundled cursive fonts.
        assert_eq!(options.len(), 18);
        assert_eq!(options[0].name, "Helvetica");
    }

    #[test]
    fn font_option_display() {
        let registry = FontRegistry::new();
        let opt = FontOption {
            id: registry.default_font(),
            name: "Helvetica".to_string(),
        };
        assert_eq!(opt.to_string(), "Helvetica");
    }

    #[test]
    fn message_variants_are_constructible() {
        let _ = Message::OpenFile;
        let _ = Message::Save;
        let registry = FontRegistry::new();
        let courier_id = registry.find_by_name("Courier").unwrap();
        let opt = FontOption {
            id: courier_id,
            name: "Courier".to_string(),
        };
        let _ = Message::FontSelected(opt);
        let _ = Message::FontSizeInput("14".to_string());
        let _ = Message::FontSizeIncrement;
        let _ = Message::FontSizeDecrement;
        let _ = Message::ZoomIn;
        let _ = Message::PreviousPage;
        let _ = Message::PageInput("5".to_string());
        let _ = Message::ToggleSidebar;
        let _ = Message::DeleteOverlay;
    }

    #[test]
    fn increment_font_size_steps_up_by_one_point() {
        assert!((increment_font_size(12.0) - 13.0).abs() < f32::EPSILON);
    }

    #[test]
    fn decrement_font_size_steps_down_by_one_point() {
        assert!((decrement_font_size(12.0) - 11.0).abs() < f32::EPSILON);
    }

    #[test]
    fn decrement_font_size_floors_at_minimum() {
        assert!((decrement_font_size(1.0) - MIN_FONT_SIZE).abs() < f32::EPSILON);
        assert!((decrement_font_size(0.5) - MIN_FONT_SIZE).abs() < f32::EPSILON);
    }
}
