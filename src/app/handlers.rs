// Message handlers, file operations, rendering tasks.

use super::{
    App, CanvasState, DEBOUNCE_MS, DocumentState, FontId, Handle, HashMap,
    MAX_CONCURRENT_THUMBNAIL_TASKS, MAX_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH, Message, PathBuf,
    PdfPosition, SCROLLBAR_MARGIN, SIDEBAR_PAGE_BUFFER, THUMBNAIL_BATCH_SIZE, TextOverlay,
};

use crate::command::Command as UndoCommand;
use crate::pdf::renderer::PdftoppmRenderer;
use crate::ui::canvas;
use crate::ui::toolbar;

impl App {
    // --- Page navigation handlers ---

    pub(super) fn handle_next_page(&mut self) -> iced::Task<Message> {
        if let Some(doc) = &self.document
            && doc.current_page < doc.page_count
        {
            return self.scroll_to_page(doc.current_page + 1);
        }
        iced::Task::none()
    }

    pub(super) fn handle_previous_page(&mut self) -> iced::Task<Message> {
        if let Some(doc) = &self.document
            && doc.current_page > 1
        {
            return self.scroll_to_page(doc.current_page - 1);
        }
        iced::Task::none()
    }

    pub(super) fn handle_go_to_page(&mut self, page: u32) -> iced::Task<Message> {
        if let Some(doc) = &self.document
            && page >= 1
            && page <= doc.page_count
        {
            return self.scroll_to_page(page);
        }
        iced::Task::none()
    }

    pub(super) fn handle_page_batch_rendered(
        &mut self,
        pages: Vec<(u32, Handle)>,
    ) -> iced::Task<Message> {
        if let Some(doc) = &mut self.document {
            for (page, handle) in pages {
                doc.page_images.insert(page, handle);
            }
            let render_task = self.render_visible_pages();
            let wait_task = self.check_ipc_wait();
            return iced::Task::batch([render_task, wait_task]);
        }
        iced::Task::none()
    }

    // --- Overlay data handlers ---

    pub(super) fn handle_place_overlay(
        &mut self,
        page: u32,
        position: PdfPosition,
        width: Option<f32>,
    ) -> iced::Task<Message> {
        let commit_task = if self.canvas.editing {
            self.handle_commit_text()
        } else {
            iced::Task::none()
        };
        if self.document.is_some() {
            let overlay = TextOverlay {
                page,
                position,
                text: String::new(),
                font: self.toolbar.font,
                font_size: self.toolbar.font_size,
                width,
            };
            let fresh_placement_base = self.undo_stack.len();
            let cmd = UndoCommand::PlaceOverlay {
                overlay: overlay.clone(),
            };
            self.execute_command(cmd);
            let doc = self.document.as_ref().unwrap();
            let idx = doc.overlays.len() - 1;
            self.canvas.active_overlay = Some(idx);
            self.canvas.editing = true;
            self.canvas.edit_start_text = Some(String::new());
            self.canvas.fresh_placement = Some(fresh_placement_base);
            if width.is_some() {
                self.editor_content = Some(iced::widget::text_editor::Content::with_text(""));
            }
            return iced::Task::batch([
                commit_task,
                iced::widget::operation::focus(self.text_input_id.clone()),
            ]);
        }
        commit_task
    }

    pub(super) fn handle_update_overlay_text(&mut self, text: String) {
        // Typing into a selected overlay begins editing it. Every text change
        // must be bracketed by an edit session, because the session is what
        // records it in the undo history on commit; an unbracketed change
        // would drift the document away from the history silently.
        if !self.canvas.editing && !self.begin_edit_session_on_selection() {
            return;
        }
        if let Some(doc) = &mut self.document
            && let Some(idx) = self.canvas.active_overlay
            && idx < doc.overlays.len()
        {
            doc.overlays[idx].text = text.clone();
            // Multi-line overlays render from editor_content, not overlay.text
            // directly (see handle_text_editor_action). Keep it in sync so the
            // IPC `type` path converges on the same state real typing would
            // produce (spe-jpw). Gate on the *target* overlay's own
            // multiline-ness (width.is_some()), not on whether editor_content
            // happens to be populated: editor_content can still hold a
            // previously edited multiline overlay's text after selection
            // moves to a different, single-line overlay (handle_select_overlay
            // doesn't touch editor_content), so `.is_some()` would clobber it
            // with unrelated text. `idx` is already the target overlay (it's
            // derived from active_overlay above and bounds-checked), so no
            // separate idx == active_overlay check is needed.
            if doc.overlays[idx].width.is_some() {
                self.editor_content = Some(iced::widget::text_editor::Content::with_text(&text));
            }
        }
    }

    /// Open an edit session on the selected overlay, capturing its current
    /// text as the undo baseline. Returns false when nothing is selected, so
    /// there is no overlay to record changes against.
    fn begin_edit_session_on_selection(&mut self) -> bool {
        let Some(doc) = &self.document else {
            return false;
        };
        let Some(index) = self.canvas.active_overlay else {
            return false;
        };
        let Some(overlay) = doc.overlays.get(index) else {
            return false;
        };
        self.canvas.edit_start_text = Some(overlay.text.clone());
        self.canvas.fresh_placement = None;
        self.canvas.editing = true;
        true
    }

    pub(super) fn handle_text_editor_action(&mut self, action: iced::widget::text_editor::Action) {
        if let Some(content) = &mut self.editor_content {
            content.perform(action);
            let new_text = content.text();
            if let Some(doc) = &mut self.document
                && let Some(idx) = self.canvas.active_overlay
                && idx < doc.overlays.len()
            {
                doc.overlays[idx].text = new_text;
            }
        }
    }

    pub(super) fn handle_move_overlay(&mut self, index: usize, new_position: PdfPosition) {
        if let Some(doc) = &self.document
            && index < doc.overlays.len()
        {
            let cmd = UndoCommand::MoveOverlay {
                index,
                from: doc.overlays[index].position,
                to: new_position,
            };
            self.execute_command(cmd);
        }
    }

    pub(super) fn handle_resize_overlay(&mut self, index: usize, old_width: f32, new_width: f32) {
        if let Some(doc) = &self.document
            && index < doc.overlays.len()
        {
            let cmd = UndoCommand::ResizeOverlay {
                index,
                old_width,
                new_width,
            };
            self.execute_command(cmd);
        }
    }

    pub(super) fn handle_change_font(&mut self, font: FontId) -> iced::Task<Message> {
        if self.document.is_none() {
            return iced::Task::none();
        }
        if let Some(idx) = self.canvas.active_overlay
            && let Some(doc) = &self.document
            && idx < doc.overlays.len()
        {
            let cmd = UndoCommand::ChangeOverlayFont {
                index: idx,
                old_font: doc.overlays[idx].font,
                new_font: font,
            };
            self.execute_command(cmd);
        }
        self.toolbar.font = font;
        self.refocus_editing_widget()
    }

    pub(super) fn handle_change_font_size(&mut self, size: f32) -> iced::Task<Message> {
        if self.document.is_none() {
            return iced::Task::none();
        }
        if let Some(idx) = self.canvas.active_overlay
            && let Some(doc) = &self.document
            && idx < doc.overlays.len()
        {
            let cmd = UndoCommand::ChangeOverlayFontSize {
                index: idx,
                old_size: doc.overlays[idx].font_size,
                new_size: size,
            };
            self.execute_command(cmd);
        }
        self.toolbar.font_size = size;
        self.toolbar.font_size_input = format!("{size}");
        self.refocus_editing_widget()
    }

    /// An ArrowUp/ArrowDown key was pressed. Iced's `text_input` doesn't
    /// expose a key-press callback or a way to read its focus state
    /// synchronously, so this dispatches a widget operation (`is_focused`)
    /// to ask the runtime whether the font-size input currently has focus;
    /// the result comes back as [`Message::FontSizeArrowKeyResult`].
    pub(super) fn handle_font_size_arrow_pressed(
        &mut self,
        increment: bool,
    ) -> iced::Task<Message> {
        iced::widget::operation::is_focused(self.toolbar.font_size_input_id.clone()).map(
            move |focused| {
                if focused {
                    Message::FontSizeArrowKeyResult(increment)
                } else {
                    Message::Noop
                }
            },
        )
    }

    /// The font-size input was confirmed focused when the arrow key was
    /// pressed: step the size through the same clamped path the stepper
    /// buttons use, then flow through `ChangeFontSize` like every other
    /// font-size change.
    pub(super) fn handle_font_size_arrow_key_result(
        &mut self,
        increment: bool,
    ) -> iced::Task<Message> {
        let size = if increment {
            toolbar::increment_font_size(self.toolbar.font_size)
        } else {
            toolbar::decrement_font_size(self.toolbar.font_size)
        };
        self.update(Message::ChangeFontSize(size))
    }

    /// Return keyboard focus to the floating text widget while an overlay is
    /// being edited. Clicking a toolbar control unfocuses the floating widget,
    /// so typing must be handed back once the toolbar interaction completes.
    fn refocus_editing_widget(&self) -> iced::Task<Message> {
        if self.canvas.editing && self.canvas.active_overlay.is_some() {
            return iced::widget::operation::focus(self.text_input_id.clone());
        }
        iced::Task::none()
    }

    /// Delete the selected overlay. Any pending text is committed first, so
    /// undoing the deletion cannot restore text the history never recorded.
    pub(super) fn handle_delete_overlay(&mut self) -> iced::Task<Message> {
        let task = if self.canvas.editing {
            self.handle_commit_text()
        } else {
            iced::Task::none()
        };
        if let Some(doc) = &self.document
            && let Some(idx) = self.canvas.active_overlay
            && idx < doc.overlays.len()
        {
            let cmd = UndoCommand::DeleteOverlay {
                overlay: doc.overlays[idx].clone(),
                index: idx,
            };
            self.execute_command(cmd);
            self.clear_edit_session();
        }
        task
    }

    /// Sync the toolbar's font/size controls to the currently active
    /// overlay's stored values, if any overlay is active. Called whenever
    /// selection changes or an undo/redo may have changed the active
    /// overlay's font — the toolbar must always reflect what's selected.
    fn sync_toolbar_to_active_overlay(&mut self) {
        let Some(doc) = &self.document else { return };
        let Some(idx) = self.canvas.active_overlay else {
            return;
        };
        let Some(overlay) = doc.overlays.get(idx) else {
            return;
        };
        self.toolbar.font = overlay.font;
        self.toolbar.font_size = overlay.font_size;
        self.toolbar.font_size_input = format!("{}", overlay.font_size);
    }

    pub(super) fn handle_select_overlay(&mut self, index: usize) -> iced::Task<Message> {
        let (task, index) = self.commit_before_targeting(index);
        if let Some(doc) = &self.document
            && index < doc.overlays.len()
        {
            self.canvas.active_overlay = Some(index);
            self.canvas.editing = false;
            self.canvas.fresh_placement = None;
            self.canvas.edit_start_text = None;
            self.sync_toolbar_to_active_overlay();
        }
        task
    }

    /// Number of overlays in the open document.
    fn overlay_count(&self) -> usize {
        self.document.as_ref().map_or(0, |doc| doc.overlays.len())
    }

    /// Close any in-progress edit before acting on `index`, which is resolved
    /// against the overlay list as it stood before the commit. Committing can
    /// discard the blank overlay being edited, shifting later entries down.
    fn commit_before_targeting(&mut self, index: usize) -> (iced::Task<Message>, usize) {
        if !self.canvas.editing {
            return (iced::Task::none(), index);
        }
        let edited = self.canvas.active_overlay;
        let count_before = self.overlay_count();
        let task = self.handle_commit_text();
        let shifted =
            self.overlay_count() < count_before && edited.is_some_and(|edited| edited < index);
        (task, if shifted { index - 1 } else { index })
    }

    pub(super) fn handle_edit_overlay(&mut self, index: usize) -> iced::Task<Message> {
        let (commit_task, index) = self.commit_before_targeting(index);
        if let Some(doc) = &self.document
            && index < doc.overlays.len()
        {
            self.canvas.active_overlay = Some(index);
            self.canvas.editing = true;
            self.canvas.fresh_placement = None;
            self.canvas.edit_start_text = Some(doc.overlays[index].text.clone());
            let width_is_some = doc.overlays[index].width.is_some();
            let text = doc.overlays[index].text.clone();
            self.sync_toolbar_to_active_overlay();
            if width_is_some {
                self.editor_content = Some(iced::widget::text_editor::Content::with_text(&text));
            }
            return iced::Task::batch([
                commit_task,
                iced::widget::operation::focus(self.text_input_id.clone()),
            ]);
        }
        commit_task
    }

    pub(super) fn handle_deselect_overlay(&mut self) -> iced::Task<Message> {
        let task = if self.canvas.editing {
            self.handle_commit_text()
        } else {
            iced::Task::none()
        };
        self.canvas.active_overlay = None;
        self.canvas.editing = false;
        task
    }

    pub(super) fn handle_commit_text(&mut self) -> iced::Task<Message> {
        if let Some(doc) = &self.document
            && let Some(idx) = self.canvas.active_overlay
            && idx < doc.overlays.len()
            && let Some(old_text) = self.canvas.edit_start_text.take()
        {
            let new_text = doc.overlays[idx].text.clone();
            if new_text.trim().is_empty() {
                self.discard_empty_overlay(idx, old_text);
            } else {
                self.canvas.fresh_placement = None;
                if old_text != new_text {
                    let cmd = UndoCommand::EditText {
                        index: idx,
                        old_text,
                        new_text,
                    };
                    self.undo_stack.push(cmd);
                    self.redo_stack.clear();
                }
            }
        }
        self.canvas.editing = false;
        self.canvas.edit_start_text = None;
        self.canvas.fresh_placement = None;
        self.editor_content = None;
        iced::Task::none()
    }

    /// Discard every trace of an edit session without touching the overlay
    /// list. Session state addresses overlays by index, so it must never
    /// outlive an operation that reorders or shortens the list.
    fn clear_edit_session(&mut self) {
        self.canvas.editing = false;
        self.canvas.active_overlay = None;
        self.canvas.edit_start_text = None;
        self.canvas.fresh_placement = None;
        self.editor_content = None;
    }

    /// Abandon an in-progress edit, returning the overlay list to the state it
    /// had when the session began: a freshly placed overlay is removed along
    /// with its placement command, and an established overlay's text reverts.
    /// Returns whether the document or the undo history changed.
    fn cancel_edit_session(&mut self) -> bool {
        if !self.canvas.editing {
            return false;
        }
        let index = self.canvas.active_overlay;
        let start_text = self.canvas.edit_start_text.clone();
        let fresh_placement = self.canvas.fresh_placement;
        self.clear_edit_session();

        let Some(doc) = &mut self.document else {
            return false;
        };
        let Some(index) = index.filter(|i| *i < doc.overlays.len()) else {
            return false;
        };
        if let Some(base_len) = fresh_placement {
            doc.overlays.remove(index);
            self.undo_stack.truncate(base_len);
            // Redo entries address overlays by index, so none of them can
            // survive the list shrinking outside the command history.
            self.redo_stack.clear();
            return true;
        }
        match start_text {
            Some(text) if doc.overlays[index].text != text => {
                doc.overlays[index].text = text;
                true
            }
            _ => false,
        }
    }

    /// Remove an overlay whose text is blank, since it would only render as an
    /// empty selection box. Abandoning a freshly placed overlay leaves no undo
    /// history, including any style commands (font, font size) recorded while
    /// it was being edited; erasing the text of an established overlay
    /// records a deletion that restores `text_at_edit_start` when undone.
    fn discard_empty_overlay(&mut self, index: usize, text_at_edit_start: String) {
        let Some(doc) = &mut self.document else {
            return;
        };
        let mut overlay = doc.overlays.remove(index);
        if let Some(base_len) = self.canvas.fresh_placement.take() {
            self.undo_stack.truncate(base_len);
        } else {
            overlay.text = text_at_edit_start;
            self.undo_stack
                .push(UndoCommand::DeleteOverlay { overlay, index });
        }
        // Redo entries address overlays by index, so none of them can survive
        // the list shrinking.
        self.redo_stack.clear();
        self.canvas.active_overlay = None;
    }

    pub(super) fn handle_toolbar_message(&mut self, msg: toolbar::Message) -> iced::Task<Message> {
        match msg {
            toolbar::Message::OpenFile => return self.update(Message::OpenFile),
            toolbar::Message::Save => return self.update(Message::Save),
            toolbar::Message::SaveAs => return self.update(Message::SaveAs),
            toolbar::Message::Undo => return self.update(Message::Undo),
            toolbar::Message::Redo => return self.update(Message::Redo),
            toolbar::Message::FontSelected(option) => {
                return self.update(Message::ChangeFont(option.id));
            }
            toolbar::Message::FontSizeInput(input) => {
                self.toolbar.font_size_input = input;
            }
            toolbar::Message::FontSizeSubmit => {
                if let Ok(size) = self.toolbar.font_size_input.parse::<f32>()
                    && size > 0.0
                {
                    return self.update(Message::ChangeFontSize(size));
                }
            }
            toolbar::Message::FontSizeIncrement => {
                let size = toolbar::increment_font_size(self.toolbar.font_size);
                return self.update(Message::ChangeFontSize(size));
            }
            toolbar::Message::FontSizeDecrement => {
                let size = toolbar::decrement_font_size(self.toolbar.font_size);
                return self.update(Message::ChangeFontSize(size));
            }
            toolbar::Message::ZoomIn => return self.update(Message::ZoomIn),
            toolbar::Message::ZoomOut => return self.update(Message::ZoomOut),
            toolbar::Message::ZoomReset => return self.update(Message::ZoomReset),
            toolbar::Message::ZoomFitWidth => return self.update(Message::ZoomFitWidth),
            toolbar::Message::PreviousPage => return self.update(Message::PreviousPage),
            toolbar::Message::NextPage => return self.update(Message::NextPage),
            toolbar::Message::PageInput(input) => {
                self.toolbar.page_input = input;
            }
            toolbar::Message::PageInputSubmit => {
                if let Ok(page) = self.toolbar.page_input.parse::<u32>() {
                    return self.update(Message::GoToPage(page));
                }
            }
            toolbar::Message::ToggleSidebar => return self.update(Message::ToggleSidebar),
            toolbar::Message::DeleteOverlay => return self.update(Message::DeleteOverlay),
        }
        iced::Task::none()
    }

    pub(super) fn handle_open_file(&mut self) -> iced::Task<Message> {
        iced::Task::perform(
            async {
                let handle = rfd::AsyncFileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .pick_file()
                    .await;
                handle.map(|h| h.path().to_path_buf())
            },
            |path| match path {
                Some(p) => Message::FileOpened(p),
                None => Message::Noop,
            },
        )
    }

    pub(super) fn handle_file_opened(&mut self, path: PathBuf) -> iced::Task<Message> {
        match lopdf::Document::load(&path) {
            Ok(doc) => {
                self.last_open_error = None;
                let page_dims = crate::pdf::page_dimensions(&doc);
                let page_count = doc.get_pages().len() as u32;
                self.document = Some(DocumentState {
                    source_path: path.clone(),
                    save_path: None,
                    page_count,
                    current_page: 1,
                    page_images: HashMap::new(),
                    page_dimensions: page_dims,
                    overlays: Vec::new(),
                });
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.canvas = CanvasState::default();
                self.editor_content = None;
                self.sidebar.thumbnails.clear();
                self.sidebar.active_batch_tasks = 0;
                self.toolbar.page_input = "1".to_string();
                let max_page_w = self
                    .document
                    .as_ref()
                    .map(|d| d.max_page_width())
                    .unwrap_or(612.0);

                // Set initial zoom to fit widest page in viewport
                if let Some(win) = self.window_size
                    && max_page_w > 0.0
                {
                    let available_w =
                        (win.width - self.effective_sidebar_width() - SCROLLBAR_MARGIN).max(1.0);
                    self.canvas.zoom = canvas::fit_to_width_zoom(max_page_w, available_w);
                }

                // Compute thumbnail DPI for sidebar rendering
                self.sidebar.thumbnail_dpi = crate::ui::sidebar::compute_thumbnail_dpi(
                    self.sidebar.width,
                    self.scale_factor,
                    max_page_w,
                );
                self.sidebar.backfill_generation += 1;

                let scroll_reset = iced::widget::operation::scroll_to(
                    self.scrollable_id.clone(),
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
                );
                let page_task = self.render_visible_pages();
                let thumb_task = self.render_visible_thumbnails();
                iced::Task::batch([scroll_reset, page_task, thumb_task])
            }
            Err(e) => {
                let message = format!("failed to open {}: {e}", path.display());
                eprintln!("Failed to open PDF: {e}");
                self.last_open_error = Some(message);
                iced::Task::none()
            }
        }
    }

    pub(super) fn handle_save(&mut self) -> iced::Task<Message> {
        if let Some(doc) = &self.document
            && let Some(save_path) = &doc.save_path
        {
            let source = doc.source_path.clone();
            let dest = save_path.clone();
            let overlays = doc.overlays.clone();
            let result =
                crate::pdf::writer::write_overlays(&source, &dest, &overlays, &self.font_registry);
            self.set_save_result(result, &dest);
            return iced::Task::none();
        }
        self.handle_save_as()
    }

    fn set_save_result(
        &mut self,
        result: Result<(), impl std::fmt::Display>,
        dest: &std::path::Path,
    ) {
        match result {
            Ok(()) => {
                let filename = dest.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                self.status_message =
                    Some((format!("Saved to {filename}"), std::time::Instant::now()));
            }
            Err(e) => {
                self.status_message =
                    Some((format!("Save failed: {e}"), std::time::Instant::now()));
            }
        }
    }

    pub(super) fn handle_save_as(&mut self) -> iced::Task<Message> {
        iced::Task::perform(
            async {
                let handle = rfd::AsyncFileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .save_file()
                    .await;
                handle.map(|h| h.path().to_path_buf())
            },
            |path| match path {
                Some(p) => Message::SaveDestinationChosen(p),
                None => Message::Noop,
            },
        )
    }

    pub(super) fn handle_save_destination(&mut self, path: PathBuf) {
        if let Some(doc) = &mut self.document {
            // Prevent saving over the source file to avoid data loss on
            // write failure (the source would already be truncated).
            if path == doc.source_path {
                self.status_message = Some((
                    "Save failed: cannot overwrite the source file".to_string(),
                    std::time::Instant::now(),
                ));
                return;
            }
            let source = doc.source_path.clone();
            let overlays = doc.overlays.clone();
            let result =
                crate::pdf::writer::write_overlays(&source, &path, &overlays, &self.font_registry);
            let succeeded = result.is_ok();
            self.set_save_result(result, &path);
            if succeeded {
                self.document.as_mut().unwrap().save_path = Some(path);
            }
        }
    }

    /// Render all pages in the visible range (plus 1-page buffer) that are not cached.
    pub(super) fn render_visible_pages(&self) -> iced::Task<Message> {
        let Some(doc) = &self.document else {
            return iced::Task::none();
        };
        let dpi = canvas::effective_dpi(self.canvas.zoom);
        let layout =
            canvas::page_layout(&doc.page_dimensions, doc.page_count, self.canvas.zoom, dpi);
        let (first, last) =
            canvas::visible_pages(&layout, self.canvas.scroll_y, self.canvas.viewport_height);
        // Expand by 1-page buffer on each side
        let buffer_first = first.saturating_sub(1).max(1);
        let buffer_last = (last + 1).min(doc.page_count);

        let uncached: Vec<u32> = (buffer_first..=buffer_last)
            .filter(|p| !doc.page_images.contains_key(p))
            .collect();
        if uncached.is_empty() {
            return iced::Task::none();
        }
        // Render the full contiguous range in one pdftoppm call.
        let range_first = *uncached.first().unwrap();
        let range_last = *uncached.last().unwrap();
        render_page_batch_task(doc.source_path.clone(), range_first, range_last, dpi as u32)
    }

    /// Backfill thumbnails for pages not yet rendered, working outward from
    /// the current page in batches of 20. Chains itself via `ThumbnailBatchRendered`
    /// until all pages are covered. Discards results from stale generations.
    pub(super) fn schedule_thumbnail_backfill(&mut self) -> iced::Task<Message> {
        if self.sidebar.active_batch_tasks >= MAX_CONCURRENT_THUMBNAIL_TASKS {
            return iced::Task::none();
        }
        let Some(doc) = &self.document else {
            return iced::Task::none();
        };
        if !self.sidebar.visible || doc.page_count == 0 {
            return iced::Task::none();
        }
        let dpi = self.sidebar.thumbnail_dpi as u32;
        if dpi == 0 {
            return iced::Task::none();
        }
        let center_page = doc.current_page;
        let mut unrendered: Vec<u32> = (1..=doc.page_count)
            .filter(|p| !self.sidebar.thumbnails.contains_key(p))
            .collect();
        if unrendered.is_empty() {
            return iced::Task::none();
        }
        // Sort nearest-first so the most relevant pages render sooner.
        unrendered.sort_by_key(|p| (*p as i64 - center_page as i64).unsigned_abs());
        let batch: Vec<u32> = unrendered.into_iter().take(THUMBNAIL_BATCH_SIZE).collect();
        // pdftoppm requires a contiguous page range (-f/-l), so we use
        // min/max of the nearest-first batch. This may re-render some
        // already-cached pages in the middle — harmless at thumbnail DPI.
        let range_first = batch.iter().copied().min().unwrap();
        let range_last = batch.iter().copied().max().unwrap();
        self.sidebar.active_batch_tasks += 1;
        render_thumbnail_batch_task(
            doc.source_path.clone(),
            range_first,
            range_last,
            dpi,
            self.sidebar.backfill_generation,
        )
    }

    /// Render thumbnails for pages visible in the sidebar (plus a buffer),
    /// skipping any that are already cached.
    pub(super) fn render_visible_thumbnails(&mut self) -> iced::Task<Message> {
        if self.sidebar.active_batch_tasks >= MAX_CONCURRENT_THUMBNAIL_TASKS {
            return iced::Task::none();
        }
        let Some(doc) = &self.document else {
            return iced::Task::none();
        };
        if !self.sidebar.visible || doc.page_count == 0 {
            return iced::Task::none();
        }
        let dpi = self.sidebar.thumbnail_dpi as u32;
        if dpi == 0 {
            return iced::Task::none();
        }
        let avg_thumb_h =
            crate::ui::sidebar::thumbnail_height(612.0, 792.0, self.sidebar.width - 16.0);
        let visible = crate::ui::sidebar::visible_pages(
            self.sidebar.scroll_y,
            self.sidebar.viewport_height.max(600.0),
            doc.page_count,
            avg_thumb_h + 8.0,
            SIDEBAR_PAGE_BUFFER,
        );
        let pages_to_render: Vec<u32> = visible
            .filter(|p| !self.sidebar.thumbnails.contains_key(p))
            .collect();
        if pages_to_render.is_empty() {
            return iced::Task::none();
        }
        let pdf_path = doc.source_path.clone();
        let generation = self.sidebar.backfill_generation;
        let mut tasks = Vec::new();
        for chunk in pages_to_render.chunks(THUMBNAIL_BATCH_SIZE) {
            let first = *chunk.first().unwrap();
            let last = *chunk.last().unwrap();
            if self.sidebar.active_batch_tasks >= MAX_CONCURRENT_THUMBNAIL_TASKS {
                break;
            }
            self.sidebar.active_batch_tasks += 1;
            tasks.push(render_thumbnail_batch_task(
                pdf_path.clone(),
                first,
                last,
                dpi,
                generation,
            ));
        }
        iced::Task::batch(tasks)
    }

    /// Scroll to a specific page by computing its y-offset and using scroll_to.
    pub(super) fn scroll_to_page(&self, page: u32) -> iced::Task<Message> {
        let Some(doc) = &self.document else {
            return iced::Task::none();
        };
        let dpi = canvas::effective_dpi(self.canvas.zoom);
        let layout =
            canvas::page_layout(&doc.page_dimensions, doc.page_count, self.canvas.zoom, dpi);
        let target_y = if (page as usize) <= layout.page_tops.len() {
            layout.page_tops[(page - 1) as usize]
        } else {
            0.0
        };
        iced::widget::operation::scroll_to(
            self.scrollable_id.clone(),
            iced::widget::scrollable::AbsoluteOffset {
                x: 0.0,
                y: target_y,
            },
        )
    }

    // --- Toast handler ---

    pub(super) fn handle_dismiss_toast(&mut self) {
        if let Some((_, time)) = &self.status_message
            && time.elapsed() >= std::time::Duration::from_secs(5)
        {
            self.status_message = None;
        }
    }

    // --- Canvas (zoom) handlers ---

    pub(super) fn handle_zoom_in(&mut self) -> iced::Task<Message> {
        self.canvas.zoom = canvas::zoom_in(self.canvas.zoom);
        self.apply_zoom_change()
    }

    pub(super) fn handle_zoom_out(&mut self) -> iced::Task<Message> {
        self.canvas.zoom = canvas::zoom_out(self.canvas.zoom);
        self.apply_zoom_change()
    }

    pub(super) fn handle_zoom_reset(&mut self) -> iced::Task<Message> {
        self.canvas.zoom = 1.0;
        self.apply_zoom_change()
    }

    pub(super) fn handle_zoom_fit_width(&mut self) -> iced::Task<Message> {
        if let (Some(doc), Some(win)) = (&self.document, self.window_size) {
            let max_page_w = doc.max_page_width();
            if max_page_w > 0.0 {
                let available_w =
                    (win.width - self.effective_sidebar_width() - SCROLLBAR_MARGIN).max(1.0);
                self.canvas.zoom = canvas::fit_to_width_zoom(max_page_w, available_w);
                return self.apply_zoom_change();
            }
        }
        iced::Task::none()
    }

    pub(super) fn handle_zoom_debounce_expired(&mut self, generation: u64) -> iced::Task<Message> {
        if generation == self.canvas.zoom_generation {
            // Clear all cached images so pages get fresh renders at
            // the new DPI (including neighbors on navigation).
            if let Some(doc) = &mut self.document {
                doc.page_images.clear();
            }
            return self.render_visible_pages();
        }
        iced::Task::none()
    }

    pub(super) fn handle_canvas_scrolled(
        &mut self,
        scroll_y: f32,
        viewport_height: f32,
    ) -> iced::Task<Message> {
        self.canvas.scroll_y = scroll_y;
        self.canvas.viewport_height = viewport_height;
        if let Some(doc) = &mut self.document {
            let dpi = canvas::effective_dpi(self.canvas.zoom);
            let layout =
                canvas::page_layout(&doc.page_dimensions, doc.page_count, self.canvas.zoom, dpi);
            let page = canvas::dominant_page(&layout, scroll_y, viewport_height);
            if doc.current_page != page {
                doc.current_page = page;
                self.toolbar.page_input = page.to_string();
            }
        }
        self.render_visible_pages()
    }

    // --- Sidebar handlers ---

    pub(super) fn handle_sidebar_drag_start(&mut self) {
        self.sidebar.dragging = true;
        self.sidebar.drag_start_x = 0.0;
        self.sidebar.drag_start_width = self.sidebar.width;
    }

    pub(super) fn handle_sidebar_resized(&mut self, cursor_x: f32) {
        if !self.sidebar.dragging {
            return;
        }
        if self.sidebar.drag_start_x == 0.0 {
            // First move: capture start X position
            self.sidebar.drag_start_x = cursor_x;
            return;
        }
        let new_width = self.sidebar.drag_start_width + (cursor_x - self.sidebar.drag_start_x);
        self.sidebar.width = new_width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
    }

    pub(super) fn handle_sidebar_resize_end(&mut self) -> iced::Task<Message> {
        if !self.sidebar.dragging {
            return iced::Task::none();
        }
        self.sidebar.dragging = false;
        self.sidebar.backfill_generation += 1;
        let generation = self.sidebar.backfill_generation;
        iced::Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(DEBOUNCE_MS)).await;
                generation
            },
            Message::SidebarResizeDebounceExpired,
        )
    }

    pub(super) fn handle_sidebar_resize_debounce_expired(
        &mut self,
        generation: u64,
    ) -> iced::Task<Message> {
        if generation == self.sidebar.backfill_generation {
            let max_page_w = self
                .document
                .as_ref()
                .map(|d| d.max_page_width())
                .unwrap_or(612.0);
            self.sidebar.thumbnail_dpi = crate::ui::sidebar::compute_thumbnail_dpi(
                self.sidebar.width,
                self.scale_factor,
                max_page_w,
            );
            self.sidebar.thumbnails.clear();
            self.sidebar.active_batch_tasks = 0;
            return self.render_visible_thumbnails();
        }
        iced::Task::none()
    }

    pub(super) fn handle_sidebar_scrolled(
        &mut self,
        scroll_y: f32,
        viewport_height: f32,
    ) -> iced::Task<Message> {
        self.sidebar.scroll_y = scroll_y;
        self.sidebar.viewport_height = viewport_height;
        self.render_visible_thumbnails()
    }

    pub(super) fn handle_thumbnail_batch_rendered(
        &mut self,
        batch: Vec<(u32, Handle)>,
        generation: u64,
    ) -> iced::Task<Message> {
        self.sidebar.active_batch_tasks = self.sidebar.active_batch_tasks.saturating_sub(1);
        if generation == self.sidebar.backfill_generation {
            for (page, handle) in batch {
                self.sidebar.thumbnails.insert(page, handle);
            }
        }
        let backfill_task = self.schedule_thumbnail_backfill();
        let wait_task = self.check_ipc_wait();
        iced::Task::batch([backfill_task, wait_task])
    }

    // --- Undo/Redo handlers ---

    /// Undo the in-progress edit if there is one, otherwise reverse the most
    /// recent command. Cancelling the edit first keeps the session from
    /// outliving the overlay it addresses, and gives one visible change per
    /// keystroke: an edit that changed nothing falls through to the history.
    pub(super) fn handle_undo(&mut self) {
        if self.cancel_edit_session() {
            return;
        }
        if let Some(cmd) = self.undo_stack.pop()
            && let Some(doc) = &mut self.document
        {
            cmd.reverse(&mut doc.overlays);
            // A selection is an index, so it only survives commands that leave
            // the list's length intact. Placing or deleting can strand it on a
            // removed or shifted overlay, so the selection goes; an in-place
            // change (text, font, size, position, width) leaves it addressing
            // the same overlay, and the toolbar resyncs to the restored values.
            let changes_count = cmd.changes_overlay_count();
            self.redo_stack.push(cmd);
            if changes_count {
                self.clear_edit_session();
            }
            self.sync_toolbar_to_active_overlay();
        }
    }

    /// Reapply the most recently undone command. An in-progress edit is
    /// committed rather than cancelled, because redo must never move the
    /// document backwards; committing also invalidates the redo stack whenever
    /// it records a command, which is what a new action should do.
    pub(super) fn handle_redo(&mut self) -> iced::Task<Message> {
        let task = if self.canvas.editing {
            self.handle_commit_text()
        } else {
            iced::Task::none()
        };
        if let Some(cmd) = self.redo_stack.pop()
            && let Some(doc) = &mut self.document
        {
            cmd.apply(&mut doc.overlays);
            let changes_count = cmd.changes_overlay_count();
            self.undo_stack.push(cmd);
            if changes_count {
                self.clear_edit_session();
            }
            self.sync_toolbar_to_active_overlay();
        }
        task
    }

    /// Common post-zoom logic: increment generation and schedule a debounced
    /// re-render. The stale cached image stays visible for instant visual
    /// feedback (scaled by draw_image) until the debounce fires.
    pub(super) fn apply_zoom_change(&mut self) -> iced::Task<Message> {
        self.canvas.zoom_generation += 1;
        self.schedule_zoom_render()
    }

    /// Schedule a debounced re-render after zoom changes.
    /// Waits 300ms, then fires `ZoomDebounceExpired` with the current generation.
    /// If the generation has changed by then, the handler ignores the stale event.
    fn schedule_zoom_render(&self) -> iced::Task<Message> {
        let generation = self.canvas.zoom_generation;
        iced::Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(DEBOUNCE_MS)).await;
                generation
            },
            Message::ZoomDebounceExpired,
        )
    }
}

/// Launch an async task to render a batch of PDF pages via pdftoppm.
fn render_batch(
    pdf_path: PathBuf,
    first_page: u32,
    last_page: u32,
    dpi: u32,
) -> Option<Vec<(u32, Handle)>> {
    let renderer = PdftoppmRenderer;
    match renderer.render_page_batch(&pdf_path, first_page, last_page, dpi) {
        Ok(images) => Some(
            images
                .into_iter()
                .map(|(page, img)| (page, canvas::image_to_handle(img)))
                .collect(),
        ),
        Err(e) => {
            eprintln!("Failed to render batch {first_page}-{last_page}: {e}");
            None
        }
    }
}

fn render_page_batch_task(
    pdf_path: PathBuf,
    first_page: u32,
    last_page: u32,
    dpi: u32,
) -> iced::Task<Message> {
    iced::Task::perform(
        async move { render_batch(pdf_path, first_page, last_page, dpi) },
        |result| match result {
            Some(handles) => Message::PageBatchRendered(handles),
            None => Message::Noop,
        },
    )
}

fn render_thumbnail_batch_task(
    pdf_path: PathBuf,
    first_page: u32,
    last_page: u32,
    dpi: u32,
    generation: u64,
) -> iced::Task<Message> {
    iced::Task::perform(
        async move { render_batch(pdf_path, first_page, last_page, dpi).map(|h| (h, generation)) },
        |result| match result {
            Some((handles, batch_gen)) => Message::ThumbnailBatchRendered(handles, batch_gen),
            None => Message::Noop,
        },
    )
}
