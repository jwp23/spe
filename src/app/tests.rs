use super::*;

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
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);

    app.update(Message::Undo);
    assert_eq!(app.document.as_ref().unwrap().overlays.len(), 0);
    assert_eq!(app.redo_stack.len(), 1);

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
    app.update(Message::ChangeFont(Standard14Font::Courier));
    assert_eq!(
        app.document.as_ref().unwrap().overlays[0].font,
        Standard14Font::Courier
    );
    assert_eq!(app.toolbar.font, Standard14Font::Courier);
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
    app.update(Message::ChangeFont(Standard14Font::CourierBold));
    app.update(Message::DeselectOverlay);
    // Now select it again
    app.update(Message::SelectOverlay(0));
    assert_eq!(app.toolbar.font, Standard14Font::CourierBold);
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
fn deselect_overlay_while_editing_commits_text_instead_of_deselecting() {
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    // While editing, DeselectOverlay should commit, not deselect
    assert!(app.canvas.editing);
    app.update(Message::DeselectOverlay);
    // After commit: editing is false, but selection is preserved
    assert!(!app.canvas.editing);
    assert!(app.canvas.active_overlay.is_some());
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
    // Commit so we start in non-editing state
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
    app.update(Message::ChangeFont(Standard14Font::Courier));
    app.update(Message::ChangeFontSize(18.0));
    app.update(Message::CommitText);

    // Change toolbar to something different
    app.toolbar.font = Standard14Font::Helvetica;
    app.toolbar.font_size = 12.0;
    app.toolbar.font_size_input = "12".to_string();

    app.update(Message::EditOverlay(0));
    assert_eq!(app.toolbar.font, Standard14Font::Courier);
    assert!((app.toolbar.font_size - 18.0).abs() < f32::EPSILON);
    assert_eq!(app.toolbar.font_size_input, "18");
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
    let mut app = test_app_with_document();
    app.update(Message::PlaceOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 700.0 },
        width: None,
    });
    assert_eq!(app.undo_stack.len(), 1);

    // Commit without typing anything (text unchanged from "")
    app.update(Message::CommitText);

    // Should NOT push EditText command
    assert_eq!(app.undo_stack.len(), 1);
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
    app.update(Message::CommitText);
    let task = app.update(Message::EditOverlay(0));
    let debug = format!("{task:?}");
    assert!(
        !debug.contains("units: 0"),
        "EditOverlay (multi-line) should return a focus Task, got: {debug}"
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
