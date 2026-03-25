use super::*;
use crate::coordinate::ConversionParams;
use crate::overlay::{PdfPosition, Standard14Font, TextOverlay};
use iced::event;
use std::collections::HashMap;

// --- PageLayout tests ---

fn uniform_page_dims(count: u32) -> HashMap<u32, (f32, f32)> {
    (1..=count).map(|p| (p, (612.0, 792.0))).collect()
}

#[test]
fn page_layout_single_page() {
    let dims = uniform_page_dims(1);
    let layout = page_layout(&dims, 1, 1.0, 72.0);
    assert_eq!(layout.page_tops.len(), 1);
    assert_eq!(layout.page_heights.len(), 1);
    // At zoom=1, dpi=72: scale=1.0, rendered=612x792
    assert!((layout.page_widths[0] - 612.0).abs() < 0.1);
    assert!((layout.page_heights[0] - 792.0).abs() < 0.1);
    // First page starts at PAGE_GAP/2
    assert!((layout.page_tops[0] - PAGE_GAP / 2.0).abs() < 0.1);
    // Total height = GAP/2 + 792 + GAP/2 = 792 + GAP
    assert!((layout.total_height - (792.0 + PAGE_GAP)).abs() < 0.1);
    assert!((layout.max_width - 612.0).abs() < 0.1);
}

#[test]
fn page_layout_two_uniform_pages() {
    let dims = uniform_page_dims(2);
    let layout = page_layout(&dims, 2, 1.0, 72.0);
    assert_eq!(layout.page_tops.len(), 2);
    // Page 1 starts at GAP/2
    assert!((layout.page_tops[0] - PAGE_GAP / 2.0).abs() < 0.1);
    // Page 2 starts at GAP/2 + 792 + GAP
    let expected_top2 = PAGE_GAP / 2.0 + 792.0 + PAGE_GAP;
    assert!((layout.page_tops[1] - expected_top2).abs() < 0.1);
    // Total = GAP/2 + 792 + GAP + 792 + GAP/2 = 2*792 + 2*GAP
    let expected_total = 2.0 * 792.0 + 2.0 * PAGE_GAP;
    assert!((layout.total_height - expected_total).abs() < 0.1);
}

#[test]
fn page_layout_mixed_page_sizes() {
    let mut dims = HashMap::new();
    dims.insert(1, (612.0, 792.0)); // Letter
    dims.insert(2, (842.0, 595.0)); // A4 landscape
    let layout = page_layout(&dims, 2, 1.0, 72.0);
    assert!((layout.page_widths[0] - 612.0).abs() < 0.1);
    assert!((layout.page_widths[1] - 842.0).abs() < 0.1);
    assert!((layout.page_heights[0] - 792.0).abs() < 0.1);
    assert!((layout.page_heights[1] - 595.0).abs() < 0.1);
    assert!((layout.max_width - 842.0).abs() < 0.1);
}

#[test]
fn page_layout_respects_zoom_and_dpi() {
    let dims = uniform_page_dims(1);
    // zoom=2.0, dpi=150 → scale = 2*150/72 ≈ 4.167
    let layout = page_layout(&dims, 1, 2.0, 150.0);
    let scale = 2.0 * 150.0 / 72.0;
    assert!((layout.page_widths[0] - 612.0 * scale).abs() < 0.1);
    assert!((layout.page_heights[0] - 792.0 * scale).abs() < 0.1);
}

// --- page_at_y tests ---

#[test]
fn page_at_y_hits_first_page() {
    let dims = uniform_page_dims(3);
    let layout = page_layout(&dims, 3, 1.0, 72.0);
    // Middle of first page
    let y = layout.page_tops[0] + layout.page_heights[0] / 2.0;
    assert_eq!(page_at_y(&layout, y), Some(1));
}

#[test]
fn page_at_y_hits_second_page() {
    let dims = uniform_page_dims(3);
    let layout = page_layout(&dims, 3, 1.0, 72.0);
    let y = layout.page_tops[1] + 10.0;
    assert_eq!(page_at_y(&layout, y), Some(2));
}

#[test]
fn page_at_y_in_gap_returns_none() {
    let dims = uniform_page_dims(2);
    let layout = page_layout(&dims, 2, 1.0, 72.0);
    // Gap is between page_tops[0]+page_heights[0] and page_tops[1]
    let gap_y = layout.page_tops[0] + layout.page_heights[0] + PAGE_GAP / 2.0;
    assert!(page_at_y(&layout, gap_y).is_none());
}

#[test]
fn page_at_y_before_first_page_returns_none() {
    let dims = uniform_page_dims(1);
    let layout = page_layout(&dims, 1, 1.0, 72.0);
    assert!(page_at_y(&layout, 0.0).is_none());
}

#[test]
fn page_at_y_past_last_page_returns_none() {
    let dims = uniform_page_dims(1);
    let layout = page_layout(&dims, 1, 1.0, 72.0);
    assert!(page_at_y(&layout, layout.total_height + 100.0).is_none());
}

// --- visible_pages tests ---

#[test]
fn visible_pages_all_visible() {
    let dims = uniform_page_dims(3);
    let layout = page_layout(&dims, 3, 1.0, 72.0);
    // Viewport tall enough to see everything
    let (first, last) = visible_pages(&layout, 0.0, layout.total_height + 100.0);
    assert_eq!(first, 1);
    assert_eq!(last, 3);
}

#[test]
fn visible_pages_first_page_only() {
    let dims = uniform_page_dims(3);
    let layout = page_layout(&dims, 3, 1.0, 72.0);
    // Viewport covers only first page
    let (first, last) = visible_pages(&layout, 0.0, layout.page_tops[0] + layout.page_heights[0]);
    assert_eq!(first, 1);
    assert_eq!(last, 1);
}

#[test]
fn visible_pages_at_boundary() {
    let dims = uniform_page_dims(3);
    let layout = page_layout(&dims, 3, 1.0, 72.0);
    // Scroll to where page 1 bottom and page 2 top are both visible
    let scroll_y = layout.page_tops[0] + layout.page_heights[0] - 50.0;
    let (first, last) = visible_pages(&layout, scroll_y, 100.0);
    assert_eq!(first, 1);
    assert_eq!(last, 2);
}

#[test]
fn visible_pages_last_page_only() {
    let dims = uniform_page_dims(3);
    let layout = page_layout(&dims, 3, 1.0, 72.0);
    let scroll_y = layout.page_tops[2];
    let (first, last) = visible_pages(&layout, scroll_y, 800.0);
    assert_eq!(first, 3);
    assert_eq!(last, 3);
}

// --- dominant_page tests ---

#[test]
fn dominant_page_first_page_at_top() {
    let dims = uniform_page_dims(3);
    let layout = page_layout(&dims, 3, 1.0, 72.0);
    assert_eq!(dominant_page(&layout, 0.0, 800.0), 1);
}

#[test]
fn dominant_page_at_boundary_picks_more_visible() {
    let dims = uniform_page_dims(2);
    let layout = page_layout(&dims, 2, 1.0, 72.0);
    // Scroll so page 2 has more visible area than page 1
    let scroll_y = layout.page_tops[1] - 100.0;
    let result = dominant_page(&layout, scroll_y, 800.0);
    assert_eq!(result, 2);
}

#[test]
fn dominant_page_fully_on_page_2() {
    let dims = uniform_page_dims(3);
    let layout = page_layout(&dims, 3, 1.0, 72.0);
    let scroll_y = layout.page_tops[1] + 10.0;
    assert_eq!(dominant_page(&layout, scroll_y, 200.0), 2);
}

// --- page_rect_in_canvas tests ---

#[test]
fn page_rect_in_canvas_centers_horizontally() {
    let dims = uniform_page_dims(1);
    let layout = page_layout(&dims, 1, 1.0, 72.0);
    let rect = page_rect_in_canvas(&layout, 1, 1000.0);
    // Page is 612px wide in 1000px canvas → x = (1000-612)/2 = 194
    assert!((rect.x - 194.0).abs() < 0.1);
    assert!((rect.y - PAGE_GAP / 2.0).abs() < 0.1);
    assert!((rect.width - 612.0).abs() < 0.1);
    assert!((rect.height - 792.0).abs() < 0.1);
}

#[test]
fn page_rect_in_canvas_second_page_position() {
    let dims = uniform_page_dims(2);
    let layout = page_layout(&dims, 2, 1.0, 72.0);
    let rect = page_rect_in_canvas(&layout, 2, 1000.0);
    let expected_y = PAGE_GAP / 2.0 + 792.0 + PAGE_GAP;
    assert!((rect.y - expected_y).abs() < 0.1);
}
use iced::widget::canvas::Program;

fn default_params() -> ConversionParams {
    ConversionParams {
        zoom: 1.0,
        dpi: 72.0,
        page_height: 792.0,
        offset_x: 0.0,
        offset_y: 0.0,
    }
}

fn overlay_at(x: f32, y: f32, text: &str) -> TextOverlay {
    TextOverlay {
        page: 1,
        position: PdfPosition { x, y },
        text: text.to_string(),
        font: Standard14Font::Courier,
        font_size: 12.0,
        width: None,
    }
}

/// Canvas bounds used in event handling tests: 1000x1000 starting at origin.
fn test_canvas_bounds() -> iced::Rectangle {
    iced::Rectangle {
        x: 0.0,
        y: 0.0,
        width: 1000.0,
        height: 1000.0,
    }
}

/// US Letter page at zoom=1, dpi=72 produces a 612x792 image.
/// Centered in 1000x1000 canvas: offset_x=194, offset_y=104.
const TEST_PAGE_DIMS: (f32, f32) = (612.0, 792.0);
const TEST_ZOOM: f32 = 1.0;
const TEST_DPI: f32 = 72.0;

fn test_page_images() -> HashMap<u32, Handle> {
    HashMap::new()
}

fn test_page_dimensions() -> HashMap<u32, (f32, f32)> {
    let mut dims = HashMap::new();
    dims.insert(1, TEST_PAGE_DIMS);
    dims
}

/// Build a PdfCanvasProgram for event handling tests (single page).
fn test_program<'a>(
    overlays: &'a [TextOverlay],
    page_images: &'a HashMap<u32, Handle>,
    page_dims: &'a HashMap<u32, (f32, f32)>,
) -> PdfCanvasProgram<'a> {
    let layout = page_layout(page_dims, 1, TEST_ZOOM, TEST_DPI);
    PdfCanvasProgram {
        page_images,
        page_layout: layout,
        page_dimensions: page_dims,
        page_count: 1,
        scroll_y: 0.0,
        viewport_height: 1000.0,
        overlays,
        zoom: TEST_ZOOM,
        dpi: TEST_DPI,
        active_overlay: None,
        editing: false,
        overlay_color: [0.0, 0.0, 1.0, 1.0],
    }
}

fn left_press_event() -> canvas::Event {
    canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
}

fn left_release_event() -> canvas::Event {
    canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
}

fn cursor_moved_event(x: f32, y: f32) -> canvas::Event {
    canvas::Event::Mouse(mouse::Event::CursorMoved {
        position: iced::Point::new(x, y),
    })
}

fn cursor_at(x: f32, y: f32) -> mouse::Cursor {
    mouse::Cursor::Available(iced::Point::new(x, y))
}

/// Decompose an update() result into (message, event_status) for assertions.
fn decompose(action: Option<canvas::Action<Message>>) -> (Option<Message>, event::Status) {
    match action {
        Some(a) => {
            let (msg, _redraw, status) = a.into_inner();
            (msg, status)
        }
        None => (None, event::Status::Ignored),
    }
}

#[test]
fn default_canvas_state() {
    let state = CanvasState::default();
    assert!((state.zoom - 1.0).abs() < f32::EPSILON);
    assert!(state.active_overlay.is_none());
    assert!(!state.editing);
    assert_eq!(state.zoom_generation, 0);
}

// --- ProgramState tests ---

#[test]
fn program_state_default_has_no_cursor_or_drag() {
    let state = ProgramState::default();
    assert!(state.cursor_position.is_none());
    assert!(state.drag.is_none());
    assert!(state.placement_drag.is_none());
}

// --- PlacementDragState tests ---

#[test]
fn placement_drag_state_construction() {
    let state = PlacementDragState {
        start_screen: iced::Point::new(100.0, 200.0),
        page: 1,
        page_screen_rect: iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(612.0, 792.0)),
    };
    assert_eq!(state.page, 1);
    assert!((state.start_screen.x - 100.0).abs() < f32::EPSILON);
    assert!((state.start_screen.y - 200.0).abs() < f32::EPSILON);
}

// --- LocalDragState tests ---

#[test]
fn local_drag_state_construction() {
    let drag = LocalDragState {
        overlay_index: 3,
        initial_pdf_position: PdfPosition { x: 100.0, y: 500.0 },
        grab_offset_x: 5.0,
        grab_offset_y: 10.0,
    };
    assert_eq!(drag.overlay_index, 3);
    assert!((drag.initial_pdf_position.x - 100.0).abs() < f32::EPSILON);
    assert!((drag.initial_pdf_position.y - 500.0).abs() < f32::EPSILON);
    assert!((drag.grab_offset_x - 5.0).abs() < f32::EPSILON);
    assert!((drag.grab_offset_y - 10.0).abs() < f32::EPSILON);
}

// --- PdfCanvasProgram tests ---

#[test]
fn pdf_canvas_program_construction_with_no_document() {
    let overlays: Vec<TextOverlay> = vec![];
    let empty_imgs: HashMap<u32, Handle> = HashMap::new();
    let empty_dims: HashMap<u32, (f32, f32)> = HashMap::new();
    let layout = page_layout(&empty_dims, 0, 1.0, 150.0);
    let program = PdfCanvasProgram {
        page_images: &empty_imgs,
        page_layout: layout,
        page_dimensions: &empty_dims,
        page_count: 0,
        scroll_y: 0.0,
        viewport_height: 1000.0,
        overlays: &overlays,
        zoom: 1.0,
        dpi: 150.0,
        active_overlay: None,
        editing: false,
        overlay_color: [0.0, 0.0, 1.0, 1.0],
    };
    assert!(program.page_images.is_empty());
    assert!(program.page_dimensions.is_empty());
    assert_eq!(program.overlays.len(), 0);
}

#[test]
fn pdf_canvas_program_construction_with_document() {
    let mut page_images = HashMap::new();
    let handle = iced::widget::image::Handle::from_rgba(1, 1, vec![0u8; 4]);
    page_images.insert(1u32, handle);
    let overlays = vec![overlay_at(72.0, 720.0, "Test")];
    let dims = test_page_dimensions();
    let layout = page_layout(&dims, 1, 1.5, 150.0);
    let program = PdfCanvasProgram {
        page_images: &page_images,
        page_layout: layout,
        page_dimensions: &dims,
        page_count: 1,
        scroll_y: 0.0,
        viewport_height: 1000.0,
        overlays: &overlays,
        zoom: 1.5,
        dpi: 150.0,
        active_overlay: Some(0),
        editing: true,
        overlay_color: [0.26, 0.53, 0.96, 1.0],
    };
    assert_eq!(program.page_images.len(), 1);
    assert!(program.page_dimensions.contains_key(&1));
    assert_eq!(program.overlays.len(), 1);
    assert_eq!(program.page_count, 1);
    assert!((program.zoom - 1.5).abs() < f32::EPSILON);
    assert!(program.editing);
}

// --- page_image_bounds tests ---

#[test]
fn page_image_bounds_centers_within_canvas() {
    // US Letter at zoom=1.0, dpi=72 → 612x792 pixels
    // Canvas is 1000x1000
    let bounds = page_image_bounds(
        (612.0, 792.0),
        1.0,
        72.0,
        iced::Rectangle {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 1000.0,
        },
    );
    // Image is 612x792, centered in 1000x1000
    assert!((bounds.width - 612.0).abs() < 0.1);
    assert!((bounds.height - 792.0).abs() < 0.1);
    // Centered horizontally: (1000 - 612) / 2 = 194
    assert!((bounds.x - 194.0).abs() < 0.1);
    // Centered vertically: (1000 - 792) / 2 = 104
    assert!((bounds.y - 104.0).abs() < 0.1);
}

#[test]
fn page_image_bounds_scales_with_zoom() {
    // US Letter at zoom=2.0, dpi=72 → 1224x1584 pixels
    let bounds = page_image_bounds(
        (612.0, 792.0),
        2.0,
        72.0,
        iced::Rectangle {
            x: 0.0,
            y: 0.0,
            width: 2000.0,
            height: 2000.0,
        },
    );
    assert!((bounds.width - 1224.0).abs() < 0.1);
    assert!((bounds.height - 1584.0).abs() < 0.1);
}

#[test]
fn page_image_bounds_scales_with_dpi() {
    // US Letter at zoom=1.0, dpi=150 → 612*150/72 = 1275 wide, 792*150/72 = 1650 tall
    let bounds = page_image_bounds(
        (612.0, 792.0),
        1.0,
        150.0,
        iced::Rectangle {
            x: 0.0,
            y: 0.0,
            width: 2000.0,
            height: 2000.0,
        },
    );
    assert!((bounds.width - 1275.0).abs() < 0.1);
    assert!((bounds.height - 1650.0).abs() < 0.1);
}

#[test]
fn page_image_bounds_accounts_for_canvas_offset() {
    // Canvas bounds start at (50, 30)
    let bounds = page_image_bounds(
        (612.0, 792.0),
        1.0,
        72.0,
        iced::Rectangle {
            x: 50.0,
            y: 30.0,
            width: 1000.0,
            height: 1000.0,
        },
    );
    // Image is 612x792, centered in 1000x1000 starting at (50, 30)
    assert!((bounds.x - (50.0 + 194.0)).abs() < 0.1);
    assert!((bounds.y - (30.0 + 104.0)).abs() < 0.1);
}

// --- image_to_handle tests ---

#[test]
fn image_to_handle_converts_rgba_image() {
    // Create a 2x2 red image
    let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        2,
        image::Rgba([255, 0, 0, 255]),
    ));
    let _handle = image_to_handle(img);
    // If we get here without panic, conversion succeeded
}

#[test]
fn image_to_handle_converts_rgb_image() {
    // Create an RGB image (no alpha channel) — should still convert
    let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        3,
        3,
        image::Rgb([0, 128, 255]),
    ));
    let _handle = image_to_handle(img);
}

#[test]
fn hit_test_returns_none_for_empty_overlays() {
    let params = default_params();
    assert!(hit_test(100.0, 100.0, &[], 1, &params).is_none());
}

#[test]
fn hit_test_finds_overlay_at_position() {
    let params = default_params();
    // Courier at 12pt: each char is 600/1000 * 12 = 7.2 px wide
    // "Hello" = 5 * 7.2 = 36px wide, 12px tall
    // Overlay at PDF (72, 720) → screen (72, 72) at zoom=1, dpi=72
    // Hit box: x=[72, 108], y=[60, 72]
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let result = hit_test(80.0, 65.0, &overlays, 1, &params);
    assert_eq!(result, Some(0));
}

#[test]
fn hit_test_returns_none_for_miss() {
    let params = default_params();
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    // Click far away from overlay
    let result = hit_test(500.0, 500.0, &overlays, 1, &params);
    assert!(result.is_none());
}

#[test]
fn hit_test_returns_topmost_for_overlapping() {
    let params = default_params();
    let overlays = vec![
        overlay_at(72.0, 720.0, "First"),
        overlay_at(72.0, 720.0, "Second"),
    ];
    // Both at same position, should return index 1 (topmost/last)
    let result = hit_test(80.0, 65.0, &overlays, 1, &params);
    assert_eq!(result, Some(1));
}

#[test]
fn hit_test_ignores_overlays_on_other_pages() {
    let params = default_params();
    let overlays = vec![TextOverlay {
        page: 2,
        position: PdfPosition { x: 72.0, y: 720.0 },
        text: "On page 2".to_string(),
        font: Standard14Font::Courier,
        font_size: 12.0,
        width: None,
    }];
    let result = hit_test(80.0, 65.0, &overlays, 1, &params);
    assert!(result.is_none());
}

#[test]
fn zoom_in_steps_up() {
    assert!((zoom_in(1.0) - 1.25).abs() < 0.01);
    assert!((zoom_in(0.5) - 0.75).abs() < 0.01);
}

#[test]
fn zoom_out_steps_down() {
    assert!((zoom_out(1.0) - 0.75).abs() < 0.01);
    assert!((zoom_out(0.5) - 0.25).abs() < 0.01);
}

#[test]
fn zoom_in_caps_at_max() {
    assert!((zoom_in(2.0) - 2.0).abs() < 0.01);
}

#[test]
fn zoom_out_caps_at_min() {
    assert!((zoom_out(0.25) - 0.25).abs() < 0.01);
}

#[test]
fn zoom_in_from_continuous_value() {
    // From a fit-to-width zoom of 0.885, zoom_in should jump to 1.0
    assert!((zoom_in(0.885) - 1.0).abs() < 0.01);
}

#[test]
fn zoom_out_from_continuous_value() {
    // From a fit-to-width zoom of 0.885, zoom_out should jump to 0.75
    assert!((zoom_out(0.885) - 0.75).abs() < 0.01);
}

#[test]
fn zoom_percent_continuous_value() {
    // 0.885 → 89%
    assert_eq!(zoom_percent(0.885), 89);
}

#[test]
fn zoom_percent_at_default() {
    assert_eq!(zoom_percent(1.0), 100);
}

#[test]
fn zoom_percent_at_150() {
    assert_eq!(zoom_percent(1.5), 150);
}

#[test]
fn effective_dpi_at_default_zoom() {
    assert!((effective_dpi(1.0) - 150.0).abs() < 0.01);
}

#[test]
fn effective_dpi_at_double_zoom() {
    assert!((effective_dpi(2.0) - 300.0).abs() < 0.01);
}

// =====================================================================
// update() tests — event handling
// =====================================================================

#[test]
fn update_ignores_click_when_no_pages() {
    let overlays: Vec<TextOverlay> = vec![];
    let empty_imgs: HashMap<u32, Handle> = HashMap::new();
    let empty_dims: HashMap<u32, (f32, f32)> = HashMap::new();
    let layout = page_layout(&empty_dims, 0, TEST_ZOOM, TEST_DPI);
    let program = PdfCanvasProgram {
        page_images: &empty_imgs,
        page_layout: layout,
        page_dimensions: &empty_dims,
        page_count: 0,
        scroll_y: 0.0,
        viewport_height: 1000.0,
        overlays: &overlays,
        zoom: TEST_ZOOM,
        dpi: TEST_DPI,
        active_overlay: None,
        editing: false,
        overlay_color: [0.0, 0.0, 1.0, 1.0],
    };
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(500.0, 500.0);

    let action = program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    // Click in gap/outside pages → deselect
    assert!(matches!(msg, Some(Message::DeselectOverlay)));
    assert_eq!(status, event::Status::Captured);
}

#[test]
fn update_ignores_click_when_cursor_unavailable() {
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = mouse::Cursor::Unavailable;

    let action = program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert!(msg.is_none());
    assert_eq!(status, event::Status::Ignored);
}

#[test]
fn update_click_on_empty_page_records_placement_drag_on_press() {
    // On mouse-down over a blank page area, placement is deferred to mouse-up.
    // In multi-page mode, page 1 starts at y=PAGE_GAP/2=8, centered at x=194.
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(300.0, 200.0);

    let action = program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    // Capture event but do not emit a message yet
    assert_eq!(status, event::Status::Captured);
    assert!(msg.is_none());
    // Placement drag state must be recorded
    assert!(state.placement_drag.is_some());
    let pd = state.placement_drag.as_ref().unwrap();
    assert_eq!(pd.page, 1);
    assert!((pd.start_screen.x - 300.0).abs() < 0.5);
    assert!((pd.start_screen.y - 200.0).abs() < 0.5);
}

#[test]
fn update_click_on_empty_page_places_single_line_overlay_on_release() {
    // In multi-page mode, page 1 starts at y=PAGE_GAP/2=8, centered at x=194
    // Page rect: (194, 8) to (806, 800)
    // Click at screen (300, 200):
    //   pdf_x = (300 - 194) / 1.0 = 106
    //   pdf_y = 792 - ((200 - 8) / 1.0) = 600
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(300.0, 200.0);

    // Press to start placement drag
    program.update(&mut state, &left_press_event(), bounds, cursor);
    assert!(state.placement_drag.is_some());

    // Release at same spot (distance < 10px) → single-line PlaceOverlay
    let action = program.update(&mut state, &left_release_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(state.placement_drag.is_none());
    match msg {
        Some(Message::PlaceOverlay {
            page,
            position,
            width,
        }) => {
            assert_eq!(page, 1);
            assert!((position.x - 106.0).abs() < 0.5);
            assert!((position.y - 600.0).abs() < 0.5);
            assert!(
                width.is_none(),
                "click should produce single-line (no width)"
            );
        }
        other => panic!("Expected PlaceOverlay, got {other:?}"),
    }
}

#[test]
fn update_drag_on_empty_page_places_multi_line_overlay_on_release() {
    // Drag from screen (300, 200) to (450, 200) — 150px horizontal drag
    // At zoom=1, dpi=72 (scale=1.0): 150 screen px = 150 PDF pts
    // Start: pdf_x = 300 - 194 = 106, pdf_y = 792 - (200 - 8) = 600
    // End: pdf_x = 450 - 194 = 256
    // Width = |256 - 106| = 150 pts
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();

    // Press at (300, 200)
    let cursor_start = cursor_at(300.0, 200.0);
    program.update(&mut state, &left_press_event(), bounds, cursor_start);
    assert!(state.placement_drag.is_some());

    // Release at (450, 200) — 150px drag, well over the 10px threshold
    let cursor_end = cursor_at(450.0, 200.0);
    let action = program.update(&mut state, &left_release_event(), bounds, cursor_end);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(state.placement_drag.is_none());
    match msg {
        Some(Message::PlaceOverlay {
            page,
            position,
            width,
        }) => {
            assert_eq!(page, 1);
            assert!((position.x - 106.0).abs() < 1.0);
            assert!((position.y - 600.0).abs() < 1.0);
            let w = width.expect("drag should produce multi-line (width Some)");
            assert!((w - 150.0).abs() < 1.0, "expected width ~150, got {w}");
        }
        other => panic!("Expected PlaceOverlay, got {other:?}"),
    }
}

#[test]
fn update_click_on_overlay_selects_it() {
    // Overlay at PDF (72, 720) → screen (266, 80) in multi-page mode
    // page at y=8, so screen_y = (792-720) + 8 = 80
    // Courier 12pt "Hello": hit box x=[266, 302], y=[68, 80]
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(270.0, 75.0);

    let action = program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(matches!(msg, Some(Message::SelectOverlay(0))));
}

#[test]
fn update_click_on_overlay_starts_drag() {
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(270.0, 75.0);

    program.update(&mut state, &left_press_event(), bounds, cursor);
    assert!(state.drag.is_some());
    let drag = state.drag.as_ref().unwrap();
    assert_eq!(drag.overlay_index, 0);
    assert!((drag.initial_pdf_position.x - 72.0).abs() < 0.01);
    assert!((drag.initial_pdf_position.y - 720.0).abs() < 0.01);
}

#[test]
fn update_click_outside_page_deselects() {
    // Page image bounds: (194, 104) to (806, 896).
    // Click at (50, 50) which is outside the page.
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(50.0, 50.0);

    let action = program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(matches!(msg, Some(Message::DeselectOverlay)));
}

#[test]
fn update_click_while_editing_commits_text_first() {
    // Iced actions carry a single message, so clicking while editing
    // returns CommitText only. The place/select happens on the next click.
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = PdfCanvasProgram {
        editing: true,
        active_overlay: Some(0),
        ..test_program(&overlays, &imgs, &dims)
    };
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(300.0, 200.0); // inside page

    let action = program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(matches!(msg, Some(Message::CommitText)));
}

#[test]
fn update_cursor_move_updates_state() {
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(400.0, 300.0);

    let action = program.update(
        &mut state,
        &cursor_moved_event(400.0, 300.0),
        bounds,
        cursor,
    );
    let (msg, _status) = decompose(action);
    assert!(msg.is_none());
    assert!(state.cursor_position.is_some());
    let pos = state.cursor_position.unwrap();
    assert!((pos.x - 400.0).abs() < 0.01);
    assert!((pos.y - 300.0).abs() < 0.01);
}

#[test]
fn update_mouse_release_without_drag_is_ignored() {
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(300.0, 200.0);

    let action = program.update(&mut state, &left_release_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert!(msg.is_none());
    assert_eq!(status, event::Status::Ignored);
}

#[test]
fn update_drag_and_release_publishes_move() {
    // 1) Click on overlay to start drag
    // 2) Move cursor
    // 3) Release → should publish MoveOverlay

    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();

    // Step 1: click on overlay at screen (270, 75) — overlay at PDF (72,720), page y-offset=8
    let cursor = cursor_at(270.0, 75.0);
    program.update(&mut state, &left_press_event(), bounds, cursor);
    assert!(state.drag.is_some());

    // Step 2: move cursor to (370, 175) — 100px right, 100px down
    let cursor = cursor_at(370.0, 175.0);
    program.update(
        &mut state,
        &cursor_moved_event(370.0, 175.0),
        bounds,
        cursor,
    );

    // Step 3: release at (370, 175)
    let action = program.update(&mut state, &left_release_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(state.drag.is_none());
    match msg {
        Some(Message::MoveOverlay(idx, pos)) => {
            assert_eq!(idx, 0);
            // The overlay moved. The new position should reflect 100px
            // shift in screen space converted to PDF space.
            // At zoom=1, dpi=72: scale=1.0, so 100 screen px = 100 PDF pts
            // Original PDF: (72, 720)
            // Shift: +100 x, -100 y (screen down = PDF y decrease)
            // Expected: (172, 620)
            assert!((pos.x - 172.0).abs() < 1.0);
            assert!((pos.y - 620.0).abs() < 1.0);
        }
        other => panic!("Expected MoveOverlay, got {other:?}"),
    }
}

#[test]
fn update_drag_release_at_same_position_no_move_message() {
    // Click on overlay, don't move, release → no MoveOverlay needed
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();

    let cursor = cursor_at(270.0, 75.0);
    program.update(&mut state, &left_press_event(), bounds, cursor);
    assert!(state.drag.is_some());

    // Release at same position
    let action = program.update(&mut state, &left_release_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert!(state.drag.is_none());
    // No movement → captured but no message
    assert_eq!(status, event::Status::Captured);
    assert!(msg.is_none());
}

#[test]
fn update_ignores_right_click() {
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(300.0, 200.0);

    let event = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
    let action = program.update(&mut state, &event, bounds, cursor);
    let (msg, status) = decompose(action);
    assert!(msg.is_none());
    assert_eq!(status, event::Status::Ignored);
}

// =====================================================================
// mouse_interaction() tests
// =====================================================================

#[test]
fn mouse_interaction_grabbing_during_drag() {
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    state.drag = Some(LocalDragState {
        overlay_index: 0,
        initial_pdf_position: PdfPosition { x: 72.0, y: 720.0 },
        grab_offset_x: 4.0,
        grab_offset_y: 6.0,
    });
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(300.0, 200.0);

    let interaction = program.mouse_interaction(&state, bounds, cursor);
    assert_eq!(interaction, mouse::Interaction::Grabbing);
}

#[test]
fn mouse_interaction_pointer_over_overlay() {
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let state = ProgramState::default();
    let bounds = test_canvas_bounds();
    // Cursor over the overlay's hit box at screen (270, 75) — page y-offset=8
    let cursor = cursor_at(270.0, 75.0);

    let interaction = program.mouse_interaction(&state, bounds, cursor);
    assert_eq!(interaction, mouse::Interaction::Pointer);
}

#[test]
fn mouse_interaction_crosshair_on_page() {
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let state = ProgramState::default();
    let bounds = test_canvas_bounds();
    // Cursor inside page image but not over any overlay
    let cursor = cursor_at(500.0, 500.0);

    let interaction = program.mouse_interaction(&state, bounds, cursor);
    assert_eq!(interaction, mouse::Interaction::Crosshair);
}

#[test]
fn mouse_interaction_default_outside_page() {
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let state = ProgramState::default();
    let bounds = test_canvas_bounds();
    // Cursor outside page image (50, 50) but inside canvas bounds
    let cursor = cursor_at(50.0, 50.0);

    let interaction = program.mouse_interaction(&state, bounds, cursor);
    assert_eq!(interaction, mouse::Interaction::default());
}

#[test]
fn mouse_interaction_default_when_no_page() {
    let overlays: Vec<TextOverlay> = vec![];
    let empty_imgs: HashMap<u32, Handle> = HashMap::new();
    let empty_dims: HashMap<u32, (f32, f32)> = HashMap::new();
    let layout = page_layout(&empty_dims, 0, TEST_ZOOM, TEST_DPI);
    let program = PdfCanvasProgram {
        page_images: &empty_imgs,
        page_layout: layout,
        page_dimensions: &empty_dims,
        page_count: 0,
        scroll_y: 0.0,
        viewport_height: 1000.0,
        overlays: &overlays,
        zoom: TEST_ZOOM,
        dpi: TEST_DPI,
        active_overlay: None,
        editing: false,
        overlay_color: [0.0, 0.0, 1.0, 1.0],
    };
    let state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(500.0, 500.0);

    let interaction = program.mouse_interaction(&state, bounds, cursor);
    assert_eq!(interaction, mouse::Interaction::default());
}

// =====================================================================
// draw() logic tests — verify coordinate consistency
// =====================================================================

#[test]
fn local_page_bounds_offsets_correctly_from_canvas_position() {
    // When canvas bounds start at (50, 30), page_image_bounds includes that offset.
    // The frame-local adjustment should subtract canvas origin.
    let canvas_bounds = iced::Rectangle {
        x: 50.0,
        y: 30.0,
        width: 1000.0,
        height: 1000.0,
    };
    let page_bounds = page_image_bounds(TEST_PAGE_DIMS, TEST_ZOOM, TEST_DPI, canvas_bounds);
    let local_page_bounds = iced::Rectangle {
        x: page_bounds.x - canvas_bounds.x,
        y: page_bounds.y - canvas_bounds.y,
        width: page_bounds.width,
        height: page_bounds.height,
    };
    // The local bounds should be the same as if canvas started at origin
    let origin_bounds = page_image_bounds(
        TEST_PAGE_DIMS,
        TEST_ZOOM,
        TEST_DPI,
        iced::Rectangle {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 1000.0,
        },
    );
    assert!((local_page_bounds.x - origin_bounds.x).abs() < 0.1);
    assert!((local_page_bounds.y - origin_bounds.y).abs() < 0.1);
    assert!((local_page_bounds.width - origin_bounds.width).abs() < 0.1);
    assert!((local_page_bounds.height - origin_bounds.height).abs() < 0.1);
}

#[test]
fn conversion_params_from_local_bounds_produce_valid_coordinates() {
    // Verify that using local (frame-relative) page bounds in ConversionParams
    // produces screen coordinates within the frame dimensions.
    let canvas_bounds = test_canvas_bounds();
    let page_bounds = page_image_bounds(TEST_PAGE_DIMS, TEST_ZOOM, TEST_DPI, canvas_bounds);
    let local_page_bounds = iced::Rectangle {
        x: page_bounds.x - canvas_bounds.x,
        y: page_bounds.y - canvas_bounds.y,
        width: page_bounds.width,
        height: page_bounds.height,
    };
    let params = ConversionParams {
        zoom: TEST_ZOOM,
        dpi: TEST_DPI,
        page_height: TEST_PAGE_DIMS.1,
        offset_x: local_page_bounds.x,
        offset_y: local_page_bounds.y,
    };
    // A point at the top-left of the PDF page (0, page_height) should map
    // to approximately local_page_bounds origin.
    let (sx, sy) = pdf_to_screen(0.0, TEST_PAGE_DIMS.1, &params);
    assert!(
        (sx - local_page_bounds.x).abs() < 0.1,
        "screen x ({sx}) should be near page left ({})",
        local_page_bounds.x
    );
    assert!(
        (sy - local_page_bounds.y).abs() < 0.1,
        "screen y ({sy}) should be near page top ({})",
        local_page_bounds.y
    );
}

// =====================================================================
// fit_to_width_zoom tests
// =====================================================================

#[test]
fn fit_to_width_zoom_us_letter_1000px() {
    // US Letter (612pt), viewport 1000px
    // zoom = sqrt(1000 * 72 / (612 * 150)) = sqrt(0.784) ≈ 0.885
    let zoom = fit_to_width_zoom(612.0, 1000.0);
    assert!((zoom - 0.885).abs() < 0.01, "zoom was {zoom}");
}

#[test]
fn fit_to_width_zoom_us_letter_1920px() {
    // zoom = sqrt(1920 * 72 / (612 * 150)) ≈ 1.227
    let zoom = fit_to_width_zoom(612.0, 1920.0);
    assert!((zoom - 1.227).abs() < 0.01, "zoom was {zoom}");
}

#[test]
fn fit_to_width_zoom_clamps_to_max() {
    // Very wide viewport should clamp to max zoom (2.0)
    let zoom = fit_to_width_zoom(100.0, 100000.0);
    assert!((zoom - 2.0).abs() < f32::EPSILON, "zoom was {zoom}");
}

#[test]
fn fit_to_width_zoom_clamps_to_min() {
    // Very narrow viewport should clamp to min zoom (0.25)
    let zoom = fit_to_width_zoom(612.0, 1.0);
    assert!((zoom - 0.25).abs() < f32::EPSILON, "zoom was {zoom}");
}

#[test]
fn fit_to_width_zoom_zero_page_width_returns_min() {
    let zoom = fit_to_width_zoom(0.0, 1000.0);
    assert!((zoom - 0.25).abs() < f32::EPSILON);
}

#[test]
fn fit_to_width_zoom_zero_viewport_returns_min() {
    let zoom = fit_to_width_zoom(612.0, 0.0);
    assert!((zoom - 0.25).abs() < f32::EPSILON);
}

// =====================================================================
// Keyboard modifier tracking tests
// =====================================================================

fn modifiers_changed_event(modifiers: iced::keyboard::Modifiers) -> canvas::Event {
    canvas::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers))
}

#[test]
fn modifiers_changed_updates_program_state() {
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(0.0, 0.0);

    assert!(!state.keyboard_modifiers.command());

    let event = modifiers_changed_event(iced::keyboard::Modifiers::COMMAND);
    let action = program.update(&mut state, &event, bounds, cursor);
    let (_msg, status) = decompose(action);
    assert_eq!(status, event::Status::Ignored);
    assert!(state.keyboard_modifiers.command());
}

// =====================================================================
// Scroll wheel tests
// =====================================================================

fn scroll_event(delta_y: f32) -> canvas::Event {
    canvas::Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y: delta_y },
    })
}

#[test]
fn ctrl_scroll_up_publishes_zoom_in() {
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    state.keyboard_modifiers = iced::keyboard::Modifiers::COMMAND;
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(500.0, 500.0);

    let action = program.update(&mut state, &scroll_event(1.0), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(matches!(msg, Some(Message::ZoomIn)));
}

#[test]
fn ctrl_scroll_down_publishes_zoom_out() {
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    state.keyboard_modifiers = iced::keyboard::Modifiers::COMMAND;
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(500.0, 500.0);

    let action = program.update(&mut state, &scroll_event(-1.0), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(matches!(msg, Some(Message::ZoomOut)));
}

#[test]
fn bare_scroll_is_not_captured() {
    let overlays: Vec<TextOverlay> = vec![];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    // No modifiers set
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(500.0, 500.0);

    let action = program.update(&mut state, &scroll_event(1.0), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Ignored);
    assert!(msg.is_none());
}

// =====================================================================
// Double-click to re-edit tests
// =====================================================================

#[test]
fn program_state_default_has_no_last_click() {
    let state = ProgramState::default();
    assert!(state.last_click.is_none());
}

#[test]
fn single_click_on_overlay_emits_select_not_edit() {
    // First click on overlay — no previous click — should emit SelectOverlay.
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(270.0, 75.0);

    let action = program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(matches!(msg, Some(Message::SelectOverlay(0))));
    // last_click is recorded after a hit
    assert!(state.last_click.is_some());
}

#[test]
fn double_click_on_overlay_emits_edit_overlay() {
    // Two rapid clicks at the same position on an overlay → EditOverlay.
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(270.0, 75.0);

    // First click: sets last_click, emits SelectOverlay
    let action = program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, _) = decompose(action);
    assert!(matches!(msg, Some(Message::SelectOverlay(0))));
    assert!(state.last_click.is_some());

    // Second click immediately: should emit EditOverlay
    let action = program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(matches!(msg, Some(Message::EditOverlay(0))));
}

#[test]
fn double_click_too_far_away_does_not_edit() {
    // Two clicks where second is more than 5px away from first → still SelectOverlay.
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();

    // First click at (270, 75)
    let cursor1 = cursor_at(270.0, 75.0);
    let action = program.update(&mut state, &left_press_event(), bounds, cursor1);
    let (msg, _) = decompose(action);
    assert!(matches!(msg, Some(Message::SelectOverlay(0))));

    // Second click at (280, 75) — 10px away, beyond 5px threshold
    let cursor2 = cursor_at(280.0, 75.0);
    let action = program.update(&mut state, &left_press_event(), bounds, cursor2);
    let (msg, _) = decompose(action);
    assert!(matches!(msg, Some(Message::SelectOverlay(0))));
}

#[test]
fn double_click_records_last_click_after_edit() {
    // After a double-click, last_click is updated with the second click's position.
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(270.0, 75.0);

    program.update(&mut state, &left_press_event(), bounds, cursor);
    program.update(&mut state, &left_press_event(), bounds, cursor);

    // last_click should reflect the position of the second (double) click
    let (_, pos) = state.last_click.as_ref().unwrap();
    assert!((pos.x - 270.0).abs() < 0.5);
    assert!((pos.y - 75.0).abs() < 0.5);
}

// =====================================================================
// Resize handle tests
// =====================================================================

/// Build a multi-line overlay (width = Some) for resize handle tests.
fn multiline_overlay_at(x: f32, y: f32, width: f32, text: &str) -> TextOverlay {
    TextOverlay {
        page: 1,
        position: PdfPosition { x, y },
        text: text.to_string(),
        font: Standard14Font::Courier,
        font_size: 12.0,
        width: Some(width),
    }
}

#[test]
fn resize_drag_state_construction() {
    let state = ResizeDragState {
        overlay_index: 2,
        initial_width: 150.0,
    };
    assert_eq!(state.overlay_index, 2);
    assert!((state.initial_width - 150.0).abs() < f32::EPSILON);
}

#[test]
fn program_state_default_has_no_resize_drag() {
    let state = ProgramState::default();
    assert!(state.resize_drag.is_none());
}

// --- Resize handle hit-test helpers ---
// The handle occupies +-4px on the right edge of a multi-line overlay's width.
// At zoom=1, dpi=72 (scale=1.0):
//   overlay at PDF (72, 720), width=150pt → handle_screen_x = 194 + 72 + 150 = 416
//   (page left x = (1000-612)/2 = 194, page top y = 8)
//
// Handle hit area: x in [412, 420], y in [sy-h, sy] (full overlay height vertically)

#[test]
fn click_on_resize_handle_starts_resize_drag() {
    // Multi-line overlay at PDF (72, 720), width=150pt.
    // At scale=1: handle_screen_x = 194 + 72 + 150 = 416
    // Overlay screen y = 792-720 + 8 = 80 (baseline). Height = 12*1 = 12.
    // Handle y range: [68, 80].
    // Click at (416, 75): should start resize_drag, not overlay move drag.
    let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = PdfCanvasProgram {
        active_overlay: Some(0),
        ..test_program(&overlays, &imgs, &dims)
    };
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(416.0, 75.0);

    let action = program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    // Should capture event. Resize drag started, no message on press.
    assert!(
        msg.is_none(),
        "expected None message on resize handle press, got {msg:?}"
    );
    assert!(state.resize_drag.is_some(), "resize_drag should be set");
    assert!(state.drag.is_none(), "overlay move drag should NOT be set");
    let rd = state.resize_drag.as_ref().unwrap();
    assert_eq!(rd.overlay_index, 0);
    assert!((rd.initial_width - 150.0).abs() < 0.5);
}

#[test]
fn resize_drag_on_single_line_overlay_does_not_start() {
    // Single-line overlays (width=None) have no resize handle.
    // Click at the same x position should fall through to normal overlay hit test.
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")]; // width=None
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = PdfCanvasProgram {
        active_overlay: Some(0),
        ..test_program(&overlays, &imgs, &dims)
    };
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    // Click at x=416, y=75 — but overlay has no width, so no handle exists there
    let cursor = cursor_at(416.0, 75.0);

    program.update(&mut state, &left_press_event(), bounds, cursor);
    assert!(
        state.resize_drag.is_none(),
        "single-line overlay should have no resize drag"
    );
}

#[test]
fn resize_drag_only_starts_on_selected_overlay() {
    // The resize handle only appears for the active (selected) overlay.
    // If no overlay is selected, clicking the handle position starts placement drag.
    let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    // active_overlay is None — not selected
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(416.0, 75.0);

    program.update(&mut state, &left_press_event(), bounds, cursor);
    assert!(
        state.resize_drag.is_none(),
        "resize drag should not start when overlay not selected"
    );
}

#[test]
fn resize_drag_release_publishes_resize_overlay_message() {
    // Drag from handle at x=416 to x=516 (100px rightward) → new_width = 150 + 100 = 250pt
    let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = PdfCanvasProgram {
        active_overlay: Some(0),
        ..test_program(&overlays, &imgs, &dims)
    };
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();

    // Press on handle
    let cursor_press = cursor_at(416.0, 75.0);
    program.update(&mut state, &left_press_event(), bounds, cursor_press);
    assert!(state.resize_drag.is_some());

    // Release 100px to the right
    let cursor_release = cursor_at(516.0, 75.0);
    let action = program.update(&mut state, &left_release_event(), bounds, cursor_release);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(state.resize_drag.is_none());
    match msg {
        Some(Message::ResizeOverlay {
            index,
            old_width,
            new_width,
        }) => {
            assert_eq!(index, 0);
            assert!(
                (old_width - 150.0).abs() < 1.0,
                "old_width should be 150, got {old_width}"
            );
            // new_width: cursor_release pdf_x - overlay.position.x
            // cursor_release.x = 516, page_left = 194, scale=1 → pdf_x = 516-194 = 322
            // overlay.position.x = 72 → new_width = 322 - 72 = 250
            assert!(
                (new_width - 250.0).abs() < 1.0,
                "new_width should be ~250, got {new_width}"
            );
        }
        other => panic!("Expected ResizeOverlay message, got {other:?}"),
    }
}

#[test]
fn resize_drag_release_enforces_minimum_width() {
    // Drag leftward past the overlay's left edge. Width clamped to 20pt.
    let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = PdfCanvasProgram {
        active_overlay: Some(0),
        ..test_program(&overlays, &imgs, &dims)
    };
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();

    // Press on handle
    let cursor_press = cursor_at(416.0, 75.0);
    program.update(&mut state, &left_press_event(), bounds, cursor_press);

    // Release far to the left (cursor_pdf_x < overlay_pdf_x + 20)
    // page_left=194, overlay.position.x=72 → at x=194+72+5=271 → pdf_x=77 → width=5 < 20
    let cursor_release = cursor_at(271.0, 75.0);
    let action = program.update(&mut state, &left_release_event(), bounds, cursor_release);
    let (msg, _status) = decompose(action);
    match msg {
        Some(Message::ResizeOverlay { new_width, .. }) => {
            assert!(
                new_width >= 20.0,
                "new_width should be at least 20, got {new_width}"
            );
        }
        other => panic!("Expected ResizeOverlay, got {other:?}"),
    }
}

#[test]
fn resize_drag_release_at_same_position_emits_no_message() {
    // If the user presses and releases the handle without moving, no resize needed.
    let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = PdfCanvasProgram {
        active_overlay: Some(0),
        ..test_program(&overlays, &imgs, &dims)
    };
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();

    let cursor = cursor_at(416.0, 75.0);
    program.update(&mut state, &left_press_event(), bounds, cursor);
    assert!(state.resize_drag.is_some());

    let action = program.update(&mut state, &left_release_event(), bounds, cursor);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(state.resize_drag.is_none());
    assert!(
        msg.is_none(),
        "no change in width → no ResizeOverlay message, got {msg:?}"
    );
}

#[test]
fn cursor_move_requests_redraw_during_resize_drag() {
    let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = PdfCanvasProgram {
        active_overlay: Some(0),
        ..test_program(&overlays, &imgs, &dims)
    };
    let mut state = ProgramState::default();
    state.resize_drag = Some(ResizeDragState {
        overlay_index: 0,
        initial_width: 150.0,
    });
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(450.0, 75.0);

    let action = program.update(&mut state, &cursor_moved_event(450.0, 75.0), bounds, cursor);
    // Should request redraw
    assert!(
        action.is_some(),
        "cursor move during resize drag should return Some(action)"
    );
}

#[test]
fn click_on_empty_page_between_overlay_clicks_prevents_double_click() {
    // Clicking blank page area clears last_click and prevents false-positive double-click.
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();

    // First: click on overlay to set last_click
    let cursor1 = cursor_at(270.0, 75.0);
    program.update(&mut state, &left_press_event(), bounds, cursor1);
    assert!(state.last_click.is_some());

    // Second: click on blank page area — should start placement drag, not edit
    // 300, 500 is well inside the page but away from the overlay
    let cursor2 = cursor_at(300.0, 500.0);
    let action = program.update(&mut state, &left_press_event(), bounds, cursor2);
    let (msg, _) = decompose(action);
    // Blank area click starts placement drag, no message on press
    assert!(msg.is_none());
    assert!(state.placement_drag.is_some());
}

#[test]
fn drag_after_commit_preserves_overlay() {
    // Regression: overlay disappeared when drag-moving after text commit.
    // Simulates: editing overlay → commit → click overlay → drag → release.
    // The ProgramState persists across program changes (editing → not editing).
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let bounds = test_canvas_bounds();
    let mut state = ProgramState::default();

    // Phase 1: User is editing. Move cursor to overlay position to set cursor_position.
    let editing_program = PdfCanvasProgram {
        editing: true,
        active_overlay: Some(0),
        ..test_program(&overlays, &imgs, &dims)
    };
    let cursor = cursor_at(270.0, 75.0);
    editing_program.update(&mut state, &cursor_moved_event(270.0, 75.0), bounds, cursor);
    assert!(
        state.cursor_position.is_some(),
        "cursor_position must be set before commit"
    );

    // Phase 2: User commits text (Enter). Program is recreated with editing=false.
    // Click while editing produces CommitText.
    let action = editing_program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, _) = decompose(action);
    assert!(matches!(msg, Some(Message::CommitText)));

    // Phase 3: After commit, program changes to editing=false.
    // ProgramState MUST persist — cursor_position must survive.
    let committed_program = PdfCanvasProgram {
        editing: false,
        active_overlay: Some(0),
        ..test_program(&overlays, &imgs, &dims)
    };
    assert!(
        state.cursor_position.is_some(),
        "cursor_position must survive program change (commit)"
    );

    // Phase 4: Click on overlay to start drag.
    let action = committed_program.update(&mut state, &left_press_event(), bounds, cursor);
    let (msg, _) = decompose(action);
    assert!(
        matches!(msg, Some(Message::SelectOverlay(0))),
        "click on overlay after commit should select it"
    );
    assert!(state.drag.is_some(), "drag must start on overlay click");

    // Phase 5: Drag preview requires cursor_position to be Some.
    // If cursor_position is None, the overlay would be invisible during drag
    // (skipped from normal rendering AND no preview drawn).
    assert!(
        state.cursor_position.is_some(),
        "cursor_position must be Some during drag for preview to render"
    );

    // Phase 6: Move cursor and release to complete the drag.
    let new_cursor = cursor_at(370.0, 175.0);
    committed_program.update(
        &mut state,
        &cursor_moved_event(370.0, 175.0),
        bounds,
        new_cursor,
    );
    let action = committed_program.update(&mut state, &left_release_event(), bounds, new_cursor);
    let (msg, _) = decompose(action);
    match msg {
        Some(Message::MoveOverlay(idx, pos)) => {
            assert_eq!(idx, 0);
            assert!((pos.x - 172.0).abs() < 1.0);
            assert!((pos.y - 620.0).abs() < 1.0);
        }
        other => panic!("Expected MoveOverlay, got {other:?}"),
    }
}

// =====================================================================
// spe-zr9: hide canvas overlay text while floating widget is editing
// =====================================================================

#[test]
fn should_draw_overlay_text_false_when_editing_active_overlay() {
    assert!(
        !super::should_draw_overlay_text(true, Some(0), 0),
        "should not draw overlay text for the overlay being edited"
    );
}

#[test]
fn should_draw_overlay_text_true_for_non_active_overlay_during_editing() {
    assert!(
        super::should_draw_overlay_text(true, Some(0), 1),
        "should draw overlay text for non-active overlays even during editing"
    );
}

#[test]
fn should_draw_overlay_text_true_when_not_editing() {
    assert!(
        super::should_draw_overlay_text(false, Some(0), 0),
        "should draw overlay text when not editing, even for active overlay"
    );
}

#[test]
fn should_draw_overlay_text_true_when_no_active_overlay() {
    assert!(
        super::should_draw_overlay_text(true, None, 0),
        "should draw when no overlay is active"
    );
}

// =====================================================================
// spe-ceg.2: tint size computation
// =====================================================================

#[test]
fn tint_size_single_line_overlay_uses_bounding_box() {
    // Single-line overlay (width=None): tint size comes from overlay_bounding_box.
    // Courier 12pt, "Hello" → width = 5 * 7.2 = 36.0, height = 12.0
    // scale=1.0 → w=36.0, h=12.0
    let overlay = overlay_at(72.0, 720.0, "Hello");
    let (w, h): (f32, f32) = super::tint_size_for_overlay(&overlay, 1.0);
    assert!((w - 36.0).abs() < 0.1, "width should be ~36, got {w}");
    assert!((h - 12.0).abs() < 0.1, "height should be 12, got {h}");
}

#[test]
fn tint_size_single_line_overlay_scales_with_scale() {
    // Same as above but scale=2.0 → w=72.0, h=24.0
    let overlay = overlay_at(72.0, 720.0, "Hello");
    let (w, h): (f32, f32) = super::tint_size_for_overlay(&overlay, 2.0);
    assert!((w - 72.0).abs() < 0.1, "width should be ~72, got {w}");
    assert!((h - 24.0).abs() < 0.1, "height should be 24, got {h}");
}

#[test]
fn tint_size_multiline_overlay_uses_width_and_line_count() {
    // Multi-line overlay (width=Some(150)), two lines of text, font_size=12.
    // scale=1.0 → w=150.0, h=12.0 * 2 = 24.0
    let overlay = TextOverlay {
        page: 1,
        position: PdfPosition { x: 72.0, y: 720.0 },
        text: "line one\nline two".to_string(),
        font: Standard14Font::Courier,
        font_size: 12.0,
        width: Some(150.0),
    };
    let (w, h): (f32, f32) = super::tint_size_for_overlay(&overlay, 1.0);
    assert!((w - 150.0).abs() < 0.1, "width should be 150, got {w}");
    assert!((h - 24.0).abs() < 0.1, "height should be 24, got {h}");
}

#[test]
fn tint_size_multiline_overlay_single_line_text_height_is_one_line() {
    // Multi-line overlay (width=Some) but text has only one line.
    // Height = font_size * 1 = 12.0
    let overlay = multiline_overlay_at(72.0, 720.0, 150.0, "Hello");
    let (w, h): (f32, f32) = super::tint_size_for_overlay(&overlay, 1.0);
    assert!((w - 150.0).abs() < 0.1, "width should be 150, got {w}");
    assert!((h - 12.0).abs() < 0.1, "height should be 12, got {h}");
}

#[test]
fn tint_alpha_constant_is_correct() {
    assert!(
        (super::OVERLAY_TINT_ALPHA - 0.08).abs() < f32::EPSILON,
        "OVERLAY_TINT_ALPHA should be 0.08"
    );
}

// =====================================================================
// spe-ceg.3: hover tracking and tint intensification
// =====================================================================

#[test]
fn overlay_tint_hover_alpha_constant_is_correct() {
    assert!(
        (super::OVERLAY_TINT_HOVER_ALPHA - 0.15).abs() < f32::EPSILON,
        "OVERLAY_TINT_HOVER_ALPHA should be 0.15"
    );
}

#[test]
fn overlay_tint_hover_border_alpha_constant_is_correct() {
    assert!(
        (super::OVERLAY_TINT_HOVER_BORDER_ALPHA - 0.5).abs() < f32::EPSILON,
        "OVERLAY_TINT_HOVER_BORDER_ALPHA should be 0.5"
    );
}

#[test]
fn program_state_default_has_no_hovered_overlay() {
    let state = ProgramState::default();
    assert!(state.hovered_overlay.is_none());
}

#[test]
fn cursor_move_over_overlay_sets_hovered_overlay() {
    // Overlay at PDF (72, 720) → screen (266, 80) in multi-page canvas.
    // Hit box includes screen (270, 75). Moving cursor there should set hovered_overlay = Some(0).
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(270.0, 75.0);

    let action = program.update(&mut state, &cursor_moved_event(270.0, 75.0), bounds, cursor);
    // Should request a redraw because hovered_overlay changed (None → Some(0))
    assert!(
        action.is_some(),
        "cursor move over overlay should request redraw"
    );
    assert_eq!(
        state.hovered_overlay,
        Some(0),
        "hovered_overlay should be Some(0) when cursor is over overlay"
    );
}

#[test]
fn cursor_move_off_overlay_clears_hovered_overlay() {
    // Start with cursor over overlay, then move away. hovered_overlay should clear.
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    state.hovered_overlay = Some(0);
    let bounds = test_canvas_bounds();

    // Move to blank page area (500, 500)
    let cursor = cursor_at(500.0, 500.0);
    let action = program.update(
        &mut state,
        &cursor_moved_event(500.0, 500.0),
        bounds,
        cursor,
    );
    // Should request redraw because hovered_overlay changed (Some(0) → None)
    assert!(
        action.is_some(),
        "cursor move off overlay should request redraw"
    );
    assert!(
        state.hovered_overlay.is_none(),
        "hovered_overlay should be None when cursor is not over any overlay"
    );
}

#[test]
fn cursor_move_with_no_hover_change_does_not_request_redraw() {
    // Cursor moves over blank page area — hovered_overlay stays None — no redraw needed.
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    // hovered_overlay already None
    let bounds = test_canvas_bounds();

    // Move over blank page area (500, 500) — no overlay there
    let cursor = cursor_at(500.0, 500.0);
    let action = program.update(
        &mut state,
        &cursor_moved_event(500.0, 500.0),
        bounds,
        cursor,
    );
    assert!(
        action.is_none(),
        "no hover change should produce no action (no redraw)"
    );
}

#[test]
fn cursor_move_during_drag_skips_hover_tracking() {
    // During an overlay drag, CursorMoved returns redraw immediately without updating hovered_overlay.
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let imgs = test_page_images();
    let dims = test_page_dimensions();
    let program = test_program(&overlays, &imgs, &dims);
    let mut state = ProgramState::default();
    state.drag = Some(LocalDragState {
        overlay_index: 0,
        initial_pdf_position: PdfPosition { x: 72.0, y: 720.0 },
        grab_offset_x: 4.0,
        grab_offset_y: 6.0,
    });
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(270.0, 75.0);

    let action = program.update(&mut state, &cursor_moved_event(270.0, 75.0), bounds, cursor);
    // Should request redraw (drag in progress) but NOT update hovered_overlay
    assert!(action.is_some(), "drag cursor move should request redraw");
    assert!(
        state.hovered_overlay.is_none(),
        "hover tracking skipped during drag"
    );
}
