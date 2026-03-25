// Message handlers, file operations, rendering tasks.

use super::*;

use crate::command::Command as UndoCommand;
use crate::pdf::renderer::PdftoppmRenderer;
use crate::ui::canvas;
use crate::ui::toolbar;

impl App {
    pub(super) fn handle_commit_text(&mut self) -> iced::Task<Message> {
        if let Some(doc) = &self.document
            && let Some(idx) = self.canvas.active_overlay
            && idx < doc.overlays.len()
            && let Some(old_text) = self.canvas.edit_start_text.take()
        {
            let new_text = doc.overlays[idx].text.clone();
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
        self.canvas.editing = false;
        self.canvas.edit_start_text = None;
        self.editor_content = None;
        iced::Task::none()
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
                eprintln!("Failed to open PDF: {e}");
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
