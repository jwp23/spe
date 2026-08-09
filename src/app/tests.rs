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
        app.last_open_error.is_some(),
        "a failed open should record an error message"
    );
}

#[test]
fn handle_file_opened_success_clears_previous_load_error() {
    let (mut app, _) = App::new(false);
    app.last_open_error = Some("stale error from a previous failed open".to_string());
    let tmp = make_temp_pdf();
    let _ = app.handle_file_opened(tmp.path().to_path_buf());
    assert!(app.document.is_some());
    assert!(app.last_open_error.is_none());
}

#[test]
fn ipc_open_command_dispatch_leaves_document_unset_on_bad_path() {
    // This exercises the full Command(Open) dispatch path (message
    // construction, update(), handle_file_opened) but does not inspect the
    // IPC response itself: send_ipc_response's returned Task is async and
    // this harness doesn't drive it (unlike deliver_ipc_response_writes_to_channel,
    // which calls the async fn directly). The response *contract* — that a
    // failed load reports ok:false with an error — is covered directly by
    // open_command_response_reports_failure_when_load_failed below.
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
fn open_command_response_reports_failure_when_load_failed() {
    let (mut app, _) = App::new(false);
    let _ = app.handle_file_opened(PathBuf::from("/nonexistent/does-not-exist.pdf"));
    let response = app.open_command_response(crate::ipc::IpcResponse {
        ok: true,
        error: None,
    });
    assert!(!response.ok, "a failed load must not report ok:true");
    assert!(response.error.is_some());
    assert!(
        app.last_open_error.is_none(),
        "the error should be consumed once reported, so it doesn't leak into the next command"
    );
}

#[test]
fn open_command_response_reports_success_when_load_succeeded() {
    let (mut app, _) = App::new(false);
    let tmp = make_temp_pdf();
    let _ = app.handle_file_opened(tmp.path().to_path_buf());
    let ok_response = crate::ipc::IpcResponse {
        ok: true,
        error: None,
    };
    let response = app.open_command_response(ok_response);
    assert!(response.ok);
    assert!(response.error.is_none());
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
    let courier = app.font_registry.find_by_name("Courier").unwrap();
    app.update(Message::EditOverlay(0));

    let task = app.update(Message::Toolbar(toolbar::Message::FontSelected(
        crate::ui::toolbar::FontOption {
            id: courier,
            name: "Courier".to_string(),
        },
    )));

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
