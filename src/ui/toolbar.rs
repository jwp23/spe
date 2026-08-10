// Font family and size selection controls.

use iced::widget::{button, row, text, text_input};

use crate::fonts::{FontId, FontRegistry};
use crate::ui::font_picker::font_picker;
use crate::ui::icons;

/// One selectable family in the font picker: what to call it, and the Iced
/// font its name is previewed in so the list shows each family in its own
/// typeface.
#[derive(Debug, Clone, PartialEq)]
pub struct FontOption {
    pub id: FontId,
    pub name: String,
    pub font: iced::Font,
    /// The face's x-height as a fraction of its em, when the face that reaches
    /// the screen is one we ship and can measure. Sizes the family's preview.
    pub x_height_ratio: Option<f32>,
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
            font: e.iced_font,
            x_height_ratio: e.x_height_ratio,
        })
        .collect()
}

/// The option a registry offers for `name`, as the toolbar would build it.
#[cfg(test)]
pub fn option_named(registry: &FontRegistry, name: &str) -> FontOption {
    font_options(registry)
        .into_iter()
        .find(|o| o.name == name)
        .unwrap_or_else(|| panic!("no font option named {name}"))
}

/// Amount the size stepper buttons/keys change the font size per press.
const FONT_SIZE_STEP: f32 = 1.0;
/// The single floor every font-size input path enforces: the stepper
/// buttons/keys, and a typed value submitted in the size field.
const MIN_FONT_SIZE: f32 = 1.0;

/// Clamp a font size to [`MIN_FONT_SIZE`]. Used both by the stepper and by
/// a typed value on submit, so every path shares one floor.
pub fn clamp_font_size(size: f32) -> f32 {
    size.max(MIN_FONT_SIZE)
}

/// Step the font size up by one increment.
pub fn increment_font_size(current: f32) -> f32 {
    current + FONT_SIZE_STEP
}

/// Step the font size down by one increment, floored at [`MIN_FONT_SIZE`].
pub fn decrement_font_size(current: f32) -> f32 {
    clamp_font_size(current - FONT_SIZE_STEP)
}

/// Persistent state for the toolbar that must survive between view calls.
pub struct ToolbarState {
    pub font: FontId,
    pub font_size: f32,
    pub font_size_input: String,
    pub page_input: String,
    /// Stable ID for the font-size text_input, used to query its focus
    /// state when handling ArrowUp/ArrowDown key presses, and to send focus
    /// back after a submit whose ChangeFontSize path refocuses the overlay
    /// editor instead.
    pub font_size_input_id: iced::widget::Id,
    /// Stable ID for the page-number text_input, used to send focus back
    /// after a submit whose GoToPage path refocuses the overlay editor
    /// instead.
    pub page_input_id: iced::widget::Id,
    /// Whether the font picker's family list is showing.
    pub font_picker_open: bool,
}

impl ToolbarState {
    pub fn new(default_font: FontId) -> Self {
        Self {
            font: default_font,
            font_size: 12.0,
            font_size_input: "12".to_string(),
            page_input: "1".to_string(),
            font_size_input_id: iced::widget::Id::unique(),
            page_input_id: iced::widget::Id::unique(),
            font_picker_open: false,
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
    /// The font picker's anchor button was pressed: show or hide the list.
    FontPickerToggled,
    /// The open font picker was closed without picking a family (Escape, or
    /// a click outside the list).
    FontPickerDismissed,
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
        let font_pick = font_picker(options, state.font, state.font_picker_open);

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
            spinner_button(icons::CARET_DOWN, Message::FontSizeDecrement, has_document),
            size_input,
            spinner_button(icons::CARET_UP, Message::FontSizeIncrement, has_document),
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
                .id(state.page_input_id.clone())
                .on_input(Message::PageInput)
                .on_submit(Message::PageInputSubmit)
                .width(40)
                .into()
        } else {
            text_input("page", &state.page_input)
                .id(state.page_input_id.clone())
                .width(40)
                .into()
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

/// A small caret-up/caret-down button flanking the font size input.
fn spinner_button(glyph: char, message: Message, enabled: bool) -> iced::Element<'static, Message> {
    let label = text(glyph).font(icons::ICON_FONT).size(14);
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
    fn font_options_carry_each_entrys_iced_font() {
        let registry = FontRegistry::new();
        let options = font_options(&registry);
        for entry in registry.all() {
            let option = options
                .iter()
                .find(|o| o.id == entry.id)
                .unwrap_or_else(|| panic!("no option for {}", entry.display_name));
            assert_eq!(
                option.font, entry.iced_font,
                "{} must be previewed in its own typeface",
                entry.display_name
            );
        }
    }

    #[test]
    fn font_option_display() {
        let registry = FontRegistry::new();
        let opt = option_named(&registry, "Helvetica");
        assert_eq!(opt.to_string(), "Helvetica");
    }

    #[test]
    fn message_variants_are_constructible() {
        let _ = Message::OpenFile;
        let _ = Message::Save;
        let registry = FontRegistry::new();
        let _ = Message::FontSelected(option_named(&registry, "Courier"));
        let _ = Message::FontSizeInput("14".to_string());
        let _ = Message::FontSizeIncrement;
        let _ = Message::FontSizeDecrement;
        let _ = Message::ZoomIn;
        let _ = Message::PreviousPage;
        let _ = Message::PageInput("5".to_string());
        let _ = Message::ToggleSidebar;
        let _ = Message::DeleteOverlay;
    }

    /// A toolbar over a loaded document, with the font picker open or closed.
    fn toolbar_simulator(font_picker_open: bool) -> iced_test::Simulator<'static, Message> {
        let registry = FontRegistry::new();
        let mut state = ToolbarState::new(registry.default_font());
        state.font_picker_open = font_picker_open;
        let ctx = ToolbarContext {
            has_document: true,
            can_undo: false,
            can_redo: false,
            has_selection: false,
            current_page: 1,
            page_count: 1,
            zoom_percent: 100,
            sidebar_visible: false,
        };
        iced_test::simulator(toolbar_view(&state, &ctx, &font_options(&registry)))
    }

    #[test]
    fn the_open_font_picker_lists_the_families() {
        let mut toolbar = toolbar_simulator(true);
        assert!(
            toolbar.find("Times Roman").is_ok(),
            "an open picker should list every family"
        );
    }

    #[test]
    fn the_closed_font_picker_lists_no_families() {
        let mut toolbar = toolbar_simulator(false);
        assert!(
            toolbar.find("Times Roman").is_err(),
            "a closed picker should list nothing"
        );
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
