// Page layout, geometry, and hit-testing for the PDF canvas.

use iced::widget::image::Handle;

use crate::coordinate::{ConversionParams, overlay_bounding_box, pdf_to_screen, render_scale};
use crate::overlay::TextOverlay;

/// Gap between pages in continuous scrolling mode (pixels).
pub const PAGE_GAP: f32 = 16.0;

/// Layout of all pages stacked vertically for continuous scrolling.
#[derive(Clone)]
pub struct PageLayout {
    /// Y-offset of each page's top edge in canvas space. Index 0 = page 1.
    pub page_tops: Vec<f32>,
    /// Rendered height of each page. Index 0 = page 1.
    pub page_heights: Vec<f32>,
    /// Rendered width of each page. Index 0 = page 1.
    pub page_widths: Vec<f32>,
    /// Total canvas height (all pages + gaps + top/bottom margins).
    pub total_height: f32,
    /// Maximum rendered page width.
    pub max_width: f32,
}

/// Compute the vertical layout of all pages at the current zoom/DPI.
pub fn page_layout(
    page_dimensions: &std::collections::HashMap<u32, (f32, f32)>,
    page_count: u32,
    zoom: f32,
    dpi: f32,
) -> PageLayout {
    let scale = render_scale(zoom, dpi);
    let mut page_tops = Vec::with_capacity(page_count as usize);
    let mut page_heights = Vec::with_capacity(page_count as usize);
    let mut page_widths = Vec::with_capacity(page_count as usize);
    let mut y = PAGE_GAP / 2.0;
    let mut max_width: f32 = 0.0;

    for page in 1..=page_count {
        let (w, h) = page_dimensions
            .get(&page)
            .copied()
            .unwrap_or((612.0, 792.0));
        let rendered_w = w * scale;
        let rendered_h = h * scale;
        page_tops.push(y);
        page_widths.push(rendered_w);
        page_heights.push(rendered_h);
        max_width = max_width.max(rendered_w);
        y += rendered_h + PAGE_GAP;
    }

    let total_height = y - PAGE_GAP / 2.0;

    PageLayout {
        page_tops,
        page_heights,
        page_widths,
        total_height,
        max_width,
    }
}

/// Return the 1-indexed page number at canvas y-coordinate, or None if in a gap or past end.
pub fn page_at_y(layout: &PageLayout, y: f32) -> Option<u32> {
    for (i, &top) in layout.page_tops.iter().enumerate() {
        let bottom = top + layout.page_heights[i];
        if y >= top && y < bottom {
            return Some(i as u32 + 1);
        }
        if y < top {
            return None; // in gap before this page
        }
    }
    None
}

/// Return (first, last) 1-indexed page range whose rendered area overlaps the viewport.
pub fn visible_pages(layout: &PageLayout, scroll_y: f32, viewport_h: f32) -> (u32, u32) {
    let view_top = scroll_y;
    let view_bottom = scroll_y + viewport_h;
    let page_count = layout.page_tops.len() as u32;

    let mut first = page_count;
    let mut last = 1;

    for (i, &top) in layout.page_tops.iter().enumerate() {
        let bottom = top + layout.page_heights[i];
        if bottom > view_top && top < view_bottom {
            let page = i as u32 + 1;
            first = first.min(page);
            last = last.max(page);
        }
    }

    if first > last {
        (1, 1)
    } else {
        (first.max(1), last.min(page_count))
    }
}

/// Return the 1-indexed page that occupies the most vertical space in the viewport.
pub fn dominant_page(layout: &PageLayout, scroll_y: f32, viewport_h: f32) -> u32 {
    let view_top = scroll_y;
    let view_bottom = scroll_y + viewport_h;
    let mut best_page = 1u32;
    let mut best_overlap = 0.0f32;

    for (i, &top) in layout.page_tops.iter().enumerate() {
        let bottom = top + layout.page_heights[i];
        let overlap_top = top.max(view_top);
        let overlap_bottom = bottom.min(view_bottom);
        let overlap = (overlap_bottom - overlap_top).max(0.0);
        if overlap > best_overlap {
            best_overlap = overlap;
            best_page = i as u32 + 1;
        }
    }

    best_page
}

/// Compute the Rectangle for a page in canvas coordinates (for drawing).
pub fn page_rect_in_canvas(layout: &PageLayout, page: u32, canvas_width: f32) -> iced::Rectangle {
    let idx = (page - 1) as usize;
    let w = layout.page_widths[idx];
    let h = layout.page_heights[idx];
    let x = (canvas_width - w) / 2.0;
    let y = layout.page_tops[idx];
    iced::Rectangle {
        x: x.max(0.0),
        y,
        width: w,
        height: h,
    }
}

/// Compute the Rectangle where the PDF page image should be drawn, centered within the canvas.
pub fn page_image_bounds(
    page_dims: (f32, f32),
    zoom: f32,
    dpi: f32,
    canvas_bounds: iced::Rectangle,
) -> iced::Rectangle {
    let scale = render_scale(zoom, dpi);
    let rendered_width = page_dims.0 * scale;
    let rendered_height = page_dims.1 * scale;
    let offset_x = (canvas_bounds.width - rendered_width) / 2.0;
    let offset_y = (canvas_bounds.height - rendered_height) / 2.0;
    iced::Rectangle {
        x: canvas_bounds.x + offset_x,
        y: canvas_bounds.y + offset_y,
        width: rendered_width,
        height: rendered_height,
    }
}

/// Convert a decoded image to an Iced image Handle.
pub fn image_to_handle(img: image::DynamicImage) -> Handle {
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Handle::from_rgba(width, height, rgba.into_raw())
}

/// Test whether a screen-space click hits any overlay on the current page.
/// Returns the index of the topmost (last-placed) overlay hit, or None.
pub fn hit_test(
    screen_x: f32,
    screen_y: f32,
    overlays: &[TextOverlay],
    current_page: u32,
    params: &ConversionParams,
) -> Option<usize> {
    // Test in reverse order so topmost (last-placed) overlay wins
    for (i, overlay) in overlays.iter().enumerate().rev() {
        if overlay.page != current_page {
            continue;
        }
        let (sx, sy) = pdf_to_screen(overlay.position.x, overlay.position.y, params);
        let bbox = overlay_bounding_box(&overlay.text, overlay.font, overlay.font_size);
        let scale = params.scale();
        let w = bbox.width * scale;
        let h = bbox.height * scale;
        // In screen space, the overlay baseline is at sy.
        // Text extends upward from baseline, so the hit box is [sy - h, sy].
        if screen_x >= sx && screen_x <= sx + w && screen_y >= sy - h && screen_y <= sy {
            return Some(i);
        }
    }
    None
}
