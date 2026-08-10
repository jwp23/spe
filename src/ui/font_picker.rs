// Font family picker: a label naming the current family, and a drop-down list
// that previews every family in its own typeface.
//
// The two halves have different jobs. The list is a specimen sheet, so each
// row is set in the family it offers. The anchor is a label read at a glance,
// so it stays in the UI font — which also keeps its width predictable, since a
// label in one known face can only grow by having a longer name.

use iced::widget::{button, column, container, row, scrollable, text};

use crate::fonts::FontId;
use crate::ui::popover::Popover;
use crate::ui::text_width::shaped_text_width;
use crate::ui::toolbar::{FontOption, Message};

/// Height of the anchor button. Fixed so the toolbar keeps a steady height.
const ANCHOR_HEIGHT: f32 = 28.0;
/// Face the anchor labels the current family in.
const ANCHOR_FONT: iced::Font = iced::Font::DEFAULT;
/// Point size of the anchor's label.
const ANCHOR_TEXT_SIZE: f32 = 16.0;
/// Point size a family whose lowercase already sits at the target height is
/// previewed at. Faces with smaller lowercase are previewed larger — see
/// [`preview_size`].
const PREVIEW_SIZE: f32 = 16.0;
/// Fraction of the em the previews normalize each face's lowercase onto.
///
/// Two faces set at one point size look the same size when their x-heights
/// match, and the cursive faces carry x-heights around 0.33 em against the
/// 0.52–0.54 em measured from the faces the Standard 14 resolve to, which is
/// why a single point size leaves them a wisp. This target is the middle of
/// that measured range, so a cursive preview lands beside the Standard 14 rows
/// rather than under them.
const TARGET_X_HEIGHT_RATIO: f32 = 0.53;
/// Most a preview may be enlarged. A face with almost no lowercase would
/// otherwise demand a row that dwarfs every other.
const MAX_PREVIEW_SCALE: f32 = 2.0;
/// Tallest the list may grow before it starts scrolling.
const LIST_MAX_HEIGHT: f32 = 320.0;
/// Horizontal padding inside the anchor button, per side.
const ANCHOR_PADDING_X: f32 = 6.0;
/// Horizontal padding inside a list row, per side.
const ROW_PADDING_X: f32 = 6.0;
/// Padding between the list's border and its rows.
const LIST_PADDING: f32 = 2.0;
/// Point size of the drop-down caret.
const CARET_SIZE: f32 = 10.0;
/// Gap between the label and the caret, so the longest name does not run into
/// it.
const CARET_GAP: f32 = 6.0;
/// Room the scrollbar takes from a row once the list is long enough to
/// scroll — the width iced's `scrollable::Scrollbar` defaults to.
const SCROLLBAR_WIDTH: f32 = 10.0;
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

/// Point size a face is previewed at, given the fraction of the em its
/// lowercase occupies.
///
/// Enlarging only: a face already at or above the target reads fine at the
/// base size, and shrinking it would undo that.
fn preview_size(x_height_ratio: Option<f32>) -> f32 {
    let Some(ratio) = x_height_ratio.filter(|r| *r > 0.0) else {
        return PREVIEW_SIZE;
    };
    let scale = (TARGET_X_HEIGHT_RATIO / ratio).clamp(1.0, MAX_PREVIEW_SCALE);
    PREVIEW_SIZE * scale
}

/// Width the anchor needs to label any family without wrapping — the longest
/// name in the UI font, plus the caret and the padding around them.
///
/// Measured over every family rather than the selected one so trying families
/// on never shifts the toolbar.
fn anchor_width(options: &[FontOption]) -> f32 {
    let widest = options
        .iter()
        .map(|option| shaped_text_width(&option.name, ANCHOR_FONT, ANCHOR_TEXT_SIZE))
        .fold(0.0_f32, f32::max);
    let caret = shaped_text_width(&DROPDOWN_CARET.to_string(), ANCHOR_FONT, CARET_SIZE);

    (widest + CARET_GAP + caret + 2.0 * ANCHOR_PADDING_X).ceil()
}

/// Width the list needs to show every row on one line, each name measured in
/// the face and at the size it previews at.
fn list_width(options: &[FontOption]) -> f32 {
    let widest = options
        .iter()
        .map(|option| {
            shaped_text_width(
                &option.name,
                option.font,
                preview_size(option.x_height_ratio),
            )
        })
        .fold(0.0_f32, f32::max);

    (widest + 2.0 * ROW_PADDING_X + 2.0 * LIST_PADDING + SCROLLBAR_WIDTH).ceil()
}

/// The always-visible control: the selected family's name, in the UI font.
fn anchor<'a>(options: &[FontOption], selected: FontId) -> iced::Element<'a, Message> {
    let name = options
        .iter()
        .find(|o| o.id == selected)
        .map_or_else(String::new, |o| o.name.clone());

    button(
        row![
            text(name)
                .font(ANCHOR_FONT)
                .size(ANCHOR_TEXT_SIZE)
                .wrapping(text::Wrapping::None)
                .width(iced::Length::Fill),
            text(DROPDOWN_CARET).font(ANCHOR_FONT).size(CARET_SIZE),
        ]
        .spacing(CARET_GAP)
        // Fills the button so there is a height to center the label within;
        // a shrunk row would sit against the top edge.
        .height(iced::Length::Fill)
        .align_y(iced::Alignment::Center),
    )
    .width(anchor_width(options))
    .height(ANCHOR_HEIGHT)
    .padding([0, ANCHOR_PADDING_X as u16])
    .on_press(Message::FontPickerToggled)
    .into()
}

/// The drop-down list of families.
fn family_list<'a>(options: &[FontOption], selected: FontId) -> iced::Element<'a, Message> {
    let entries = column(options.iter().map(|option| family_entry(option, selected)));

    container(scrollable(entries).height(iced::Length::Shrink))
        .width(list_width(options))
        .max_height(LIST_MAX_HEIGHT)
        .padding(LIST_PADDING as u16)
        .style(container::rounded_box)
        .into()
}

/// One row of the list: a family's name, set in that family at the size that
/// family needs to read as itself.
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
            .size(preview_size(option.x_height_ratio))
            .wrapping(text::Wrapping::None),
    )
    .width(iced::Length::Fill)
    .padding([2, ROW_PADDING_X as u16])
    .style(style)
    .on_press(Message::FontSelected(option.clone()))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::FontRegistry;
    use crate::ui::test_harness::{Harness, Screenshot, load_bundled_fonts};
    use crate::ui::toolbar::option_named;
    use iced_test::simulator;

    const VIEWPORT: iced::Size = iced::Size::new(320.0, 400.0);

    /// The part of the viewport the open list occupies, clear of the anchor.
    const LIST_REGION: iced::Rectangle = iced::Rectangle {
        x: 0.0,
        y: ANCHOR_HEIGHT + crate::ui::popover::PANEL_GAP,
        width: VIEWPORT.width,
        height: 200.0,
    };

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

    /// A face whose lowercase sits far below the target, like the bundled
    /// scripts.
    const SMALL_X_HEIGHT: f32 = 0.33;

    #[test]
    fn a_face_of_unknown_x_height_previews_at_the_base_size() {
        // The Standard 14 render through system faces we cannot measure, so
        // there is nothing to normalize onto and the base size stands.
        let size = preview_size(None);
        assert!(
            (size - PREVIEW_SIZE).abs() < f32::EPSILON,
            "expected the base {PREVIEW_SIZE}, got {size}"
        );
    }

    #[test]
    fn a_nonsensical_x_height_previews_at_the_base_size() {
        // A ratio this far outside (0, 1) means the face's metrics are not to
        // be trusted, and a NaN would otherwise poison the layout it feeds.
        for ratio in [0.0, -0.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let size = preview_size(Some(ratio));
            assert!(
                (size - PREVIEW_SIZE).abs() < f32::EPSILON,
                "an x-height of {ratio} previewed at {size}, not the base {PREVIEW_SIZE}"
            );
        }
    }

    #[test]
    fn a_face_with_a_small_x_height_previews_larger() {
        let cursive = preview_size(Some(SMALL_X_HEIGHT));
        assert!(
            cursive > PREVIEW_SIZE,
            "a face with an x-height of {SMALL_X_HEIGHT} em previewed at {cursive}, \
             no larger than the {PREVIEW_SIZE} a normal face gets"
        );
    }

    #[test]
    fn a_face_already_at_the_target_x_height_previews_at_the_base_size() {
        let size = preview_size(Some(TARGET_X_HEIGHT_RATIO));
        assert!(
            (size - PREVIEW_SIZE).abs() < 0.01,
            "expected the base {PREVIEW_SIZE}, got {size}"
        );
    }

    #[test]
    fn preview_scaling_is_capped() {
        // A face with almost no lowercase would otherwise demand a preview so
        // large it dwarfs every other row.
        let size = preview_size(Some(0.01));
        assert!(
            (size - PREVIEW_SIZE * MAX_PREVIEW_SCALE).abs() < 0.01,
            "expected the cap at {}, got {size}",
            PREVIEW_SIZE * MAX_PREVIEW_SCALE
        );
    }

    #[test]
    fn a_face_taller_than_the_target_is_never_shrunk() {
        // Scaling down would make an already-legible face harder to read, so
        // the base size is a floor.
        let size = preview_size(Some(0.75));
        assert!(
            size >= PREVIEW_SIZE,
            "a tall face previewed at {size}, below the {PREVIEW_SIZE} floor"
        );
    }

    #[test]
    fn the_anchor_fits_the_longest_family_name() {
        let registry = FontRegistry::new();
        let options = crate::ui::toolbar::font_options(&registry);
        let widest = options
            .iter()
            .map(|o| shaped_text_width(&o.name, ANCHOR_FONT, ANCHOR_TEXT_SIZE))
            .fold(0.0_f32, f32::max);

        assert!(
            anchor_width(&options) > widest,
            "the anchor is {} wide but its longest name needs {widest}",
            anchor_width(&options)
        );
    }

    #[test]
    fn the_anchor_is_drawn_as_wide_as_its_longest_name_needs() {
        // `wrapping(None)` keeps an over-long name from spilling below the
        // button, which means a too-narrow anchor now hides the tail of the
        // name instead of painting outside itself. Only the drawn width shows
        // whether the whole name actually fits.
        load_bundled_fonts();
        let registry = FontRegistry::new();
        let options = crate::ui::toolbar::font_options(&registry);
        let shot = Harness::new(
            font_picker(&options, options[0].id, false),
            iced::Size::new(600.0, 200.0),
        )
        .screenshot();

        let band = 0..ANCHOR_HEIGHT as u32;
        let right_edge = (0..600)
            .rev()
            .find(|x| band.clone().any(|y| shot.darkening(*x, y) > 20.0))
            .expect("the anchor painted nothing") as f32;
        let needed = anchor_width(&options);

        assert!(
            (right_edge - needed).abs() <= 2.0,
            "the anchor needs {needed} to show every name but was drawn {right_edge} wide"
        );
    }

    #[test]
    fn the_anchor_centers_its_label_vertically() {
        load_bundled_fonts();
        let registry = FontRegistry::new();
        let options = crate::ui::toolbar::font_options(&registry);
        let shot = Harness::new(
            font_picker(&options, options[0].id, false),
            iced::Size::new(600.0, 200.0),
        )
        .screenshot();

        // The label is light glyphs on the filled button. Sampling stops short
        // of the button's ends so its rounded corners, which show the page
        // behind them, cannot pass for label ink.
        let ends = ANCHOR_PADDING_X as u32 + 2;
        let columns = ends..anchor_width(&options) as u32 - ends;
        let is_label_ink = |x: u32, y: u32| {
            let (r, g, b) = shot.pixel(x, y);
            r > 200 && g > 200 && b > 200
        };
        let rows: Vec<u32> = (0..ANCHOR_HEIGHT as u32)
            .filter(|y| columns.clone().any(|x| is_label_ink(x, *y)))
            .collect();

        let above = *rows.first().expect("the anchor painted no label") as f32;
        let below = ANCHOR_HEIGHT - 1.0 - *rows.last().expect("the anchor painted no label") as f32;
        assert!(
            (above - below).abs() <= 2.0,
            "the label sits {above}px below the button's top and {below}px above its bottom"
        );
    }

    #[test]
    fn the_anchor_is_the_same_width_whichever_family_is_selected() {
        // The toolbar must not shift as families are tried on.
        load_bundled_fonts();
        let registry = FontRegistry::new();
        let options = crate::ui::toolbar::font_options(&registry);

        let painted_width = |selected: FontId| {
            let shot = Harness::new(
                font_picker(&options, selected, false),
                iced::Size::new(600.0, 200.0),
            )
            .screenshot();
            let band = 0..ANCHOR_HEIGHT as u32;
            (0..600)
                .rev()
                .find(|x| band.clone().any(|y| shot.darkening(*x, y) > 20.0))
        };

        let reference = painted_width(options[0].id);
        assert!(reference.is_some(), "the anchor painted nothing");
        for option in &options {
            assert_eq!(
                painted_width(option.id),
                reference,
                "selecting {} moved the anchor's right edge",
                option.name
            );
        }
    }

    #[test]
    fn the_anchor_keeps_the_longest_name_inside_its_button() {
        // The name used to wrap and paint its second line below the button.
        load_bundled_fonts();
        let registry = FontRegistry::new();
        let options = crate::ui::toolbar::font_options(&registry);
        let longest = options
            .iter()
            .max_by_key(|o| o.name.len())
            .expect("the registry offers families");
        let shot = Harness::new(
            font_picker(&options, longest.id, false),
            iced::Size::new(600.0, 200.0),
        )
        .screenshot();

        let spilled = (ANCHOR_HEIGHT.ceil() as u32..200)
            .flat_map(|y| (0..600).map(move |x| (x, y)))
            .filter(|(x, y)| shot.darkening(*x, *y) > 20.0)
            .count();
        assert_eq!(
            spilled, 0,
            "{} painted {spilled} pixels below the {ANCHOR_HEIGHT}px anchor",
            longest.name
        );
    }

    #[test]
    fn the_anchor_names_the_family_in_the_ui_font() {
        // The list is the specimen sheet; the anchor is a label, so its
        // appearance must not follow the family's typeface.
        let registry = FontRegistry::new();
        let pacifico = option_named(&registry, "Pacifico");
        let unstyled = FontOption {
            font: iced::Font::DEFAULT,
            ..pacifico.clone()
        };

        let in_family = picker_screenshot(std::slice::from_ref(&pacifico), pacifico.id, false);
        let in_ui_font = picker_screenshot(&[unstyled], pacifico.id, false);

        let anchor_region = iced::Rectangle {
            x: 0.0,
            y: 0.0,
            width: VIEWPORT.width,
            height: ANCHOR_HEIGHT,
        };
        assert_eq!(
            in_family.differing_pixels(&in_ui_font, anchor_region),
            0,
            "the anchor rendered Pacifico's name differently from the UI font"
        );
    }

    #[test]
    fn the_list_fits_the_widest_previewed_row() {
        let registry = FontRegistry::new();
        let options = crate::ui::toolbar::font_options(&registry);
        let widest = options
            .iter()
            .map(|o| shaped_text_width(&o.name, o.font, preview_size(o.x_height_ratio)))
            .fold(0.0_f32, f32::max);

        assert!(
            list_width(&options) > widest,
            "the list is {} wide but its widest row needs {widest}",
            list_width(&options)
        );
    }

    #[test]
    fn a_family_is_previewed_in_its_own_typeface() {
        let registry = FontRegistry::new();
        let pacifico = option_named(&registry, "Pacifico");
        let unstyled = FontOption {
            font: iced::Font::DEFAULT,
            ..pacifico.clone()
        };

        let own_typeface = picker_screenshot(std::slice::from_ref(&pacifico), pacifico.id, true);
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
