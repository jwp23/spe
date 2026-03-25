// Page rendering program — draws page backgrounds and images without event handling.

use std::collections::HashMap;

use iced::mouse;
use iced::widget::canvas;
use iced::widget::image::Handle;

use super::{CANVAS_BACKGROUND, PageLayout, page_rect_in_canvas, visible_pages};

/// Canvas program that renders page backgrounds and images.
/// No event handling (update returns None, mouse_interaction returns default).
pub struct PdfPagesProgram<'a> {
    pub page_images: &'a HashMap<u32, Handle>,
    pub page_layout: PageLayout,
    pub page_dimensions: &'a HashMap<u32, (f32, f32)>,
    pub page_count: u32,
    pub scroll_y: f32,
    pub viewport_height: f32,
    pub zoom: f32,
    pub dpi: f32,
}

impl<'a> canvas::Program<crate::app::Message> for PdfPagesProgram<'a> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        _event: &canvas::Event,
        _bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<canvas::Action<crate::app::Message>> {
        None
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // Gray background
        frame.fill_rectangle(iced::Point::ORIGIN, bounds.size(), CANVAS_BACKGROUND);

        if self.page_layout.page_tops.is_empty() {
            return vec![frame.into_geometry()];
        }

        // Determine visible pages
        let (first, last) = visible_pages(&self.page_layout, self.scroll_y, self.viewport_height);

        // Draw each visible page
        for page in first..=last {
            let page_rect = page_rect_in_canvas(&self.page_layout, page, bounds.width);

            // Draw white page background
            frame.fill_rectangle(
                iced::Point::new(page_rect.x, page_rect.y),
                iced::Size::new(page_rect.width, page_rect.height),
                iced::Color::WHITE,
            );

            // Draw page image if cached
            if let Some(handle) = self.page_images.get(&page) {
                frame.draw_image(page_rect, handle);
            }
        }

        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_program_can_be_constructed() {
        let images = HashMap::new();
        let dims = HashMap::from([(1, (612.0, 792.0))]);
        let layout = super::super::page_layout(&dims, 1, 1.0, 72.0);
        let _program = PdfPagesProgram {
            page_images: &images,
            page_layout: layout,
            page_dimensions: &dims,
            page_count: 1,
            scroll_y: 0.0,
            viewport_height: 800.0,
            zoom: 1.0,
            dpi: 72.0,
        };
    }
}
