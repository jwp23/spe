// Headless UI harness for tests: build an element, feed it real events, and
// read back the pixels the renderer actually produced.

use iced::mouse;
use iced_test::core::clipboard;
use iced_test::core::renderer::Headless;
use iced_test::runtime::{UserInterface, user_interface};

/// Size of the headless render surface, in logical pixels. Large enough to
/// hold a US Letter page at zoom 1 with room around it.
pub const RENDER_SIZE: iced::Size = iced::Size {
    width: 900.0,
    height: 700.0,
};

/// Drives a single element through the real Iced runtime without a window.
///
/// Events go in the way the runtime delivers them, so widgets react through
/// the same paths the app uses, and messages come back as the widget emits
/// them.
pub struct Harness<'a, Message> {
    ui: UserInterface<'a, Message, iced::Theme, iced::Renderer>,
    renderer: iced::Renderer,
    size: iced::Size,
    cursor: mouse::Cursor,
}

impl<'a, Message> Harness<'a, Message> {
    /// Lay out `element` in a `size` viewport with no cursor present.
    pub fn new(element: impl Into<iced::Element<'a, Message>>, size: iced::Size) -> Self {
        let mut renderer = iced_test::futures::futures::executor::block_on(
            // Pinned to the software backend so rendered pixels do not depend
            // on whether a GPU is present.
            <iced::Renderer as Headless>::new(
                iced::Font::DEFAULT,
                iced::Pixels(16.0),
                Some("tiny-skia"),
            ),
        )
        .expect("software renderer should be available without a GPU or display");

        let ui = UserInterface::build(
            element.into(),
            size,
            user_interface::Cache::default(),
            &mut renderer,
        );

        Self {
            ui,
            renderer,
            size,
            cursor: mouse::Cursor::Unavailable,
        }
    }

    /// Park the cursor at `position`, delivering the move event.
    pub fn move_cursor(&mut self, position: iced::Point) -> Vec<Message> {
        self.cursor = mouse::Cursor::Available(position);
        self.publish(&[iced::Event::Mouse(mouse::Event::CursorMoved { position })])
    }

    /// Move to `position` and press-and-release the left button there.
    pub fn click(&mut self, position: iced::Point) -> Vec<Message> {
        let mut messages = self.move_cursor(position);
        messages.extend(self.press_left());
        messages.extend(self.release_left());
        messages
    }

    /// Press the left button without releasing it, so a drag can be left in
    /// flight and drawn mid-gesture.
    pub fn press_left(&mut self) -> Vec<Message> {
        self.publish(&[iced::Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))])
    }

    /// Release the left button.
    pub fn release_left(&mut self) -> Vec<Message> {
        self.publish(&[iced::Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        ))])
    }

    /// Press `key` with no modifiers held.
    pub fn press_key(&mut self, key: iced::keyboard::Key) -> Vec<Message> {
        self.publish(&[iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: iced::keyboard::key::Physical::Unidentified(
                iced::keyboard::key::NativeCode::Unidentified,
            ),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        })])
    }

    fn publish(&mut self, events: &[iced::Event]) -> Vec<Message> {
        let mut messages = Vec::new();
        let _ = self.ui.update(
            events,
            self.cursor,
            &mut self.renderer,
            &mut clipboard::Null,
            &mut messages,
        );
        messages
    }

    /// Draw the current state over a white background and capture the pixels.
    pub fn screenshot(&mut self) -> Screenshot {
        // `UserInterface::draw` only paints an overlay whose layout a prior
        // event pass computed, so ask for a redraw first — the same thing the
        // runtime does before every frame.
        let _ = self.publish(&[iced::Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))]);

        self.ui.draw(
            &mut self.renderer,
            &iced::Theme::Light,
            &iced_test::core::renderer::Style {
                text_color: iced::Color::BLACK,
            },
            self.cursor,
        );

        let width = self.size.width as u32;
        let height = self.size.height as u32;
        let rgba =
            self.renderer
                .screenshot(iced::Size::new(width, height), 1.0, iced::Color::WHITE);

        Screenshot {
            width,
            height,
            rgba,
        }
    }
}

/// Composited RGBA pixels of a headless render.
pub struct Screenshot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Screenshot {
    /// The RGB triple at (x, y).
    pub fn pixel(&self, x: u32, y: u32) -> (u8, u8, u8) {
        assert!(
            x < self.width && y < self.height,
            "sample ({x}, {y}) is outside the {}x{} render surface",
            self.width,
            self.height
        );
        let i = ((y * self.width + x) * 4) as usize;
        (self.rgba[i], self.rgba[i + 1], self.rgba[i + 2])
    }

    /// How much darker than white the pixel at (x, y) is, averaged over RGB.
    /// Measured on the real composited output, so it accounts for whatever
    /// blending the renderer actually performs.
    pub fn darkening(&self, x: u32, y: u32) -> f32 {
        let (r, g, b) = self.pixel(x, y);
        (3.0 * 255.0 - f32::from(r) - f32::from(g) - f32::from(b)) / 3.0
    }

    /// Rows in `x`'s column that are darker than white by at least `threshold`.
    pub fn darkened_rows(&self, x: u32, threshold: f32, height: u32) -> Vec<u32> {
        (0..height)
            .filter(|y| self.darkening(x, *y) >= threshold)
            .collect()
    }

    /// Pixels painted in a strong, saturated blue — selection borders, resize
    /// handles, and the floating editor's own outline, which are opaque,
    /// unlike the pale overlay tint.
    ///
    /// The threshold is derived from `SELECTION_COLOR` itself (halfway to its
    /// blue/red channel gap) so it tracks the renderer's actual selection
    /// color instead of a magic number that could silently fall out of sync.
    pub fn selection_blue_pixels(&self) -> usize {
        let threshold = Self::selection_blue_threshold();
        self.rgba
            .chunks_exact(4)
            .filter(|p| f32::from(p[2]) - f32::from(p[0]) > threshold)
            .count()
    }

    /// Bounding box of the strongly blue pixels as (left, top, right, bottom),
    /// or None if none exist.
    pub fn selection_blue_bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let threshold = Self::selection_blue_threshold();
        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        for y in 0..self.height {
            for x in 0..self.width {
                let (r, _, b) = self.pixel(x, y);
                if f32::from(b) - f32::from(r) <= threshold {
                    continue;
                }
                bounds = Some(match bounds {
                    None => (x, y, x, y),
                    Some((l, t, rt, bt)) => (l.min(x), t.min(y), rt.max(x), bt.max(y)),
                });
            }
        }
        bounds
    }

    fn selection_blue_threshold() -> f32 {
        let color = crate::ui::canvas::SELECTION_COLOR;
        (color.b - color.r) * 255.0 / 2.0
    }

    /// Number of pixels inside `region` whose colour differs from `other`'s.
    pub fn differing_pixels(&self, other: &Screenshot, region: iced::Rectangle) -> u32 {
        let x0 = region.x.max(0.0) as u32;
        let y0 = region.y.max(0.0) as u32;
        let x1 = (region.x + region.width).min(self.width as f32) as u32;
        let y1 = (region.y + region.height).min(self.height as f32) as u32;
        (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|(x, y)| self.pixel(*x, *y) != other.pixel(*x, *y))
            .count() as u32
    }
}

/// Make the bundled TrueType fonts resolvable by name in headless renders.
/// The app loads them through `iced::font::load` at startup; tests have no
/// runtime to run that task, so they load into the same global font system
/// directly.
pub fn load_bundled_fonts() {
    use std::sync::Once;

    static LOADED: Once = Once::new();
    LOADED.call_once(|| {
        let registry = crate::fonts::FontRegistry::new();
        for entry in registry.all() {
            if let crate::fonts::PdfEmbedding::TrueType { bytes } = &entry.embedding {
                iced_test::renderer::graphics::text::font_system()
                    .write()
                    .expect("font system lock should not be poisoned")
                    .load_font(std::borrow::Cow::Borrowed(*bytes));
            }
        }
    });
}
