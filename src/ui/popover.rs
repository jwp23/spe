// A panel that floats over the rest of the UI, anchored under a control.

use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Layout, Shell, layout, mouse, overlay, renderer};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

/// Gap between the anchor and the panel that drops out of it.
pub const PANEL_GAP: f32 = 2.0;

/// A control (`anchor`) that, while `open`, drops a `panel` over whatever is
/// below it.
///
/// The panel lives in an overlay, so it neither reserves layout space nor is
/// clipped by its parent. Because overlays see events first, a press outside
/// the panel — including one on the anchor itself — and the Escape key both
/// emit `on_dismiss` instead of reaching the widgets underneath.
pub struct Popover<'a, Message> {
    anchor: Element<'a, Message>,
    panel: Element<'a, Message>,
    open: bool,
    on_dismiss: Message,
}

impl<'a, Message> Popover<'a, Message> {
    pub fn new(
        anchor: impl Into<Element<'a, Message>>,
        panel: impl Into<Element<'a, Message>>,
        open: bool,
        on_dismiss: Message,
    ) -> Self {
        Self {
            anchor: anchor.into(),
            panel: panel.into(),
            open,
            on_dismiss,
        }
    }
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for Popover<'_, Message>
where
    Message: Clone,
{
    fn size(&self) -> Size<Length> {
        self.anchor.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.anchor.as_widget().size_hint()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.anchor), Tree::new(&self.panel)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.anchor, &self.panel]);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.anchor
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.anchor.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.anchor
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    #[allow(clippy::too_many_arguments)]
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.anchor.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.anchor.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        let mut children = tree.children.iter_mut();
        let anchor_tree = children.next().expect("anchor tree");
        let panel_tree = children.next().expect("panel tree");

        if !self.open {
            return self.anchor.as_widget_mut().overlay(
                anchor_tree,
                layout,
                renderer,
                viewport,
                translation,
            );
        }

        Some(overlay::Element::new(Box::new(PanelOverlay {
            panel: &mut self.panel,
            tree: panel_tree,
            anchor_bounds: layout.bounds() + translation,
            on_dismiss: self.on_dismiss.clone(),
        })))
    }
}

impl<'a, Message> From<Popover<'a, Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(popover: Popover<'a, Message>) -> Self {
        Element::new(popover)
    }
}

/// The floating half of a [`Popover`], laid out under its anchor.
struct PanelOverlay<'a, 'b, Message> {
    panel: &'b mut Element<'a, Message>,
    tree: &'b mut Tree,
    anchor_bounds: Rectangle,
    on_dismiss: Message,
}

impl<Message> overlay::Overlay<Message, iced::Theme, iced::Renderer>
    for PanelOverlay<'_, '_, Message>
where
    Message: Clone,
{
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let viewport = Rectangle::with_size(bounds);
        let node = self.panel.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );

        let size = node.size();
        let below = self.anchor_bounds.y + self.anchor_bounds.height + PANEL_GAP;
        // Flip above the anchor when there isn't room below, then clamp, so
        // the whole panel stays reachable however near an edge the anchor is.
        let y = if below + size.height <= viewport.height {
            below
        } else {
            (self.anchor_bounds.y - PANEL_GAP - size.height).max(0.0)
        };
        let x = self
            .anchor_bounds
            .x
            .min((viewport.width - size.width).max(0.0))
            .max(0.0);

        node.move_to(iced::Point::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.panel.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.panel
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        self.panel.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &layout.bounds(),
        );

        if shell.is_event_captured() {
            return;
        }

        if dismisses(event, cursor, layout.bounds()) {
            shell.publish(self.on_dismiss.clone());
            shell.capture_event();
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.panel.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        )
    }
}

/// Whether `event` asks for an open panel covering `panel_bounds` to close:
/// Escape, or a mouse press landing anywhere else.
fn dismisses(event: &Event, cursor: mouse::Cursor, panel_bounds: Rectangle) -> bool {
    match event {
        Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) => matches!(
            key,
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
        ),
        Event::Mouse(mouse::Event::ButtonPressed(_)) => !cursor.is_over(panel_bounds),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_harness::Harness;
    use iced::widget::{button, container, text};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Msg {
        Anchor,
        Item,
        Dismissed,
    }

    const VIEWPORT: iced::Size = iced::Size::new(400.0, 300.0);
    /// Fixed anchor box, so the panel below it lands at a known position.
    const ANCHOR_SIZE: (f32, f32) = (100.0, 40.0);
    /// A point inside the panel, which opens directly under the anchor.
    const IN_PANEL: iced::Point = iced::Point::new(50.0, 55.0);
    /// A point clear of both anchor and panel.
    const OUTSIDE: iced::Point = iced::Point::new(320.0, 220.0);
    const ON_ANCHOR: iced::Point = iced::Point::new(50.0, 20.0);

    fn popover_element(open: bool) -> iced::Element<'static, Msg> {
        let anchor = button(text("Anchor"))
            .width(ANCHOR_SIZE.0)
            .height(ANCHOR_SIZE.1)
            .padding(0)
            .on_press(Msg::Anchor);
        let panel = container(
            button(text("Item"))
                .width(120)
                .height(30)
                .padding(0)
                .on_press(Msg::Item),
        );
        Popover::new(anchor, panel, open, Msg::Dismissed).into()
    }

    fn harness(open: bool) -> Harness<'static, Msg> {
        Harness::new(popover_element(open), VIEWPORT)
    }

    #[test]
    fn a_closed_popover_does_not_place_its_panel() {
        assert_eq!(harness(false).click(IN_PANEL), Vec::new());
    }

    #[test]
    fn a_closed_popover_still_delivers_clicks_to_its_anchor() {
        assert_eq!(harness(false).click(ON_ANCHOR), vec![Msg::Anchor]);
    }

    #[test]
    fn an_open_popover_delivers_clicks_to_its_panel() {
        assert_eq!(harness(true).click(IN_PANEL), vec![Msg::Item]);
    }

    #[test]
    fn clicking_away_from_an_open_panel_dismisses_it() {
        assert_eq!(harness(true).click(OUTSIDE), vec![Msg::Dismissed]);
    }

    #[test]
    fn clicking_the_anchor_of_an_open_popover_dismisses_it() {
        assert_eq!(harness(true).click(ON_ANCHOR), vec![Msg::Dismissed]);
    }

    #[test]
    fn escape_dismisses_an_open_panel() {
        let escape = iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape);
        assert_eq!(harness(true).press_key(escape), vec![Msg::Dismissed]);
    }

    #[test]
    fn escape_is_ignored_while_the_popover_is_closed() {
        let escape = iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape);
        assert_eq!(harness(false).press_key(escape), Vec::new());
    }
}
