// Font family picker: a drop-down list that previews every family in its own
// typeface.

use iced::widget::{button, column, container, row, scrollable, text};

use crate::fonts::FontId;
use crate::ui::popover::Popover;
use crate::ui::toolbar::{FontOption, Message};

/// Width of both the anchor and the list, so the toolbar never shifts when a
/// family with a longer name is chosen.
const PICKER_WIDTH: f32 = 150.0;
/// Height of the anchor button. Fixed so the toolbar keeps a steady height
/// whatever the previewed typeface's metrics are.
const ANCHOR_HEIGHT: f32 = 28.0;
/// Point size family names are previewed at. Large enough for the cursive
/// families to read as themselves.
const PREVIEW_SIZE: f32 = 16.0;
/// Width of the list. Wider than the anchor so the longest family names
/// ("Helvetica Bold Oblique") stay on one line.
const LIST_WIDTH: f32 = 220.0;
/// Tallest the list may grow before it starts scrolling.
const LIST_MAX_HEIGHT: f32 = 320.0;
/// Plain glyph marking the anchor as a drop-down. The Phosphor icon subset
/// cannot gain new glyphs (see README.md's "Phosphor Icon Font" section), so
/// this uses a text glyph like the font-size steppers do.
const DROPDOWN_CARET: char = '\u{25BE}';

/// The font picker: an anchor naming the current family, and — while `open` —
/// a list of every family, each previewed in its own typeface.
pub fn font_picker<'a>(
    options: &[FontOption],
    selected: FontId,
    open: bool,
) -> iced::Element<'a, Message> {
    Popover::new(
        anchor(options, selected),
        family_list(options, selected),
        open,
        Message::FontPickerDismissed,
    )
    .into()
}

/// The always-visible control: the selected family's name, in its own font.
fn anchor<'a>(options: &[FontOption], selected: FontId) -> iced::Element<'a, Message> {
    let current = options.iter().find(|o| o.id == selected);
    let name = current.map_or_else(String::new, |o| o.name.clone());
    let font = current.map_or_else(iced::Font::default, |o| o.font);

    button(
        row![
            text(name)
                .font(font)
                .size(PREVIEW_SIZE)
                .width(iced::Length::Fill),
            text(DROPDOWN_CARET).size(10),
        ]
        .align_y(iced::Alignment::Center),
    )
    .width(PICKER_WIDTH)
    .height(ANCHOR_HEIGHT)
    .padding([0, 6])
    .on_press(Message::FontPickerToggled)
    .into()
}

/// The drop-down list of families.
fn family_list<'a>(options: &[FontOption], selected: FontId) -> iced::Element<'a, Message> {
    let entries = column(options.iter().map(|option| family_entry(option, selected)));

    container(scrollable(entries).height(iced::Length::Shrink))
        .width(LIST_WIDTH)
        .max_height(LIST_MAX_HEIGHT)
        .padding(2)
        .style(container::rounded_box)
        .into()
}

/// One row of the list: a family's name, set in that family.
fn family_entry<'a>(option: &FontOption, selected: FontId) -> iced::Element<'a, Message> {
    let is_selected = option.id == selected;
    let style = if is_selected {
        button::primary
    } else {
        button::text
    };

    button(
        text(option.name.clone())
            .font(option.font)
            .size(PREVIEW_SIZE),
    )
    .width(iced::Length::Fill)
    .padding([2, 6])
    .style(style)
    .on_press(Message::FontSelected(option.clone()))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::FontRegistry;
    use crate::ui::test_harness::{Harness, Screenshot, load_bundled_fonts};
    use crate::ui::toolbar::font_options;
    use iced_test::simulator;

    const VIEWPORT: iced::Size = iced::Size::new(320.0, 400.0);

    /// The part of the viewport the open list occupies, clear of the anchor.
    const LIST_REGION: iced::Rectangle = iced::Rectangle {
        x: 0.0,
        y: ANCHOR_HEIGHT + crate::ui::popover::PANEL_GAP,
        width: PICKER_WIDTH,
        height: 200.0,
    };

    fn option_named(registry: &FontRegistry, name: &str) -> FontOption {
        font_options(registry)
            .into_iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("no font option named {name}"))
    }

    /// Helvetica and Courier, in that order — two families the pixel and
    /// selector tests can tell apart.
    fn two_families(registry: &FontRegistry) -> Vec<FontOption> {
        vec![
            option_named(registry, "Helvetica"),
            option_named(registry, "Courier"),
        ]
    }

    fn picker_screenshot(options: &[FontOption], selected: FontId, open: bool) -> Screenshot {
        load_bundled_fonts();
        Harness::new(font_picker(options, selected, open), VIEWPORT).screenshot()
    }

    #[test]
    fn a_family_is_previewed_in_its_own_typeface() {
        let registry = FontRegistry::new();
        let pacifico = option_named(&registry, "Pacifico");
        let unstyled = FontOption {
            font: iced::Font::DEFAULT,
            ..pacifico.clone()
        };

        let own_typeface = picker_screenshot(&[pacifico.clone()], pacifico.id, true);
        let default_typeface = picker_screenshot(&[unstyled], pacifico.id, true);

        let differing = own_typeface.differing_pixels(&default_typeface, LIST_REGION);
        assert!(
            differing > 100,
            "Pacifico's entry rendered the same as the default typeface ({differing} pixels differ)"
        );
    }

    #[test]
    fn the_selected_family_is_marked_in_the_list() {
        let registry = FontRegistry::new();
        let options = two_families(&registry);

        let first_selected = picker_screenshot(&options, options[0].id, true);
        let second_selected = picker_screenshot(&options, options[1].id, true);

        let differing = first_selected.differing_pixels(&second_selected, LIST_REGION);
        assert!(
            differing > 100,
            "the list looks identical whichever family is selected ({differing} pixels differ)"
        );
    }

    #[test]
    fn clicking_a_family_selects_it() {
        let registry = FontRegistry::new();
        let options = two_families(&registry);
        let mut sim = simulator(font_picker(&options, options[0].id, true));

        let _ = sim
            .click("Courier")
            .expect("an open picker should list Courier");

        let messages: Vec<_> = sim.into_messages().collect();
        assert!(
            matches!(&messages[..], [Message::FontSelected(picked)] if picked.id == options[1].id),
            "expected a single Courier selection, got {messages:?}"
        );
    }

    #[test]
    fn a_closed_picker_lists_no_families() {
        let registry = FontRegistry::new();
        let options = two_families(&registry);
        let mut sim = simulator(font_picker(&options, options[0].id, false));

        assert!(
            sim.find("Courier").is_err(),
            "a closed picker must not show the family list"
        );
    }

    #[test]
    fn the_closed_picker_shows_the_current_family() {
        let registry = FontRegistry::new();
        let options = two_families(&registry);
        let mut sim = simulator(font_picker(&options, options[1].id, false));

        assert!(
            sim.find("Courier").is_ok(),
            "the anchor should name the selected family"
        );
    }

    #[test]
    fn pressing_the_anchor_toggles_the_picker() {
        let registry = FontRegistry::new();
        let options = two_families(&registry);
        let mut sim = simulator(font_picker(&options, options[0].id, false));

        let _ = sim
            .click("Helvetica")
            .expect("the anchor should name the selected family");

        let messages: Vec<_> = sim.into_messages().collect();
        assert!(
            matches!(&messages[..], [Message::FontPickerToggled]),
            "expected the anchor to toggle the picker, got {messages:?}"
        );
    }

    #[test]
    fn escape_dismisses_the_open_picker() {
        let registry = FontRegistry::new();
        let options = two_families(&registry);
        let mut sim = simulator(font_picker(&options, options[0].id, true));

        let _ = sim.tap_key(iced::keyboard::key::Named::Escape);

        let messages: Vec<_> = sim.into_messages().collect();
        assert!(
            matches!(&messages[..], [Message::FontPickerDismissed]),
            "expected Escape to dismiss the picker, got {messages:?}"
        );
    }
}
