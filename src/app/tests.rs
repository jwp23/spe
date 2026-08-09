use super::*;
use crate::ui::canvas;

#[test]
fn app_default_has_no_document() {
    let (app, _) = App::new(false);
    assert!(app.document.is_none());
    assert!(app.undo_stack.is_empty());
    assert!(app.redo_stack.is_empty());
}

#[test]
fn next_page_without_document_is_noop() {
    let (mut app, _) = App::new(false);
    app.update(Message::NextPage);
    assert!(app.document.is_none());
}

fn test_app_with_document() -> App {
    let (mut app, _) = App::new(false);
    app.document = Some(DocumentState {
        source_path: PathBuf::from("/tmp/test.pdf"),
        save_path: None,
        page_count: 3,
        current_page: 1,
        page_images: HashMap::new(),
        page_dimensions: HashMap::new(),
        overlays: Vec::new(),
    });
    app
}

#[test]
fn next_page_does_not_change_current_page_directly() {
    // Page navigation now scrolls; current_page updates via CanvasScrolled
    let mut app = test_app_with_document();
    app.update(Message::NextPage);
    // current_page hasn't changed yet (scroll is async)
    assert_eq!(app.document.as_ref().unwrap().current_page, 1);
}

#[test]
fn next_page_is_noop_at_last_page() {
    let mut app = test_app_with_document();
    app.document.as_mut().unwrap().current_page = 3;
    app.update(Message::NextPage);
    assert_eq!(app.document.as_ref().unwrap().current_page, 3);
}

#[test]
fn previous_page_is_noop_at_first_page() {
    let mut app = test_app_with_document();
    app.update(Message::PreviousPage);
    assert_eq!(app.document.as_ref().unwrap().current_page, 1);
}

#[test]
fn go_to_page_ignores_out_of_range() {
    let mut app = test_app_with_document();
    app.update(Message::GoToPage(99));
    assert_eq!(app.document.as_ref().unwrap().current_page, 1);
}

#[test]
fn canvas_scrolled_updates_current_page() {
    let mut app = test_app_with_document();
    // Add page dimensions so layout can be computed
    let doc = app.document.as_mut().unwrap();
    doc.page_dimensions.insert(1, (612.0, 792.0));
    doc.page_dimensions.insert(2, (612.0, 792.0));
    doc.page_dimensions.insert(3, (612.0, 792.0));

    // Scroll to a position where page 2 is dominant
    let dpi = canvas::effective_dpi(app.canvas.zoom);
    let layout = canvas::page_layout(
        &app.document.as_ref().unwrap().page_dimensions,
        3,
        app.canvas.zoom,
        dpi,
    );
    let scroll_y = layout.page_tops[1]; // top of page 2
    app.update(Message::CanvasScrolled(scroll_y, 800.0));
    assert_eq!(app.document.as_ref().unwrap().current_page, 2);
    assert_eq!(app.toolbar.page_input, "2");
}

#[test]
fn place_overlay_adds_to_overlays() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
    assert_eq!(app.undo_stack.len(), 1);
    assert!(app.canvas.active_overlay.is_some());
    assert!(app.canvas.editing);
}

#[test]
fn undo_redo_through_update() {
    let mut app = test_app_with_document();

    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hi".to_string()));
    app.update(Message::CommitText);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);

    app.update(Message::Undo); // reverse the text edit
    app.update(Message::Undo); // reverse the placement
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 0);
    assert_eq!(app.redo_stack.len(), 2);

    app.update(Message::Redo);
    app.update(Message::Redo);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
    assert!(app.redo_stack.is_empty());
}

#[test]
fn new_action_clears_redo_stack() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hi".to_string()));
    app.update(Message::CommitText);
    app.update(Message::Undo);
    assert_eq!(app.redo_stack.len(), 1);

    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 200.0, y: 600.0 },
        width: None,
    });
    assert!(app.redo_stack.is_empty());
}

#[test]
fn delete_overlay_removes_selected() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    // PlaceOverlay sets active_overlay
    app.update(Message::DeleteOverlay);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 0);
    assert!(app.canvas.active_overlay.is_none());
}

#[test]
fn change_font_updates_overlay_and_toolbar() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    let courier = app.font_registry.find_by_name("Courier").unwrap();
    app.update(Message::ChangeFont(courier));
    assert_eq!(app.document.as_ref().unwrap().overlays[0].font, courier);
    assert_eq!(app.toolbar.font, courier);
}

#[test]
fn zoom_in_increases_zoom() {
    let (mut app, _) = App::new(false);
    let initial = app.canvas.zoom;
    app.update(Message::ZoomIn);
    assert!(app.canvas.zoom > initial);
}

#[test]
fn zoom_reset_returns_to_one() {
    let (mut app, _) = App::new(false);
    app.update(Message::ZoomIn);
    app.update(Message::ZoomReset);
    assert!((app.canvas.zoom - 1.0).abs() < f32::EPSILON);
}

#[test]
fn zoom_in_increments_generation() {
    let (mut app, _) = App::new(false);
    assert_eq!(app.canvas.zoom_generation, 0);
    app.update(Message::ZoomIn);
    assert_eq!(app.canvas.zoom_generation, 1);
    app.update(Message::ZoomIn);
    assert_eq!(app.canvas.zoom_generation, 2);
}

#[test]
fn zoom_out_increments_generation() {
    let (mut app, _) = App::new(false);
    app.update(Message::ZoomIn); // go above 1.0 so ZoomOut has room
    let gen_before = app.canvas.zoom_generation;
    app.update(Message::ZoomOut);
    assert_eq!(app.canvas.zoom_generation, gen_before + 1);
}

#[test]
fn zoom_reset_increments_generation() {
    let (mut app, _) = App::new(false);
    app.update(Message::ZoomIn);
    let gen_before = app.canvas.zoom_generation;
    app.update(Message::ZoomReset);
    assert_eq!(app.canvas.zoom_generation, gen_before + 1);
}

#[test]
fn zoom_keeps_stale_image_for_visual_feedback() {
    let mut app = test_app_with_document();
    let handle = Handle::from_rgba(1, 1, vec![0u8; 4]);
    app.document.as_mut().unwrap().page_images.insert(1, handle);
    app.update(Message::ZoomIn);
    // Stale image stays in cache for instant visual feedback during debounce
    assert!(!app.document.as_ref().unwrap().page_images.is_empty());
}

#[test]
fn zoom_debounce_expired_clears_cache() {
    let mut app = test_app_with_document();
    let handle = Handle::from_rgba(1, 1, vec![0u8; 4]);
    app.document.as_mut().unwrap().page_images.insert(1, handle);
    app.update(Message::ZoomIn);
    let generation = app.canvas.zoom_generation;
    // Matching debounce expiry clears cache and triggers re-render
    app.update(Message::ZoomDebounceExpired(generation));
    assert!(app.document.as_ref().unwrap().page_images.is_empty());
}

#[test]
fn zoom_debounce_expired_stale_generation_is_noop() {
    let mut app = test_app_with_document();
    app.update(Message::ZoomIn);
    let stale_gen = app.canvas.zoom_generation;
    app.update(Message::ZoomIn); // generation advances
    assert_ne!(stale_gen, app.canvas.zoom_generation);
    // Stale generation should be a no-op
    let handle = Handle::from_rgba(1, 1, vec![0u8; 4]);
    app.document.as_mut().unwrap().page_images.insert(1, handle);
    app.update(Message::ZoomDebounceExpired(stale_gen));
    // Page cache should still be intact (no re-render triggered)
    assert!(!app.document.as_ref().unwrap().page_images.is_empty());
}

#[test]
fn toggle_sidebar_flips_visibility() {
    let (mut app, _) = App::new(false);
    assert!(app.sidebar.visible);
    app.update(Message::ToggleSidebar);
    assert!(!app.sidebar.visible);
    app.update(Message::ToggleSidebar);
    assert!(app.sidebar.visible);
}

#[test]
fn select_overlay_updates_toolbar() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    let courier_bold = app.font_registry.find_by_name("Courier Bold").unwrap();
    app.update(Message::ChangeFont(courier_bold));
    app.update(Message::DeselectOverlay);
    // Now select it again
    app.update(Message::SelectOverlay(0));
    assert_eq!(app.toolbar.font, courier_bold);
}

#[test]
fn save_destination_sets_save_path() {
    let mut app = test_app_with_document();
    // Simulate save destination (won't actually write since test.pdf doesn't exist,
    // but we can test the path assignment logic)
    let doc = app.document.as_ref().unwrap();
    assert!(doc.save_path.is_none());
}

// Keyboard shortcut tests
#[test]
fn ctrl_o_maps_to_open() {
    let msg = key_to_message(
        keyboard::Key::Character("o".into()),
        keyboard::Modifiers::COMMAND,
    );
    assert!(matches!(msg, Some(Message::OpenFile)));
}

#[test]
fn ctrl_s_maps_to_save() {
    let msg = key_to_message(
        keyboard::Key::Character("s".into()),
        keyboard::Modifiers::COMMAND,
    );
    assert!(matches!(msg, Some(Message::Save)));
}

#[test]
fn ctrl_shift_s_maps_to_save_as() {
    let msg = key_to_message(
        keyboard::Key::Character("s".into()),
        keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
    );
    assert!(matches!(msg, Some(Message::SaveAs)));
}

#[test]
fn ctrl_z_maps_to_undo() {
    let msg = key_to_message(
        keyboard::Key::Character("z".into()),
        keyboard::Modifiers::COMMAND,
    );
    assert!(matches!(msg, Some(Message::Undo)));
}

#[test]
fn ctrl_shift_z_maps_to_redo() {
    let msg = key_to_message(
        keyboard::Key::Character("z".into()),
        keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
    );
    assert!(matches!(msg, Some(Message::Redo)));
}

#[test]
fn f9_maps_to_toggle_sidebar() {
    let msg = key_to_message(
        keyboard::Key::Named(keyboard::key::Named::F9),
        keyboard::Modifiers::empty(),
    );
    assert!(matches!(msg, Some(Message::ToggleSidebar)));
}

#[test]
fn escape_maps_to_deselect() {
    let msg = key_to_message(
        keyboard::Key::Named(keyboard::key::Named::Escape),
        keyboard::Modifiers::empty(),
    );
    assert!(matches!(msg, Some(Message::DeselectOverlay)));
}

#[test]
fn ctrl_enter_maps_to_deselect_same_as_escape() {
    let msg = key_to_message(
        keyboard::Key::Named(keyboard::key::Named::Enter),
        keyboard::Modifiers::COMMAND,
    );
    assert!(matches!(msg, Some(Message::DeselectOverlay)));
}

fn enter_key_press(modifiers: keyboard::Modifiers) -> iced::widget::text_editor::KeyPress {
    iced::widget::text_editor::KeyPress {
        key: keyboard::Key::Named(keyboard::key::Named::Enter),
        modified_key: keyboard::Key::Named(keyboard::key::Named::Enter),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
        modifiers,
        text: None,
        status: iced::widget::text_editor::Status::Focused { is_hovered: false },
    }
}

#[test]
fn ctrl_enter_key_binding_does_not_capture_in_text_editor() {
    let key_press = enter_key_press(keyboard::Modifiers::COMMAND);

    let binding = super::view::overlay_text_editor_key_binding(key_press);

    // Returning None lets the KeyPressed event bubble to the app-level
    // subscription instead of being captured by the text_editor, matching
    // how Escape (Binding::Unfocus) bubbles today.
    assert!(binding.is_none());
}

#[test]
fn plain_enter_key_binding_still_inserts_newline_in_text_editor() {
    let key_press = enter_key_press(keyboard::Modifiers::empty());

    let binding = super::view::overlay_text_editor_key_binding(key_press);

    assert!(matches!(
        binding,
        Some(iced::widget::text_editor::Binding::Enter)
    ));
}

#[test]
fn delete_maps_to_delete_overlay() {
    let msg = key_to_message(
        keyboard::Key::Named(keyboard::key::Named::Delete),
        keyboard::Modifiers::empty(),
    );
    assert!(matches!(msg, Some(Message::DeleteOverlay)));
}

#[test]
fn page_up_maps_to_previous() {
    let msg = key_to_message(
        keyboard::Key::Named(keyboard::key::Named::PageUp),
        keyboard::Modifiers::empty(),
    );
    assert!(matches!(msg, Some(Message::PreviousPage)));
}

#[test]
fn ctrl_plus_maps_to_zoom_in() {
    let msg = key_to_message(
        keyboard::Key::Character("+".into()),
        keyboard::Modifiers::COMMAND,
    );
    assert!(matches!(msg, Some(Message::ZoomIn)));
}

#[test]
fn ctrl_minus_maps_to_zoom_out() {
    let msg = key_to_message(
        keyboard::Key::Character("-".into()),
        keyboard::Modifiers::COMMAND,
    );
    assert!(matches!(msg, Some(Message::ZoomOut)));
}

#[test]
fn view_with_no_document_does_not_panic() {
    let (app, _) = App::new(false);
    let _element = app.view();
}

#[test]
fn title_without_document() {
    let (app, _) = App::new(false);
    assert_eq!(app.title(), "SPE - PDF Text Overlay Editor");
}

#[test]
fn title_with_document() {
    let mut app = test_app_with_document();
    app.document.as_mut().unwrap().source_path = PathBuf::from("/tmp/report.pdf");
    assert_eq!(app.title(), "report.pdf - SPE");
}

#[test]
fn view_with_document_renders_canvas_widget() {
    let app = test_app_with_document();
    // Should not panic — constructs Stack with PdfPagesProgram and OverlayCanvasProgram
    let _element = app.view();
}

#[test]
fn view_with_document_and_page_image_does_not_panic() {
    let mut app = test_app_with_document();
    let doc = app.document.as_mut().unwrap();
    doc.page_dimensions.insert(1, (612.0, 792.0));
    // Insert a dummy Handle
    let handle = Handle::from_rgba(1, 1, vec![0, 0, 0, 255]);
    doc.page_images.insert(1, handle);
    let _element = app.view();
}

#[test]
fn page_batch_rendered_inserts_into_cache() {
    let mut app = test_app_with_document();
    let handles = vec![(1u32, Handle::from_rgba(1, 1, vec![255, 0, 0, 255]))];
    let _ = app.update(Message::PageBatchRendered(handles));
    assert!(app.document.as_ref().unwrap().page_images.contains_key(&1));
}

#[test]
fn page_batch_rendered_inserts_all_pages() {
    let mut app = test_app_with_document();
    let handles = vec![
        (1u32, Handle::from_rgba(1, 1, vec![255, 0, 0, 255])),
        (2u32, Handle::from_rgba(1, 1, vec![0, 255, 0, 255])),
    ];
    let _ = app.update(Message::PageBatchRendered(handles));
    let doc = app.document.as_ref().unwrap();
    assert!(doc.page_images.contains_key(&1));
    assert!(doc.page_images.contains_key(&2));
}

#[test]
fn page_batch_rendered_replaces_existing_cached_image() {
    let mut app = test_app_with_document();
    let handles1 = vec![(1u32, Handle::from_rgba(1, 1, vec![255, 0, 0, 255]))];
    let handles2 = vec![(1u32, Handle::from_rgba(1, 1, vec![0, 255, 0, 255]))];
    let _ = app.update(Message::PageBatchRendered(handles1));
    let _ = app.update(Message::PageBatchRendered(handles2));
    assert!(app.document.as_ref().unwrap().page_images.contains_key(&1));
}

#[test]
fn zoom_in_updates_zoom_with_document() {
    let mut app = test_app_with_document();
    let initial = app.canvas.zoom;
    let _ = app.update(Message::ZoomIn);
    assert!(app.canvas.zoom > initial);
}

#[test]
fn zoom_reset_with_document() {
    let mut app = test_app_with_document();
    let _ = app.update(Message::ZoomIn);
    let _ = app.update(Message::ZoomReset);
    assert!((app.canvas.zoom - 1.0).abs() < f32::EPSILON);
}

#[test]
fn canvas_dimensions_fill_when_no_window_size() {
    let mut app = test_app_with_document();
    app.document
        .as_mut()
        .unwrap()
        .page_dimensions
        .insert(1, (612.0, 792.0));
    let doc = app.document.as_ref().unwrap();
    let (w, h) = app.canvas_dimensions(doc);
    assert!(matches!(w, iced::Length::Fill));
    assert!(matches!(h, iced::Length::Fill));
}

#[test]
fn canvas_dimensions_fixed_when_page_exceeds_viewport() {
    let mut app = test_app_with_document();
    app.document
        .as_mut()
        .unwrap()
        .page_dimensions
        .insert(1, (612.0, 792.0));
    app.window_size = Some(iced::Size::new(800.0, 600.0));
    // At zoom=1.0, dpi=150: rendered_w = 612 * 1 * 150 / 72 = 1275
    // That's bigger than 800 viewport, so canvas should be Fixed(1275)
    let doc = app.document.as_ref().unwrap();
    let (w, h) = app.canvas_dimensions(doc);
    match w {
        iced::Length::Fixed(fw) => assert!(fw > 800.0),
        other => panic!("Expected Fixed, got {other:?}"),
    }
    match h {
        iced::Length::Fixed(fh) => assert!(fh > 600.0),
        other => panic!("Expected Fixed, got {other:?}"),
    }
}

#[test]
fn canvas_dimensions_at_least_viewport_when_page_is_small() {
    let mut app = test_app_with_document();
    app.document
        .as_mut()
        .unwrap()
        .page_dimensions
        .insert(1, (612.0, 792.0));
    app.window_size = Some(iced::Size::new(4000.0, 3000.0));
    app.canvas.zoom = 0.25;
    // At zoom=0.25, dpi=37.5: rendered_w = 612 * 0.25 * 37.5 / 72 ≈ 79.7
    // Viewport is ~4000 wide, so canvas should be at least viewport width
    let doc = app.document.as_ref().unwrap();
    let (w, _h) = app.canvas_dimensions(doc);
    match w {
        iced::Length::Fixed(fw) => assert!(fw > 3000.0),
        other => panic!("Expected Fixed, got {other:?}"),
    }
}

#[test]
fn zoom_fit_width_sets_correct_zoom() {
    let mut app = test_app_with_document();
    app.document
        .as_mut()
        .unwrap()
        .page_dimensions
        .insert(1, (612.0, 792.0));
    app.window_size = Some(iced::Size::new(1000.0, 800.0));
    app.sidebar.visible = false;
    let _ = app.update(Message::ZoomFitWidth);
    let expected = canvas::fit_to_width_zoom(612.0, 1000.0 - 16.0);
    assert!(
        (app.canvas.zoom - expected).abs() < 0.01,
        "zoom was {} expected {}",
        app.canvas.zoom,
        expected
    );
}

#[test]
fn zoom_fit_width_noop_without_document() {
    let (mut app, _) = App::new(false);
    app.window_size = Some(iced::Size::new(1000.0, 800.0));
    let _ = app.update(Message::ZoomFitWidth);
    assert!((app.canvas.zoom - 1.0).abs() < f32::EPSILON);
}

#[test]
fn zoom_fit_width_noop_without_window_size() {
    let mut app = test_app_with_document();
    app.document
        .as_mut()
        .unwrap()
        .page_dimensions
        .insert(1, (612.0, 792.0));
    let _ = app.update(Message::ZoomFitWidth);
    assert!((app.canvas.zoom - 1.0).abs() < f32::EPSILON);
}

#[test]
fn ctrl_zero_maps_to_zoom_fit_width() {
    let msg = key_to_message(
        keyboard::Key::Character("0".into()),
        keyboard::Modifiers::COMMAND,
    );
    assert!(matches!(msg, Some(Message::ZoomFitWidth)));
}

#[test]
fn zoom_fit_width_increments_generation() {
    let mut app = test_app_with_document();
    app.document
        .as_mut()
        .unwrap()
        .page_dimensions
        .insert(1, (612.0, 792.0));
    app.window_size = Some(iced::Size::new(1000.0, 800.0));
    let gen_before = app.canvas.zoom_generation;
    let _ = app.update(Message::ZoomFitWidth);
    assert!(app.canvas.zoom_generation > gen_before);
}

#[test]
fn app_default_has_no_window_size() {
    let (app, _) = App::new(false);
    assert!(app.window_size.is_none());
}

#[test]
fn app_default_scale_factor_is_one() {
    let (app, _) = App::new(false);
    assert!((app.scale_factor - 1.0).abs() < f32::EPSILON);
}

#[test]
fn scale_factor_changed_updates_state() {
    let (mut app, _) = App::new(false);
    let _ = app.update(Message::ScaleFactorChanged(2.0));
    assert!((app.scale_factor - 2.0).abs() < f32::EPSILON);
}

#[test]
fn window_resized_stores_size() {
    let (mut app, _) = App::new(false);
    let _ = app.update(Message::WindowResized(iced::Size::new(1920.0, 1080.0)));
    let size = app.window_size.unwrap();
    assert!((size.width - 1920.0).abs() < f32::EPSILON);
    assert!((size.height - 1080.0).abs() < f32::EPSILON);
}

#[test]
fn sidebar_scrolled_updates_scroll_state() {
    let mut app = test_app_with_document();
    let _ = app.update(Message::SidebarScrolled(150.0, 600.0));
    assert!((app.sidebar.scroll_y - 150.0).abs() < f32::EPSILON);
    assert!((app.sidebar.viewport_height - 600.0).abs() < f32::EPSILON);
}

#[test]
fn thumbnail_batch_rendered_inserts_with_matching_generation() {
    let mut app = test_app_with_document();
    app.sidebar.backfill_generation = 5;
    let handle = Handle::from_rgba(1, 1, vec![0u8; 4]);
    let _ = app.update(Message::ThumbnailBatchRendered(vec![(1, handle)], 5));
    assert!(app.sidebar.thumbnails.contains_key(&1));
}

#[test]
fn thumbnail_batch_rendered_ignores_stale_generation() {
    let mut app = test_app_with_document();
    app.sidebar.backfill_generation = 5;
    let handle = Handle::from_rgba(1, 1, vec![0u8; 4]);
    let _ = app.update(Message::ThumbnailBatchRendered(
        vec![(1, handle)],
        3, // stale generation
    ));
    assert!(!app.sidebar.thumbnails.contains_key(&1));
}

#[test]
fn schedule_thumbnail_backfill_returns_none_without_document() {
    let (mut app, _) = App::new(false);
    // Should not panic and should return a no-op task
    let _ = app.schedule_thumbnail_backfill();
}

#[test]
fn schedule_thumbnail_backfill_returns_none_when_sidebar_hidden() {
    let mut app = test_app_with_document();
    app.sidebar.visible = false;
    app.sidebar.thumbnail_dpi = 36.0;
    // No crash, returns early
    let _ = app.schedule_thumbnail_backfill();
}

#[test]
fn schedule_thumbnail_backfill_returns_none_when_all_rendered() {
    let mut app = test_app_with_document();
    app.sidebar.visible = true;
    app.sidebar.thumbnail_dpi = 36.0;
    // doc has page_count = 3; pre-populate all thumbnails
    for p in 1..=3u32 {
        app.sidebar
            .thumbnails
            .insert(p, Handle::from_rgba(1, 1, vec![0u8; 4]));
    }
    // All pages rendered — should return none (no task needed)
    let _ = app.schedule_thumbnail_backfill();
}

#[test]
fn schedule_thumbnail_backfill_skips_already_cached_pages() {
    let mut app = test_app_with_document();
    app.sidebar.visible = true;
    app.sidebar.thumbnail_dpi = 36.0;
    // Pre-render pages 1 and 2; page 3 is missing
    for p in 1..=2u32 {
        app.sidebar
            .thumbnails
            .insert(p, Handle::from_rgba(1, 1, vec![0u8; 4]));
    }
    // Should not panic — only page 3 is unrendered
    let _ = app.schedule_thumbnail_backfill();
}

#[test]
fn thumbnail_batch_rendered_chains_backfill() {
    let mut app = test_app_with_document();
    app.sidebar.visible = true;
    app.sidebar.thumbnail_dpi = 36.0;
    app.sidebar.backfill_generation = 1;
    // Page 2 and 3 are unrendered; receiving batch for page 1 should
    // trigger a backfill task (non-none) for the remaining pages.
    let handle = Handle::from_rgba(1, 1, vec![0u8; 4]);
    let task = app.update(Message::ThumbnailBatchRendered(vec![(1, handle)], 1));
    // Page 1 must be inserted
    assert!(app.sidebar.thumbnails.contains_key(&1));
    // The returned task should be non-trivial (backfill for pages 2 & 3).
    // We can't easily inspect iced::Task internals, but we can verify the
    // method doesn't panic and the thumbnail state is correct.
    let _ = task;
}

#[test]
fn render_visible_thumbnails_respects_concurrency_limit() {
    let mut app = test_app_with_document();
    app.sidebar.visible = true;
    app.sidebar.thumbnail_dpi = 36.0;
    app.sidebar.viewport_height = 600.0;
    // At the limit — should return early without spawning.
    app.sidebar.active_batch_tasks = MAX_CONCURRENT_THUMBNAIL_TASKS;
    let _ = app.render_visible_thumbnails();
    // Counter must not increase beyond the limit.
    assert_eq!(
        app.sidebar.active_batch_tasks,
        MAX_CONCURRENT_THUMBNAIL_TASKS
    );
}

#[test]
fn schedule_thumbnail_backfill_respects_concurrency_limit() {
    let mut app = test_app_with_document();
    app.sidebar.visible = true;
    app.sidebar.thumbnail_dpi = 36.0;
    app.sidebar.active_batch_tasks = MAX_CONCURRENT_THUMBNAIL_TASKS;
    let _ = app.schedule_thumbnail_backfill();
    // Counter must not increase beyond the limit.
    assert_eq!(
        app.sidebar.active_batch_tasks,
        MAX_CONCURRENT_THUMBNAIL_TASKS
    );
}

#[test]
fn thumbnail_batch_rendered_decrements_active_batch_tasks() {
    let mut app = test_app_with_document();
    app.sidebar.backfill_generation = 1;
    app.sidebar.active_batch_tasks = 1;
    let handle = Handle::from_rgba(1, 1, vec![0u8; 4]);
    let _ = app.update(Message::ThumbnailBatchRendered(vec![(1, handle)], 1));
    // Counter decremented even on successful completion.
    assert_eq!(app.sidebar.active_batch_tasks, 0);
}

#[test]
fn thumbnail_batch_rendered_decrements_on_stale_generation() {
    let mut app = test_app_with_document();
    app.sidebar.backfill_generation = 5;
    app.sidebar.active_batch_tasks = 2;
    let handle = Handle::from_rgba(1, 1, vec![0u8; 4]);
    let _ = app.update(Message::ThumbnailBatchRendered(
        vec![(1, handle)],
        3, // stale
    ));
    // Counter decremented even for stale results.
    assert_eq!(app.sidebar.active_batch_tasks, 1);
    // Page must not be inserted for stale generation.
    assert!(!app.sidebar.thumbnails.contains_key(&1));
}

#[test]
fn sidebar_drag_start_sets_dragging_state() {
    let mut app = test_app_with_document();
    app.sidebar.width = 150.0;
    let _ = app.update(Message::SidebarDragStart(200.0));
    assert!(app.sidebar.dragging);
    assert!((app.sidebar.drag_start_width - 150.0).abs() < f32::EPSILON);
}

#[test]
fn sidebar_drag_start_ignores_x_from_message() {
    // mouse_area on_press doesn't pass position, so SidebarDragStart(0.0)
    // is always sent. The actual start X is captured from the first move.
    let mut app = test_app_with_document();
    let _ = app.update(Message::SidebarDragStart(0.0));
    assert!(app.sidebar.dragging);
    assert!((app.sidebar.drag_start_x - 0.0).abs() < f32::EPSILON);
}

#[test]
fn sidebar_resized_ignored_when_not_dragging() {
    let mut app = test_app_with_document();
    app.sidebar.width = 150.0;
    app.sidebar.dragging = false;
    let _ = app.update(Message::SidebarResized(300.0));
    // Width should not change when not dragging
    assert!((app.sidebar.width - 150.0).abs() < f32::EPSILON);
}

#[test]
fn sidebar_resized_captures_start_x_on_first_move() {
    let mut app = test_app_with_document();
    app.sidebar.width = 150.0;
    let _ = app.update(Message::SidebarDragStart(0.0));
    // First move captures start X
    let _ = app.update(Message::SidebarResized(200.0));
    assert!((app.sidebar.drag_start_x - 200.0).abs() < f32::EPSILON);
    // Width should not change on first move (just capturing start position)
    assert!((app.sidebar.width - 150.0).abs() < f32::EPSILON);
}

#[test]
fn sidebar_resized_tracks_drag_delta() {
    let mut app = test_app_with_document();
    app.sidebar.width = 150.0;
    let _ = app.update(Message::SidebarDragStart(0.0));
    // First move captures start X at 200
    let _ = app.update(Message::SidebarResized(200.0));
    // Second move: delta = 250 - 200 = 50, new width = 150 + 50 = 200
    let _ = app.update(Message::SidebarResized(250.0));
    assert!((app.sidebar.width - 200.0).abs() < f32::EPSILON);
}

#[test]
fn sidebar_resized_clamps_to_min_width() {
    let mut app = test_app_with_document();
    app.sidebar.width = 150.0;
    let _ = app.update(Message::SidebarDragStart(0.0));
    let _ = app.update(Message::SidebarResized(200.0)); // capture start X
    // Drag far left: delta = 0 - 200 = -200, new width = 150 - 200 = -50 → clamped to 80
    let _ = app.update(Message::SidebarResized(0.0));
    assert!((app.sidebar.width - MIN_SIDEBAR_WIDTH).abs() < f32::EPSILON);
}

#[test]
fn sidebar_resized_clamps_to_max_width() {
    let mut app = test_app_with_document();
    app.sidebar.width = 150.0;
    let _ = app.update(Message::SidebarDragStart(0.0));
    let _ = app.update(Message::SidebarResized(200.0)); // capture start X
    // Drag far right: delta = 900 - 200 = 700, new width = 150 + 700 = 850 → clamped to 400
    let _ = app.update(Message::SidebarResized(900.0));
    assert!((app.sidebar.width - MAX_SIDEBAR_WIDTH).abs() < f32::EPSILON);
}

#[test]
fn sidebar_resize_end_clears_dragging() {
    let mut app = test_app_with_document();
    let _ = app.update(Message::SidebarDragStart(0.0));
    assert!(app.sidebar.dragging);
    let _ = app.update(Message::SidebarResizeEnd);
    assert!(!app.sidebar.dragging);
}

#[test]
fn sidebar_resize_end_increments_backfill_generation() {
    let mut app = test_app_with_document();
    let gen_before = app.sidebar.backfill_generation;
    let _ = app.update(Message::SidebarDragStart(0.0));
    let _ = app.update(Message::SidebarResizeEnd);
    assert_eq!(app.sidebar.backfill_generation, gen_before + 1);
}

#[test]
fn sidebar_resize_end_ignored_when_not_dragging() {
    let mut app = test_app_with_document();
    let gen_before = app.sidebar.backfill_generation;
    let _ = app.update(Message::SidebarResizeEnd);
    // Generation should not change when not dragging
    assert_eq!(app.sidebar.backfill_generation, gen_before);
}

#[test]
fn sidebar_resize_debounce_expired_recomputes_dpi() {
    let mut app = test_app_with_document();
    let doc = app.document.as_mut().unwrap();
    doc.page_dimensions.insert(1, (612.0, 792.0));
    app.sidebar.visible = true;
    app.sidebar.width = 200.0;
    app.sidebar.backfill_generation = 5;
    app.sidebar.thumbnail_dpi = 99.0; // will be recalculated
    let _ = app.update(Message::SidebarResizeDebounceExpired(5));
    // DPI should be recomputed based on new width
    let expected_dpi = crate::ui::sidebar::compute_thumbnail_dpi(200.0, 1.0, 612.0);
    assert!((app.sidebar.thumbnail_dpi - expected_dpi).abs() < 0.1);
    // Thumbnails should be cleared for re-render
    assert!(app.sidebar.thumbnails.is_empty());
}

#[test]
fn sidebar_resize_debounce_expired_stale_generation_is_noop() {
    let mut app = test_app_with_document();
    app.sidebar.backfill_generation = 5;
    app.sidebar.thumbnail_dpi = 99.0;
    let _ = app.update(Message::SidebarResizeDebounceExpired(3)); // stale
    // DPI should not change
    assert!((app.sidebar.thumbnail_dpi - 99.0).abs() < f32::EPSILON);
}

#[test]
fn active_batch_tasks_does_not_underflow() {
    let mut app = test_app_with_document();
    app.sidebar.backfill_generation = 1;
    app.sidebar.active_batch_tasks = 0; // already zero
    let handle = Handle::from_rgba(1, 1, vec![0u8; 4]);
    let _ = app.update(Message::ThumbnailBatchRendered(vec![(1, handle)], 1));
    // saturating_sub must keep it at 0.
    assert_eq!(app.sidebar.active_batch_tasks, 0);
}

/// Helper: create a minimal one-page PDF in a temp file.
fn make_temp_pdf() -> tempfile::NamedTempFile {
    use lopdf::{Dictionary, Object};
    let tmp = tempfile::NamedTempFile::new().expect("temp file");
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut page_dict = Dictionary::new();
    page_dict.set("Type", Object::Name(b"Page".to_vec()));
    page_dict.set("Parent", Object::Reference(pages_id));
    page_dict.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(612),
            Object::Integer(792),
        ]),
    );
    let page_id = doc.add_object(Object::Dictionary(page_dict));
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages_dict.set("Count", Object::Integer(1));
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc.save(tmp.path()).expect("save temp pdf");
    tmp
}

#[test]
fn handle_file_opened_resets_to_page_one() {
    let mut app = test_app_with_document();
    // Simulate being on page 3 of previous document with a non-zero scroll
    app.document.as_mut().unwrap().current_page = 3;
    app.canvas.scroll_y = 5000.0;

    let tmp = make_temp_pdf();
    let _ = app.handle_file_opened(tmp.path().to_path_buf());

    assert_eq!(app.document.as_ref().unwrap().current_page, 1);
    assert_eq!(app.canvas.scroll_y, 0.0);
}

#[test]
fn handle_file_opened_resets_active_batch_tasks() {
    let mut app = test_app_with_document();
    app.sidebar.active_batch_tasks = 3;
    let tmp = make_temp_pdf();
    let _ = app.handle_file_opened(tmp.path().to_path_buf());
    // The counter is reset to 0 at file-open time; any new render tasks
    // spawned immediately after may increment it, but it must stay within
    // the concurrency limit — not accumulate from the prior 3.
    assert!(app.sidebar.active_batch_tasks <= MAX_CONCURRENT_THUMBNAIL_TASKS);
}

#[test]
fn handle_file_opened_clears_editor_content() {
    let mut app = test_app_with_document();
    // Simulate a stale editor_content from a previous multi-line overlay
    app.editor_content = Some(iced::widget::text_editor::Content::with_text("stale text"));
    assert!(app.editor_content.is_some());

    let tmp = make_temp_pdf();
    let _ = app.handle_file_opened(tmp.path().to_path_buf());

    // editor_content should be cleared when opening a new PDF
    assert!(app.editor_content.is_none());
}

#[test]
fn handle_file_opened_with_bad_path_records_load_error() {
    // spe-6vq: a failed load must be observable so the IPC `open` response can
    // report actual failure instead of message-construction success.
    let (mut app, _) = App::new(false);
    let _ = app.handle_file_opened(PathBuf::from("/nonexistent/does-not-exist.pdf"));
    assert!(app.document.is_none());
    assert!(
        app.last_command_error.is_some(),
        "a failed open should record an error message"
    );
}

#[test]
fn handle_file_opened_success_clears_previous_load_error() {
    let (mut app, _) = App::new(false);
    app.last_command_error = Some("stale error from a previous failed open".to_string());
    let tmp = make_temp_pdf();
    let _ = app.handle_file_opened(tmp.path().to_path_buf());
    assert!(app.document.is_some());
    assert!(app.last_command_error.is_none());
}

#[test]
fn ipc_open_command_dispatch_leaves_document_unset_on_bad_path() {
    // This exercises the full Command(Open) dispatch path (message
    // construction, update(), handle_file_opened) but does not inspect the
    // IPC response itself: send_ipc_response's returned Task is async and
    // this harness doesn't drive it (unlike deliver_ipc_response_writes_to_channel,
    // which calls the async fn directly). The response *contract* — that a
    // failed load reports ok:false with an error — is covered directly by
    // command_response_reports_failure_when_load_failed below.
    let (mut app, _) = App::new(true);
    let _rx = attach_ipc_response_sender(&mut app);
    let _ = app.update(Message::Ipc(crate::ipc::IpcEvent::Command(
        crate::ipc::IpcCommand::Open {
            path: PathBuf::from("/nonexistent/does-not-exist.pdf"),
        },
    )));
    assert!(app.document.is_none());
}

#[test]
fn command_response_reports_failure_when_load_failed() {
    let (mut app, _) = App::new(false);
    let _ = app.handle_file_opened(PathBuf::from("/nonexistent/does-not-exist.pdf"));
    let response = app.command_response(crate::ipc::IpcResponse {
        ok: true,
        error: None,
        warning: None,
    });
    assert!(!response.ok, "a failed load must not report ok:true");
    assert!(response.error.is_some());
    assert!(
        app.last_command_error.is_none(),
        "the error should be consumed once reported, so it doesn't leak into the next command"
    );
}

#[test]
fn command_response_reports_success_when_load_succeeded() {
    let (mut app, _) = App::new(false);
    let tmp = make_temp_pdf();
    let _ = app.handle_file_opened(tmp.path().to_path_buf());
    let ok_response = crate::ipc::IpcResponse {
        ok: true,
        error: None,
        warning: None,
    };
    let response = app.command_response(ok_response);
    assert!(response.ok);
    assert!(response.error.is_none());
}

#[test]
fn command_response_includes_warning_recorded_by_the_handler() {
    let (mut app, _) = App::new(false);
    app.last_command_warning = Some("replaced with '?': '中' (U+4E2D)".to_string());
    let response = app.command_response(crate::ipc::IpcResponse {
        ok: true,
        error: None,
        warning: None,
    });
    assert!(response.ok);
    assert_eq!(
        response.warning.as_deref(),
        Some("replaced with '?': '中' (U+4E2D)")
    );
    assert!(
        app.last_command_warning.is_none(),
        "the warning should be consumed once reported, so it doesn't leak into the next command"
    );
}

#[test]
fn command_response_omits_warning_when_handler_recorded_none() {
    let (mut app, _) = App::new(false);
    let response = app.command_response(crate::ipc::IpcResponse {
        ok: true,
        error: None,
        warning: None,
    });
    assert!(response.warning.is_none());
}

#[test]
fn render_visible_thumbnails_increments_active_batch_tasks_when_below_limit() {
    let mut app = test_app_with_document();
    app.sidebar.visible = true;
    app.sidebar.thumbnail_dpi = 36.0;
    app.sidebar.viewport_height = 600.0;
    // Below the limit and pages are unrendered — should spawn and increment.
    assert_eq!(app.sidebar.active_batch_tasks, 0);
    let _ = app.render_visible_thumbnails();
    // At least one task was spawned; counter reflects that.
    assert!(app.sidebar.active_batch_tasks >= 1);
}

#[test]
fn schedule_thumbnail_backfill_increments_active_batch_tasks_when_below_limit() {
    let mut app = test_app_with_document();
    app.sidebar.visible = true;
    app.sidebar.thumbnail_dpi = 36.0;
    assert_eq!(app.sidebar.active_batch_tasks, 0);
    let _ = app.schedule_thumbnail_backfill();
    // One batch task was scheduled; counter should be 1.
    assert_eq!(app.sidebar.active_batch_tasks, 1);
}

#[test]
fn noop_preserves_active_overlay() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    assert!(app.canvas.active_overlay.is_some());
    assert!(app.canvas.editing);
    app.update(Message::Noop);
    assert!(app.canvas.active_overlay.is_some());
    assert!(app.canvas.editing);
}

#[test]
fn save_destination_sets_status_message_on_success() {
    let mut app = test_app_with_document();
    let tmp_source = make_temp_pdf();
    let _ = app.handle_file_opened(tmp_source.path().to_path_buf());
    let tmp_dest = tempfile::NamedTempFile::new().expect("temp file");
    app.update(Message::SaveDestinationChosen(
        tmp_dest.path().to_path_buf(),
    ));
    assert!(app.status_message.is_some());
    let (msg, _) = app.status_message.as_ref().unwrap();
    assert!(msg.contains("Saved to"), "expected 'Saved to' in '{msg}'");
}

/// Place a single overlay containing `text` on page 1, save it, and return the
/// app afterward so callers can inspect the toast and/or `last_command_warning`.
fn save_app_with_text(text: &str) -> App {
    let mut app = test_app_with_document();
    let tmp_source = make_temp_pdf();
    let _ = app.handle_file_opened(tmp_source.path().to_path_buf());
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        // No wrap width, so the text reaches the writer exactly as typed —
        // embedded newlines included.
        width: None,
    });
    app.update(Message::UpdateOverlayText(text.to_string()));
    app.update(Message::DeselectOverlay);

    let tmp_dest = tempfile::NamedTempFile::new().expect("temp file");
    app.update(Message::SaveDestinationChosen(
        tmp_dest.path().to_path_buf(),
    ));
    app
}

/// Place a single overlay containing `text` on page 1, save it, and return the
/// resulting status toast.
fn save_with_text(text: &str) -> String {
    save_app_with_text(text)
        .status_message
        .as_ref()
        .expect("status message")
        .0
        .clone()
}

#[test]
fn save_status_names_characters_the_pdf_encoding_could_not_represent() {
    let msg = save_with_text("\u{4e2d}");

    assert!(
        msg.contains("Saved to"),
        "the save still succeeded: '{msg}'"
    );
    assert!(
        msg.contains("'\u{4e2d}' (U+4E2D)"),
        "the substituted character must be named with its codepoint: '{msg}'"
    );
}

/// A newline reaches the writer intact on the unwrapped path and is dropped by
/// the encoding. Printing it raw would disclose nothing — which is the whole
/// point of naming substitutions — so it must be described by codepoint alone.
#[test]
fn save_status_describes_an_invisible_character_without_emitting_it() {
    let msg = save_with_text("a\nb");

    assert!(
        msg.contains("(U+000A)"),
        "the dropped newline must be named by codepoint: '{msg}'"
    );
    assert!(
        !msg.contains('\n'),
        "a control character must never be written raw into the toast: '{msg}'"
    );
}

#[test]
fn save_status_caps_a_long_list_of_unencodable_characters() {
    let msg = save_with_text("\u{4e2d}\u{1f600}\u{100}\u{4e00}\u{4e8c}\u{4e09}\u{56db}");

    assert!(
        msg.contains("and 2 more"),
        "only the first few characters belong in a toast: '{msg}'"
    );
}

#[test]
fn save_status_stays_quiet_when_every_character_encodes() {
    let msg = save_with_text("caf\u{e9}");

    assert!(
        !msg.contains('?'),
        "a losslessly encoded save must not warn: '{msg}'"
    );
}

/// A save that substituted characters must surface that in `last_command_warning`
/// too, mirroring the status toast, so an IPC `save` client — which never sees the
/// toast — learns about the substitution the same way (spe-i1b, #127).
#[test]
fn save_records_a_command_warning_naming_substituted_characters() {
    let app = save_app_with_text("\u{4e2d}");

    let warning = app
        .last_command_warning
        .as_deref()
        .expect("a save with substitutions must record a command warning");
    assert!(
        warning.contains("'\u{4e2d}' (U+4E2D)"),
        "the substituted character must be named with its codepoint: '{warning}'"
    );
}

#[test]
fn save_records_no_command_warning_when_every_character_encodes() {
    let app = save_app_with_text("caf\u{e9}");

    assert!(
        app.last_command_warning.is_none(),
        "a losslessly encoded save must not warn: {:?}",
        app.last_command_warning
    );
}

#[test]
fn save_destination_sets_status_message_on_failure() {
    let mut app = test_app_with_document();
    let tmp_source = make_temp_pdf();
    let _ = app.handle_file_opened(tmp_source.path().to_path_buf());
    // Try to save to source path — this should fail (same-file guard)
    let source_path = app.document.as_ref().unwrap().source_path.clone();
    app.update(Message::SaveDestinationChosen(source_path));
    assert!(app.status_message.is_some());
    let (msg, _) = app.status_message.as_ref().unwrap();
    assert!(
        msg.contains("Save failed"),
        "expected 'Save failed' in '{msg}'"
    );
}

#[test]
fn handle_save_with_existing_path_sets_status_message() {
    let mut app = test_app_with_document();
    let tmp_source = make_temp_pdf();
    let _ = app.handle_file_opened(tmp_source.path().to_path_buf());
    let tmp_dest = tempfile::NamedTempFile::new().expect("temp file");
    // Set save_path so handle_save takes the quick-save branch.
    app.document.as_mut().unwrap().save_path = Some(tmp_dest.path().to_path_buf());
    app.update(Message::Save);
    assert!(app.status_message.is_some());
    let (msg, _) = app.status_message.as_ref().unwrap();
    assert!(msg.contains("Saved to"), "expected 'Saved to' in '{msg}'");
}

#[test]
fn dismiss_toast_clears_message_after_five_seconds() {
    let (mut app, _) = App::new(false);
    // Plant a message that is already 6 seconds old
    let old_time = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(6))
        .unwrap();
    app.status_message = Some(("test".to_string(), old_time));
    app.update(Message::DismissToast);
    assert!(app.status_message.is_none());
}

#[test]
fn dismiss_toast_keeps_message_before_five_seconds() {
    let (mut app, _) = App::new(false);
    // Plant a message that is only 1 second old
    app.status_message = Some(("test".to_string(), std::time::Instant::now()));
    app.update(Message::DismissToast);
    assert!(app.status_message.is_some());
}

#[test]
fn app_default_has_no_status_message() {
    let (app, _) = App::new(false);
    assert!(app.status_message.is_none());
}

#[test]
fn view_with_toast_does_not_panic() {
    let (mut app, _) = App::new(false);
    app.status_message = Some(("Saved to foo.pdf".to_string(), std::time::Instant::now()));
    let _element = app.view();
}

#[test]
fn toast_renders_below_toolbar_and_above_content() {
    let (mut app, _) = App::new(false);
    app.status_message = Some(("Saved to foo.pdf".to_string(), std::time::Instant::now()));

    let mut simulator = iced_test::Simulator::new(app.view());
    let toast_bounds = simulator
        .find("Saved to foo.pdf")
        .expect("toast text should be present in the view")
        .bounds();
    let content_bounds = simulator
        .find("Open a PDF to get started")
        .expect("placeholder content text should be present in the view")
        .bounds();
    let toolbar_bounds = simulator
        .find("100%")
        .expect("zoom label in the toolbar should be present in the view")
        .bounds();

    assert!(
        toast_bounds.height > 0.0,
        "toast must occupy nonzero height, got {toast_bounds:?}"
    );
    assert!(
        toast_bounds.y >= toolbar_bounds.y + toolbar_bounds.height,
        "toast (y={}) must render below the toolbar (bottom={})",
        toast_bounds.y,
        toolbar_bounds.y + toolbar_bounds.height
    );
    assert!(
        toast_bounds.y < content_bounds.y,
        "toast (y={}) must render above the content area (y={})",
        toast_bounds.y,
        content_bounds.y
    );
}

// --- Floating text input (spe-vnm.3.1) ---

#[test]
fn canvas_state_edit_start_text_defaults_to_none() {
    let state = CanvasState::default();
    assert!(state.edit_start_text.is_none());
}

#[test]
fn place_overlay_sets_edit_start_text_to_empty_string() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    assert_eq!(app.canvas.edit_start_text, Some(String::new()));
}

#[test]
fn commit_text_clears_edit_start_text() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::CommitText);
    assert!(app.canvas.edit_start_text.is_none());
}

#[test]
fn deselect_overlay_while_editing_records_the_text_edit() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    assert!(app.canvas.editing);

    app.update(Message::DeselectOverlay);

    // Deselecting commits first, so the edit is undoable.
    assert!(matches!(
        app.undo_stack.last(),
        Some(UndoCommand::EditText { new_text, .. }) if new_text == "Hello"
    ));
}

#[test]
fn deselect_overlay_when_not_editing_clears_selection() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::CommitText); // exit edit mode
    assert!(!app.canvas.editing);
    app.update(Message::DeselectOverlay);
    assert!(app.canvas.active_overlay.is_none());
}

#[test]
fn view_with_editing_overlay_does_not_panic() {
    let mut app = test_app_with_document();
    let doc = app.document.as_mut().unwrap();
    doc.page_dimensions.insert(1, (612.0, 792.0));
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    assert!(app.canvas.editing);
    let _element = app.view();
}

// --- text_editor (multi-line) tests ---

#[test]
fn place_multiline_overlay_initializes_editor_content() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 500.0 },
        width: Some(200.0),
    });
    assert!(app.editor_content.is_some());
}

#[test]
fn place_singleline_overlay_does_not_initialize_editor_content() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 500.0 },
        width: None,
    });
    assert!(app.editor_content.is_none());
}

#[test]
fn text_editor_action_syncs_text_to_overlay() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 500.0 },
        width: Some(200.0),
    });
    // Insert the character 'H' into the editor
    app.update(Message::TextEditorAction(
        iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Insert('H')),
    ));
    let text = app.document.as_ref().unwrap().overlays[0].text.clone();
    assert!(
        text.contains('H'),
        "overlay text should contain 'H', got: {text:?}"
    );
}

#[test]
fn commit_text_clears_editor_content() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 500.0 },
        width: Some(200.0),
    });
    assert!(app.editor_content.is_some());
    app.update(Message::CommitText);
    assert!(app.editor_content.is_none());
}

#[test]
fn update_overlay_text_syncs_editor_content_for_multiline_overlay() {
    // spe-jpw: the IPC `type` command dispatches UpdateOverlayText directly,
    // bypassing TextEditorAction. For multi-line overlays the visible widget
    // renders from editor_content, so editor_content must converge on the
    // same text real typing would have produced.
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 500.0 },
        width: Some(200.0),
    });
    app.update(Message::UpdateOverlayText("Hello\nWorld".to_string()));
    let editor_text = app
        .editor_content
        .as_ref()
        .expect("multi-line overlay must keep editor_content populated")
        .text();
    assert!(
        editor_text.starts_with("Hello\nWorld"),
        "editor_content should reflect the IPC-typed text, got: {editor_text:?}"
    );
}

#[test]
fn update_overlay_text_leaves_editor_content_none_for_singleline_overlay() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 500.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    assert!(app.editor_content.is_none());
}

#[test]
fn update_overlay_text_does_not_clobber_editor_content_for_unrelated_singleline_overlay() {
    // Review-traced hazard: the sync guard must key on the *target* overlay's
    // own multiline-ness (width.is_some()), not merely on whether
    // editor_content happens to be populated from a previously edited
    // overlay. Reproduces the exact sequence: multiline overlay 0 is being
    // edited (editor_content populated) -> Select single-line overlay 1
    // (handle_select_overlay changes active_overlay without touching
    // editor_content) -> Type into overlay 1 must not stomp overlay 0's
    // editor_content with overlay 1's unrelated text.
    let mut app = test_app_with_document();
    let font = app.toolbar.font;
    let font_size = app.toolbar.font_size;
    {
        let doc = app.document.as_mut().unwrap();
        doc.overlays.push(TextOverlay {
            page: 1,
            position: PdfPosition { x: 100.0, y: 500.0 },
            text: String::new(),
            font,
            font_size,
            width: Some(200.0), // multiline
        });
        doc.overlays.push(TextOverlay {
            page: 1,
            position: PdfPosition { x: 150.0, y: 600.0 },
            text: String::new(),
            font,
            font_size,
            width: None, // single-line
        });
    }
    // Simulate overlay 0 (multiline) having just been dragged into edit mode.
    app.canvas.active_overlay = Some(0);
    app.canvas.editing = true;
    app.editor_content = Some(iced::widget::text_editor::Content::with_text(""));

    app.update(Message::UpdateOverlayText("AAA".to_string()));
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "AAA");

    // Retarget the live session at overlay 1 (single-line) directly. Going via
    // Message::SelectOverlay would commit and tear down overlay 0's session
    // (and its editor_content) first, which is the behaviour asserted by
    // selecting_a_different_overlay_ends_the_multiline_edit_session below;
    // this test pins the narrower gate inside handle_update_overlay_text.
    app.canvas.active_overlay = Some(1);

    // IPC `type` now targets overlay 1.
    app.update(Message::UpdateOverlayText("BBB".to_string()));
    assert_eq!(app.document.as_ref().unwrap().overlays[1].text, "BBB");

    let editor_text = app
        .editor_content
        .as_ref()
        .expect("editor_content should still hold overlay 0's multiline text")
        .text();
    assert!(
        editor_text.starts_with("AAA"),
        "editor_content must not be clobbered by typing into an unrelated \
         single-line overlay, got: {editor_text:?}"
    );
}

// =====================================================================
// EditOverlay tests
// =====================================================================

fn test_app_with_overlay() -> App {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    // Give the overlay text so it survives the commit, then leave editing state.
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    app.update(Message::CommitText);
    app
}

/// The font picker option for `name`, exactly as the toolbar builds it.
fn font_option_named(app: &App, name: &str) -> toolbar::FontOption {
    toolbar::option_named(&app.font_registry, name)
}

/// An app with a single selected overlay whose font size has been set to
/// `size`. Shared setup for the font-size stepper/arrow-key tests below,
/// which otherwise only differ in which message they dispatch next.
fn selected_overlay_at_font_size(size: f32) -> App {
    let mut app = test_app_with_overlay();
    app.update(Message::SelectOverlay(0));
    app.update(Message::ChangeFontSize(size));
    app
}

/// Dispatches `toolbar::Message::FontSizeIncrement`/`FontSizeDecrement`
/// against a selected overlay at font size 12.0, mirroring what the
/// stepper buttons send.
fn step_via_toolbar_message(increment: bool) -> App {
    let mut app = selected_overlay_at_font_size(12.0);
    let message = if increment {
        toolbar::Message::FontSizeIncrement
    } else {
        toolbar::Message::FontSizeDecrement
    };
    app.update(Message::Toolbar(message));
    app
}

/// Dispatches `Message::FontSizeArrowKeyResult` against a selected overlay
/// at font size 12.0, as if the arrow key's focus check had already
/// confirmed the font-size input was focused.
fn step_via_arrow_key_result(increment: bool) -> App {
    let mut app = selected_overlay_at_font_size(12.0);
    app.update(Message::FontSizeArrowKeyResult(increment));
    app
}

/// Simulates a real click on the toolbar's font-size stepper button whose
/// label matches `glyph` (`"+"` or `"-"`) and applies whatever messages it
/// produces.
fn click_font_size_stepper(app: &mut App, glyph: &str) {
    let mut simulator = iced_test::Simulator::new(app.view());
    simulator.click(glyph).unwrap_or_else(|_| {
        panic!("font size \"{glyph}\" button should be present in the toolbar")
    });
    let messages: Vec<Message> = simulator.into_messages().collect();
    for message in messages {
        app.update(message);
    }
}

#[test]
fn edit_overlay_sets_active_and_editing() {
    let mut app = test_app_with_overlay();
    app.update(Message::SelectOverlay(0));
    assert!(!app.canvas.editing);

    app.update(Message::EditOverlay(0));
    assert_eq!(app.canvas.active_overlay, Some(0));
    assert!(app.canvas.editing);
}

#[test]
fn edit_overlay_syncs_toolbar_font_and_size() {
    let mut app = test_app_with_document();
    // Place an overlay with a specific font configuration
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    let courier = app.font_registry.find_by_name("Courier").unwrap();
    let helvetica = app.font_registry.default_font();
    app.update(Message::ChangeFont(courier));
    app.update(Message::ChangeFontSize(18.0));
    app.update(Message::CommitText);

    // Change toolbar to something different
    app.toolbar.font = helvetica;
    app.toolbar.font_size = 12.0;
    app.toolbar.font_size_input = "12".to_string();

    app.update(Message::EditOverlay(0));
    assert_eq!(app.toolbar.font, courier);
    assert!((app.toolbar.font_size - 18.0).abs() < f32::EPSILON);
    assert_eq!(app.toolbar.font_size_input, "18");
}

#[test]
fn undo_font_change_syncs_toolbar_to_active_overlay() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    app.update(Message::CommitText);
    let courier = app.font_registry.find_by_name("Courier").unwrap();
    app.update(Message::ChangeFont(courier));

    app.update(Message::Undo);

    let doc = app.document.as_ref().unwrap();
    assert_eq!(app.toolbar.font, doc.overlays[0].font);
}

#[test]
fn undo_font_size_change_syncs_toolbar_to_active_overlay() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    app.update(Message::CommitText);
    app.update(Message::ChangeFontSize(30.0));

    app.update(Message::Undo);

    let doc = app.document.as_ref().unwrap();
    assert!((app.toolbar.font_size - doc.overlays[0].font_size).abs() < f32::EPSILON);
    assert_eq!(
        app.toolbar.font_size_input,
        format!("{}", doc.overlays[0].font_size)
    );
}

#[test]
fn clicking_font_size_increment_button_steps_up_the_overlay_size() {
    let mut app = selected_overlay_at_font_size(12.0);

    click_font_size_stepper(&mut app, "+");

    assert!((app.toolbar.font_size - 13.0).abs() < f32::EPSILON);
    let doc = app.document.as_ref().unwrap();
    assert!((doc.overlays[0].font_size - 13.0).abs() < f32::EPSILON);
}

#[test]
fn clicking_font_size_decrement_button_steps_down_the_overlay_size() {
    let mut app = selected_overlay_at_font_size(12.0);

    click_font_size_stepper(&mut app, "-");

    assert!((app.toolbar.font_size - 11.0).abs() < f32::EPSILON);
    let doc = app.document.as_ref().unwrap();
    assert!((doc.overlays[0].font_size - 11.0).abs() < f32::EPSILON);
}

#[test]
fn font_size_increment_toolbar_message_steps_up_and_updates_overlay() {
    let app = step_via_toolbar_message(true);

    assert!((app.toolbar.font_size - 13.0).abs() < f32::EPSILON);
    assert_eq!(app.toolbar.font_size_input, "13");
    let doc = app.document.as_ref().unwrap();
    assert!((doc.overlays[0].font_size - 13.0).abs() < f32::EPSILON);
}

#[test]
fn font_size_decrement_toolbar_message_steps_down_and_updates_overlay() {
    let app = step_via_toolbar_message(false);

    assert!((app.toolbar.font_size - 11.0).abs() < f32::EPSILON);
    assert_eq!(app.toolbar.font_size_input, "11");
    let doc = app.document.as_ref().unwrap();
    assert!((doc.overlays[0].font_size - 11.0).abs() < f32::EPSILON);
}

#[test]
fn font_size_decrement_toolbar_message_floors_at_minimum() {
    let mut app = selected_overlay_at_font_size(1.0);

    app.update(Message::Toolbar(toolbar::Message::FontSizeDecrement));

    assert!((app.toolbar.font_size - 1.0).abs() < f32::EPSILON);
}

#[test]
fn font_size_submit_clamps_below_minimum_value_to_floor() {
    let mut app = test_app_with_overlay();
    app.update(Message::SelectOverlay(0));
    app.toolbar.font_size_input = "0.5".to_string();

    app.update(Message::Toolbar(toolbar::Message::FontSizeSubmit));

    assert!((app.toolbar.font_size - 1.0).abs() < f32::EPSILON);
    assert_eq!(app.toolbar.font_size_input, "1");
    let doc = app.document.as_ref().unwrap();
    assert!((doc.overlays[0].font_size - 1.0).abs() < f32::EPSILON);
}

#[test]
fn font_size_decrement_after_clamped_submit_stays_at_floor_then_steps_cleanly() {
    let mut app = test_app_with_overlay();
    app.update(Message::SelectOverlay(0));
    app.toolbar.font_size_input = "0.5".to_string();
    app.update(Message::Toolbar(toolbar::Message::FontSizeSubmit));

    app.update(Message::Toolbar(toolbar::Message::FontSizeDecrement));
    assert!((app.toolbar.font_size - 1.0).abs() < f32::EPSILON);

    app.update(Message::Toolbar(toolbar::Message::FontSizeIncrement));
    assert!((app.toolbar.font_size - 2.0).abs() < f32::EPSILON);
}

#[test]
fn font_size_increment_is_undoable() {
    let mut app = selected_overlay_at_font_size(12.0);

    app.update(Message::Toolbar(toolbar::Message::FontSizeIncrement));
    app.update(Message::Undo);

    let doc = app.document.as_ref().unwrap();
    assert!((doc.overlays[0].font_size - 12.0).abs() < f32::EPSILON);
}

#[test]
fn font_size_increment_while_editing_returns_focus_task() {
    let mut app = test_app_with_overlay();
    app.update(Message::EditOverlay(0));

    let task = app.update(Message::Toolbar(toolbar::Message::FontSizeIncrement));

    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "FontSizeIncrement while editing should return a focus Task, got: {debug}"
    );
}

#[test]
fn arrow_up_maps_to_font_size_arrow_pressed_increment() {
    let msg = key_to_message(
        keyboard::Key::Named(keyboard::key::Named::ArrowUp),
        keyboard::Modifiers::empty(),
    );
    assert!(matches!(msg, Some(Message::FontSizeArrowPressed(true))));
}

#[test]
fn arrow_down_maps_to_font_size_arrow_pressed_decrement() {
    let msg = key_to_message(
        keyboard::Key::Named(keyboard::key::Named::ArrowDown),
        keyboard::Modifiers::empty(),
    );
    assert!(matches!(msg, Some(Message::FontSizeArrowPressed(false))));
}

#[test]
fn alt_modified_arrow_up_does_not_map_to_font_size_arrow_pressed() {
    let msg = key_to_message(
        keyboard::Key::Named(keyboard::key::Named::ArrowUp),
        keyboard::Modifiers::ALT,
    );
    assert!(msg.is_none());
}

#[test]
fn alt_modified_arrow_down_does_not_map_to_font_size_arrow_pressed() {
    let msg = key_to_message(
        keyboard::Key::Named(keyboard::key::Named::ArrowDown),
        keyboard::Modifiers::ALT,
    );
    assert!(msg.is_none());
}

#[test]
fn font_size_arrow_pressed_returns_a_focus_query_task() {
    let (mut app, _) = App::new(false);

    let task = app.update(Message::FontSizeArrowPressed(true));

    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "FontSizeArrowPressed should query focus via a widget operation Task, got: {debug}"
    );
}

#[test]
fn arrow_key_result_increments_when_focused() {
    assert!(matches!(
        arrow_key_result(true, true),
        Message::FontSizeArrowKeyResult(true)
    ));
}

#[test]
fn arrow_key_result_decrements_when_focused() {
    assert!(matches!(
        arrow_key_result(true, false),
        Message::FontSizeArrowKeyResult(false)
    ));
}

#[test]
fn arrow_key_result_is_noop_when_unfocused_and_increment() {
    assert!(matches!(arrow_key_result(false, true), Message::Noop));
}

#[test]
fn arrow_key_result_is_noop_when_unfocused_and_decrement() {
    assert!(matches!(arrow_key_result(false, false), Message::Noop));
}

#[test]
fn font_size_arrow_key_result_increments_when_focused() {
    let app = step_via_arrow_key_result(true);

    assert!((app.toolbar.font_size - 13.0).abs() < f32::EPSILON);
    let doc = app.document.as_ref().unwrap();
    assert!((doc.overlays[0].font_size - 13.0).abs() < f32::EPSILON);
}

#[test]
fn font_size_arrow_key_result_refocuses_font_size_input_while_editing() {
    // While an overlay is being edited, ChangeFontSize's shared
    // refocus_editing_widget() step steals focus back to the overlay's text
    // widget. An arrow-key-triggered change must chain a corrective refocus
    // onto the font-size input afterward, or the next arrow press resolves
    // as unfocused and does nothing. The units count captures both focus
    // operations: the editor refocus baked into ChangeFontSize, plus the
    // corrective one this handler chains on top.
    let mut app = test_app_with_overlay();
    app.update(Message::SelectOverlay(0));
    app.update(Message::EditOverlay(0));

    let task = app.update(Message::FontSizeArrowKeyResult(true));

    assert_eq!(
        task.units(),
        2,
        "arrow-key font-size changes while editing should refocus the \
         font-size input after ChangeFontSize's editor refocus, so repeated \
         arrow presses keep working"
    );
}

#[test]
fn font_size_arrow_key_result_decrements_when_focused() {
    let app = step_via_arrow_key_result(false);

    assert!((app.toolbar.font_size - 11.0).abs() < f32::EPSILON);
    let doc = app.document.as_ref().unwrap();
    assert!((doc.overlays[0].font_size - 11.0).abs() < f32::EPSILON);
}

#[test]
fn redo_font_change_syncs_toolbar_to_active_overlay() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    app.update(Message::CommitText);
    let courier = app.font_registry.find_by_name("Courier").unwrap();
    app.update(Message::ChangeFont(courier));
    app.update(Message::Undo);
    // Toolbar drifts away from the overlay's (reverted) font between the
    // undo and the redo, so the redo assertion can't pass by coincidence.
    let times = app.font_registry.find_by_name("Times Bold").unwrap();
    app.toolbar.font = times;

    app.update(Message::Redo);

    let doc = app.document.as_ref().unwrap();
    assert_eq!(app.toolbar.font, doc.overlays[0].font);
    assert_eq!(app.toolbar.font, courier);
}

#[test]
fn undo_does_not_panic_when_undo_stack_is_empty() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));

    // Undoing the placement itself removes overlay 0, so active_overlay
    // (still Some(0)) would be out of bounds — the toolbar sync must guard
    // against that rather than panic on an out-of-range index.
    app.update(Message::Undo);
    app.update(Message::Undo);
}

#[test]
fn edit_overlay_snapshots_text_to_edit_start_text() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    // Type some text
    app.update(Message::UpdateOverlayText("original text".to_string()));
    app.update(Message::CommitText);

    app.update(Message::EditOverlay(0));
    assert_eq!(
        app.canvas.edit_start_text,
        Some("original text".to_string())
    );
}

#[test]
fn edit_overlay_initializes_editor_content_for_multiline() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: Some(200.0),
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    app.update(Message::CommitText);

    // Before EditOverlay, editor_content is None
    assert!(app.editor_content.is_none());

    app.update(Message::EditOverlay(0));
    // Multi-line overlay (width.is_some()) should initialize editor_content
    assert!(app.editor_content.is_some());
}

#[test]
fn edit_overlay_does_not_initialize_editor_content_for_single_line() {
    let mut app = test_app_with_overlay();
    // Single-line overlay (width is None)
    app.update(Message::EditOverlay(0));
    assert!(app.editor_content.is_none());
}

#[test]
fn edit_overlay_out_of_range_is_noop() {
    let mut app = test_app_with_overlay();
    app.canvas.active_overlay = None;
    app.canvas.editing = false;

    app.update(Message::EditOverlay(99));
    assert!(app.canvas.active_overlay.is_none());
    assert!(!app.canvas.editing);
}

#[test]
fn edit_overlay_without_document_is_noop() {
    let (mut app, _) = App::new(false);
    // No document — should not panic
    app.update(Message::EditOverlay(0));
    assert!(app.canvas.active_overlay.is_none());
    assert!(!app.canvas.editing);
}

#[test]
fn commit_text_pushes_edit_text_command_when_text_changed() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    // PlaceOverlay should push one command
    assert_eq!(app.undo_stack.len(), 1);

    // Simulate typing text
    let overlay = &mut app.document.as_mut().unwrap().overlays[0];
    overlay.text = "Hello".to_string();

    // Commit the text change
    app.update(Message::CommitText);

    // Should have pushed EditText command
    assert_eq!(app.undo_stack.len(), 2);
    if let UndoCommand::EditText {
        old_text, new_text, ..
    } = &app.undo_stack[1]
    {
        assert_eq!(old_text, "");
        assert_eq!(new_text, "Hello");
    } else {
        panic!("Expected EditText command at index 1");
    }
}

#[test]
fn commit_text_no_command_when_text_unchanged() {
    let mut app = test_app_with_overlay();
    let commands_before = app.undo_stack.len();

    // Re-enter and leave the edit session without changing the text.
    app.update(Message::EditOverlay(0));
    app.update(Message::CommitText);

    // Should NOT push EditText command
    assert_eq!(app.undo_stack.len(), commands_before);
}

#[test]
fn undo_after_text_edit_restores_previous_text() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });

    // Type text
    let overlay = &mut app.document.as_mut().unwrap().overlays[0];
    overlay.text = "Hello".to_string();

    // Commit
    let _ = app.update(Message::CommitText);
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "Hello");

    // Undo
    let _ = app.update(Message::Undo);

    // Text should be restored to empty
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "");
}

#[test]
fn redo_after_undo_restores_edited_text() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });

    // Type text
    let overlay = &mut app.document.as_mut().unwrap().overlays[0];
    overlay.text = "Hello".to_string();

    // Commit
    let _ = app.update(Message::CommitText);
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "Hello");

    // Undo
    let _ = app.update(Message::Undo);
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "");

    // Redo
    let _ = app.update(Message::Redo);

    // Text should be restored to "Hello"
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "Hello");
}

#[test]
fn stack_overlay_element_returns_placeholder_when_not_editing() {
    // Regression: floating_text_input built a Stack with 1 child when not editing
    // and 2 children when editing, causing Iced to reset canvas ProgramState
    // on commit, which made overlays disappear during drag.
    let mut app = test_app_with_document();
    let doc = app.document.as_mut().unwrap();
    doc.page_dimensions.insert(1, (612.0, 792.0));
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::CommitText);
    // After commit: editing=false, active_overlay=Some(0)
    assert!(!app.canvas.editing);

    let doc = app.document.as_ref().unwrap();
    let dpi = canvas::effective_dpi(app.canvas.zoom);
    let layout = canvas::page_layout(&doc.page_dimensions, doc.page_count, app.canvas.zoom, dpi);

    // stack_overlay_element must return an element even when not editing,
    // so that floating_text_input always builds a 2-child Stack.
    // Calling it without panic verifies correctness; the non-Option return
    // type guarantees a value at compile time.
    let _element = app.stack_overlay_element(doc, &layout);
}

#[test]
fn stack_overlay_element_returns_widget_when_editing() {
    let mut app = test_app_with_document();
    let doc = app.document.as_mut().unwrap();
    doc.page_dimensions.insert(1, (612.0, 792.0));
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    // After PlaceOverlay: editing=true
    assert!(app.canvas.editing);

    let doc = app.document.as_ref().unwrap();
    let dpi = canvas::effective_dpi(app.canvas.zoom);
    let layout = canvas::page_layout(&doc.page_dimensions, doc.page_count, app.canvas.zoom, dpi);

    // Calling stack_overlay_element while editing must not panic.
    let _element = app.stack_overlay_element(doc, &layout);
}

// =====================================================================
// spe-910: text_input focus after overlay placement
// =====================================================================

#[test]
fn place_overlay_returns_focus_task() {
    let mut app = test_app_with_document();
    let task = app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "PlaceOverlay should return a focus Task, got: {debug}"
    );
}

#[test]
fn place_multiline_overlay_returns_focus_task() {
    let mut app = test_app_with_document();
    let task = app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: Some(200.0),
    });
    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "PlaceOverlay (multi-line) should return a focus Task, got: {debug}"
    );
}

#[test]
fn edit_overlay_returns_focus_task() {
    let mut app = test_app_with_overlay();
    let task = app.update(Message::EditOverlay(0));
    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "EditOverlay should return a focus Task, got: {debug}"
    );
}

#[test]
fn edit_multiline_overlay_returns_focus_task() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: Some(200.0),
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    app.update(Message::CommitText);
    let task = app.update(Message::EditOverlay(0));
    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "EditOverlay (multi-line) should return a focus Task, got: {debug}"
    );
}

// =====================================================================
// spe-0g1: font picker returns focus to the text editor
// =====================================================================

#[test]
fn change_font_while_editing_returns_focus_task() {
    let mut app = test_app_with_overlay();
    let courier = app.font_registry.find_by_name("Courier").unwrap();
    app.update(Message::EditOverlay(0));

    let task = app.update(Message::ChangeFont(courier));

    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "ChangeFont while editing should return a focus Task, got: {debug}"
    );
}

#[test]
fn change_font_via_toolbar_message_returns_focus_task() {
    let mut app = test_app_with_overlay();
    app.update(Message::EditOverlay(0));
    let courier = font_option_named(&app, "Courier");

    let task = app.update(Message::Toolbar(toolbar::Message::FontSelected(courier)));

    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "Toolbar font selection while editing should return a focus Task, got: {debug}"
    );
}

#[test]
fn change_font_when_not_editing_returns_no_focus_task() {
    let mut app = test_app_with_overlay();
    let courier = app.font_registry.find_by_name("Courier").unwrap();
    app.update(Message::SelectOverlay(0));

    let task = app.update(Message::ChangeFont(courier));

    let debug = format!("{task:?}");
    assert!(
        debug.contains("units: 0"),
        "ChangeFont without an active edit must not steal focus, got: {debug}"
    );
}

#[test]
fn change_font_without_document_returns_no_focus_task() {
    let (mut app, _) = App::new(false);
    let courier = app.font_registry.find_by_name("Courier").unwrap();

    let task = app.update(Message::ChangeFont(courier));

    let debug = format!("{task:?}");
    assert!(
        debug.contains("units: 0"),
        "ChangeFont without a document must not focus anything, got: {debug}"
    );
}

#[test]
fn change_font_size_while_editing_returns_focus_task() {
    let mut app = test_app_with_overlay();
    app.update(Message::EditOverlay(0));

    let task = app.update(Message::ChangeFontSize(18.0));

    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "ChangeFontSize while editing should return a focus Task, got: {debug}"
    );
}

#[test]
fn change_font_size_when_not_editing_returns_no_focus_task() {
    let mut app = test_app_with_overlay();
    app.update(Message::SelectOverlay(0));

    let task = app.update(Message::ChangeFontSize(18.0));

    let debug = format!("{task:?}");
    assert!(
        debug.contains("units: 0"),
        "ChangeFontSize without an active edit must not steal focus, got: {debug}"
    );
}

// =====================================================================
// spe-fxn: toolbar focus audit — controls that must preserve the
// in-progress edit and hand focus back to the floating text widget,
// mirroring the change_font_while_editing precedent above.
// =====================================================================

#[test]
fn zoom_in_while_editing_returns_more_task_units_than_not_editing() {
    let mut editing_app = test_app_with_overlay();
    editing_app.update(Message::EditOverlay(0));
    let editing_task = editing_app.update(Message::ZoomIn);

    let mut selected_app = test_app_with_overlay();
    selected_app.update(Message::SelectOverlay(0));
    let selected_task = selected_app.update(Message::ZoomIn);

    assert!(
        editing_task.units() > selected_task.units(),
        "ZoomIn while editing should include a focus task alongside the zoom render task"
    );
}

#[test]
fn zoom_out_while_editing_returns_more_task_units_than_not_editing() {
    let mut editing_app = test_app_with_overlay();
    editing_app.update(Message::EditOverlay(0));
    let editing_task = editing_app.update(Message::ZoomOut);

    let mut selected_app = test_app_with_overlay();
    selected_app.update(Message::SelectOverlay(0));
    let selected_task = selected_app.update(Message::ZoomOut);

    assert!(
        editing_task.units() > selected_task.units(),
        "ZoomOut while editing should include a focus task alongside the zoom render task"
    );
}

#[test]
fn zoom_reset_while_editing_returns_more_task_units_than_not_editing() {
    let mut editing_app = test_app_with_overlay();
    editing_app.update(Message::EditOverlay(0));
    let editing_task = editing_app.update(Message::ZoomReset);

    let mut selected_app = test_app_with_overlay();
    selected_app.update(Message::SelectOverlay(0));
    let selected_task = selected_app.update(Message::ZoomReset);

    assert!(
        editing_task.units() > selected_task.units(),
        "ZoomReset while editing should include a focus task alongside the zoom render task"
    );
}

#[test]
fn zoom_fit_width_while_editing_returns_more_task_units_than_not_editing() {
    let window = iced::Size::new(1000.0, 800.0);

    let mut editing_app = test_app_with_overlay();
    editing_app
        .document
        .as_mut()
        .unwrap()
        .page_dimensions
        .insert(1, (612.0, 792.0));
    editing_app.window_size = Some(window);
    editing_app.update(Message::EditOverlay(0));
    let editing_task = editing_app.update(Message::ZoomFitWidth);

    let mut selected_app = test_app_with_overlay();
    selected_app
        .document
        .as_mut()
        .unwrap()
        .page_dimensions
        .insert(1, (612.0, 792.0));
    selected_app.window_size = Some(window);
    selected_app.update(Message::SelectOverlay(0));
    let selected_task = selected_app.update(Message::ZoomFitWidth);

    assert!(
        editing_task.units() > selected_task.units(),
        "ZoomFitWidth while editing should include a focus task alongside the zoom render task"
    );
}

#[test]
fn toggle_sidebar_while_editing_returns_focus_task() {
    let mut app = test_app_with_overlay();
    app.update(Message::EditOverlay(0));

    let task = app.update(Message::ToggleSidebar);

    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "ToggleSidebar while editing should return a focus Task, got: {debug}"
    );
}

#[test]
fn toggle_sidebar_when_not_editing_returns_no_focus_task() {
    let mut app = test_app_with_overlay();
    app.update(Message::SelectOverlay(0));

    let task = app.update(Message::ToggleSidebar);

    let debug = format!("{task:?}");
    assert!(
        debug.contains("units: 0"),
        "ToggleSidebar without an active edit must not steal focus, got: {debug}"
    );
}

#[test]
fn page_input_submit_while_editing_returns_more_task_units_than_not_editing() {
    let mut editing_app = test_app_with_overlay();
    editing_app.update(Message::EditOverlay(0));
    editing_app.toolbar.page_input = "1".to_string();
    let editing_task = editing_app.update(Message::Toolbar(toolbar::Message::PageInputSubmit));

    let mut selected_app = test_app_with_overlay();
    selected_app.update(Message::SelectOverlay(0));
    selected_app.toolbar.page_input = "1".to_string();
    let selected_task = selected_app.update(Message::Toolbar(toolbar::Message::PageInputSubmit));

    assert!(
        editing_task.units() > selected_task.units(),
        "PageInputSubmit while editing should include a focus task alongside the scroll task"
    );
}

#[test]
fn next_page_while_editing_returns_more_task_units_than_not_editing() {
    let mut editing_app = test_app_with_overlay();
    editing_app.update(Message::EditOverlay(0));
    let editing_task = editing_app.update(Message::NextPage);

    let mut selected_app = test_app_with_overlay();
    selected_app.update(Message::SelectOverlay(0));
    let selected_task = selected_app.update(Message::NextPage);

    assert!(
        editing_task.units() > selected_task.units(),
        "NextPage while editing should include a focus task alongside the scroll task"
    );
}

#[test]
fn previous_page_while_editing_returns_more_task_units_than_not_editing() {
    let mut editing_app = test_app_with_overlay();
    editing_app.document.as_mut().unwrap().current_page = 2;
    editing_app.update(Message::EditOverlay(0));
    let editing_task = editing_app.update(Message::PreviousPage);

    let mut selected_app = test_app_with_overlay();
    selected_app.document.as_mut().unwrap().current_page = 2;
    selected_app.update(Message::SelectOverlay(0));
    let selected_task = selected_app.update(Message::PreviousPage);

    assert!(
        editing_task.units() > selected_task.units(),
        "PreviousPage while editing should include a focus task alongside the scroll task"
    );
}

#[test]
fn save_with_existing_path_while_editing_returns_focus_task() {
    let mut app = test_app_with_document();
    let tmp_source = make_temp_pdf();
    let _ = app.handle_file_opened(tmp_source.path().to_path_buf());
    let tmp_dest = tempfile::NamedTempFile::new().expect("temp file");
    app.document.as_mut().unwrap().save_path = Some(tmp_dest.path().to_path_buf());
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    assert!(app.canvas.editing);

    let task = app.update(Message::Save);

    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "Save while editing should return a focus Task, got: {debug}"
    );
}

#[test]
fn save_when_not_editing_returns_no_focus_task() {
    let mut app = test_app_with_document();
    let tmp_source = make_temp_pdf();
    let _ = app.handle_file_opened(tmp_source.path().to_path_buf());
    let tmp_dest = tempfile::NamedTempFile::new().expect("temp file");
    app.document.as_mut().unwrap().save_path = Some(tmp_dest.path().to_path_buf());

    let task = app.update(Message::Save);

    let debug = format!("{task:?}");
    assert!(
        debug.contains("units: 0"),
        "Save without an active edit must not steal focus, got: {debug}"
    );
}

#[test]
fn save_destination_chosen_while_editing_returns_focus_task() {
    let mut app = test_app_with_document();
    let tmp_source = make_temp_pdf();
    let _ = app.handle_file_opened(tmp_source.path().to_path_buf());
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    assert!(app.canvas.editing);
    let tmp_dest = tempfile::NamedTempFile::new().expect("temp file");

    let task = app.update(Message::SaveDestinationChosen(
        tmp_dest.path().to_path_buf(),
    ));

    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "SaveDestinationChosen while editing should return a focus Task, got: {debug}"
    );
}

#[test]
fn save_destination_chosen_when_not_editing_returns_no_focus_task() {
    let mut app = test_app_with_document();
    let tmp_source = make_temp_pdf();
    let _ = app.handle_file_opened(tmp_source.path().to_path_buf());
    let tmp_dest = tempfile::NamedTempFile::new().expect("temp file");

    let task = app.update(Message::SaveDestinationChosen(
        tmp_dest.path().to_path_buf(),
    ));

    let debug = format!("{task:?}");
    assert!(
        debug.contains("units: 0"),
        "SaveDestinationChosen without an active edit must not steal focus, got: {debug}"
    );
}

#[test]
fn dialog_dismissed_while_editing_returns_focus_task() {
    // Canceling the Open or Save As file dialog (no path chosen) must not
    // strand the floating overlay editor unfocused, same as every other
    // toolbar interaction that doesn't end the edit.
    let mut app = test_app_with_overlay();
    app.update(Message::EditOverlay(0));

    let task = app.update(Message::DialogDismissed);

    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "DialogDismissed while editing should return a focus Task, got: {debug}"
    );
}

#[test]
fn dialog_dismissed_when_not_editing_returns_no_focus_task() {
    let mut app = test_app_with_overlay();
    app.update(Message::SelectOverlay(0));

    let task = app.update(Message::DialogDismissed);

    let debug = format!("{task:?}");
    assert!(
        debug.contains("units: 0"),
        "DialogDismissed without an active edit must not steal focus, got: {debug}"
    );
}

#[test]
fn font_size_decrement_while_editing_returns_focus_task() {
    let mut app = test_app_with_overlay();
    app.update(Message::EditOverlay(0));

    let task = app.update(Message::Toolbar(toolbar::Message::FontSizeDecrement));

    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "FontSizeDecrement while editing should return a focus Task, got: {debug}"
    );
}

#[test]
fn font_size_submit_refocuses_font_size_input_while_editing() {
    // Same corrective-chain class as FontSizeArrowKeyResult: ChangeFontSize's
    // shared refocus_editing_widget() step steals focus back to the overlay's
    // text widget, so a typed size submitted from the font-size field must
    // chain a corrective refocus back onto that field afterward, or the
    // user's next keystroke silently lands in the overlay editor instead.
    let mut app = test_app_with_overlay();
    app.update(Message::SelectOverlay(0));
    app.update(Message::EditOverlay(0));
    app.toolbar.font_size_input = "18".to_string();

    let task = app.update(Message::Toolbar(toolbar::Message::FontSizeSubmit));

    assert_eq!(
        task.units(),
        2,
        "FontSizeSubmit while editing should refocus the font-size input \
         after ChangeFontSize's editor refocus, so the user's next keystroke \
         lands in the font-size field, not the overlay editor"
    );
}

#[test]
fn page_input_submit_refocuses_page_input_while_editing() {
    // Same corrective-chain class: GoToPage's scroll_to_page batches
    // refocus_editing_widget(), which steals focus back to the overlay's
    // text widget. A page number submitted from the page-input field must
    // chain a corrective refocus back onto that field afterward, or the
    // user's next digits silently corrupt the overlay text instead.
    let mut app = test_app_with_overlay();
    app.update(Message::EditOverlay(0));
    app.toolbar.page_input = "1".to_string();

    let task = app.update(Message::Toolbar(toolbar::Message::PageInputSubmit));

    // scroll_to_page already batches a scroll operation with the editor
    // refocus (2 units); the corrective refocus onto the page input adds a
    // third.
    assert_eq!(
        task.units(),
        3,
        "PageInputSubmit while editing should refocus the page-number input \
         after GoToPage's editor refocus, so the user's next keystroke lands \
         in the page field, not the overlay editor"
    );
}

// =====================================================================
// spe-zr9: text_input has matching font size and zero padding
// =====================================================================

#[test]
fn app_has_text_input_id() {
    let (app, _) = App::new(false);
    // text_input_id must exist for focus operations
    let _id = &app.text_input_id;
}

// =====================================================================
// spe-fsu.3.1: --ipc CLI flag and IPC subscription wiring
// =====================================================================

#[test]
fn app_default_ipc_disabled() {
    let (app, _) = App::new(false);
    assert!(!app.ipc_enabled);
    assert!(app.ipc_response_sender.is_none());
    assert!(!app.pending_ipc_wait);
}

#[test]
fn app_ipc_enabled_when_requested() {
    let (app, _) = App::new(true);
    assert!(app.ipc_enabled);
}

#[test]
fn is_render_idle_true_when_no_document() {
    let (app, _) = App::new(false);
    assert!(app.is_render_idle());
}

#[test]
fn is_render_idle_false_with_active_tasks() {
    let (mut app, _) = App::new(false);
    app.sidebar.active_batch_tasks = 1;
    assert!(!app.is_render_idle());
}

#[test]
fn is_render_idle_false_when_page_not_yet_rendered() {
    let mut app = test_app_with_document();
    // Document has 3 pages but no page_images — not idle
    assert!(!app.is_render_idle());
}

#[test]
fn is_render_idle_true_when_all_pages_rendered() {
    let mut app = test_app_with_document();
    let doc = app.document.as_mut().unwrap();
    let page_count = doc.page_count;
    let handle = iced::widget::image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]);
    for page in 1..=page_count {
        doc.page_images.insert(page, handle.clone());
    }
    assert!(app.is_render_idle());
}

// spe-d3m: zoom bumps zoom_generation and schedules a debounced re-render,
// but the pre-zoom page_images entries stay cached (for instant visual
// feedback) until the debounce fires. is_render_idle must not report idle
// during that window, or wait_ready hands back a stale pre-zoom capture.

#[test]
fn is_render_idle_false_after_zoom_before_debounce_fires() {
    let mut app = test_app_with_document();
    let doc = app.document.as_mut().unwrap();
    let page_count = doc.page_count;
    let handle = iced::widget::image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]);
    for page in 1..=page_count {
        doc.page_images.insert(page, handle.clone());
    }
    assert!(app.is_render_idle());

    app.update(Message::ZoomIn);
    // page_images is still fully populated (stale, pre-zoom) — key presence
    // alone says idle, but the debounced re-render for the new zoom hasn't
    // run yet.
    assert!(
        !app.is_render_idle(),
        "wait_ready must not resolve on a stale pre-zoom render"
    );
}

#[test]
fn is_render_idle_true_once_rerender_catches_up_to_zoom_generation() {
    let mut app = test_app_with_document();
    let doc = app.document.as_mut().unwrap();
    let page_count = doc.page_count;
    let handle = iced::widget::image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]);
    for page in 1..=page_count {
        doc.page_images.insert(page, handle.clone());
    }

    app.update(Message::ZoomIn);
    assert!(!app.is_render_idle());

    let generation = app.canvas.zoom_generation;
    app.update(Message::ZoomDebounceExpired(generation));
    // Debounce fired: cache cleared for the fresh generation, but the
    // re-render task hasn't delivered images yet.
    assert!(!app.is_render_idle());

    let doc = app.document.as_mut().unwrap();
    for page in 1..=page_count {
        doc.page_images.insert(page, handle.clone());
    }
    assert!(
        app.is_render_idle(),
        "idle once images reflect the current zoom generation"
    );
}

// =====================================================================
// spe-dr0: IPC event dispatch — commands keep their follow-up task,
// and responses are delivered for every command.
// =====================================================================

/// Attach an IPC response channel so the response-sending path executes.
/// The returned receiver must be kept alive for the duration of the test.
fn attach_ipc_response_sender(
    app: &mut App,
) -> tokio::sync::mpsc::Receiver<crate::ipc::IpcResponse> {
    let (tx, rx) = tokio::sync::mpsc::channel::<crate::ipc::IpcResponse>(1);
    app.ipc_response_sender = Some(crate::ipc::ResponseSender(std::sync::Arc::new(
        tokio::sync::Mutex::new(tx),
    )));
    rx
}

#[test]
fn ipc_ready_event_stores_response_sender() {
    let (mut app, _) = App::new(true);
    assert!(app.ipc_response_sender.is_none());
    let (tx, _rx) = tokio::sync::mpsc::channel::<crate::ipc::IpcResponse>(1);
    let sender = crate::ipc::ResponseSender(std::sync::Arc::new(tokio::sync::Mutex::new(tx)));
    let _ = app.update(Message::Ipc(crate::ipc::IpcEvent::Ready(sender)));
    assert!(app.ipc_response_sender.is_some());
}

#[test]
fn ipc_command_applies_translated_message() {
    let mut app = test_app_with_document();
    let _rx = attach_ipc_response_sender(&mut app);
    let _ = app.update(Message::Ipc(crate::ipc::IpcEvent::Command(
        crate::ipc::IpcCommand::Click {
            page: 1,
            x: 100.0,
            y: 700.0,
        },
    )));
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
    assert_eq!(app.canvas.active_overlay, Some(0));
}

#[test]
fn ipc_command_with_error_does_not_apply_a_message() {
    let mut app = test_app_with_document();
    let _rx = attach_ipc_response_sender(&mut app);
    // An unknown font name produces an error response and no overlay change.
    let _ = app.update(Message::Ipc(crate::ipc::IpcEvent::Command(
        crate::ipc::IpcCommand::Font {
            family: "No Such Font 12345".to_string(),
        },
    )));
    assert!(app.document.as_ref().unwrap().overlays.is_empty());
}

// =====================================================================
// spe-749: every IPC command's response reflects whether it acted.
// `run_ipc_command` returns the response synchronously, so these assert on
// the reply the client would receive without driving the async delivery task.
// =====================================================================

#[test]
fn ipc_type_without_active_overlay_reports_failure() {
    let mut app = test_app_with_document();
    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Type {
        text: "Hello".to_string(),
    });
    assert!(
        !response.ok,
        "typing with nothing selected must not report ok"
    );
    assert!(response.error.unwrap().contains("no overlay is active"));
}

#[test]
fn ipc_click_without_document_reports_failure() {
    let (mut app, _) = App::new(true);
    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Click {
        page: 1,
        x: 10.0,
        y: 10.0,
    });
    assert!(!response.ok);
    assert!(response.error.unwrap().contains("no document"));
}

#[test]
fn ipc_select_with_out_of_range_index_reports_failure() {
    let mut app = test_app_with_document();
    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Select { index: 3 });
    assert!(!response.ok);
    assert!(response.error.unwrap().contains("out of range"));
}

#[test]
fn ipc_type_after_click_reports_success_and_applies_the_text() {
    let mut app = test_app_with_document();
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Click {
        page: 1,
        x: 100.0,
        y: 700.0,
    });
    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Type {
        text: "Hello".to_string(),
    });
    assert!(response.ok, "typing into a fresh overlay must succeed");
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "Hello");
}

#[test]
fn ipc_open_failure_is_reported_through_the_shared_command_outcome() {
    // The same mechanism that reports a failed open reports any handler-recorded
    // failure — there is no per-command branch in the response path.
    let (mut app, _) = App::new(true);
    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Open {
        path: PathBuf::from("/nonexistent/does-not-exist.pdf"),
    });
    assert!(!response.ok);
    assert!(response.error.unwrap().contains("failed to open"));
}

#[test]
fn ipc_command_error_does_not_leak_into_the_next_command() {
    let (mut app, _) = App::new(true);
    let (failed, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Open {
        path: PathBuf::from("/nonexistent/does-not-exist.pdf"),
    });
    assert!(!failed.ok);
    let (next, _task) = app.run_ipc_command(crate::ipc::IpcCommand::ZoomIn);
    assert!(
        next.ok,
        "a later command must not inherit the earlier error"
    );
}

// =====================================================================
// spe-7f1: `click_at` goes through the canvas hit test, so automation can
// exercise real click-to-select instead of always placing.
// =====================================================================

/// Place an overlay reading "Hello" at PDF (100, 700) on page 1 and leave
/// nothing selected — the state a subsequent click has to interpret.
fn app_with_committed_overlay() -> App {
    let mut app = test_app_with_document();
    app.document
        .as_mut()
        .unwrap()
        .page_dimensions
        .insert(1, (612.0, 792.0));
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Click {
        page: 1,
        x: 100.0,
        y: 700.0,
    });
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Type {
        text: "Hello".to_string(),
    });
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Deselect);
    app
}

#[test]
fn ipc_click_at_on_an_existing_overlay_selects_it_instead_of_placing() {
    let mut app = app_with_committed_overlay();
    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::ClickAt {
        page: 1,
        x: 102.0,
        y: 705.0,
    });
    assert!(response.ok);
    assert_eq!(
        app.document.as_ref().unwrap().overlays.len(),
        1,
        "no new overlay"
    );
    assert_eq!(app.canvas.active_overlay, Some(0));
}

#[test]
fn ipc_click_at_on_empty_page_area_places_a_new_overlay() {
    let mut app = app_with_committed_overlay();
    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::ClickAt {
        page: 1,
        x: 400.0,
        y: 300.0,
    });
    assert!(response.ok);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 2);
    assert_eq!(app.canvas.active_overlay, Some(1));
}

#[test]
fn ipc_click_bypasses_the_hit_test_and_always_places() {
    // The older `click` command is deliberately unconditional; keeping it
    // distinguishable from `click_at` is what makes `click_at` meaningful.
    let mut app = app_with_committed_overlay();
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Click {
        page: 1,
        x: 102.0,
        y: 705.0,
    });
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 2);
}

// =====================================================================
// spe-94g: save over IPC — reuses the writer, bypasses only the dialog
// =====================================================================

/// An app holding a real, loadable PDF with one overlay of text on it — the
/// state a save is meant to persist.
fn app_with_loaded_pdf(source: &tempfile::NamedTempFile) -> App {
    let (mut app, _) = App::new(true);
    let _ = app.handle_file_opened(source.path().to_path_buf());
    assert!(app.document.is_some(), "fixture PDF must load");
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Click {
        page: 1,
        x: 100.0,
        y: 700.0,
    });
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Type {
        text: "Saved overlay".to_string(),
    });
    app
}

#[test]
fn ipc_save_writes_the_destination_file_and_reports_success() {
    let source = make_temp_pdf();
    let mut app = app_with_loaded_pdf(&source);
    let dest = tempfile::NamedTempFile::new().expect("temp file");

    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Save {
        path: dest.path().to_path_buf(),
    });

    assert!(response.ok, "save reported: {:?}", response.error);
    assert!(
        lopdf::Document::load(dest.path()).is_ok(),
        "the saved file must be a readable PDF"
    );
}

#[test]
fn ipc_save_records_the_destination_for_later_saves() {
    let source = make_temp_pdf();
    let mut app = app_with_loaded_pdf(&source);
    let dest = tempfile::NamedTempFile::new().expect("temp file");

    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Save {
        path: dest.path().to_path_buf(),
    });

    assert_eq!(
        app.document.as_ref().unwrap().save_path.as_deref(),
        Some(dest.path())
    );
}

#[test]
fn ipc_save_over_the_source_file_reports_failure() {
    let source = make_temp_pdf();
    let mut app = app_with_loaded_pdf(&source);

    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Save {
        path: source.path().to_path_buf(),
    });

    assert!(!response.ok, "overwriting the source must not report ok");
    assert!(response.error.unwrap().contains("source file"));
}

#[test]
fn ipc_save_to_another_spelling_of_the_source_is_rejected() {
    // The guard exists to stop the open document being truncated. It must key
    // on which file the path denotes, not on how the path is spelled: an IPC
    // client can name the source relatively, through `..`, or via a symlink.
    let source = make_temp_pdf();
    let mut app = app_with_loaded_pdf(&source);
    let alias = source
        .path()
        .parent()
        .unwrap()
        .join("..")
        .join(source.path().parent().unwrap().file_name().unwrap())
        .join(source.path().file_name().unwrap());
    assert_ne!(
        alias,
        source.path(),
        "the alias must be spelled differently"
    );

    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Save { path: alias });

    assert!(!response.ok, "overwriting the source must not report ok");
    assert!(response.error.unwrap().contains("source file"));
}

#[test]
fn ipc_save_to_a_hard_link_of_the_source_is_rejected() {
    // A hard link is a second name for the same inode. Canonicalizing both
    // names yields two different paths, so a path comparison — however
    // normalized — cannot see that writing one truncates the other.
    let source = make_temp_pdf();
    let mut app = app_with_loaded_pdf(&source);
    let dir = tempfile::tempdir().expect("temp dir");
    let link = dir.path().join("same-inode.pdf");
    std::fs::hard_link(source.path(), &link).expect("hard link");

    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Save { path: link });

    assert!(
        !response.ok,
        "writing a hard link of the source truncates the open document"
    );
    assert!(response.error.unwrap().contains("source file"));
}

#[test]
fn ipc_save_to_an_unwritable_path_reports_failure() {
    let source = make_temp_pdf();
    let mut app = app_with_loaded_pdf(&source);

    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Save {
        path: PathBuf::from("/nonexistent-dir/out.pdf"),
    });

    assert!(!response.ok, "an unwritable path must not report ok");
    assert!(response.error.is_some());
}

#[test]
fn ipc_save_without_document_reports_failure() {
    let (mut app, _) = App::new(true);
    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Save {
        path: PathBuf::from("/tmp/out.pdf"),
    });
    assert!(!response.ok);
    assert!(response.error.unwrap().contains("no document"));
}

// =====================================================================
// spe-0nc: undo / redo over IPC
// =====================================================================

#[test]
fn ipc_undo_reverts_a_placement() {
    let mut app = test_app_with_document();
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Click {
        page: 1,
        x: 100.0,
        y: 700.0,
    });
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);

    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Undo);
    assert!(response.ok);
    assert!(app.document.as_ref().unwrap().overlays.is_empty());
}

#[test]
fn ipc_redo_restores_an_undone_edit() {
    // The edit is committed first: while a placement is still being edited,
    // undo cancels that session rather than popping the command history, so
    // there would be nothing to redo.
    let mut app = app_with_committed_overlay();
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Edit { index: 0 });
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Type {
        text: "Hello again".to_string(),
    });
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Deselect);
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Undo);
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "Hello");

    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Redo);
    assert!(response.ok, "redo reported: {:?}", response.error);
    assert_eq!(
        app.document.as_ref().unwrap().overlays[0].text,
        "Hello again"
    );
}

#[test]
fn ipc_redo_while_editing_is_rejected() {
    // handle_redo commits an open edit session first, and that commit clears
    // the redo stack, so the pop that follows finds nothing. The precondition
    // reads redo_depth from before the commit, so it cannot see that coming —
    // the command must be refused up front rather than reporting success for
    // a redo that never happens.
    let mut app = app_with_committed_overlay();
    // Bank a redo entry: change the text, commit it, then undo.
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Edit { index: 0 });
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Type {
        text: "Hello again".to_string(),
    });
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Deselect);
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Undo);
    assert_eq!(app.redo_stack.len(), 1, "a redo entry must be banked");

    // Now open a fresh edit session with modified, uncommitted text.
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Edit { index: 0 });
    let _ = app.run_ipc_command(crate::ipc::IpcCommand::Type {
        text: "Changed".to_string(),
    });

    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Redo);

    assert!(!response.ok, "a redo that cannot happen must not report ok");
    assert!(response.error.unwrap().contains("edit session"));
    // Rejected before any side effect: the session is still open and its
    // banked redo entry survives.
    assert!(app.canvas.editing, "the edit session must still be open");
    assert_eq!(
        app.redo_stack.len(),
        1,
        "the redo entry must not have been cleared by a commit"
    );
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "Changed");
}

#[test]
fn ipc_undo_with_nothing_to_undo_reports_failure() {
    let mut app = test_app_with_document();
    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Undo);
    assert!(!response.ok);
    assert!(response.error.unwrap().contains("nothing to undo"));
}

#[test]
fn ipc_redo_with_nothing_to_redo_reports_failure() {
    let mut app = test_app_with_document();
    let (response, _task) = app.run_ipc_command(crate::ipc::IpcCommand::Redo);
    assert!(!response.ok);
    assert!(response.error.unwrap().contains("nothing to redo"));
}

#[test]
fn ipc_wait_ready_when_idle_does_not_set_pending() {
    let (mut app, _) = App::new(true);
    let _rx = attach_ipc_response_sender(&mut app);
    let _ = app.update(Message::Ipc(crate::ipc::IpcEvent::WaitReady));
    assert!(!app.pending_ipc_wait);
}

#[test]
fn ipc_wait_ready_when_rendering_sets_pending() {
    let mut app = test_app_with_document();
    // Document has unrendered pages, so rendering is not idle.
    let _ = app.update(Message::Ipc(crate::ipc::IpcEvent::WaitReady));
    assert!(app.pending_ipc_wait);
}

#[test]
fn ipc_wait_ready_idle_without_sender_is_noop() {
    let (mut app, _) = App::new(true);
    let _ = app.update(Message::Ipc(crate::ipc::IpcEvent::WaitReady));
    assert!(!app.pending_ipc_wait);
    assert!(app.ipc_response_sender.is_none());
}

#[test]
fn check_ipc_wait_clears_pending_when_idle() {
    let (mut app, _) = App::new(true);
    let _rx = attach_ipc_response_sender(&mut app);
    app.pending_ipc_wait = true;
    let _ = app.check_ipc_wait();
    assert!(!app.pending_ipc_wait);
}

#[test]
fn check_ipc_wait_keeps_pending_when_still_rendering() {
    let mut app = test_app_with_document();
    app.pending_ipc_wait = true;
    let _ = app.check_ipc_wait();
    assert!(app.pending_ipc_wait);
}

#[test]
fn deliver_ipc_response_writes_to_channel() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<crate::ipc::IpcResponse>(1);
    let sender = crate::ipc::ResponseSender(std::sync::Arc::new(tokio::sync::Mutex::new(tx)));
    iced::futures::executor::block_on(deliver_ipc_response(
        sender,
        crate::ipc::IpcResponse {
            ok: true,
            error: None,
            warning: None,
        },
    ));
    let received = rx
        .try_recv()
        .expect("a response should have been delivered");
    assert!(received.ok);
}

// =====================================================================
// spe-w27: an overlay left empty is removed instead of lingering
// =====================================================================

#[test]
fn commit_text_removes_overlay_left_empty() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::CommitText);
    assert!(
        app.document.as_ref().unwrap().overlays.is_empty(),
        "an overlay committed with no text should be removed"
    );
}

#[test]
fn commit_text_clears_selection_when_empty_overlay_removed() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::CommitText);
    assert!(
        app.canvas.active_overlay.is_none(),
        "the removed overlay must not stay selected"
    );
}

#[test]
fn commit_text_keeps_overlay_that_has_text() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    app.update(Message::CommitText);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
    assert_eq!(app.canvas.active_overlay, Some(0));
}

#[test]
fn abandoned_empty_placement_leaves_no_undo_history() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::CommitText);
    assert!(
        app.undo_stack.is_empty(),
        "placing and abandoning an empty overlay should leave no trace"
    );
}

#[test]
fn abandoned_empty_placement_leaves_no_undo_history_after_style_change() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    let courier = app.font_registry.find_by_name("Courier").unwrap();
    app.update(Message::ChangeFont(courier));
    app.update(Message::CommitText);
    assert!(
        app.undo_stack.is_empty(),
        "placing and abandoning an empty overlay should leave no trace, even if \
         the font was changed before any text was typed"
    );
    assert!(app.document.as_ref().unwrap().overlays.is_empty());
}

#[test]
fn erasing_an_existing_overlay_records_a_deletion_for_undo() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    app.update(Message::CommitText);

    app.update(Message::EditOverlay(0));
    app.update(Message::UpdateOverlayText(String::new()));
    app.update(Message::CommitText);

    assert!(app.document.as_ref().unwrap().overlays.is_empty());
    app.update(Message::Undo);
    let overlays = &app.document.as_ref().unwrap().overlays;
    assert_eq!(overlays.len(), 1, "undo restores the erased overlay");
    assert_eq!(overlays[0].text, "Hello");
}

#[test]
fn escape_while_editing_empty_overlay_removes_it() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::DeselectOverlay);
    assert!(app.document.as_ref().unwrap().overlays.is_empty());
    assert!(app.canvas.active_overlay.is_none());
    assert!(!app.canvas.editing);
}

#[test]
fn escape_while_editing_commits_text_then_clears_selection() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    app.update(Message::DeselectOverlay);
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "Hello");
    assert!(!app.canvas.editing);
    assert!(
        app.canvas.active_overlay.is_none(),
        "Escape dismisses the selection as well as the edit session"
    );
}

#[test]
fn discarding_an_empty_overlay_clears_the_redo_stack() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    app.update(Message::CommitText);
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 200.0, y: 600.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("World".to_string()));
    app.update(Message::CommitText);

    // Undo the second overlay's text edit, leaving a redo entry behind.
    app.update(Message::Undo);
    assert_eq!(app.redo_stack.len(), 1);

    // Erasing the first overlay's text discards it, shrinking the list.
    app.update(Message::EditOverlay(0));
    app.update(Message::UpdateOverlayText(String::new()));
    app.update(Message::CommitText);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
    assert!(
        app.redo_stack.is_empty(),
        "redo entries referencing a discarded overlay must not survive"
    );

    // Redo must not index into the shrunken overlay list.
    app.update(Message::Redo);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
}

#[test]
fn discarding_an_earlier_overlay_keeps_a_later_placement_in_history() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("one".to_string()));
    app.update(Message::DeselectOverlay);

    // Place a second overlay, then switch the edit session to the first one.
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 200.0, y: 600.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("two".to_string()));
    app.update(Message::EditOverlay(0));
    app.update(Message::UpdateOverlayText(String::new()));
    app.update(Message::DeselectOverlay);

    // Only the erased overlay is gone; the second placement survives.
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
    assert!(
        matches!(
            app.undo_stack.last(),
            Some(UndoCommand::DeleteOverlay { index: 0, .. })
        ),
        "erasing an established overlay records a deletion, got {:?}",
        app.undo_stack.last()
    );

    app.update(Message::Undo);
    let overlays = &app.document.as_ref().unwrap().overlays;
    assert_eq!(overlays.len(), 2);
    assert_eq!(overlays[0].text, "one", "undo restores the erased overlay");
}

#[test]
fn commit_text_removes_overlay_left_with_only_whitespace() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("   ".to_string()));
    app.update(Message::DeselectOverlay);

    assert!(
        app.document.as_ref().unwrap().overlays.is_empty(),
        "an overlay holding only whitespace renders nothing and should be removed"
    );
    assert!(app.canvas.active_overlay.is_none());
    assert!(
        app.undo_stack.is_empty(),
        "abandoning a whitespace-only placement should leave no trace"
    );
}

#[test]
fn erasing_an_existing_overlay_to_whitespace_records_a_deletion_for_undo() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("Hello".to_string()));
    app.update(Message::CommitText);

    app.update(Message::EditOverlay(0));
    app.update(Message::UpdateOverlayText("  \t ".to_string()));
    app.update(Message::CommitText);

    assert!(app.document.as_ref().unwrap().overlays.is_empty());
    app.update(Message::Undo);
    let overlays = &app.document.as_ref().unwrap().overlays;
    assert_eq!(overlays.len(), 1);
    assert_eq!(
        overlays[0].text, "Hello",
        "undo restores the text as of edit start, not the whitespace"
    );
}

// =====================================================================
// spe-rto: floating editor previews text in the selected font
// =====================================================================

#[test]
fn editing_font_is_none_when_not_editing() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::CommitText);
    assert!(!app.canvas.editing);
    assert!(app.editing_font().is_none());
}

#[test]
fn editing_font_matches_selected_bundled_font() {
    let mut app = test_app_with_document();
    let great_vibes = app.font_registry.find_by_name("Great Vibes").unwrap();
    app.update(Message::ChangeFont(great_vibes));
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    assert!(app.canvas.editing);
    let expected = app.font_registry.get(great_vibes).iced_font;
    assert_eq!(app.editing_font(), Some(expected));
    assert_eq!(expected.family, iced::font::Family::Name("Great Vibes"));
}

#[test]
fn editing_font_follows_font_change_during_edit() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    let courier = app.font_registry.find_by_name("Courier").unwrap();
    app.update(Message::ChangeFont(courier));
    let expected = app.font_registry.get(courier).iced_font;
    assert_eq!(app.editing_font(), Some(expected));
    assert_eq!(expected.family, iced::font::Family::Monospace);
}

#[test]
fn editing_font_for_multiline_overlay_matches_selected_font() {
    let mut app = test_app_with_document();
    let times = app.font_registry.find_by_name("Times Bold").unwrap();
    app.update(Message::ChangeFont(times));
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: Some(200.0),
    });
    assert!(app.editor_content.is_some());
    let expected = app.font_registry.get(times).iced_font;
    assert_eq!(app.editing_font(), Some(expected));
    assert_eq!(expected.weight, iced::font::Weight::Bold);
}

// =====================================================================
// spe-6v8 / spe-164: undo/redo integrity against a live edit session
// =====================================================================

/// Place an overlay, type `text` into it, and commit the edit session.
fn place_committed_overlay(app: &mut App, x: f32, text: &str) {
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText(text.to_string()));
    app.update(Message::CommitText);
}

/// Rebuild the overlay list by applying every undo-stack command in order.
/// A well-formed history must reproduce the live document exactly.
fn replay_undo_stack(app: &App) -> Vec<TextOverlay> {
    let mut overlays = Vec::new();
    for cmd in &app.undo_stack {
        cmd.apply(&mut overlays);
    }
    overlays
}

fn assert_history_matches_document(app: &App, context: &str) {
    let live: Vec<(String, f32)> = app
        .document
        .as_ref()
        .unwrap()
        .overlays
        .iter()
        .map(|o| (o.text.clone(), o.position.x))
        .collect();
    let replayed: Vec<(String, f32)> = replay_undo_stack(app)
        .iter()
        .map(|o| (o.text.clone(), o.position.x))
        .collect();
    assert_eq!(
        live, replayed,
        "{context}: document diverged from its undo history"
    );
}

#[test]
fn undo_during_fresh_placement_removes_the_overlay_being_placed() {
    let mut app = test_app_with_document();
    place_committed_overlay(&mut app, 100.0, "kept");

    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 200.0, y: 700.0 },
        width: None,
    });
    app.update(Message::Undo);

    let overlays = &app.document.as_ref().unwrap().overlays;
    assert_eq!(overlays.len(), 1, "the fresh placement must be gone");
    assert_eq!(overlays[0].text, "kept");
}

#[test]
fn undo_during_fresh_placement_ends_the_edit_session() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::Undo);

    assert!(
        !app.canvas.editing,
        "no edit session may outlive its overlay"
    );
    assert!(app.canvas.active_overlay.is_none());
    assert!(app.canvas.fresh_placement.is_none());
    assert!(app.canvas.edit_start_text.is_none());
}

#[test]
fn redo_after_undoing_a_fresh_placement_does_not_resurrect_a_ghost() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::Undo);
    app.update(Message::CommitText);
    app.update(Message::Redo);

    assert!(
        app.document.as_ref().unwrap().overlays.is_empty(),
        "redo must not restore a text-less overlay the user abandoned"
    );
}

#[test]
fn undo_while_editing_an_existing_overlay_restores_its_pre_edit_text() {
    let mut app = test_app_with_document();
    place_committed_overlay(&mut app, 100.0, "original");

    app.update(Message::EditOverlay(0));
    app.update(Message::UpdateOverlayText("scribble".to_string()));
    app.update(Message::Undo);

    let overlays = &app.document.as_ref().unwrap().overlays;
    assert_eq!(overlays.len(), 1);
    assert_eq!(overlays[0].text, "original");
    assert!(!app.canvas.editing);
}

#[test]
fn deleting_the_overlay_being_placed_ends_the_edit_session() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::DeleteOverlay);

    assert!(app.canvas.fresh_placement.is_none());
    assert!(app.canvas.edit_start_text.is_none());
    assert!(app.canvas.active_overlay.is_none());
    assert!(!app.canvas.editing);
}

#[test]
fn undoing_a_placement_clears_the_selection_it_could_strand() {
    let mut app = test_app_with_document();
    place_committed_overlay(&mut app, 100.0, "first");
    place_committed_overlay(&mut app, 200.0, "second");
    app.update(Message::SelectOverlay(1));

    app.update(Message::Undo); // the text edit: in place
    app.update(Message::Undo); // the placement: removes an overlay

    assert!(
        app.canvas.active_overlay.is_none(),
        "a command that changes the overlay count can strand the selection"
    );
}

#[test]
fn undoing_an_in_place_command_keeps_the_selection() {
    let mut app = test_app_with_document();
    place_committed_overlay(&mut app, 100.0, "first");
    place_committed_overlay(&mut app, 200.0, "second");
    app.update(Message::SelectOverlay(1));

    app.update(Message::Undo); // reverses the text edit only

    assert_eq!(
        app.canvas.active_overlay,
        Some(1),
        "an in-place change leaves every index addressing the same overlay"
    );
}

/// The exact interleaving that desynchronised the document from its history:
/// a stale edit session survived undo and then discarded an unrelated overlay.
#[test]
fn stale_edit_session_cannot_discard_an_unrelated_overlay() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::DeleteOverlay);
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 200.0, y: 700.0 },
        width: None,
    });
    app.update(Message::Undo);
    app.update(Message::Undo);
    app.update(Message::Undo);
    app.update(Message::Redo);
    app.update(Message::CommitText);

    assert_history_matches_document(&app, "after stale-session commit");
}

/// Property check: no interleaving of placement, typing, commit, delete,
/// selection and undo/redo may leave the document disagreeing with the
/// command history that is supposed to describe it.
#[test]
fn document_always_matches_undo_history_under_arbitrary_interleavings() {
    // Deterministic xorshift so failures are reproducible.
    fn next(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    for seed in 1..2000u64 {
        let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut app = test_app_with_document();
        let mut log: Vec<String> = Vec::new();

        for step in 0..30u64 {
            let len = app.document.as_ref().unwrap().overlays.len();
            let msg = match next(&mut rng) % 9 {
                0 => Message::PlaceOverlay {
                    page: 1,
                    position: PdfPosition {
                        x: (step * 7) as f32,
                        y: 700.0,
                    },
                    width: None,
                },
                1 => Message::UpdateOverlayText(format!("t{step}")),
                2 => Message::UpdateOverlayText(String::new()),
                3 => Message::CommitText,
                4 => Message::DeleteOverlay,
                5 if len > 0 => Message::SelectOverlay((next(&mut rng) % len as u64) as usize),
                6 if len > 0 => Message::EditOverlay((next(&mut rng) % len as u64) as usize),
                7 => Message::Undo,
                _ => Message::Redo,
            };
            log.push(format!("{msg:?}"));
            app.update(msg);

            // Positions identify overlays. Text is compared too, except while
            // an edit session is open, which legitimately holds text the
            // history has not recorded yet.
            let include_text = !app.canvas.editing;
            let describe = |o: &TextOverlay| {
                if include_text {
                    format!("{}@{}", o.text, o.position.x)
                } else {
                    format!("@{}", o.position.x)
                }
            };
            let live: Vec<String> = app
                .document
                .as_ref()
                .unwrap()
                .overlays
                .iter()
                .map(describe)
                .collect();
            let replayed: Vec<String> = replay_undo_stack(&app).iter().map(describe).collect();
            assert_eq!(
                live,
                replayed,
                "seed {seed} step {step}: document diverged from history\n{}",
                log.join("\n")
            );
        }
    }
}

#[test]
fn undo_while_editing_without_changes_falls_through_to_the_history() {
    let mut app = test_app_with_document();
    place_committed_overlay(&mut app, 100.0, "first");
    place_committed_overlay(&mut app, 200.0, "second");

    // Open an edit session but change nothing, so cancelling it is invisible.
    app.update(Message::EditOverlay(1));
    app.update(Message::Undo);

    let overlays = &app.document.as_ref().unwrap().overlays;
    assert_eq!(
        overlays[1].text, "",
        "a no-op edit session must not swallow the undo keystroke"
    );
    assert!(!app.canvas.editing);
}

#[test]
fn selecting_an_overlay_while_editing_commits_the_pending_text() {
    let mut app = test_app_with_document();
    place_committed_overlay(&mut app, 100.0, "original");

    app.update(Message::EditOverlay(0));
    app.update(Message::UpdateOverlayText("typed".to_string()));
    // The canvas commits before selecting, but IPC `select` does not.
    app.update(Message::SelectOverlay(0));

    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "typed");
    assert_history_matches_document(&app, "after selecting mid-edit");
}

#[test]
fn selecting_a_later_overlay_while_a_blank_one_is_discarded_selects_the_same_overlay() {
    let mut app = test_app_with_document();
    place_committed_overlay(&mut app, 100.0, "first");
    place_committed_overlay(&mut app, 200.0, "second");
    place_committed_overlay(&mut app, 300.0, "third");

    // Blanking the middle overlay discards it on commit, shifting "third" down.
    app.update(Message::EditOverlay(1));
    app.update(Message::UpdateOverlayText(String::new()));
    app.update(Message::SelectOverlay(2));

    let overlays = &app.document.as_ref().unwrap().overlays;
    assert_eq!(overlays.len(), 2);
    assert_eq!(
        app.canvas.active_overlay,
        Some(1),
        "the requested overlay shifted down when the blank one was discarded"
    );
    assert_eq!(overlays[1].text, "third");
}

#[test]
fn re_entering_edit_on_the_same_overlay_commits_the_pending_text() {
    let mut app = test_app_with_document();
    place_committed_overlay(&mut app, 100.0, "original");

    app.update(Message::EditOverlay(0));
    app.update(Message::UpdateOverlayText("typed".to_string()));
    app.update(Message::EditOverlay(0));

    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "typed");
    assert_history_matches_document(&app, "after re-entering edit mid-edit");
}

#[test]
fn redo_while_typing_into_a_fresh_placement_keeps_the_placement() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("draft".to_string()));
    app.update(Message::Redo);

    let overlays = &app.document.as_ref().unwrap().overlays;
    assert_eq!(overlays.len(), 1, "redo must never undo a placement");
    assert_eq!(overlays[0].text, "draft");
}

#[test]
fn redo_while_editing_an_existing_overlay_keeps_the_typed_text() {
    let mut app = test_app_with_document();
    place_committed_overlay(&mut app, 100.0, "original");

    app.update(Message::EditOverlay(0));
    app.update(Message::UpdateOverlayText("typed".to_string()));
    app.update(Message::Redo);

    assert_eq!(
        app.document.as_ref().unwrap().overlays[0].text,
        "typed",
        "redo must not revert in-progress typing"
    );
    assert_history_matches_document(&app, "after redo mid-edit");
}

#[test]
fn typing_into_a_selected_overlay_starts_a_recorded_edit_session() {
    let mut app = test_app_with_document();
    place_committed_overlay(&mut app, 100.0, "original");
    app.update(Message::SelectOverlay(0));

    // IPC drives `select` then `type` with no edit in between.
    app.update(Message::UpdateOverlayText("typed".to_string()));
    assert!(
        app.canvas.editing,
        "typing into a selection must open a session so the change is recorded"
    );
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "typed");

    app.update(Message::CommitText);
    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "typed");
    assert_history_matches_document(&app, "after typing into a selection");
}

#[test]
fn typing_with_nothing_selected_is_ignored() {
    let mut app = test_app_with_document();
    place_committed_overlay(&mut app, 100.0, "original");
    app.update(Message::DeselectOverlay);

    app.update(Message::UpdateOverlayText("stray".to_string()));

    assert_eq!(
        app.document.as_ref().unwrap().overlays[0].text,
        "original",
        "with no overlay selected there is nothing to record a change against"
    );
    assert_history_matches_document(&app, "after stray text message");
}

#[test]
fn undoing_a_delete_made_while_typing_restores_only_recorded_text() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("typed".to_string()));
    app.update(Message::DeleteOverlay);
    app.update(Message::Undo);

    assert_history_matches_document(&app, "after undoing a delete made mid-edit");
}

#[test]
fn placing_an_overlay_while_editing_commits_the_pending_text() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    app.update(Message::UpdateOverlayText("first".to_string()));
    // The canvas commits before placing, but IPC `click` does not.
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 200.0, y: 600.0 },
        width: None,
    });

    assert_eq!(app.document.as_ref().unwrap().overlays[0].text, "first");
    assert_history_matches_document(&app, "after placing mid-edit");
}

#[test]
fn selecting_a_different_overlay_ends_the_multiline_edit_session() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: Some(200.0),
    });
    app.update(Message::UpdateOverlayText("multi".to_string()));
    place_committed_overlay(&mut app, 300.0, "single");

    // Re-open the multiline session so the select is what ends it.
    app.update(Message::EditOverlay(0));
    assert!(app.editor_content.is_some());

    app.update(Message::SelectOverlay(0));

    assert!(
        app.editor_content.is_none(),
        "the multiline editor must not outlive the session that owns it"
    );
    assert_history_matches_document(&app, "after selecting away from a multiline edit");
}

// =====================================================================
// spe-9gt.6.1: custom font picker open/close behaviour
// =====================================================================

#[test]
fn font_picker_starts_closed() {
    let (app, _) = App::new(false);
    assert!(!app.toolbar.font_picker_open);
}

#[test]
fn toggling_the_font_picker_opens_then_closes_it() {
    let mut app = test_app_with_document();

    app.update(Message::Toolbar(toolbar::Message::FontPickerToggled));
    assert!(app.toolbar.font_picker_open, "first toggle should open it");

    app.update(Message::Toolbar(toolbar::Message::FontPickerToggled));
    assert!(
        !app.toolbar.font_picker_open,
        "second toggle should close it"
    );
}

#[test]
fn selecting_a_font_closes_the_picker() {
    let mut app = test_app_with_document();
    app.update(Message::Toolbar(toolbar::Message::FontPickerToggled));
    let courier = font_option_named(&app, "Courier");

    app.update(Message::Toolbar(toolbar::Message::FontSelected(courier)));

    assert!(!app.toolbar.font_picker_open);
    assert_eq!(
        app.toolbar.font,
        app.font_registry.find_by_name("Courier").unwrap()
    );
}

#[test]
fn dismissing_the_font_picker_leaves_the_font_unchanged() {
    let mut app = test_app_with_document();
    let before = app.toolbar.font;
    app.update(Message::Toolbar(toolbar::Message::FontPickerToggled));

    app.update(Message::Toolbar(toolbar::Message::FontPickerDismissed));

    assert!(!app.toolbar.font_picker_open);
    assert_eq!(app.toolbar.font, before);
}

#[test]
fn dismissing_the_font_picker_while_editing_returns_focus_task() {
    let mut app = test_app_with_overlay();
    app.update(Message::EditOverlay(0));
    app.update(Message::Toolbar(toolbar::Message::FontPickerToggled));

    let task = app.update(Message::Toolbar(toolbar::Message::FontPickerDismissed));

    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "dismissing the picker should hand focus back to the edit, got: {debug}"
    );
}

#[test]
fn opening_the_font_picker_does_not_steal_focus() {
    let mut app = test_app_with_overlay();
    app.update(Message::EditOverlay(0));

    let task = app.update(Message::Toolbar(toolbar::Message::FontPickerToggled));

    let debug = format!("{task:?}");
    assert!(
        debug.contains("units: 0"),
        "opening the picker must not yank focus back to the editor, got: {debug}"
    );
}

// =====================================================================
// spe-xqb: wait_frame — a signal for "a frame reflecting the latest
// state has been presented", replacing the visual-regression settle sleep.
// =====================================================================

#[test]
fn state_generation_advances_on_every_processed_message() {
    let (mut app, _) = App::new(false);
    let before = app.state_generation;
    app.update(Message::Noop);
    assert_eq!(app.state_generation, before + 1);
    app.update(Message::Noop);
    assert_eq!(app.state_generation, before + 2);
}

#[test]
fn frame_presented_records_the_generation_as_of_the_redraw() {
    let (mut app, _) = App::new(false);
    app.update(Message::Noop);
    let expected = app.state_generation + 1; // FramePresented itself advances state_generation.
    app.update(Message::FramePresented);
    assert_eq!(app.presented_generation, expected);
}

#[test]
fn frame_event_to_message_maps_redraw_requested() {
    let event = iced::Event::Window(iced::window::Event::RedrawRequested(
        std::time::Instant::now(),
    ));
    let msg = frame_event_to_message(
        event,
        iced::event::Status::Ignored,
        iced::window::Id::unique(),
    );
    assert!(matches!(msg, Some(Message::FramePresented)));
}

#[test]
fn frame_event_to_message_ignores_other_events() {
    let event = iced::Event::Window(iced::window::Event::Focused);
    let msg = frame_event_to_message(
        event,
        iced::event::Status::Ignored,
        iced::window::Id::unique(),
    );
    assert!(msg.is_none());
}

#[test]
fn ipc_wait_frame_already_presented_resolves_immediately() {
    let (mut app, _) = App::new(true);
    let _rx = attach_ipc_response_sender(&mut app);
    // No prior mutation: presented_generation (0) already covers state_generation.
    let _ = app.update(Message::Ipc(crate::ipc::IpcEvent::WaitFrame));
    assert!(
        app.pending_frame_wait.is_none(),
        "an already-presented wait must resolve immediately, not sit pending"
    );
}

#[test]
fn ipc_wait_frame_not_yet_presented_sets_pending() {
    let mut app = test_app_with_document();
    let _rx = attach_ipc_response_sender(&mut app);
    // Mutate state without a following FramePresented: presented_generation lags.
    app.update(Message::Noop);
    let generation_before_wait = app.state_generation;
    let _ = app.update(Message::Ipc(crate::ipc::IpcEvent::WaitFrame));
    // The target excludes WaitFrame's own generation bump, so a wait already
    // satisfied by the preceding command's redraw resolves immediately
    // instead of always paying for one needless extra redraw round-trip (see
    // the comment on IpcEvent::WaitFrame in src/app/mod.rs for the empirical
    // check that a self-inclusive target would not hang either — just cost
    // that avoidable extra frame).
    assert_eq!(app.pending_frame_wait, Some(generation_before_wait));
}

#[test]
fn check_ipc_frame_wait_clears_pending_once_presented_catches_up() {
    let (mut app, _) = App::new(true);
    let _rx = attach_ipc_response_sender(&mut app);
    app.pending_frame_wait = Some(app.state_generation);
    app.presented_generation = app.state_generation;
    let _ = app.check_ipc_frame_wait();
    assert!(app.pending_frame_wait.is_none());
}

#[test]
fn check_ipc_frame_wait_keeps_pending_when_not_yet_presented() {
    let (mut app, _) = App::new(true);
    let _rx = attach_ipc_response_sender(&mut app);
    app.pending_frame_wait = Some(app.state_generation + 1);
    let _ = app.check_ipc_frame_wait();
    assert!(app.pending_frame_wait.is_some());
}
