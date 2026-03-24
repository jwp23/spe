// E2E tests using iced_test Simulator.
//
// These tests exercise the full view -> simulator -> message cycle.
// They may require a GPU context (Mesa llvmpipe in CI) and are
// marked #[ignore] to avoid failing in headless environments.

use iced_test::simulator;
use spe::app::{App, Message};

/// Build the view and run it through the simulator. Verifies the view
/// renders without panic and returns any messages produced.
fn verify_view_renders(app: &App) {
    let element = app.view();
    let _ui = simulator(element);
}

#[test]
#[ignore]
fn app_launches_with_empty_state() {
    let (app, _) = App::new();
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn empty_state_shows_welcome_text() {
    let (app, _) = App::new();
    let element = app.view();
    let mut ui = simulator(element);
    assert!(
        ui.find("Open a PDF to get started").is_ok(),
        "Welcome text should be visible when no document is loaded"
    );
}

#[test]
#[ignore]
fn sidebar_toggle_updates_state_and_renders() {
    let (mut app, _) = App::new();
    assert!(app.sidebar.visible);
    verify_view_renders(&app);

    let _ = app.update(Message::ToggleSidebar);
    assert!(!app.sidebar.visible);
    verify_view_renders(&app);

    let _ = app.update(Message::ToggleSidebar);
    assert!(app.sidebar.visible);
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn zoom_cycle_renders_correctly() {
    let (mut app, _) = App::new();
    assert!((app.canvas.zoom - 1.0).abs() < f32::EPSILON);

    let _ = app.update(Message::ZoomIn);
    assert!(app.canvas.zoom > 1.0);
    verify_view_renders(&app);

    let _ = app.update(Message::ZoomReset);
    assert!((app.canvas.zoom - 1.0).abs() < f32::EPSILON);
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn undo_redo_with_view_rebuild() {
    use spe::overlay::PdfPosition;
    use std::collections::HashMap;
    use std::path::PathBuf;

    let (mut app, _) = App::new();
    app.document = Some(spe::app::DocumentState {
        source_path: PathBuf::from("/tmp/test.pdf"),
        save_path: None,
        page_count: 1,
        current_page: 1,
        page_images: HashMap::new(),
        page_dimensions: HashMap::new(),
        overlays: Vec::new(),
    });
    verify_view_renders(&app);

    // Place an overlay
    let _ = app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
    });
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
    verify_view_renders(&app);

    // Undo
    let _ = app.update(Message::Undo);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 0);
    verify_view_renders(&app);

    // Redo
    let _ = app.update(Message::Redo);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn page_navigation_with_document() {
    use std::collections::HashMap;
    use std::path::PathBuf;

    let (mut app, _) = App::new();
    let mut dims = HashMap::new();
    dims.insert(1, (612.0, 792.0));
    dims.insert(2, (612.0, 792.0));
    dims.insert(3, (612.0, 792.0));
    app.document = Some(spe::app::DocumentState {
        source_path: PathBuf::from("/tmp/test.pdf"),
        save_path: None,
        page_count: 3,
        current_page: 1,
        page_images: HashMap::new(),
        page_dimensions: dims,
        overlays: Vec::new(),
    });
    verify_view_renders(&app);

    // NextPage scrolls; simulate the resulting CanvasScrolled
    let _ = app.update(Message::NextPage);
    let dpi = spe::ui::canvas::effective_dpi(app.canvas.zoom);
    let layout = spe::ui::canvas::page_layout(
        &app.document.as_ref().unwrap().page_dimensions,
        3,
        app.canvas.zoom,
        dpi,
    );
    let _ = app.update(Message::CanvasScrolled(layout.page_tops[1], 800.0));
    assert_eq!(app.document.as_ref().unwrap().current_page, 2);
    verify_view_renders(&app);

    // PreviousPage scrolls back
    let _ = app.update(Message::PreviousPage);
    let _ = app.update(Message::CanvasScrolled(layout.page_tops[0], 800.0));
    assert_eq!(app.document.as_ref().unwrap().current_page, 1);
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn delete_overlay_with_selection() {
    use spe::overlay::PdfPosition;
    use std::collections::HashMap;
    use std::path::PathBuf;

    let (mut app, _) = App::new();
    app.document = Some(spe::app::DocumentState {
        source_path: PathBuf::from("/tmp/test.pdf"),
        save_path: None,
        page_count: 1,
        current_page: 1,
        page_images: HashMap::new(),
        page_dimensions: HashMap::new(),
        overlays: Vec::new(),
    });

    let _ = app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
    });
    assert!(app.canvas.active_overlay.is_some());
    verify_view_renders(&app);

    let _ = app.update(Message::DeleteOverlay);
    assert!(app.document.as_ref().unwrap().overlays.is_empty());
    assert!(app.canvas.active_overlay.is_none());
    verify_view_renders(&app);
}

// --- Thumbnail sidebar tests ---

/// Create a multi-page DocumentState for sidebar tests.
fn test_document_multipage(page_count: u32) -> spe::app::DocumentState {
    use std::collections::HashMap;
    use std::path::PathBuf;

    let mut page_images = HashMap::new();
    let mut page_dimensions = HashMap::new();
    for p in 1..=page_count {
        let pixels = vec![255u8; 100 * 130 * 4];
        page_images.insert(p, iced::widget::image::Handle::from_rgba(100, 130, pixels));
        page_dimensions.insert(p, (612.0, 792.0));
    }

    spe::app::DocumentState {
        source_path: PathBuf::from("/tmp/test.pdf"),
        save_path: None,
        page_count,
        current_page: 1,
        page_images,
        page_dimensions,
        overlays: Vec::new(),
    }
}

#[test]
#[ignore]
fn sidebar_renders_with_thumbnails() {
    let (mut app, _) = App::new();
    app.document = Some(test_document_multipage(3));
    // Simulate thumbnails already rendered
    for p in 1..=3 {
        let pixels = vec![200u8; 50 * 65 * 4];
        app.sidebar
            .thumbnails
            .insert(p, iced::widget::image::Handle::from_rgba(50, 65, pixels));
    }
    app.sidebar.thumbnail_dpi = 12.0;
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn sidebar_click_navigates_to_page() {
    let (mut app, _) = App::new();
    app.document = Some(test_document_multipage(5));
    app.sidebar.thumbnail_dpi = 12.0;

    // Clicking page 3 thumbnail should navigate canvas
    let _ = app.update(Message::SidebarPageClicked(3));
    // GoToPage fires scroll_to — verify state is consistent
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn sidebar_scroll_independent_of_canvas() {
    let (mut app, _) = App::new();
    app.document = Some(test_document_multipage(10));
    app.sidebar.thumbnail_dpi = 12.0;
    app.canvas.scroll_y = 0.0;

    // Scroll sidebar without affecting canvas
    let _ = app.update(Message::SidebarScrolled(200.0, 600.0));
    assert!((app.sidebar.scroll_y - 200.0).abs() < f32::EPSILON);
    assert!((app.canvas.scroll_y - 0.0).abs() < f32::EPSILON);
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn sidebar_current_page_tracks_canvas_navigation() {
    let (mut app, _) = App::new();
    let mut doc = test_document_multipage(3);
    doc.current_page = 1;
    app.document = Some(doc);
    app.sidebar.thumbnail_dpi = 12.0;
    verify_view_renders(&app);

    // Simulate scrolling to page 2
    let dpi = spe::ui::canvas::effective_dpi(app.canvas.zoom);
    let layout = spe::ui::canvas::page_layout(
        &app.document.as_ref().unwrap().page_dimensions,
        3,
        app.canvas.zoom,
        dpi,
    );
    let _ = app.update(Message::CanvasScrolled(layout.page_tops[1], 800.0));
    assert_eq!(app.document.as_ref().unwrap().current_page, 2);
    // Sidebar should render with page 2 highlighted
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn sidebar_resize_updates_width_and_renders() {
    let (mut app, _) = App::new();
    app.document = Some(test_document_multipage(3));
    app.sidebar.thumbnail_dpi = 12.0;
    verify_view_renders(&app);

    // Start drag
    let _ = app.update(Message::SidebarDragStart(120.0));
    assert!(app.sidebar.dragging);

    // Drag to widen sidebar — first move captures start X
    let _ = app.update(Message::SidebarResized(120.0));
    // Second move actually resizes
    let _ = app.update(Message::SidebarResized(200.0));
    assert!(app.sidebar.width > 120.0);
    verify_view_renders(&app);

    // End drag
    let _ = app.update(Message::SidebarResizeEnd);
    assert!(!app.sidebar.dragging);
    verify_view_renders(&app);
}

// --- Tests with loaded page images (canvas rendering) ---

/// Create a DocumentState with a synthetic page image for the given page.
fn test_document_with_image() -> spe::app::DocumentState {
    use std::collections::HashMap;
    use std::path::PathBuf;

    let pixels = vec![255u8; 100 * 130 * 4]; // RGBA white, ~US Letter proportions
    let handle = iced::widget::image::Handle::from_rgba(100, 130, pixels);
    let mut page_images = HashMap::new();
    page_images.insert(1, handle);
    let mut page_dimensions = HashMap::new();
    page_dimensions.insert(1, (612.0, 792.0));

    spe::app::DocumentState {
        source_path: PathBuf::from("/tmp/test.pdf"),
        save_path: None,
        page_count: 1,
        current_page: 1,
        page_images,
        page_dimensions,
        overlays: Vec::new(),
    }
}

#[test]
#[ignore]
fn canvas_renders_with_loaded_page_image() {
    let (mut app, _) = App::new();
    app.document = Some(test_document_with_image());
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn canvas_renders_with_overlays_on_page() {
    use spe::overlay::{PdfPosition, Standard14Font, TextOverlay};

    let (mut app, _) = App::new();
    let mut doc = test_document_with_image();
    doc.overlays.push(TextOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        text: "Hello, world!".to_string(),
        font: Standard14Font::Helvetica,
        font_size: 12.0,
        width: None,
    });
    app.document = Some(doc);
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn canvas_renders_with_selected_overlay() {
    use spe::overlay::{PdfPosition, Standard14Font, TextOverlay};

    let (mut app, _) = App::new();
    let mut doc = test_document_with_image();
    doc.overlays.push(TextOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        text: "Selected text".to_string(),
        font: Standard14Font::Courier,
        font_size: 14.0,
        width: None,
    });
    app.document = Some(doc);
    app.canvas.active_overlay = Some(0);
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn zoom_with_loaded_document_renders() {
    let (mut app, _) = App::new();
    app.document = Some(test_document_with_image());
    verify_view_renders(&app);

    let _ = app.update(Message::ZoomIn);
    verify_view_renders(&app);

    let _ = app.update(Message::ZoomOut);
    verify_view_renders(&app);

    let _ = app.update(Message::ZoomReset);
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn page_navigation_with_rendered_pages() {
    let (mut app, _) = App::new();
    let mut doc = test_document_with_image();
    doc.page_count = 3;

    // Add images and dimensions for pages 2 and 3
    let pixels2 = vec![200u8; 100 * 130 * 4];
    doc.page_images
        .insert(2, iced::widget::image::Handle::from_rgba(100, 130, pixels2));
    doc.page_dimensions.insert(2, (612.0, 792.0));

    let pixels3 = vec![180u8; 100 * 130 * 4];
    doc.page_images
        .insert(3, iced::widget::image::Handle::from_rgba(100, 130, pixels3));
    doc.page_dimensions.insert(3, (612.0, 792.0));

    app.document = Some(doc);
    verify_view_renders(&app);

    // NextPage scrolls; simulate CanvasScrolled to update current_page
    let dpi = spe::ui::canvas::effective_dpi(app.canvas.zoom);
    let layout = spe::ui::canvas::page_layout(
        &app.document.as_ref().unwrap().page_dimensions,
        3,
        app.canvas.zoom,
        dpi,
    );

    let _ = app.update(Message::NextPage);
    let _ = app.update(Message::CanvasScrolled(layout.page_tops[1], 800.0));
    assert_eq!(app.document.as_ref().unwrap().current_page, 2);
    verify_view_renders(&app);

    let _ = app.update(Message::NextPage);
    let _ = app.update(Message::CanvasScrolled(layout.page_tops[2], 800.0));
    assert_eq!(app.document.as_ref().unwrap().current_page, 3);
    verify_view_renders(&app);

    let _ = app.update(Message::PreviousPage);
    let _ = app.update(Message::CanvasScrolled(layout.page_tops[1], 800.0));
    assert_eq!(app.document.as_ref().unwrap().current_page, 2);
    verify_view_renders(&app);
}
