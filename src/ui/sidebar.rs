// Thumbnail sidebar: collapsible panel with lazy-loaded page thumbnails.

use std::collections::HashMap;

use iced::mouse;
use iced::widget::canvas;
use iced::widget::image::Handle;

use crate::app::Message;
use crate::overlay::TextOverlay;

/// State for the thumbnail sidebar.
pub struct SidebarState {
    pub visible: bool,
    pub thumbnails: HashMap<u32, Handle>,
    /// Current sidebar width in pixels (user-resizable).
    pub width: f32,
    /// Current scroll position within the sidebar (pixels from top).
    pub scroll_y: f32,
    /// Height of the sidebar viewport in pixels.
    pub viewport_height: f32,
    /// DPI used for thumbnail rendering (derived from width).
    pub thumbnail_dpi: f32,
    /// Whether the user is currently dragging the resize handle.
    pub dragging: bool,
    /// Monotonically increasing counter; incremented on resize to invalidate
    /// stale thumbnail batches in flight.
    pub backfill_generation: u64,
    /// Phase [0, 1) for the shimmer animation on loading placeholders.
    pub shimmer_phase: f32,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            visible: true,
            thumbnails: HashMap::new(),
            width: DEFAULT_SIDEBAR_WIDTH,
            scroll_y: 0.0,
            viewport_height: 0.0,
            thumbnail_dpi: 0.0,
            dragging: false,
            backfill_generation: 0,
            shimmer_phase: 0.0,
        }
    }
}

/// Default width of the sidebar in pixels.
pub const DEFAULT_SIDEBAR_WIDTH: f32 = 120.0;

/// Compute the range of pages that should have thumbnails rendered,
/// based on scroll position, viewport height, and a buffer of extra pages.
///
/// Returns an inclusive range of 1-indexed page numbers.
pub fn visible_pages(
    scroll_offset: f32,
    viewport_height: f32,
    page_count: u32,
    thumbnail_height: f32,
    buffer: u32,
) -> std::ops::RangeInclusive<u32> {
    if page_count == 0 || thumbnail_height <= 0.0 {
        // Return an empty range (start > end)
        #[allow(clippy::reversed_empty_ranges)]
        return 1..=0;
    }
    let first_visible = (scroll_offset / thumbnail_height).floor() as u32;
    let last_visible = ((scroll_offset + viewport_height) / thumbnail_height).ceil() as u32;
    let start = first_visible.saturating_sub(buffer).max(1);
    let end = (last_visible + buffer).min(page_count);
    start..=end
}

/// Horizontal padding subtracted from sidebar width when computing thumbnail render DPI.
const THUMBNAIL_PADDING: f32 = 16.0;

/// Compute the DPI at which thumbnails should be rendered so they fill the
/// usable sidebar width at the given display scale factor.
///
/// - `sidebar_width`: full sidebar width in logical pixels
/// - `scale_factor`: HiDPI multiplier (1.0 for standard, 2.0 for HiDPI)
/// - `max_page_width_pts`: widest page in the document, in PDF points
pub fn compute_thumbnail_dpi(
    sidebar_width: f32,
    scale_factor: f32,
    max_page_width_pts: f32,
) -> f32 {
    let usable_width = (sidebar_width - THUMBNAIL_PADDING).max(1.0);
    let page_width_inches = if max_page_width_pts > 0.0 {
        max_page_width_pts / 72.0
    } else {
        8.5 // fallback to US Letter width
    };
    ((usable_width * scale_factor) / page_width_inches).max(1.0)
}

/// Compute the thumbnail height for a page, maintaining aspect ratio
/// within the given sidebar width.
pub fn thumbnail_height(page_width: f32, page_height: f32, sidebar_width: f32) -> f32 {
    if page_width <= 0.0 {
        return sidebar_width;
    }
    sidebar_width * (page_height / page_width)
}

/// Highlight border color for the current page thumbnail (#4fc3f7).
const CURRENT_PAGE_BORDER_COLOR: iced::Color = iced::Color {
    r: 0.310,
    g: 0.765,
    b: 0.969,
    a: 1.0,
};

/// Width of the current-page highlight border in pixels.
const CURRENT_PAGE_BORDER_WIDTH: f32 = 2.0;

/// Canvas program that draws a single page thumbnail: white background,
/// cached image or gray placeholder, and a highlight border for the current page.
pub struct ThumbnailProgram<'a> {
    pub page: u32,
    pub thumbnail: Option<&'a Handle>,
    pub is_current_page: bool,
    pub overlays: &'a [TextOverlay],
    pub page_width: f32,
    pub page_height: f32,
    pub thumbnail_dpi: f32,
    pub overlay_color: iced::Color,
}

impl<'a> canvas::Program<Message> for ThumbnailProgram<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // White page background
        frame.fill_rectangle(iced::Point::ORIGIN, bounds.size(), iced::Color::WHITE);

        // Draw cached thumbnail or gray placeholder
        if let Some(handle) = self.thumbnail {
            frame.draw_image(
                iced::Rectangle::new(iced::Point::ORIGIN, bounds.size()),
                handle,
            );
        } else {
            frame.fill_rectangle(
                iced::Point::ORIGIN,
                bounds.size(),
                iced::Color::from_rgb(0.85, 0.85, 0.85),
            );
        }

        // Highlight border for current page
        if self.is_current_page {
            let stroke = canvas::Stroke {
                style: canvas::Style::Solid(CURRENT_PAGE_BORDER_COLOR),
                width: CURRENT_PAGE_BORDER_WIDTH,
                ..canvas::Stroke::default()
            };
            let half = CURRENT_PAGE_BORDER_WIDTH / 2.0;
            frame.stroke_rectangle(
                iced::Point::new(half, half),
                iced::Size::new(
                    bounds.width - CURRENT_PAGE_BORDER_WIDTH,
                    bounds.height - CURRENT_PAGE_BORDER_WIDTH,
                ),
                stroke,
            );
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_program_construction() {
        let program = ThumbnailProgram {
            page: 1,
            thumbnail: None,
            is_current_page: false,
            overlays: &[],
            page_width: 612.0,
            page_height: 792.0,
            thumbnail_dpi: 12.0,
            overlay_color: iced::Color::from_rgb(0.0, 0.0, 1.0),
        };
        assert_eq!(program.page, 1);
    }

    #[test]
    fn sidebar_default_is_visible() {
        let state = SidebarState::default();
        assert!(state.visible);
        assert!(state.thumbnails.is_empty());
    }

    #[test]
    fn visible_pages_basic() {
        // Viewport shows 5 pages, buffer of 2
        let range = visible_pages(0.0, 500.0, 20, 100.0, 2);
        assert_eq!(*range.start(), 1);
        assert_eq!(*range.end(), 7); // 5 visible + 2 buffer
    }

    #[test]
    fn visible_pages_scrolled() {
        // Scrolled down 300px → first visible is page 3 (index 3)
        let range = visible_pages(300.0, 500.0, 20, 100.0, 2);
        assert_eq!(*range.start(), 1); // 3 - 2 buffer = 1
        assert_eq!(*range.end(), 10); // ceil((300+500)/100) + 2 = 10
    }

    #[test]
    fn visible_pages_clamps_to_page_count() {
        let range = visible_pages(1800.0, 500.0, 20, 100.0, 2);
        assert_eq!(*range.end(), 20);
    }

    #[test]
    fn visible_pages_start_never_below_one() {
        let range = visible_pages(0.0, 200.0, 5, 100.0, 5);
        assert_eq!(*range.start(), 1);
    }

    #[test]
    fn visible_pages_zero_page_count() {
        let range = visible_pages(0.0, 500.0, 0, 100.0, 2);
        assert!(range.is_empty());
    }

    #[test]
    fn thumbnail_height_letter_page() {
        // US Letter: 612 x 792 points
        let h = thumbnail_height(612.0, 792.0, DEFAULT_SIDEBAR_WIDTH);
        // Expected: 120 * (792/612) ≈ 155.3
        assert!((h - 155.29).abs() < 0.1);
    }

    #[test]
    fn thumbnail_height_landscape() {
        // Landscape: 792 x 612
        let h = thumbnail_height(792.0, 612.0, DEFAULT_SIDEBAR_WIDTH);
        // Expected: 120 * (612/792) ≈ 92.7
        assert!((h - 92.73).abs() < 0.1);
    }

    #[test]
    fn thumbnail_height_zero_width_fallback() {
        let h = thumbnail_height(0.0, 500.0, DEFAULT_SIDEBAR_WIDTH);
        assert!((h - DEFAULT_SIDEBAR_WIDTH).abs() < f32::EPSILON);
    }

    #[test]
    fn sidebar_width_constant() {
        assert!((DEFAULT_SIDEBAR_WIDTH - 120.0).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_thumbnail_dpi_standard_letter() {
        // US Letter 8.5" wide, sidebar width, 1x scale
        // usable_width = sidebar_width - THUMBNAIL_PADDING
        // DPI = (usable_width * scale) / (page_width_pts / 72)
        let dpi = compute_thumbnail_dpi(120.0, 1.0, 612.0);
        // usable = 120 - 16 = 104, page_inches = 612/72 = 8.5
        // dpi = 104 / 8.5 ≈ 12.24
        assert!((dpi - 12.24).abs() < 0.1);
    }

    #[test]
    fn compute_thumbnail_dpi_hidpi() {
        let dpi = compute_thumbnail_dpi(120.0, 2.0, 612.0);
        // usable = 104, dpi = (104 * 2) / 8.5 ≈ 24.47
        assert!((dpi - 24.47).abs() < 0.1);
    }

    #[test]
    fn compute_thumbnail_dpi_wider_sidebar() {
        let dpi = compute_thumbnail_dpi(200.0, 1.0, 612.0);
        // usable = 200 - 16 = 184, dpi = 184 / 8.5 ≈ 21.65
        assert!((dpi - 21.65).abs() < 0.1);
    }

    #[test]
    fn compute_thumbnail_dpi_zero_page_width_returns_minimum() {
        let dpi = compute_thumbnail_dpi(120.0, 1.0, 0.0);
        assert!(dpi > 0.0);
    }

    #[test]
    fn sidebar_default_has_new_fields() {
        let state = SidebarState::default();
        assert!((state.width - DEFAULT_SIDEBAR_WIDTH).abs() < f32::EPSILON);
        assert!((state.scroll_y - 0.0).abs() < f32::EPSILON);
        assert!((state.viewport_height - 0.0).abs() < f32::EPSILON);
        assert!((state.thumbnail_dpi - 0.0).abs() < f32::EPSILON);
        assert!(!state.dragging);
        assert_eq!(state.backfill_generation, 0);
        assert!((state.shimmer_phase - 0.0).abs() < f32::EPSILON);
    }
}
