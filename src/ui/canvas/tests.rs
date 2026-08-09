use super::*;
use crate::app::Message;
use crate::coordinate::ConversionParams;
use crate::fonts::FontRegistry;
use crate::overlay::{OverlayBox, PdfPosition, TextOverlay};
use crate::test_render::{RENDER_SIZE, RenderedCanvas, render_element};
use iced::event;
use iced::mouse;
use iced::widget::canvas;
use iced::widget::image::Handle;
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
        font: FontRegistry::new().find_by_name("Courier").unwrap(),
        font_size: 12.0,
        width: None,
        min_height: None,
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

fn test_page_dimensions() -> HashMap<u32, (f32, f32)> {
    let mut dims = HashMap::new();
    dims.insert(1, TEST_PAGE_DIMS);
    dims
}

/// Build an OverlayCanvasProgram for event handling tests (single page).
fn test_program<'a>(
    overlays: &'a [TextOverlay],
    page_dims: &'a HashMap<u32, (f32, f32)>,
    registry: &'a FontRegistry,
) -> OverlayCanvasProgram<'a> {
    let layout = page_layout(page_dims, 1, TEST_ZOOM, TEST_DPI);
    OverlayCanvasProgram {
        page_layout: layout,
        page_dimensions: page_dims,
        scroll_y: 0.0,
        viewport_height: 1000.0,
        overlays,
        zoom: TEST_ZOOM,
        dpi: TEST_DPI,
        active_overlay: None,
        editing: false,
        overlay_color: [0.0, 0.0, 1.0, 1.0],
        font_registry: registry,
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

/// ProgramState with an active drag on overlay index 0 at PDF (72, 720).
fn state_with_drag() -> ProgramState {
    ProgramState {
        drag: Some(LocalDragState {
            overlay_index: 0,
            anchor: OverlayAnchor {
                page: 1,
                position: PdfPosition { x: 72.0, y: 720.0 },
                width: None,
            },
            grab_offset_x: 4.0,
            grab_offset_y: 6.0,
        }),
        ..ProgramState::default()
    }
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

/// Program over a document that has no pages at all.
fn no_pages_program<'a>(
    overlays: &'a [TextOverlay],
    empty_dims: &'a HashMap<u32, (f32, f32)>,
    registry: &'a FontRegistry,
) -> OverlayCanvasProgram<'a> {
    OverlayCanvasProgram {
        page_layout: page_layout(empty_dims, 0, TEST_ZOOM, TEST_DPI),
        ..test_program(overlays, empty_dims, registry)
    }
}

/// Deliver a left press at `cursor` to a single-page program over `overlays`,
/// returning what it published and the widget state it left behind.
fn press_on(
    overlays: &[TextOverlay],
    active_overlay: Option<usize>,
    cursor: mouse::Cursor,
) -> (Option<Message>, event::Status, ProgramState) {
    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = OverlayCanvasProgram {
        active_overlay,
        ..test_program(overlays, &dims, &registry)
    };
    let mut state = ProgramState::default();
    let action = program.update(
        &mut state,
        &left_press_event(),
        test_canvas_bounds(),
        cursor,
    );
    let (msg, status) = decompose(action);
    (msg, status, state)
}

/// Deliver a left release at `cursor` to a single-page program over
/// `overlays`, continuing `state`.
///
/// `overlays` need not be the list the press saw — an IPC delete or an undo
/// can change it while a drag is in flight, which is exactly what the drag
/// anchoring has to survive.
fn release_on(
    state: &mut ProgramState,
    overlays: &[TextOverlay],
    active_overlay: Option<usize>,
    cursor: mouse::Cursor,
) -> (Option<Message>, event::Status) {
    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = OverlayCanvasProgram {
        active_overlay,
        ..test_program(overlays, &dims, &registry)
    };
    let action = program.update(state, &left_release_event(), test_canvas_bounds(), cursor);
    decompose(action)
}

/// Grab the overlay at (270, 75) in `before`, check the drag took hold of
/// `expected_index`, then release at (400, 300) against `after` — the overlay
/// list having changed underneath the drag, as an IPC delete or an undo can do.
///
/// Returns what the release published and the state it left, so each caller
/// can say what should have become of its own drag.
fn drag_across_a_list_change(
    before: &[TextOverlay],
    expected_index: usize,
    after: &[TextOverlay],
) -> (Option<Message>, ProgramState) {
    let (_, _, mut state) = press_on(before, None, cursor_at(270.0, 75.0));
    assert_eq!(
        state
            .drag
            .as_ref()
            .expect("press should start a drag")
            .overlay_index,
        expected_index
    );
    let (msg, _) = release_on(&mut state, after, None, cursor_at(400.0, 300.0));
    (msg, state)
}

/// Press at `press` then release at `release` on a blank single-page canvas,
/// and return the placement the canvas published. The placement drag must be
/// recorded on press and cleared on release either way, so that is checked
/// here rather than restated by every caller.
fn placement_from(press: mouse::Cursor, release: mouse::Cursor) -> Message {
    let overlays: Vec<TextOverlay> = vec![];
    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();

    program.update(&mut state, &left_press_event(), bounds, press);
    assert!(
        state.placement_drag.is_some(),
        "press on a blank page should start a placement drag"
    );

    let action = program.update(&mut state, &left_release_event(), bounds, release);
    let (msg, status) = decompose(action);
    assert_eq!(status, event::Status::Captured);
    assert!(
        state.placement_drag.is_none(),
        "release should end the placement drag"
    );
    msg.expect("a placement gesture must publish a placement")
}

/// The single-line placement `press`/`release` published, or a panic naming
/// what came instead.
fn single_line_placement_from(press: mouse::Cursor, release: mouse::Cursor) -> (u32, PdfPosition) {
    match placement_from(press, release) {
        Message::PlaceOverlay { page, position } => (page, position),
        other => panic!("Expected PlaceOverlay, got {other:?}"),
    }
}

/// The dragged-out text box `press`/`release` published, as
/// (page, top-left corner, width, height).
fn text_box_placement_from(
    press: mouse::Cursor,
    release: mouse::Cursor,
) -> (u32, PdfPosition, f32, f32) {
    match placement_from(press, release) {
        Message::PlaceTextBox {
            page,
            top_left,
            width,
            height,
        } => (page, top_left, width, height),
        other => panic!("Expected PlaceTextBox, got {other:?}"),
    }
}

/// Assert the canvas published a move of overlay `index` to `expected`.
fn assert_moved_to(msg: Option<Message>, index: usize, expected: PdfPosition) {
    match msg {
        Some(Message::MoveOverlay(idx, pos)) => {
            assert_eq!(idx, index);
            assert!(
                (pos.x - expected.x).abs() < 1.0,
                "x should be {}, got {}",
                expected.x,
                pos.x
            );
            assert!(
                (pos.y - expected.y).abs() < 1.0,
                "y should be {}, got {}",
                expected.y,
                pos.y
            );
        }
        other => panic!("Expected MoveOverlay, got {other:?}"),
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
        anchor: OverlayAnchor {
            page: 1,
            position: PdfPosition { x: 100.0, y: 500.0 },
            width: None,
        },
        grab_offset_x: 5.0,
        grab_offset_y: 10.0,
    };
    assert_eq!(drag.overlay_index, 3);
    assert!((drag.anchor.position.x - 100.0).abs() < f32::EPSILON);
    assert!((drag.anchor.position.y - 500.0).abs() < f32::EPSILON);
    assert!((drag.grab_offset_x - 5.0).abs() < f32::EPSILON);
    assert!((drag.grab_offset_y - 10.0).abs() < f32::EPSILON);
}

// --- OverlayCanvasProgram tests ---

#[test]
fn overlay_canvas_program_construction_with_no_document() {
    let overlays: Vec<TextOverlay> = vec![];
    let empty_dims: HashMap<u32, (f32, f32)> = HashMap::new();
    let layout = page_layout(&empty_dims, 0, 1.0, 150.0);
    let registry = FontRegistry::new();
    let program = OverlayCanvasProgram {
        page_layout: layout,
        page_dimensions: &empty_dims,
        scroll_y: 0.0,
        viewport_height: 1000.0,
        overlays: &overlays,
        zoom: 1.0,
        dpi: 150.0,
        active_overlay: None,
        editing: false,
        overlay_color: [0.0, 0.0, 1.0, 1.0],
        font_registry: &registry,
    };
    assert!(program.page_dimensions.is_empty());
    assert_eq!(program.overlays.len(), 0);
}

#[test]
fn overlay_canvas_program_construction_with_document() {
    let overlays = vec![overlay_at(72.0, 720.0, "Test")];
    let dims = test_page_dimensions();
    let layout = page_layout(&dims, 1, 1.5, 150.0);
    let registry = FontRegistry::new();
    let program = OverlayCanvasProgram {
        page_layout: layout,
        page_dimensions: &dims,
        scroll_y: 0.0,
        viewport_height: 1000.0,
        overlays: &overlays,
        zoom: 1.5,
        dpi: 150.0,
        active_overlay: Some(0),
        editing: true,
        overlay_color: [0.26, 0.53, 0.96, 1.0],
        font_registry: &registry,
    };
    assert!(program.page_dimensions.contains_key(&1));
    assert_eq!(program.overlays.len(), 1);
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
    let registry = FontRegistry::new();
    assert!(hit_test(100.0, 100.0, &[], 1, &params, &registry).is_none());
}

#[test]
fn hit_test_finds_overlay_at_position() {
    let params = default_params();
    let registry = FontRegistry::new();
    // Courier at 12pt: each char is 600/1000 * 12 = 7.2 px wide
    // "Hello" = 5 * 7.2 = 36px wide, 12px tall
    // Overlay at PDF (72, 720) → screen (72, 72) at zoom=1, dpi=72
    // Hit box: x=[72, 108], y=[60, 72]
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let result = hit_test(80.0, 65.0, &overlays, 1, &params, &registry);
    assert_eq!(result, Some(0));
}

#[test]
fn hit_test_returns_none_for_miss() {
    let params = default_params();
    let registry = FontRegistry::new();
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    // Click far away from overlay
    let result = hit_test(500.0, 500.0, &overlays, 1, &params, &registry);
    assert!(result.is_none());
}

#[test]
fn hit_test_returns_topmost_for_overlapping() {
    let params = default_params();
    let registry = FontRegistry::new();
    let overlays = vec![
        overlay_at(72.0, 720.0, "First"),
        overlay_at(72.0, 720.0, "Second"),
    ];
    // Both at same position, should return index 1 (topmost/last)
    let result = hit_test(80.0, 65.0, &overlays, 1, &params, &registry);
    assert_eq!(result, Some(1));
}

// spe-7f1: the screen-space mouse path and the PDF-space automation path must
// make the same hit-test decision, so `hit_test` is a thin conversion in front
// of `hit_test_pdf` rather than a second implementation.

#[test]
fn hit_test_pdf_finds_overlay_at_position() {
    let registry = FontRegistry::new();
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    // Courier at 12pt: "Hello" is 36pt wide and 12pt tall, extending up and
    // right from the baseline at (72, 720).
    assert_eq!(hit_test_pdf(80.0, 725.0, &overlays, 1, &registry), Some(0));
}

#[test]
fn hit_test_pdf_returns_none_for_miss() {
    let registry = FontRegistry::new();
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    assert!(hit_test_pdf(400.0, 400.0, &overlays, 1, &registry).is_none());
}

#[test]
fn hit_test_pdf_returns_topmost_for_overlapping() {
    let registry = FontRegistry::new();
    let overlays = vec![
        overlay_at(72.0, 720.0, "First"),
        overlay_at(72.0, 720.0, "Second"),
    ];
    assert_eq!(hit_test_pdf(80.0, 725.0, &overlays, 1, &registry), Some(1));
}

#[test]
fn hit_test_pdf_ignores_overlays_on_other_pages() {
    let registry = FontRegistry::new();
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    assert!(hit_test_pdf(80.0, 725.0, &overlays, 2, &registry).is_none());
}

#[test]
fn hit_test_agrees_with_hit_test_pdf_at_every_zoom_and_offset() {
    let registry = FontRegistry::new();
    let overlays = vec![
        overlay_at(72.0, 720.0, "Hello"),
        overlay_at(200.0, 400.0, "Second overlay"),
    ];
    // A grid of PDF points covering both overlays, just inside and just outside
    // each edge, and empty space.
    //
    // Deliberately no point sits exactly on an edge. The hit box is half-open
    // (`Rectangle::contains` is `x <= p < x + w`), and the screen path reaches
    // its verdict through a `pdf -> screen -> pdf` round trip that is not
    // bit-exact: at zoom 2.5/dpi 150 an exact 108.0 comes back as 107.999992,
    // landing on the other side of the boundary. Any two implementations
    // related by that round trip disagree within a rounding error of an edge,
    // so asserting agreement there would be asserting something untrue rather
    // than catching a real divergence.
    let probes = [
        (72.5, 719.5),
        (80.0, 725.0),
        (107.5, 731.5),
        (108.5, 732.5),
        (110.0, 725.0),
        (200.5, 400.5),
        (250.0, 410.0),
        (400.0, 400.0),
        (0.0, 0.0),
    ];
    for (zoom, dpi, offset_x, offset_y) in [
        (1.0, 72.0, 0.0, 0.0),
        (2.5, 150.0, 37.0, 11.0),
        (0.5, 96.0, -8.0, 4.0),
    ] {
        let params = ConversionParams {
            zoom,
            dpi,
            page_height: 792.0,
            offset_x,
            offset_y,
        };
        for (pdf_x, pdf_y) in probes {
            let (screen_x, screen_y) = pdf_to_screen(pdf_x, pdf_y, &params);
            assert_eq!(
                hit_test(screen_x, screen_y, &overlays, 1, &params, &registry),
                hit_test_pdf(pdf_x, pdf_y, &overlays, 1, &registry),
                "paths disagreed at PDF ({pdf_x}, {pdf_y}) with zoom {zoom} dpi {dpi}"
            );
        }
    }
}

#[test]
fn hit_test_ignores_overlays_on_other_pages() {
    let params = default_params();
    let registry = FontRegistry::new();
    let overlays = vec![TextOverlay {
        page: 2,
        position: PdfPosition { x: 72.0, y: 720.0 },
        text: "On page 2".to_string(),
        font: registry.find_by_name("Courier").unwrap(),
        font_size: 12.0,
        width: None,
        min_height: None,
    }];
    let result = hit_test(80.0, 65.0, &overlays, 1, &params, &registry);
    assert!(result.is_none());
}

#[test]
fn hit_test_finds_explicit_width_overlay_beyond_its_glyphs() {
    let params = default_params();
    let registry = FontRegistry::new();
    // "Hi" is only 2 * 7.2 = 14.4px wide at Courier 12pt, but the overlay's
    // explicit width (200pt) makes the tinted, clickable area much wider —
    // the same area draw_single_overlay paints via overlay_text_box.
    let overlays = vec![TextOverlay {
        page: 1,
        position: PdfPosition { x: 72.0, y: 720.0 },
        text: "Hi".to_string(),
        font: registry.find_by_name("Courier").unwrap(),
        font_size: 12.0,
        width: Some(200.0),
        min_height: None,
    }];
    // Click well past the glyphs but still inside the explicit-width box.
    let result = hit_test(150.0, 65.0, &overlays, 1, &params, &registry);
    assert_eq!(result, Some(0));
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
    let empty_dims: HashMap<u32, (f32, f32)> = HashMap::new();
    let registry = FontRegistry::new();
    let program = no_pages_program(&overlays, &empty_dims, &registry);
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

    let (msg, status, _state) = press_on(&overlays, None, mouse::Cursor::Unavailable);
    assert!(msg.is_none());
    assert_eq!(status, event::Status::Ignored);
}

#[test]
fn update_click_on_empty_page_records_placement_drag_on_press() {
    // On mouse-down over a blank page area, placement is deferred to mouse-up.
    // In multi-page mode, page 1 starts at y=PAGE_GAP/2=8, centered at x=194.
    let overlays: Vec<TextOverlay> = vec![];

    let (msg, status, state) = press_on(&overlays, None, cursor_at(300.0, 200.0));
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
    let cursor = cursor_at(300.0, 200.0);

    // Release at the same spot (distance < 10px) → single-line PlaceOverlay
    let (page, position) = single_line_placement_from(cursor, cursor);
    assert_eq!(page, 1);
    assert!((position.x - 106.0).abs() < 0.5);
    assert!((position.y - 600.0).abs() < 0.5);
}

#[test]
fn update_drag_on_empty_page_places_multi_line_overlay_on_release() {
    // Drag from screen (300, 200) to (450, 200) — 150px horizontal drag
    // At zoom=1, dpi=72 (scale=1.0): 150 screen px = 150 PDF pts
    // Start: pdf_x = 300 - 194 = 106, pdf_y = 792 - (200 - 8) = 600
    // End: pdf_x = 450 - 194 = 256
    // Width = |256 - 106| = 150 pts
    // Press at (300, 200), release at (450, 200) — 150px drag, well over the
    // 10px threshold
    let (page, top_left, width, _) =
        text_box_placement_from(cursor_at(300.0, 200.0), cursor_at(450.0, 200.0));
    assert_eq!(page, 1);
    assert!((top_left.x - 106.0).abs() < 1.0);
    assert!((top_left.y - 600.0).abs() < 1.0);
    assert!(
        (width - 150.0).abs() < 1.0,
        "expected width ~150, got {width}"
    );
}

#[test]
fn a_placement_drag_publishes_the_whole_rectangle_it_drew() {
    // spe-x9e: the release kept only the horizontal extent and the y the drag
    // started at, so the box the editor opened bore no relation to the
    // rectangle drawn on screen.
    // Drag (300, 200) -> (450, 300): 150 x 100 screen px, scale 1.
    let (page, top_left, width, height) =
        text_box_placement_from(cursor_at(300.0, 200.0), cursor_at(450.0, 300.0));
    assert_eq!(page, 1);
    assert!((top_left.x - 106.0).abs() < 1.0, "left: {}", top_left.x);
    assert!((top_left.y - 600.0).abs() < 1.0, "top: {}", top_left.y);
    assert!((width - 150.0).abs() < 1.0, "width: {width}");
    assert!((height - 100.0).abs() < 1.0, "height: {height}");
}

#[test]
fn a_placement_drag_reports_the_same_rectangle_whichever_corner_it_started_at() {
    // Dragging up and left draws the same rectangle as dragging down and
    // right, so it must place the same box.
    let downward = text_box_placement_from(cursor_at(300.0, 200.0), cursor_at(450.0, 300.0));
    let upward = text_box_placement_from(cursor_at(450.0, 300.0), cursor_at(300.0, 200.0));
    assert_eq!(downward.0, upward.0);
    assert!((downward.1.x - upward.1.x).abs() < 1.0, "left edge differs");
    assert!((downward.1.y - upward.1.y).abs() < 1.0, "top edge differs");
    assert!((downward.2 - upward.2).abs() < 1.0, "width differs");
    assert!((downward.3 - upward.3).abs() < 1.0, "height differs");
}

#[test]
fn a_placement_drag_off_the_page_stops_at_the_page_edge() {
    // The page occupies screen x 194..806 and y 8..800, so a drag released
    // far outside it draws a box that ends at the page's bottom-right corner.
    let (_, top_left, width, height) =
        text_box_placement_from(cursor_at(300.0, 200.0), cursor_at(2000.0, 2000.0));
    assert!(
        (top_left.x + width - 612.0).abs() < 1.0,
        "box runs to {} but the page ends at 612",
        top_left.x + width
    );
    assert!(
        (top_left.y - height).abs() < 1.0,
        "box bottom is {} but the page ends at 0",
        top_left.y - height
    );
}

#[test]
fn update_click_on_overlay_selects_it() {
    // Overlay at PDF (72, 720) → screen (266, 80) in multi-page mode
    // page at y=8, so screen_y = (792-720) + 8 = 80
    // Courier 12pt "Hello": hit box x=[266, 302], y=[68, 80]
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];

    let (msg, status, _state) = press_on(&overlays, None, cursor_at(270.0, 75.0));
    assert_eq!(status, event::Status::Captured);
    assert!(matches!(msg, Some(Message::SelectOverlay(0))));
}

#[test]
fn update_click_on_overlay_starts_drag() {
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(270.0, 75.0);

    program.update(&mut state, &left_press_event(), bounds, cursor);
    assert!(state.drag.is_some());
    let drag = state.drag.as_ref().unwrap();
    assert_eq!(drag.overlay_index, 0);
    assert!((drag.anchor.position.x - 72.0).abs() < 0.01);
    assert!((drag.anchor.position.y - 720.0).abs() < 0.01);
}

#[test]
fn update_click_outside_page_deselects() {
    // Page image bounds: (194, 104) to (806, 896).
    // Click at (50, 50) which is outside the page.
    let overlays: Vec<TextOverlay> = vec![];

    let (msg, status, _state) = press_on(&overlays, None, cursor_at(50.0, 50.0));
    assert_eq!(status, event::Status::Captured);
    assert!(matches!(msg, Some(Message::DeselectOverlay)));
}

#[test]
fn update_click_while_editing_commits_text_first() {
    // Iced actions carry a single message, so clicking while editing
    // returns CommitText only. The place/select happens on the next click.
    let overlays: Vec<TextOverlay> = vec![];
    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = OverlayCanvasProgram {
        editing: true,
        active_overlay: Some(0),
        ..test_program(&overlays, &dims, &registry)
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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
    // The overlay moved. The new position reflects a 100px shift in screen
    // space converted to PDF space: at zoom=1, dpi=72 scale is 1.0, so 100
    // screen px = 100 PDF pts, and screen-down is PDF-y-decreasing, taking
    // (72, 720) to (172, 620).
    assert_moved_to(msg, 0, PdfPosition { x: 172.0, y: 620.0 });
}

#[test]
fn update_drag_release_at_same_position_no_move_message() {
    // Click on overlay, don't move, release → no MoveOverlay needed
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
    let state = state_with_drag();
    let bounds = test_canvas_bounds();
    let cursor = cursor_at(300.0, 200.0);

    let interaction = program.mouse_interaction(&state, bounds, cursor);
    assert_eq!(interaction, mouse::Interaction::Grabbing);
}

#[test]
fn mouse_interaction_pointer_over_overlay() {
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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
    let empty_dims: HashMap<u32, (f32, f32)> = HashMap::new();
    let registry = FontRegistry::new();
    let program = no_pages_program(&overlays, &empty_dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let (msg, status, state) = press_on(&overlays, None, cursor_at(270.0, 75.0));
    assert_eq!(status, event::Status::Captured);
    assert!(matches!(msg, Some(Message::SelectOverlay(0))));
    // last_click is recorded after a hit
    assert!(state.last_click.is_some());
}

#[test]
fn double_click_on_overlay_emits_edit_overlay() {
    // Two rapid clicks at the same position on an overlay → EditOverlay.
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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
        font: FontRegistry::new().find_by_name("Courier").unwrap(),
        font_size: 12.0,
        width: Some(width),
        min_height: None,
    }
}

#[test]
fn resize_drag_state_construction() {
    let state = ResizeDragState {
        overlay_index: 2,
        anchor: OverlayAnchor {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            width: None,
        },
        edge: ResizeEdge::Right,
        initial_box: OverlayBox {
            width: 150.0,
            min_height: 0.0,
        },
    };
    assert_eq!(state.overlay_index, 2);
    assert_eq!(state.edge, ResizeEdge::Right);
    assert!((state.initial_box.width - 150.0).abs() < f32::EPSILON);
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
    let (msg, status, state) = press_on(&overlays, Some(0), cursor_at(416.0, 75.0));
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
    assert!((rd.initial_box.width - 150.0).abs() < 0.5);
    assert_eq!(
        rd.edge,
        ResizeEdge::Right,
        "the right-edge bar resizes width only"
    );
}

#[test]
fn resize_drag_on_single_line_overlay_does_not_start() {
    // Single-line overlays (width=None) have no resize handle.
    // Click at the same x position should fall through to normal overlay hit test.
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")]; // width=None
    // Click at x=416, y=75 — but overlay has no width, so no handle exists there
    let (_, _, state) = press_on(&overlays, Some(0), cursor_at(416.0, 75.0));
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
    // active_overlay is None — not selected
    let (_, _, state) = press_on(&overlays, None, cursor_at(416.0, 75.0));
    assert!(
        state.resize_drag.is_none(),
        "resize drag should not start when overlay not selected"
    );
}

#[test]
fn resize_drag_release_publishes_resize_overlay_message() {
    // Drag from handle at x=416 to x=516 (100px rightward) → new_width = 150 + 100 = 250pt
    let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
    let (_, _, mut state) = press_on(&overlays, Some(0), cursor_at(416.0, 75.0));
    assert!(state.resize_drag.is_some());
    let (msg, status) = release_on(&mut state, &overlays, Some(0), cursor_at(516.0, 75.0));
    assert_eq!(status, event::Status::Captured);
    assert!(state.resize_drag.is_none());
    match msg {
        Some(Message::ResizeOverlay {
            index,
            old_box,
            new_box,
        }) => {
            assert_eq!(index, 0);
            let (old_width, new_width) = (old_box.width, new_box.width);
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
    let (_, _, mut state) = press_on(&overlays, Some(0), cursor_at(416.0, 75.0));
    let (msg, _status) = release_on(&mut state, &overlays, Some(0), cursor_at(271.0, 75.0));
    match msg {
        Some(Message::ResizeOverlay { new_box, .. }) => {
            assert!(
                new_box.width >= super::MIN_BOX_DIMENSION,
                "new width should be at least the minimum, got {}",
                new_box.width
            );
        }
        other => panic!("Expected ResizeOverlay, got {other:?}"),
    }
}

#[test]
fn resize_drag_release_at_same_position_emits_no_message() {
    // If the user presses and releases the handle without moving, no resize needed.
    let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
    let (_, _, mut state) = press_on(&overlays, Some(0), cursor_at(416.0, 75.0));
    assert!(state.resize_drag.is_some());
    let (msg, status) = release_on(&mut state, &overlays, Some(0), cursor_at(416.0, 75.0));
    assert_eq!(status, event::Status::Captured);
    assert!(state.resize_drag.is_none());
    assert!(
        msg.is_none(),
        "no change in width → no ResizeOverlay message, got {msg:?}"
    );
}

#[test]
fn cursor_move_requests_redraw_during_resize_drag() {
    let registry = FontRegistry::new();
    let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "Hello")];
    let dims = test_page_dimensions();
    let program = OverlayCanvasProgram {
        active_overlay: Some(0),
        ..test_program(&overlays, &dims, &registry)
    };
    let mut state = ProgramState::default();
    state.resize_drag = Some(ResizeDragState {
        overlay_index: 0,
        anchor: OverlayAnchor::of(&overlays[0]),
        edge: ResizeEdge::Right,
        initial_box: OverlayBox::of(&overlays[0]).unwrap(),
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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
    let registry = FontRegistry::new();
    // Regression: overlay disappeared when drag-moving after text commit.
    // Simulates: editing overlay → commit → click overlay → drag → release.
    // The ProgramState persists across program changes (editing → not editing).
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];

    let dims = test_page_dimensions();
    let bounds = test_canvas_bounds();
    let mut state = ProgramState::default();

    // Phase 1: User is editing. Move cursor to overlay position to set cursor_position.
    let editing_program = OverlayCanvasProgram {
        editing: true,
        active_overlay: Some(0),
        ..test_program(&overlays, &dims, &registry)
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
    let committed_program = OverlayCanvasProgram {
        editing: false,
        active_overlay: Some(0),
        ..test_program(&overlays, &dims, &registry)
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
    assert_moved_to(msg, 0, PdfPosition { x: 172.0, y: 620.0 });
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
// spe-ceg.2 / spe-ner: overlay text box geometry (tint and hover border)
// =====================================================================

/// Rendered line height for a 12pt overlay at scale 1.0.
const LINE_12PT: f32 = 12.0 * super::TEXT_LINE_HEIGHT_RATIO;

#[test]
fn text_box_top_edge_is_one_font_size_above_the_baseline() {
    // draw_overlay_text places the text top at baseline - font_size, so the
    // tint must start there rather than at the baseline.
    let overlay = overlay_at(72.0, 720.0, "Hello");
    let rect = super::overlay_text_box(&overlay, 40.0, 100.0, 1.0, &FontRegistry::new());
    assert!(
        (rect.x - 40.0).abs() < 0.1,
        "x should be 40, got {}",
        rect.x
    );
    assert!(
        (rect.y - 88.0).abs() < 0.1,
        "top should be baseline - font_size = 88, got {}",
        rect.y
    );
}

#[test]
fn single_line_text_box_width_comes_from_the_font_bounding_box() {
    // Courier 12pt, "Hello" → 5 * 7.2 = 36.0 wide, one rendered line tall.
    let overlay = overlay_at(72.0, 720.0, "Hello");
    let rect = super::overlay_text_box(&overlay, 0.0, 100.0, 1.0, &FontRegistry::new());
    assert!(
        (rect.width - 36.0).abs() < 0.1,
        "width should be ~36, got {}",
        rect.width
    );
    assert!(
        (rect.height - LINE_12PT).abs() < 0.1,
        "height should be one line ({LINE_12PT}), got {}",
        rect.height
    );
}

#[test]
fn multiline_text_box_grows_downward_from_the_first_line() {
    // spe-ner: text is drawn top-down from baseline - font_size, so a 3-line
    // overlay must extend *below* the baseline, not upward from it.
    let overlay = multiline_overlay_at(72.0, 720.0, 150.0, "one\ntwo\nthree");
    let rect = super::overlay_text_box(&overlay, 0.0, 100.0, 1.0, &FontRegistry::new());
    assert!(
        (rect.y - 88.0).abs() < 0.1,
        "top should be baseline - font_size = 88, got {}",
        rect.y
    );
    let expected_height = 3.0 * LINE_12PT;
    assert!(
        (rect.height - expected_height).abs() < 0.1,
        "height should be 3 rendered lines ({expected_height}), got {}",
        rect.height
    );
    assert!(
        rect.y + rect.height > 100.0,
        "box must extend below the first line's baseline, got bottom {}",
        rect.y + rect.height
    );
}

#[test]
fn multiline_text_box_width_comes_from_the_overlay_width() {
    let overlay = multiline_overlay_at(72.0, 720.0, 150.0, "Hello");
    let rect = super::overlay_text_box(&overlay, 0.0, 100.0, 1.0, &FontRegistry::new());
    assert!(
        (rect.width - 150.0).abs() < 0.1,
        "width should be the overlay width 150, got {}",
        rect.width
    );
    assert!(
        (rect.height - LINE_12PT).abs() < 0.1,
        "single line of text is one rendered line tall, got {}",
        rect.height
    );
}

#[test]
fn text_box_scales_with_the_render_scale() {
    let overlay = overlay_at(72.0, 720.0, "Hello");
    let rect = super::overlay_text_box(&overlay, 0.0, 100.0, 2.0, &FontRegistry::new());
    assert!(
        (rect.width - 72.0).abs() < 0.1,
        "width should double to 72, got {}",
        rect.width
    );
    assert!(
        (rect.height - 2.0 * LINE_12PT).abs() < 0.1,
        "height should double, got {}",
        rect.height
    );
    assert!(
        (rect.y - 76.0).abs() < 0.1,
        "top should be baseline - 2 * font_size = 76, got {}",
        rect.y
    );
}

#[test]
fn text_line_height_ratio_matches_the_canvas_text_default() {
    // TEXT_LINE_HEIGHT_RATIO mirrors iced's own default. If an iced upgrade
    // changes it, the tint silently stops matching the text again (spe-ner).
    // Compares the *resolved* line height for a known font size rather than the
    // LineHeight variant, so it also catches a switch to an absolute default.
    let font_size = 100.0;
    let resolved: f32 = canvas::Text::default()
        .line_height
        .to_absolute(iced::Pixels(font_size))
        .into();
    let expected = font_size * super::TEXT_LINE_HEIGHT_RATIO;
    assert!(
        (resolved - expected).abs() < 0.01,
        "iced lays out {font_size}pt canvas text on {resolved}px lines, but \
         TEXT_LINE_HEIGHT_RATIO expects {expected}px"
    );
}

/// Courier 12pt advances 7.2pt per character, so a 72pt-wide box holds ten
/// characters. This text has no explicit line breaks and cannot fit.
const WRAPPING_TEXT: &str = "mmmm mmmm mmmm mmmm mmmm mmmm";
const WRAPPING_BOX_WIDTH: f32 = 72.0;

#[test]
fn a_text_box_spans_the_lines_its_text_wraps_onto() {
    // spe-0hl: the box counted `\n`s, so text that wrapped inside the box the
    // user drew was framed as if it were a single line.
    let overlay = multiline_overlay_at(72.0, 720.0, WRAPPING_BOX_WIDTH, WRAPPING_TEXT);
    let rect = super::overlay_text_box(&overlay, 0.0, 100.0, 1.0, &FontRegistry::new());
    assert!(
        rect.height >= 3.0 * LINE_12PT,
        "{WRAPPING_TEXT:?} wraps onto at least 3 lines in a {WRAPPING_BOX_WIDTH}pt box, \
         but the box is {} tall (one line is {LINE_12PT})",
        rect.height
    );
}

#[test]
fn the_resize_handle_reaches_the_last_wrapped_line() {
    // spe-0hl: the handle spans the text box's right edge, so it only reaches
    // every wrapped line once the box itself does.
    let overlays = vec![multiline_overlay_at(
        72.0,
        720.0,
        WRAPPING_BOX_WIDTH,
        WRAPPING_TEXT,
    )];
    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = OverlayCanvasProgram {
        active_overlay: Some(0),
        ..test_program(&overlays, &dims, &registry)
    };
    // Baseline is screen (266, 80), so the box starts at y = 68 and the third
    // wrapped line runs from 96.8 to 111.2.
    let mut state = ProgramState::default();
    let cursor = cursor_at(266.0 + WRAPPING_BOX_WIDTH, 68.0 + 2.5 * LINE_12PT);

    program.update(
        &mut state,
        &left_press_event(),
        test_canvas_bounds(),
        cursor,
    );
    assert!(
        state.resize_drag.is_some(),
        "pressing beside the last wrapped line should start a resize"
    );
}

#[test]
fn a_box_dragged_taller_than_its_text_keeps_the_room_it_was_given() {
    // spe-x9e: the box the user dragged out is the box they get, whitespace
    // below the text included.
    let mut overlay = multiline_overlay_at(72.0, 720.0, 150.0, "one");
    overlay.min_height = Some(100.0);
    let rect = super::overlay_text_box(&overlay, 0.0, 100.0, 1.0, &FontRegistry::new());
    assert!(
        (rect.height - 100.0).abs() < 0.1,
        "a 100pt box holding one line should stay 100pt tall, got {}",
        rect.height
    );
}

#[test]
fn a_box_grows_past_its_minimum_once_the_text_needs_the_room() {
    // spe-b2h: typing past the bottom of the box grows it rather than
    // scrolling inside it.
    let mut overlay = multiline_overlay_at(72.0, 720.0, 150.0, "one\ntwo\nthree\nfour\nfive");
    overlay.min_height = Some(20.0);
    let rect = super::overlay_text_box(&overlay, 0.0, 100.0, 1.0, &FontRegistry::new());
    assert!(
        (rect.height - 5.0 * LINE_12PT).abs() < 0.1,
        "five lines need {} but the box is {}",
        5.0 * LINE_12PT,
        rect.height
    );
}

#[test]
fn a_minimum_height_scales_with_the_render_scale() {
    let mut overlay = multiline_overlay_at(72.0, 720.0, 150.0, "one");
    overlay.min_height = Some(100.0);
    let rect = super::overlay_text_box(&overlay, 0.0, 100.0, 2.0, &FontRegistry::new());
    assert!(
        (rect.height - 200.0).abs() < 0.1,
        "at double scale a 100pt box is 200px tall, got {}",
        rect.height
    );
}

#[test]
fn empty_overlay_text_box_is_one_line_tall() {
    let overlay = multiline_overlay_at(72.0, 720.0, 150.0, "");
    let rect = super::overlay_text_box(&overlay, 0.0, 100.0, 1.0, &FontRegistry::new());
    assert!(
        (rect.height - LINE_12PT).abs() < 0.1,
        "empty text still occupies one line, got {}",
        rect.height
    );
}

// =====================================================================
// spe-ceg.3 / spe-i4e: tint opacity
// =====================================================================

/// Roughly how much darker than a white page a tint reads, in 0-255 units,
/// after compositing `color` at `alpha` over white.
///
/// This is a guardrail, not a colorimetric model: it weights the stored
/// sRGB-encoded channels with the BT.709 luma coefficients without linearizing
/// them, while wgpu composites in linear space. The absolute number is
/// therefore optimistic — the real on-screen drop for the default tint is
/// nearer 27 than the 38 computed here. What it does reliably capture is
/// ordering: brighter tints score higher, so it catches an opacity regression.
fn tint_luminance_drop_over_white_page(color: [f32; 4], alpha: f32) -> f32 {
    let luma = 0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2];
    255.0 * (1.0 - luma) * alpha
}

/// Score a tint must reach to read as a highlight rather than as blank paper.
/// `committed_overlay_paints_a_visible_tint_without_hover` measures the real
/// composited output and is the authoritative check; this one is a cheap
/// arithmetic guard on the constant itself.
/// Calibrated against observed renders: the old 0.15 alpha scored 19 and was
/// reported invisible (spe-i4e); the current 0.30 alpha scores 38 and is
/// clearly visible in screenshots. 30 sits between them, closer to the value
/// that was confirmed good.
const PERCEPTIBLE_LUMINANCE_DROP: f32 = 30.0;

#[test]
fn resting_tint_is_perceptible_against_a_white_page() {
    // spe-i4e: the tint is always drawn, but at too low an opacity it is
    // indistinguishable from the page, so it reads as "only shows on hover".
    let drop = tint_luminance_drop_over_white_page(
        crate::config::AppConfig::default().overlay_color,
        super::tint_alpha(false),
    );
    assert!(
        drop >= PERCEPTIBLE_LUMINANCE_DROP,
        "resting tint is only {drop} darker than white paper, needs >= {PERCEPTIBLE_LUMINANCE_DROP}"
    );
}

#[test]
fn hovered_tint_is_more_prominent_than_the_resting_tint() {
    assert!(
        super::tint_alpha(true) > super::tint_alpha(false),
        "hover should deepen the tint"
    );
}

#[test]
fn overlay_tint_hover_border_alpha_is_stronger_than_the_hover_fill() {
    assert!(
        super::OVERLAY_TINT_HOVER_BORDER_ALPHA > super::tint_alpha(true),
        "the hover border must stand out against its own fill"
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
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

    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = test_program(&overlays, &dims, &registry);
    let mut state = state_with_drag();
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

// =====================================================================
// spe-dcj: the floating edit widget draws its own border, so the canvas
// must not also draw a selection box for the overlay being edited
// =====================================================================

#[test]
fn should_draw_selection_box_false_while_editing_active_overlay() {
    assert!(
        !super::should_draw_selection_box(true, Some(0), 0),
        "the floating edit widget already draws a border around the edited overlay"
    );
}

#[test]
fn should_draw_selection_box_true_when_selected_and_not_editing() {
    assert!(
        super::should_draw_selection_box(false, Some(0), 0),
        "a selected overlay that is not being edited shows a selection box"
    );
}

#[test]
fn should_draw_selection_box_false_for_unselected_overlay() {
    assert!(
        !super::should_draw_selection_box(false, Some(0), 1),
        "only the selected overlay gets a selection box"
    );
}

#[test]
fn should_draw_selection_box_false_when_nothing_selected() {
    assert!(
        !super::should_draw_selection_box(false, None, 0),
        "no selection means no selection box"
    );
}

// =====================================================================
// spe-i4e / spe-ner: headless rendering of the overlay canvas
//
// These render the real canvas Program through iced's software renderer so
// the drawing code itself runs under plain `cargo test` — no GPU, display or
// compositor required. The pure geometry tests above pin down the tint rect;
// these prove it actually reaches the screen.
// =====================================================================

/// Size of the headless render surface, in logical pixels.
/// Render an overlay canvas program over a white page, headlessly.
///
/// When `cursor` is given it is delivered as a real cursor-moved event first,
/// so hover state is established through the same path the app uses.
fn render_overlay_canvas(
    program: OverlayCanvasProgram<'_>,
    cursor: Option<iced::Point>,
) -> RenderedCanvas {
    let element: iced::Element<Message> = iced::widget::canvas(program)
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into();
    render_element(element, cursor)
}

/// Screen-space baseline of an overlay inside the rendered canvas.
fn rendered_baseline(overlay: &TextOverlay, page_dims: &HashMap<u32, (f32, f32)>) -> (f32, f32) {
    let layout = page_layout(page_dims, 1, TEST_ZOOM, TEST_DPI);
    let page_rect = page_rect_in_canvas(&layout, overlay.page, RENDER_SIZE.width);
    let (_, page_h) = page_dims[&overlay.page];
    let params = ConversionParams {
        zoom: TEST_ZOOM,
        dpi: TEST_DPI,
        page_height: page_h,
        offset_x: page_rect.x,
        offset_y: page_rect.y,
    };
    crate::coordinate::pdf_to_screen(overlay.position.x, overlay.position.y, &params)
}

#[test]
fn committed_overlay_paints_a_visible_tint_without_hover() {
    // spe-i4e: with nothing hovered the tint must still read as a highlight
    // against white paper. Measured on the real composited pixels.
    let dims = uniform_page_dims(1);
    let registry = FontRegistry::new();
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let canvas = render_overlay_canvas(test_program(&overlays, &dims, &registry), None);

    let (sx, sy) = rendered_baseline(&overlays[0], &dims);
    // Sample just inside the top-left of the tint, clear of the glyphs.
    let x = sx as u32 + 2;
    let y = (sy - 12.0 * TEST_ZOOM) as u32 + 2;
    let darkening = canvas.darkening(x, y);

    // Calibrated on real composited output: the old 0.15 alpha that was
    // reported invisible measures ~25 here, the current 0.30 alpha ~51.
    assert!(
        darkening >= 40.0,
        "resting tint at ({x}, {y}) is only {darkening} darker than white paper"
    );
}

#[test]
fn hovered_overlay_paints_a_deeper_tint_than_a_resting_one() {
    let dims = uniform_page_dims(1);
    let registry = FontRegistry::new();
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let (sx, sy) = rendered_baseline(&overlays[0], &dims);
    let x = sx as u32 + 2;
    let y = (sy - 12.0 * TEST_ZOOM) as u32 + 2;

    let resting =
        render_overlay_canvas(test_program(&overlays, &dims, &registry), None).darkening(x, y);
    // Park the cursor over the overlay so the canvas hit-tests it as hovered.
    let hovered = render_overlay_canvas(
        test_program(&overlays, &dims, &registry),
        Some(iced::Point::new(sx + 4.0, sy - 4.0)),
    )
    .darkening(x, y);

    assert!(
        hovered > resting,
        "hovered tint ({hovered}) should be deeper than resting tint ({resting})"
    );
}

#[test]
fn multiline_tint_covers_the_rows_its_text_occupies() {
    // spe-ner: the tint band must start at the top of the first line and run
    // downward over every line, not float above the text.
    let dims = uniform_page_dims(1);
    let registry = FontRegistry::new();
    let overlays = vec![multiline_overlay_at(72.0, 600.0, 150.0, "one\ntwo\nthree")];
    let canvas = render_overlay_canvas(test_program(&overlays, &dims, &registry), None);

    let (sx, sy) = rendered_baseline(&overlays[0], &dims);
    // Sample a column near the right edge of the box, past the short glyphs,
    // so only the tint contributes.
    let column = (sx + 140.0 * TEST_ZOOM) as u32;
    let rows = canvas.darkened_rows(column, 10.0, RENDER_SIZE.height as u32);
    assert!(!rows.is_empty(), "no tint band found in column {column}");

    let scaled_font_size = 12.0 * TEST_ZOOM;
    let expected_top = sy - scaled_font_size;
    let expected_bottom = expected_top + 3.0 * scaled_font_size * super::TEXT_LINE_HEIGHT_RATIO;
    let top = *rows.first().unwrap() as f32;
    let bottom = *rows.last().unwrap() as f32;

    assert!(
        (top - expected_top).abs() <= 1.5,
        "tint starts at row {top}, expected the first line's top {expected_top}"
    );
    assert!(
        (bottom - expected_bottom).abs() <= 1.5,
        "tint ends at row {bottom}, expected the third line's bottom {expected_bottom}"
    );
}

#[test]
fn selected_overlay_paints_a_selection_border() {
    let dims = uniform_page_dims(1);
    let registry = FontRegistry::new();
    let overlays = vec![multiline_overlay_at(72.0, 600.0, 150.0, "one\ntwo")];
    let mut program = test_program(&overlays, &dims, &registry);
    program.active_overlay = Some(0);

    let canvas = render_overlay_canvas(program, None);
    assert!(
        canvas.selection_blue_pixels() > 0,
        "a selected overlay should paint a selection border and resize handle"
    );
}

#[test]
fn overlay_being_edited_paints_no_selection_border() {
    // The floating text widget draws its own border while editing, so the
    // canvas must stay out of the way (#113).
    let dims = uniform_page_dims(1);
    let registry = FontRegistry::new();
    let overlays = vec![multiline_overlay_at(72.0, 600.0, 150.0, "one\ntwo")];
    let mut program = test_program(&overlays, &dims, &registry);
    program.active_overlay = Some(0);
    program.editing = true;

    let canvas = render_overlay_canvas(program, None);
    assert_eq!(
        canvas.selection_blue_pixels(),
        0,
        "the canvas must not draw a second selection border while editing"
    );
}

// =====================================================================
// spe-x2z: the selection box outlines the same geometry as the tint
// =====================================================================

/// Render a selected (not editing) overlay and return the drawn selection
/// bounds together with the text box they are supposed to outline.
fn selection_bounds_and_text_box(overlay: TextOverlay) -> ((u32, u32, u32, u32), iced::Rectangle) {
    let dims = uniform_page_dims(1);
    let registry = FontRegistry::new();
    let overlays = vec![overlay];
    let mut program = test_program(&overlays, &dims, &registry);
    program.active_overlay = Some(0);

    let (sx, sy) = rendered_baseline(&overlays[0], &dims);
    let text_box = super::overlay_text_box(
        &overlays[0],
        sx,
        sy,
        crate::coordinate::render_scale(TEST_ZOOM, TEST_DPI),
        &registry,
    );
    let bounds = render_overlay_canvas(program, None)
        .selection_blue_bounds()
        .expect("a selected overlay must paint a selection border");
    (bounds, text_box)
}

#[test]
fn multiline_selection_box_outlines_every_line_of_the_text_box() {
    // spe-x2z: draw_selection_box used a "one font size tall, grows upward"
    // box, so a multi-line overlay's border cut off after the first line and
    // disagreed with the tint it is meant to frame.
    let (bounds, text_box) =
        selection_bounds_and_text_box(multiline_overlay_at(72.0, 600.0, 150.0, "one\ntwo\nthree"));
    let (_, top, _, bottom) = bounds;

    let expected_top = text_box.y - super::SELECTION_BOX_PADDING;
    let expected_bottom = text_box.y + text_box.height + super::SELECTION_BOX_PADDING;
    assert!(
        (top as f32 - expected_top).abs() <= 2.0,
        "selection border starts at row {top}, expected the text box top {expected_top}"
    );
    assert!(
        (bottom as f32 - expected_bottom).abs() <= 2.0,
        "selection border ends at row {bottom}, expected the text box bottom {expected_bottom}"
    );
}

#[test]
fn selection_box_rect_pads_the_text_box_on_every_side() {
    // The border frames the tint, so it is the text box grown by the padding
    // rather than a separately computed rectangle (spe-x2z).
    let text_box = iced::Rectangle {
        x: 40.0,
        y: 88.0,
        width: 150.0,
        height: 28.8,
    };
    let rect = super::selection_box_rect(text_box);
    let pad = super::SELECTION_BOX_PADDING;
    assert!((rect.x - (text_box.x - pad)).abs() < 0.01, "x: {}", rect.x);
    assert!((rect.y - (text_box.y - pad)).abs() < 0.01, "y: {}", rect.y);
    assert!(
        (rect.width - (text_box.width + 2.0 * pad)).abs() < 0.01,
        "width: {}",
        rect.width
    );
    assert!(
        (rect.height - (text_box.height + 2.0 * pad)).abs() < 0.01,
        "height: {}",
        rect.height
    );
}

#[test]
fn single_line_selection_box_matches_the_tinted_text_box() {
    // The tint is one line height tall but the border was only one font size,
    // leaving 0.2 * font_size of tint hanging below the border.
    let (bounds, text_box) = selection_bounds_and_text_box(overlay_at(72.0, 600.0, "Hello"));
    let (_, top, _, bottom) = bounds;

    let expected_top = text_box.y - super::SELECTION_BOX_PADDING;
    let expected_bottom = text_box.y + text_box.height + super::SELECTION_BOX_PADDING;
    assert!(
        (top as f32 - expected_top).abs() <= 2.0,
        "selection border starts at row {top}, expected {expected_top}"
    );
    assert!(
        (bottom as f32 - expected_bottom).abs() <= 2.0,
        "selection border ends at row {bottom}, expected {expected_bottom}"
    );
}

#[test]
fn resize_handle_hit_area_covers_the_last_line_of_a_multiline_overlay() {
    // The handle is drawn down the whole right edge of the selection box, so
    // clicking beside the last line must start a resize, not a move.
    let overlays = vec![multiline_overlay_at(72.0, 720.0, 150.0, "one\ntwo\nthree")];
    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = OverlayCanvasProgram {
        active_overlay: Some(0),
        ..test_program(&overlays, &dims, &registry)
    };
    let mut state = ProgramState::default();
    let bounds = test_canvas_bounds();
    // Baseline is screen y=80, so the third line ends at 68 + 3 * 14.4 = 111.2.
    let cursor = cursor_at(416.0, 105.0);

    program.update(&mut state, &left_press_event(), bounds, cursor);
    assert!(
        state.resize_drag.is_some(),
        "the resize handle must extend beside every line of the overlay"
    );
}

// =====================================================================
// spe-m66: the floating editor lays text out on the same lines as the
// canvas, so text does not jump when an edit session starts or ends
// =====================================================================

impl RenderedCanvas {
    /// Top row of each horizontal band of ink, scanning the whole surface.
    /// A band is a run of consecutive rows containing at least one pixel
    /// darker than `threshold`, so one band is one line of text.
    fn ink_band_tops(&self, threshold: f32) -> Vec<u32> {
        let mut tops = Vec::new();
        let mut in_band = false;
        for y in 0..self.height() {
            let inked = (0..self.width).any(|x| self.darkening(x, y) >= threshold);
            if inked && !in_band {
                tops.push(y);
            }
            in_band = inked;
        }
        tops
    }
}

/// Render the floating multi-line editor exactly as the app configures it,
/// and return the top row of each rendered line of text.
fn editor_line_tops(font_size: f32) -> Vec<u32> {
    let content = iced::widget::text_editor::Content::with_text("H\nH\nH");
    let editor: iced::Element<Message> = iced::widget::text_editor(&content)
        .size(iced::Pixels(font_size))
        .line_height(super::TEXT_LINE_HEIGHT)
        .padding(iced::Padding::ZERO)
        .style(|_theme, _status| iced::widget::text_editor::Style {
            background: iced::Background::Color(iced::Color::TRANSPARENT),
            border: iced::Border::default(),
            placeholder: iced::Color::BLACK,
            value: iced::Color::BLACK,
            selection: iced::Color::TRANSPARENT,
        })
        .into();
    render_element(editor, None).ink_band_tops(100.0)
}

#[test]
fn the_editor_lays_lines_out_on_the_canvas_line_height() {
    // spe-m66: text_editor defaults to LineHeight::Relative(1.3) while the
    // canvas (and the saved PDF's leading) use 1.2, so text shifted downward
    // line by line the moment an overlay entered edit mode.
    let font_size = 60.0;
    let tops = editor_line_tops(font_size);
    assert!(
        tops.len() >= 2,
        "expected at least two rendered lines of text, found {tops:?}"
    );

    let spacing = (tops[1] - tops[0]) as f32;
    let expected = font_size * super::TEXT_LINE_HEIGHT_RATIO;
    assert!(
        (spacing - expected).abs() <= 1.5,
        "editor lines are {spacing}px apart, but the canvas lays {font_size}px \
         text out on {expected}px lines"
    );
}

// =====================================================================
// spe-01a: widget-local drag state addresses overlays by index, and the
// canvas program has no way to observe an IPC delete/undo mid-drag
// =====================================================================

#[test]
fn drag_release_after_the_dragged_overlay_is_deleted_publishes_no_move() {
    // Deleting the dragged overlay leaves index 0 addressing a *different*
    // overlay, so releasing would silently drag the survivor to the cursor.
    let before = vec![
        overlay_at(72.0, 720.0, "first"),
        overlay_at(72.0, 600.0, "second"),
    ];
    let after = vec![overlay_at(72.0, 600.0, "second")];
    let (msg, state) = drag_across_a_list_change(&before, 0, &after);
    assert!(
        !matches!(msg, Some(Message::MoveOverlay(..))),
        "a drag whose overlay is gone must not move whatever took its index, got {msg:?}"
    );
    assert!(state.drag.is_none(), "the stale drag must be cleared");
}

#[test]
fn resize_release_after_the_resized_overlay_is_deleted_publishes_no_resize() {
    let before = vec![
        multiline_overlay_at(72.0, 720.0, 150.0, "first"),
        multiline_overlay_at(72.0, 600.0, 150.0, "second"),
    ];
    let (_, _, mut state) = press_on(&before, Some(0), cursor_at(416.0, 75.0));
    assert!(
        state.resize_drag.is_some(),
        "press on the handle should start a resize"
    );

    let after = vec![multiline_overlay_at(72.0, 600.0, 150.0, "second")];
    let (msg, _) = release_on(&mut state, &after, Some(0), cursor_at(516.0, 75.0));
    assert!(
        !matches!(msg, Some(Message::ResizeOverlay { .. })),
        "a resize whose overlay is gone must not resize whatever took its index, got {msg:?}"
    );
    assert!(
        state.resize_drag.is_none(),
        "the stale resize must be cleared"
    );
}

#[test]
fn drag_release_still_moves_an_overlay_that_only_shifted_index() {
    // Deleting an *earlier* overlay shifts the dragged one down a slot. The
    // drag anchors on where the overlay actually is, so it must follow it
    // rather than give up.
    let before = vec![
        overlay_at(72.0, 600.0, "first"),
        overlay_at(72.0, 720.0, "dragged"),
    ];
    let after = vec![overlay_at(72.0, 720.0, "dragged")];
    let (msg, _) = drag_across_a_list_change(&before, 1, &after);
    assert!(
        matches!(msg, Some(Message::MoveOverlay(0, _))),
        "the dragged overlay moved to index 0 and should still be the one moved, got {msg:?}"
    );
}

#[test]
fn overlay_anchor_resolves_to_the_unchanged_index() {
    let overlays = vec![overlay_at(72.0, 720.0, "a"), overlay_at(72.0, 600.0, "b")];
    let anchor = OverlayAnchor::of(&overlays[1]);
    assert_eq!(anchor.resolve(&overlays, 1), Some(1));
}

#[test]
fn overlay_anchor_follows_its_overlay_to_a_new_index() {
    let overlays = vec![overlay_at(72.0, 600.0, "b")];
    let anchor = OverlayAnchor {
        page: 1,
        position: PdfPosition { x: 72.0, y: 600.0 },
        width: None,
    };
    assert_eq!(anchor.resolve(&overlays, 1), Some(0));
}

#[test]
fn overlay_anchor_does_not_resolve_once_its_overlay_is_gone() {
    let overlays = vec![overlay_at(72.0, 600.0, "b")];
    let anchor = OverlayAnchor {
        page: 1,
        position: PdfPosition { x: 72.0, y: 720.0 },
        width: None,
    };
    assert!(anchor.resolve(&overlays, 0).is_none());
}

#[test]
fn overlay_anchor_distinguishes_overlays_on_different_pages() {
    let mut on_page_two = overlay_at(72.0, 720.0, "a");
    on_page_two.page = 2;
    let overlays = vec![on_page_two];
    let anchor = OverlayAnchor {
        page: 1,
        position: PdfPosition { x: 72.0, y: 720.0 },
        width: None,
    };
    assert!(anchor.resolve(&overlays, 0).is_none());
}

// =====================================================================
// spe-x2z: single-line box width must come from the font the canvas
// actually renders with, not from the PDF's AFM metrics
// =====================================================================

fn overlay_with_font(font_name: &str, font_size: f32, text: &str) -> TextOverlay {
    TextOverlay {
        page: 1,
        position: PdfPosition { x: 72.0, y: 600.0 },
        text: text.to_string(),
        font: FontRegistry::new().find_by_name(font_name).unwrap(),
        font_size,
        width: None,
        min_height: None,
    }
}

impl RenderedCanvas {
    /// Rightmost column holding glyph ink between `top` and `bottom`.
    ///
    /// The threshold sits far above the overlay tint's darkening (~51 on a
    /// white page) so only the near-black text counts.
    fn rightmost_ink_column(&self, top: u32, bottom: u32) -> Option<u32> {
        (0..self.width)
            .filter(|x| (top..=bottom).any(|y| self.darkening(*x, y) >= 150.0))
            .next_back()
    }
}

#[test]
fn the_text_box_covers_every_glyph_of_a_proportional_font_overlay() {
    // The box width came from the PDF AFM widths, but the canvas renders with
    // whatever system font resolves for the family — Times' advances there run
    // up to a third wider — so trailing glyphs were drawn past the tint and
    // outside the click target.
    let dims = uniform_page_dims(1);
    let registry = FontRegistry::new();
    let overlays = vec![overlay_with_font("Times Bold", 36.0, "Hello world")];
    let canvas = render_overlay_canvas(test_program(&overlays, &dims, &registry), None);

    let (sx, sy) = rendered_baseline(&overlays[0], &dims);
    let text_box = super::overlay_text_box(
        &overlays[0],
        sx,
        sy,
        crate::coordinate::render_scale(TEST_ZOOM, TEST_DPI),
        &registry,
    );
    let rightmost = canvas
        .rightmost_ink_column(text_box.y as u32, (text_box.y + text_box.height) as u32)
        .expect("the overlay text should have been rendered");

    let box_right = text_box.x + text_box.width;
    assert!(
        rightmost as f32 <= box_right,
        "text reaches column {rightmost} but the box ends at {box_right}"
    );
}

// =====================================================================
// spe-01a: anchors must not retarget onto a different overlay that
// happens to sit at the same place
// =====================================================================

#[test]
fn anchor_resolves_to_the_topmost_of_two_overlays_stacked_at_one_spot() {
    // Clicking the same spot twice leaves two overlays sharing a position.
    // hit_test picks the topmost, so the drag that started is the topmost
    // one's, and re-resolving must pick the same one back.
    let stacked_below = overlay_at(72.0, 720.0, "below");
    let stacked_above = overlay_at(72.0, 720.0, "above");
    let before = vec![
        overlay_at(72.0, 600.0, "elsewhere"),
        stacked_below.clone(),
        stacked_above.clone(),
    ];
    let anchor = OverlayAnchor::of(&before[2]);

    // An IPC delete removes the unrelated overlay, shifting the stack down.
    let after = vec![stacked_below, stacked_above];
    assert_eq!(
        anchor.resolve(&after, 2),
        Some(1),
        "resolve must match hit_test's topmost-wins order"
    );
}

#[test]
fn anchor_does_not_resolve_onto_an_overlay_of_a_different_shape() {
    // A multi-line overlay's drag must never land on a single-line overlay:
    // resizing one would silently convert it into a wrapped box.
    let multiline = multiline_overlay_at(72.0, 720.0, 150.0, "text");
    let single_line = overlay_at(72.0, 720.0, "text");
    let anchor = OverlayAnchor::of(&multiline);

    assert!(
        anchor.resolve(&[single_line], 0).is_none(),
        "a single-line overlay cannot stand in for a multi-line one"
    );
}

#[test]
fn anchor_does_not_resolve_onto_a_box_of_a_different_width() {
    // Widths differ, so the recorded old_width would restore a size this
    // overlay never had if undo replayed the resize.
    let dragged = multiline_overlay_at(72.0, 720.0, 150.0, "text");
    let other = multiline_overlay_at(72.0, 720.0, 300.0, "text");
    let anchor = OverlayAnchor::of(&dragged);

    assert!(anchor.resolve(&[other], 0).is_none());
}

#[test]
fn resize_release_does_not_reshape_a_single_line_overlay_that_took_the_index() {
    let before = vec![
        multiline_overlay_at(72.0, 720.0, 150.0, "wrapped"),
        overlay_at(72.0, 720.0, "plain"),
    ];
    let (_, _, mut state) = press_on(&before, Some(0), cursor_at(416.0, 75.0));
    assert!(state.resize_drag.is_some(), "press should start a resize");

    // The wrapped overlay is deleted mid-drag; the single-line one inherits
    // index 0 and sits at the same position.
    let after = vec![overlay_at(72.0, 720.0, "plain")];
    let (msg, _) = release_on(&mut state, &after, Some(0), cursor_at(516.0, 75.0));
    assert!(
        !matches!(msg, Some(Message::ResizeOverlay { .. })),
        "resizing must not turn a single-line overlay into a wrapped one, got {msg:?}"
    );
}

/// Top row of the first line of ink drawn by the single-line edit widget.
/// `line_height` of `None` leaves iced's own default in place.
fn text_input_first_line_top(
    font_size: f32,
    line_height: Option<iced::widget::text::LineHeight>,
) -> u32 {
    let mut input = iced::widget::text_input("", "H")
        .size(iced::Pixels(font_size))
        .padding(iced::Padding::ZERO)
        .style(|_theme, _status| iced::widget::text_input::Style {
            background: iced::Background::Color(iced::Color::TRANSPARENT),
            border: iced::Border::default(),
            icon: iced::Color::BLACK,
            placeholder: iced::Color::BLACK,
            value: iced::Color::BLACK,
            selection: iced::Color::TRANSPARENT,
        });
    if let Some(line_height) = line_height {
        input = input.line_height(line_height);
    }
    *render_element(input.into(), None)
        .ink_band_tops(100.0)
        .first()
        .expect("the input should render its text")
}

#[test]
fn the_single_line_edit_widget_needs_its_line_height_set_explicitly() {
    // spe-m66: text_input keeps iced's own default (1.3) unless told
    // otherwise. If a future iced made the default match the canvas this
    // would fail, and `.line_height(TEXT_LINE_HEIGHT)` in view.rs could go.
    let font_size = 60.0;
    let defaulted = text_input_first_line_top(font_size, None);
    let pinned = text_input_first_line_top(font_size, Some(super::TEXT_LINE_HEIGHT));
    assert_ne!(
        defaulted, pinned,
        "iced's default already matches TEXT_LINE_HEIGHT, so pinning it is dead code"
    );
}

#[test]
fn both_edit_widgets_put_their_first_line_on_the_same_row() {
    // A single-line and a multi-line overlay must start their text on the same
    // row, so switching an overlay between them never shifts the text.
    let font_size = 60.0;
    let input_top = text_input_first_line_top(font_size, Some(super::TEXT_LINE_HEIGHT));
    let editor_top = *editor_line_tops(font_size)
        .first()
        .expect("the editor should render its text");

    assert!(
        input_top.abs_diff(editor_top) <= 1,
        "single-line edit starts at row {input_top} but multi-line edit starts at {editor_top}"
    );
}

// =====================================================================
// spe-x9e: a box resizes vertically and diagonally, not only sideways
// =====================================================================
//
// The reference overlay is at PDF (72, 720), 150pt wide, holding one line of
// 12pt Courier. At scale 1 on a 1000x1000 canvas its text box is x 266..416,
// y 68..82.4, so the right-edge bar sits at x=416, the bottom bar at y=82.4,
// and the corner where they meet.

const RIGHT_HANDLE_X: f32 = 416.0;
const BOTTOM_HANDLE_Y: f32 = 82.4;

fn reference_box_overlay() -> TextOverlay {
    multiline_overlay_at(72.0, 720.0, 150.0, "Hello")
}

/// Press a handle of the reference overlay and release at `release`, returning
/// the box the canvas published.
fn resize_box_from(press: mouse::Cursor, release: mouse::Cursor) -> OverlayBox {
    let overlays = vec![reference_box_overlay()];
    let (_, _, mut state) = press_on(&overlays, Some(0), press);
    assert!(
        state.resize_drag.is_some(),
        "the press should have grabbed a resize handle"
    );
    let (msg, _) = release_on(&mut state, &overlays, Some(0), release);
    match msg {
        Some(Message::ResizeOverlay { new_box, .. }) => new_box,
        other => panic!("Expected ResizeOverlay, got {other:?}"),
    }
}

/// The handle a press at (`x`, `y`) grabs on the reference overlay.
fn grabbed_edge(x: f32, y: f32) -> Option<ResizeEdge> {
    let overlays = vec![reference_box_overlay()];
    let (_, _, state) = press_on(&overlays, Some(0), cursor_at(x, y));
    state.resize_drag.map(|drag| drag.edge)
}

#[test]
fn the_right_edge_bar_grabs_a_width_resize() {
    assert_eq!(grabbed_edge(RIGHT_HANDLE_X, 75.0), Some(ResizeEdge::Right));
}

#[test]
fn the_bottom_edge_bar_grabs_a_height_resize() {
    assert_eq!(
        grabbed_edge(300.0, BOTTOM_HANDLE_Y),
        Some(ResizeEdge::Bottom)
    );
}

#[test]
fn the_corner_grabs_a_resize_of_both_dimensions() {
    assert_eq!(
        grabbed_edge(RIGHT_HANDLE_X, BOTTOM_HANDLE_Y),
        Some(ResizeEdge::Corner),
        "where the two bars meet, the drag should resize diagonally"
    );
}

#[test]
fn dragging_the_bottom_edge_changes_the_height_and_leaves_the_width() {
    // Release at screen y=200 → PDF y=600. The box top is one 12pt font size
    // above the baseline at 720, so the box becomes 732 - 600 = 132 tall.
    let new_box = resize_box_from(cursor_at(300.0, BOTTOM_HANDLE_Y), cursor_at(300.0, 200.0));
    assert!(
        (new_box.min_height - 132.0).abs() < 1.0,
        "expected a 132pt box, got {}",
        new_box.min_height
    );
    assert!(
        (new_box.width - 150.0).abs() < 0.01,
        "a vertical drag must leave the width alone, got {}",
        new_box.width
    );
}

#[test]
fn dragging_the_right_edge_changes_the_width_and_leaves_the_height() {
    let overlays = vec![TextOverlay {
        min_height: Some(90.0),
        ..reference_box_overlay()
    }];
    let (_, _, mut state) = press_on(&overlays, Some(0), cursor_at(RIGHT_HANDLE_X, 75.0));
    let (msg, _) = release_on(&mut state, &overlays, Some(0), cursor_at(516.0, 75.0));
    match msg {
        Some(Message::ResizeOverlay { new_box, .. }) => {
            assert!((new_box.width - 250.0).abs() < 1.0, "{}", new_box.width);
            assert!(
                (new_box.min_height - 90.0).abs() < 0.01,
                "a horizontal drag must leave the height alone, got {}",
                new_box.min_height
            );
        }
        other => panic!("Expected ResizeOverlay, got {other:?}"),
    }
}

#[test]
fn dragging_the_corner_changes_both_dimensions() {
    let new_box = resize_box_from(
        cursor_at(RIGHT_HANDLE_X, BOTTOM_HANDLE_Y),
        cursor_at(516.0, 200.0),
    );
    assert!((new_box.width - 250.0).abs() < 1.0, "{}", new_box.width);
    assert!(
        (new_box.min_height - 132.0).abs() < 1.0,
        "{}",
        new_box.min_height
    );
}

#[test]
fn a_resize_dragged_off_the_page_stops_at_the_page_edge() {
    // The page occupies screen x 194..806 and y 8..800, so the box can grow
    // no further than its own left/top edge to the page's bottom-right corner.
    let new_box = resize_box_from(
        cursor_at(RIGHT_HANDLE_X, BOTTOM_HANDLE_Y),
        cursor_at(2000.0, 2000.0),
    );
    assert!(
        (new_box.width - 540.0).abs() < 1.0,
        "the box should stop at the page's right edge, got {}",
        new_box.width
    );
    assert!(
        (new_box.min_height - 732.0).abs() < 1.0,
        "the box should stop at the page's bottom edge, got {}",
        new_box.min_height
    );
}

#[test]
fn a_vertical_resize_enforces_the_minimum_height() {
    // Dragging the bottom edge up past the box's own top.
    let new_box = resize_box_from(cursor_at(300.0, BOTTOM_HANDLE_Y), cursor_at(300.0, 20.0));
    assert!(
        new_box.min_height >= super::MIN_BOX_DIMENSION,
        "got {}",
        new_box.min_height
    );
}

/// The pointer shape the canvas asks for with the cursor at (`x`, `y`) over
/// the selected reference overlay.
fn interaction_at(x: f32, y: f32) -> mouse::Interaction {
    let overlays = vec![reference_box_overlay()];
    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = OverlayCanvasProgram {
        active_overlay: Some(0),
        ..test_program(&overlays, &dims, &registry)
    };
    program.mouse_interaction(
        &ProgramState::default(),
        test_canvas_bounds(),
        cursor_at(x, y),
    )
}

#[test]
fn each_handle_asks_for_the_pointer_that_matches_the_way_it_moves() {
    assert_eq!(
        interaction_at(RIGHT_HANDLE_X, 75.0),
        mouse::Interaction::ResizingHorizontally
    );
    assert_eq!(
        interaction_at(300.0, BOTTOM_HANDLE_Y),
        mouse::Interaction::ResizingVertically
    );
    assert_eq!(
        interaction_at(RIGHT_HANDLE_X, BOTTOM_HANDLE_Y),
        mouse::Interaction::ResizingDiagonallyDown
    );
}

#[test]
fn a_resize_in_flight_keeps_showing_its_own_pointer() {
    let overlays = vec![reference_box_overlay()];
    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let program = OverlayCanvasProgram {
        active_overlay: Some(0),
        ..test_program(&overlays, &dims, &registry)
    };
    let state = ProgramState {
        resize_drag: Some(ResizeDragState {
            overlay_index: 0,
            anchor: OverlayAnchor::of(&overlays[0]),
            edge: ResizeEdge::Bottom,
            initial_box: OverlayBox::of(&overlays[0]).unwrap(),
        }),
        ..ProgramState::default()
    };
    assert_eq!(
        program.mouse_interaction(&state, test_canvas_bounds(), cursor_at(500.0, 500.0)),
        mouse::Interaction::ResizingVertically,
        "the pointer must not revert to horizontal once the drag leaves the handle"
    );
}

#[test]
fn a_single_line_overlay_has_no_resize_handles_anywhere() {
    let overlays = vec![overlay_at(72.0, 720.0, "Hello")];
    let dims = test_page_dimensions();
    let registry = FontRegistry::new();
    let params = ConversionParams {
        offset_x: 194.0,
        offset_y: 8.0,
        ..default_params()
    };
    let _ = dims;
    assert_eq!(
        super::resize_handle_hit(
            RIGHT_HANDLE_X,
            BOTTOM_HANDLE_Y,
            &overlays[0],
            &params,
            &registry
        ),
        None
    );
}

#[test]
fn the_resize_handles_are_drawn_along_the_bottom_of_the_box_as_well_as_its_side() {
    // The handles are what the user aims at, so they have to be visible where
    // the hit test says they are.
    let dims = uniform_page_dims(1);
    let registry = FontRegistry::new();
    let overlays = vec![multiline_overlay_at(72.0, 600.0, 150.0, "one\ntwo")];
    let mut program = test_program(&overlays, &dims, &registry);
    program.active_overlay = Some(0);

    let (sx, sy) = rendered_baseline(&overlays[0], &dims);
    let text_box = super::overlay_text_box(
        &overlays[0],
        sx,
        sy,
        crate::coordinate::render_scale(TEST_ZOOM, TEST_DPI),
        &registry,
    );
    let canvas = render_overlay_canvas(program, None);
    let (_, _, _, bottom) = canvas
        .selection_blue_bounds()
        .expect("a selected box paints its handles");

    // The bottom handle straddles the box's bottom edge, so blue ink must
    // reach past where the selection border alone would end.
    let border_bottom = text_box.y + text_box.height + super::SELECTION_BOX_PADDING;
    assert!(
        bottom as f32 >= border_bottom - 1.0,
        "blue ink stops at row {bottom} but the bottom handle should reach {border_bottom}"
    );
}

// =====================================================================
// spe-qrj: a resize drag shows the box it would commit, the way a
// placement drag shows the box it would place
// =====================================================================

/// Grab a resize handle at (`grab_x`, `grab_y`) and drag to (`x`, `y`) without
/// releasing, returning the blue bounds the canvas draws mid-drag.
fn resize_preview_bounds(
    overlay: TextOverlay,
    grab_x: f32,
    grab_y: f32,
    x: f32,
    y: f32,
) -> (u32, u32, u32, u32) {
    let dims = uniform_page_dims(1);
    let registry = FontRegistry::new();
    let overlays = vec![overlay];
    let mut program = test_program(&overlays, &dims, &registry);
    program.active_overlay = Some(0);
    let element: iced::Element<Message> = iced::widget::canvas(program)
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into();
    let grab = iced::Point::new(grab_x, grab_y);
    let to = iced::Point::new(x, y);
    let steps = [
        (
            iced::Event::Mouse(mouse::Event::CursorMoved { position: grab }),
            grab,
        ),
        (
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            grab,
        ),
        (
            iced::Event::Mouse(mouse::Event::CursorMoved { position: to }),
            to,
        ),
    ];
    crate::test_render::render_element_after(element, &steps, Some(to))
        .selection_blue_bounds()
        .expect("a resize in flight must draw the box it would commit")
}

#[test]
fn a_resize_in_flight_draws_the_box_it_would_commit() {
    // The overlay is at PDF (72, 600) in a 150x100pt box: on a 900px-wide
    // surface its text box is x 216..366, y 188..288. Dragging the corner in
    // to screen (300, 250) shrinks it to x 216..300, y 188..250, so the blue
    // ink must stop there rather than at the box's committed edges.
    let mut overlay = multiline_overlay_at(72.0, 600.0, 150.0, "one");
    overlay.min_height = Some(100.0);
    let (_, _, right, bottom) = resize_preview_bounds(overlay, 366.0, 288.0, 300.0, 250.0);

    let pad = super::SELECTION_BOX_PADDING;
    assert!(
        (right as f32 - (300.0 + pad)).abs() <= 2.0,
        "the preview's right edge is at column {right}, expected ~{}",
        300.0 + pad
    );
    assert!(
        (bottom as f32 - (250.0 + pad)).abs() <= 2.0,
        "the preview's bottom edge is at row {bottom}, expected ~{}",
        250.0 + pad
    );
}

#[test]
fn a_vertical_resize_in_flight_previews_only_the_height() {
    let mut overlay = multiline_overlay_at(72.0, 600.0, 150.0, "one");
    overlay.min_height = Some(100.0);
    let (_, _, right, bottom) = resize_preview_bounds(overlay, 300.0, 288.0, 300.0, 250.0);

    let pad = super::SELECTION_BOX_PADDING;
    assert!(
        (right as f32 - (366.0 + pad)).abs() <= 2.0,
        "a vertical resize must keep the box's right edge at ~{}, got {right}",
        366.0 + pad
    );
    assert!(
        (bottom as f32 - (250.0 + pad)).abs() <= 2.0,
        "the preview's bottom edge is at row {bottom}, expected ~{}",
        250.0 + pad
    );
}
