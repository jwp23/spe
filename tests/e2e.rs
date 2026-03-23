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

    app.update(Message::ToggleSidebar);
    assert!(!app.sidebar.visible);
    verify_view_renders(&app);

    app.update(Message::ToggleSidebar);
    assert!(app.sidebar.visible);
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn zoom_cycle_renders_correctly() {
    let (mut app, _) = App::new();
    assert!((app.canvas.zoom - 1.0).abs() < f32::EPSILON);

    app.update(Message::ZoomIn);
    assert!(app.canvas.zoom > 1.0);
    verify_view_renders(&app);

    app.update(Message::ZoomReset);
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
    app.update(Message::PlaceOverlay(PdfPosition { x: 100.0, y: 700.0 }));
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
    verify_view_renders(&app);

    // Undo
    app.update(Message::Undo);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 0);
    verify_view_renders(&app);

    // Redo
    app.update(Message::Redo);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
    verify_view_renders(&app);
}

#[test]
#[ignore]
fn page_navigation_with_document() {
    use std::collections::HashMap;
    use std::path::PathBuf;

    let (mut app, _) = App::new();
    app.document = Some(spe::app::DocumentState {
        source_path: PathBuf::from("/tmp/test.pdf"),
        save_path: None,
        page_count: 3,
        current_page: 1,
        page_images: HashMap::new(),
        page_dimensions: HashMap::new(),
        overlays: Vec::new(),
    });
    verify_view_renders(&app);

    app.update(Message::NextPage);
    assert_eq!(app.document.as_ref().unwrap().current_page, 2);
    verify_view_renders(&app);

    app.update(Message::PreviousPage);
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

    app.update(Message::PlaceOverlay(PdfPosition { x: 100.0, y: 700.0 }));
    assert!(app.canvas.active_overlay.is_some());
    verify_view_renders(&app);

    app.update(Message::DeleteOverlay);
    assert!(app.document.as_ref().unwrap().overlays.is_empty());
    assert!(app.canvas.active_overlay.is_none());
    verify_view_renders(&app);
}
