// Headless rendering harness shared by the tests that assert on pixels.
//
// Geometry that only exists once a widget is laid out — where a border lands,
// how tall an editor grew — is checked by rendering it and reading the
// surface, so the assertions describe what a user would see rather than what
// the code intended.

use iced::mouse;

/// Surface the harness renders onto. Large enough to hold a US Letter page at
/// zoom 1 with room around it.
pub(crate) const RENDER_SIZE: iced::Size = iced::Size {
    width: 900.0,
    height: 700.0,
};

/// A headlessly rendered frame, as RGBA pixels over a white background.
pub(crate) struct RenderedCanvas {
    pub(crate) width: u32,
    pub(crate) rgba: Vec<u8>,
}

impl RenderedCanvas {
    pub(crate) fn height(&self) -> u32 {
        (self.rgba.len() as u32 / 4) / self.width
    }

    pub(crate) fn pixel(&self, x: u32, y: u32) -> (u8, u8, u8) {
        assert!(
            x < self.width && y < self.height(),
            "sample ({x}, {y}) is outside the {}x{} render surface",
            self.width,
            self.height()
        );
        let i = ((y * self.width + x) * 4) as usize;
        (self.rgba[i], self.rgba[i + 1], self.rgba[i + 2])
    }

    /// How much darker than white the pixel at (x, y) is, averaged over RGB.
    /// Measured on the real composited output, so it accounts for whatever
    /// blending the renderer actually performs.
    pub(crate) fn darkening(&self, x: u32, y: u32) -> f32 {
        let (r, g, b) = self.pixel(x, y);
        (3.0 * 255.0 - r as f32 - g as f32 - b as f32) / 3.0
    }

    /// Rows in `x`'s column that are darker than white by at least `threshold`.
    pub(crate) fn darkened_rows(&self, x: u32, threshold: f32, height: u32) -> Vec<u32> {
        (0..height)
            .filter(|y| self.darkening(x, *y) >= threshold)
            .collect()
    }
}

/// Render a widget over a white background, headlessly.
///
/// When `cursor` is given it is delivered as a real cursor-moved event first,
/// so hover state is established through the same path the app uses.
pub(crate) fn render_element(
    element: iced::Element<'_, crate::app::Message>,
    cursor: Option<iced::Point>,
) -> RenderedCanvas {
    use iced_test::core::renderer::Headless;
    use iced_test::runtime::{UserInterface, user_interface};

    let mut renderer = iced_test::futures::futures::executor::block_on(
        // Pinned to the software backend so the rendered pixels these tests
        // assert on do not depend on whether a GPU is present.
        <iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ),
    )
    .expect("software renderer should be available without a GPU or display");

    let mut ui = UserInterface::build(
        element,
        RENDER_SIZE,
        user_interface::Cache::default(),
        &mut renderer,
    );

    let pointer = match cursor {
        Some(position) => mouse::Cursor::Available(position),
        None => mouse::Cursor::Unavailable,
    };
    if let Some(position) = cursor {
        let _ = ui.update(
            &[iced::Event::Mouse(mouse::Event::CursorMoved { position })],
            pointer,
            &mut renderer,
            &mut iced_test::core::clipboard::Null,
            &mut Vec::new(),
        );
    }

    ui.draw(
        &mut renderer,
        &iced::Theme::Light,
        &iced_test::core::renderer::Style {
            text_color: iced::Color::BLACK,
        },
        pointer,
    );

    let width = RENDER_SIZE.width as u32;
    let height = RENDER_SIZE.height as u32;
    let rgba = renderer.screenshot(iced::Size::new(width, height), 1.0, iced::Color::WHITE);
    RenderedCanvas { width, rgba }
}
