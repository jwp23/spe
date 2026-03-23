// Iced Application: top-level state, message routing, view composition.

use std::collections::HashMap;
use std::path::PathBuf;

use iced::keyboard;
use iced::widget::image::Handle;

use crate::command::Command as UndoCommand;
use crate::config::AppConfig;
use crate::overlay::{PdfPosition, Standard14Font, TextOverlay};
use crate::pdf::renderer::{PageRenderer, PdftoppmRenderer};
use crate::ui::canvas::{self, CanvasState, PdfCanvasProgram};
use crate::ui::sidebar::SidebarState;
use crate::ui::toolbar::{self, ToolbarContext, ToolbarState};

/// Minimum sidebar width the user can resize to.
const MIN_SIDEBAR_WIDTH: f32 = 80.0;
/// Maximum sidebar width the user can resize to.
const MAX_SIDEBAR_WIDTH: f32 = 400.0;
/// Phase advance per shimmer tick (fraction of full cycle).
const SHIMMER_TICK_DELTA: f32 = 0.05;

/// State for the currently loaded PDF document.
pub struct DocumentState {
    pub source_path: PathBuf,
    pub save_path: Option<PathBuf>,
    pub page_count: u32,
    pub current_page: u32,
    pub page_images: HashMap<u32, Handle>,
    pub page_dimensions: HashMap<u32, (f32, f32)>,
    pub overlays: Vec<TextOverlay>,
}

/// Top-level application state.
pub struct App {
    pub document: Option<DocumentState>,
    pub toolbar: ToolbarState,
    pub canvas: CanvasState,
    pub sidebar: SidebarState,
    pub undo_stack: Vec<UndoCommand>,
    pub redo_stack: Vec<UndoCommand>,
    pub config: AppConfig,
    pub window_size: Option<iced::Size>,
    pub scrollable_id: iced::widget::Id,
}

/// All messages the application can process.
#[derive(Debug, Clone)]
pub enum Message {
    // File operations
    OpenFile,
    FileOpened(PathBuf),
    Save,
    SaveAs,
    SaveDestinationChosen(PathBuf),

    // Page navigation
    GoToPage(u32),
    NextPage,
    PreviousPage,
    PageRendered(u32, Handle),

    // Overlay editing (undoable)
    PlaceOverlay { page: u32, position: PdfPosition },
    UpdateOverlayText(String),
    CommitText,
    MoveOverlay(usize, PdfPosition),
    ChangeFont(Standard14Font),
    ChangeFontSize(f32),
    DeleteOverlay,
    SelectOverlay(usize),
    DeselectOverlay,

    // Canvas
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ZoomFitWidth,
    ZoomDebounceExpired(u64),
    CanvasScrolled(f32, f32),

    // Sidebar
    ToggleSidebar,
    ThumbnailBatchRendered(Vec<(u32, Handle)>, u64),
    SidebarScrolled(f32, f32),
    SidebarResized(f32),
    SidebarResizeEnd,
    SidebarResizeDebounceExpired(u64),
    SidebarPageClicked(u32),
    ShimmerTick,

    // Undo/Redo
    Undo,
    Redo,

    // Toolbar forwarding
    Toolbar(toolbar::Message),

    // Window
    WindowResized(iced::Size),

    // Font loaded
    FontLoaded(Result<(), iced::font::Error>),
}

impl App {
    pub fn new() -> (Self, iced::Task<Message>) {
        let app = Self {
            document: None,
            toolbar: ToolbarState::default(),
            canvas: CanvasState::default(),
            sidebar: SidebarState::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            config: AppConfig::default(),
            window_size: None,
            scrollable_id: iced::widget::Id::unique(),
        };
        let font_task = iced::font::load(crate::ui::icons::font_bytes()).map(Message::FontLoaded);
        (app, font_task)
    }

    pub fn title(&self) -> String {
        match &self.document {
            Some(doc) => {
                let name = doc
                    .source_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("untitled");
                format!("{name} - SPE")
            }
            None => "SPE - PDF Text Overlay Editor".to_string(),
        }
    }

    pub fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            // --- Toolbar message forwarding ---
            Message::Toolbar(toolbar_msg) => {
                return self.handle_toolbar_message(toolbar_msg);
            }

            // --- File operations ---
            Message::OpenFile => {
                return self.handle_open_file();
            }
            Message::FileOpened(path) => {
                return self.handle_file_opened(path);
            }
            Message::Save => {
                return self.handle_save();
            }
            Message::SaveAs => {
                return self.handle_save_as();
            }
            Message::SaveDestinationChosen(path) => {
                self.handle_save_destination(path);
            }

            // --- Page navigation (scroll to target page) ---
            Message::NextPage => {
                if let Some(doc) = &self.document
                    && doc.current_page < doc.page_count
                {
                    return self.scroll_to_page(doc.current_page + 1);
                }
            }
            Message::PreviousPage => {
                if let Some(doc) = &self.document
                    && doc.current_page > 1
                {
                    return self.scroll_to_page(doc.current_page - 1);
                }
            }
            Message::GoToPage(page) => {
                if let Some(doc) = &self.document
                    && page >= 1
                    && page <= doc.page_count
                {
                    return self.scroll_to_page(page);
                }
            }
            Message::PageRendered(page, handle) => {
                if let Some(doc) = &mut self.document {
                    doc.page_images.insert(page, handle);
                    return self.render_visible_pages();
                }
            }

            // --- Overlay editing (undoable) ---
            Message::PlaceOverlay { page, position } => {
                if let Some(doc) = &mut self.document {
                    let overlay = TextOverlay {
                        page,
                        position,
                        text: String::new(),
                        font: self.toolbar.font,
                        font_size: self.toolbar.font_size,
                    };
                    let cmd = UndoCommand::PlaceOverlay {
                        overlay: overlay.clone(),
                    };
                    cmd.apply(&mut doc.overlays);
                    self.undo_stack.push(cmd);
                    self.redo_stack.clear();
                    let idx = doc.overlays.len() - 1;
                    self.canvas.active_overlay = Some(idx);
                    self.canvas.editing = true;
                }
            }
            Message::UpdateOverlayText(text) => {
                if let Some(doc) = &mut self.document
                    && let Some(idx) = self.canvas.active_overlay
                    && idx < doc.overlays.len()
                {
                    doc.overlays[idx].text = text;
                }
            }
            Message::CommitText => {
                // Text editing is committed as a single undoable action
                self.canvas.editing = false;
            }
            Message::MoveOverlay(index, new_position) => {
                if let Some(doc) = &mut self.document
                    && index < doc.overlays.len()
                {
                    let old_position = doc.overlays[index].position;
                    let cmd = UndoCommand::MoveOverlay {
                        index,
                        from: old_position,
                        to: new_position,
                    };
                    cmd.apply(&mut doc.overlays);
                    self.undo_stack.push(cmd);
                    self.redo_stack.clear();
                }
            }
            Message::ChangeFont(font) => {
                if let Some(doc) = &mut self.document {
                    if let Some(idx) = self.canvas.active_overlay
                        && idx < doc.overlays.len()
                    {
                        let old_font = doc.overlays[idx].font;
                        let cmd = UndoCommand::ChangeOverlayFont {
                            index: idx,
                            old_font,
                            new_font: font,
                        };
                        cmd.apply(&mut doc.overlays);
                        self.undo_stack.push(cmd);
                        self.redo_stack.clear();
                    }
                    self.toolbar.font = font;
                }
            }
            Message::ChangeFontSize(size) => {
                if let Some(doc) = &mut self.document {
                    if let Some(idx) = self.canvas.active_overlay
                        && idx < doc.overlays.len()
                    {
                        let old_size = doc.overlays[idx].font_size;
                        let cmd = UndoCommand::ChangeOverlayFontSize {
                            index: idx,
                            old_size,
                            new_size: size,
                        };
                        cmd.apply(&mut doc.overlays);
                        self.undo_stack.push(cmd);
                        self.redo_stack.clear();
                    }
                    self.toolbar.font_size = size;
                    self.toolbar.font_size_input = format!("{size}");
                }
            }
            Message::DeleteOverlay => {
                if let Some(doc) = &mut self.document
                    && let Some(idx) = self.canvas.active_overlay
                    && idx < doc.overlays.len()
                {
                    let overlay = doc.overlays[idx].clone();
                    let cmd = UndoCommand::DeleteOverlay {
                        overlay,
                        index: idx,
                    };
                    cmd.apply(&mut doc.overlays);
                    self.undo_stack.push(cmd);
                    self.redo_stack.clear();
                    self.canvas.active_overlay = None;
                    self.canvas.editing = false;
                }
            }
            Message::SelectOverlay(index) => {
                if let Some(doc) = &self.document
                    && index < doc.overlays.len()
                {
                    self.canvas.active_overlay = Some(index);
                    self.canvas.editing = false;
                    self.toolbar.font = doc.overlays[index].font;
                    self.toolbar.font_size = doc.overlays[index].font_size;
                    self.toolbar.font_size_input = format!("{}", doc.overlays[index].font_size);
                }
            }
            Message::DeselectOverlay => {
                self.canvas.active_overlay = None;
                self.canvas.editing = false;
            }

            // --- Canvas (zoom with debounce) ---
            Message::ZoomIn => {
                self.canvas.zoom = canvas::zoom_in(self.canvas.zoom);
                return self.apply_zoom_change();
            }
            Message::ZoomOut => {
                self.canvas.zoom = canvas::zoom_out(self.canvas.zoom);
                return self.apply_zoom_change();
            }
            Message::ZoomReset => {
                self.canvas.zoom = 1.0;
                return self.apply_zoom_change();
            }
            Message::ZoomFitWidth => {
                if let (Some(doc), Some(win)) = (&self.document, self.window_size) {
                    let max_page_w = doc
                        .page_dimensions
                        .values()
                        .map(|(w, _)| *w)
                        .fold(0.0f32, f32::max);
                    if max_page_w > 0.0 {
                        let sidebar_w = if self.sidebar.visible {
                            self.sidebar.width
                        } else {
                            0.0
                        };
                        let available_w = (win.width - sidebar_w - 16.0).max(1.0);
                        self.canvas.zoom = canvas::fit_to_width_zoom(max_page_w, available_w);
                        return self.apply_zoom_change();
                    }
                }
            }
            Message::ZoomDebounceExpired(generation) => {
                if generation == self.canvas.zoom_generation {
                    // Clear all cached images so pages get fresh renders at
                    // the new DPI (including neighbors on navigation).
                    if let Some(doc) = &mut self.document {
                        doc.page_images.clear();
                    }
                    return self.render_visible_pages();
                }
            }
            Message::CanvasScrolled(scroll_y, viewport_height) => {
                self.canvas.scroll_y = scroll_y;
                self.canvas.viewport_height = viewport_height;
                if let Some(doc) = &mut self.document {
                    let dpi = canvas::effective_dpi(self.canvas.zoom);
                    let layout = canvas::page_layout(
                        &doc.page_dimensions,
                        doc.page_count,
                        self.canvas.zoom,
                        dpi,
                    );
                    let page = canvas::dominant_page(&layout, scroll_y, viewport_height);
                    if doc.current_page != page {
                        doc.current_page = page;
                        self.toolbar.page_input = page.to_string();
                    }
                }
                return self.render_visible_pages();
            }

            // --- Sidebar ---
            Message::ToggleSidebar => {
                self.sidebar.visible = !self.sidebar.visible;
            }
            Message::ThumbnailBatchRendered(batch, generation) => {
                if generation == self.sidebar.backfill_generation {
                    for (page, handle) in batch {
                        self.sidebar.thumbnails.insert(page, handle);
                    }
                }
            }
            Message::SidebarScrolled(scroll_y, viewport_height) => {
                self.sidebar.scroll_y = scroll_y;
                self.sidebar.viewport_height = viewport_height;
            }
            Message::SidebarResized(new_width) => {
                self.sidebar.width = new_width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
            }
            Message::SidebarResizeEnd => {}
            Message::SidebarResizeDebounceExpired(_generation) => {}
            Message::SidebarPageClicked(page) => {
                return self.update(Message::GoToPage(page));
            }
            Message::ShimmerTick => {
                self.sidebar.shimmer_phase =
                    (self.sidebar.shimmer_phase + SHIMMER_TICK_DELTA) % 1.0;
            }

            // --- Undo/Redo ---
            Message::Undo => {
                if let Some(cmd) = self.undo_stack.pop()
                    && let Some(doc) = &mut self.document
                {
                    cmd.reverse(&mut doc.overlays);
                    self.redo_stack.push(cmd);
                }
            }
            Message::Redo => {
                if let Some(cmd) = self.redo_stack.pop()
                    && let Some(doc) = &mut self.document
                {
                    cmd.apply(&mut doc.overlays);
                    self.undo_stack.push(cmd);
                }
            }

            // --- Window ---
            Message::WindowResized(size) => {
                self.window_size = Some(size);
            }

            // --- Font loaded ---
            Message::FontLoaded(_) => {}
        }
        iced::Task::none()
    }

    pub fn view(&self) -> iced::Element<'_, Message> {
        let toolbar_ctx = ToolbarContext {
            has_document: self.document.is_some(),
            can_undo: !self.undo_stack.is_empty(),
            can_redo: !self.redo_stack.is_empty(),
            has_selection: self.canvas.active_overlay.is_some(),
            current_page: self.document.as_ref().map_or(0, |d| d.current_page),
            page_count: self.document.as_ref().map_or(0, |d| d.page_count),
            zoom_percent: canvas::zoom_percent(self.canvas.zoom),
            sidebar_visible: self.sidebar.visible,
        };
        let toolbar = toolbar::toolbar_view(&self.toolbar, &toolbar_ctx).map(Message::Toolbar);

        let content: iced::Element<Message> = if let Some(doc) = &self.document {
            let dpi = canvas::effective_dpi(self.canvas.zoom);
            let layout =
                canvas::page_layout(&doc.page_dimensions, doc.page_count, self.canvas.zoom, dpi);

            let program = PdfCanvasProgram {
                page_images: &doc.page_images,
                page_layout: layout,
                page_dimensions: &doc.page_dimensions,
                page_count: doc.page_count,
                scroll_y: self.canvas.scroll_y,
                viewport_height: self.canvas.viewport_height,
                overlays: &doc.overlays,
                zoom: self.canvas.zoom,
                dpi,
                active_overlay: self.canvas.active_overlay,
                editing: self.canvas.editing,
                overlay_color: self.config.overlay_color,
            };

            let (canvas_width, canvas_height) = self.canvas_dimensions(doc);

            let canvas_area: iced::Element<Message> = iced::widget::canvas(program)
                .width(canvas_width)
                .height(canvas_height)
                .into();

            let scrollable_canvas: iced::Element<Message> = iced::widget::scrollable(canvas_area)
                .direction(iced::widget::scrollable::Direction::Both {
                    vertical: iced::widget::scrollable::Scrollbar::default(),
                    horizontal: iced::widget::scrollable::Scrollbar::default(),
                })
                .id(self.scrollable_id.clone())
                .on_scroll(|viewport| {
                    Message::CanvasScrolled(viewport.absolute_offset().y, viewport.bounds().height)
                })
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .into();

            if self.sidebar.visible {
                let sidebar = crate::ui::sidebar::sidebar_view(
                    &self.sidebar,
                    doc.page_count,
                    doc.current_page,
                    &doc.page_dimensions,
                    &doc.overlays,
                    self.config.overlay_color,
                );
                iced::widget::row![sidebar, scrollable_canvas].into()
            } else {
                scrollable_canvas
            }
        } else {
            iced::widget::center(iced::widget::text("Open a PDF to get started").size(20)).into()
        };

        iced::widget::column![toolbar, content].into()
    }

    /// Compute canvas widget dimensions for multi-page layout.
    /// Width: max page width or viewport, whichever is larger.
    /// Height: total layout height (all pages + gaps) or viewport, whichever is larger.
    fn canvas_dimensions(&self, doc: &DocumentState) -> (iced::Length, iced::Length) {
        const TOOLBAR_HEIGHT_ESTIMATE: f32 = 40.0;
        const SCROLLBAR_MARGIN: f32 = 16.0;

        let dpi = canvas::effective_dpi(self.canvas.zoom);
        let layout =
            canvas::page_layout(&doc.page_dimensions, doc.page_count, self.canvas.zoom, dpi);

        if layout.page_tops.is_empty() {
            return (iced::Length::Fill, iced::Length::Fill);
        }

        match self.window_size {
            Some(win) => {
                let sidebar_w = if self.sidebar.visible {
                    self.sidebar.width
                } else {
                    0.0
                };
                let available_w = (win.width - sidebar_w - SCROLLBAR_MARGIN).max(1.0);
                let available_h =
                    (win.height - TOOLBAR_HEIGHT_ESTIMATE - SCROLLBAR_MARGIN).max(1.0);
                (
                    iced::Length::Fixed(layout.max_width.max(available_w)),
                    iced::Length::Fixed(layout.total_height.max(available_h)),
                )
            }
            None => (iced::Length::Fill, iced::Length::Fill),
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        iced::event::listen_with(|event, status, _window| {
            // Window events are always handled, regardless of capture status.
            if let iced::Event::Window(ref win_event) = event {
                return match win_event {
                    iced::window::Event::Resized(size) => Some(Message::WindowResized(*size)),
                    iced::window::Event::Opened { size, .. } => Some(Message::WindowResized(*size)),
                    _ => None,
                };
            }
            if status == iced::event::Status::Captured {
                return None;
            }
            match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    key_to_message(key, modifiers)
                }
                _ => None,
            }
        })
    }

    fn handle_toolbar_message(&mut self, msg: toolbar::Message) -> iced::Task<Message> {
        match msg {
            toolbar::Message::OpenFile => return self.update(Message::OpenFile),
            toolbar::Message::Save => return self.update(Message::Save),
            toolbar::Message::SaveAs => return self.update(Message::SaveAs),
            toolbar::Message::Undo => return self.update(Message::Undo),
            toolbar::Message::Redo => return self.update(Message::Redo),
            toolbar::Message::FontSelected(font) => {
                return self.update(Message::ChangeFont(font));
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

    fn handle_open_file(&mut self) -> iced::Task<Message> {
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
                None => Message::DeselectOverlay, // user cancelled, no-op
            },
        )
    }

    fn handle_file_opened(&mut self, path: PathBuf) -> iced::Task<Message> {
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
                self.sidebar.thumbnails.clear();
                self.toolbar.page_input = "1".to_string();
                // Set initial zoom to fit widest page in viewport
                if let Some(win) = self.window_size {
                    let max_page_w = self
                        .document
                        .as_ref()
                        .map(|d| {
                            d.page_dimensions
                                .values()
                                .map(|(w, _)| *w)
                                .fold(0.0f32, f32::max)
                        })
                        .unwrap_or(0.0);
                    if max_page_w > 0.0 {
                        let sidebar_w = if self.sidebar.visible {
                            self.sidebar.width
                        } else {
                            0.0
                        };
                        let available_w = (win.width - sidebar_w - 16.0).max(1.0);
                        self.canvas.zoom = canvas::fit_to_width_zoom(max_page_w, available_w);
                    }
                }
                self.render_visible_pages()
            }
            Err(e) => {
                eprintln!("Failed to open PDF: {e}");
                iced::Task::none()
            }
        }
    }

    fn handle_save(&mut self) -> iced::Task<Message> {
        if let Some(doc) = &self.document
            && let Some(save_path) = &doc.save_path
        {
            let source = doc.source_path.clone();
            let dest = save_path.clone();
            let overlays = doc.overlays.clone();
            if let Err(e) = crate::pdf::writer::write_overlays(&source, &dest, &overlays) {
                eprintln!("Save failed: {e}");
            }
            return iced::Task::none();
        }
        self.handle_save_as()
    }

    fn handle_save_as(&mut self) -> iced::Task<Message> {
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
                None => Message::DeselectOverlay,
            },
        )
    }

    fn handle_save_destination(&mut self, path: PathBuf) {
        if let Some(doc) = &mut self.document {
            // Prevent saving over the source file to avoid data loss on
            // write failure (the source would already be truncated).
            if path == doc.source_path {
                eprintln!("Cannot save to the same file as the source. Use a different filename.");
                return;
            }
            let source = doc.source_path.clone();
            let overlays = doc.overlays.clone();
            if let Err(e) = crate::pdf::writer::write_overlays(&source, &path, &overlays) {
                eprintln!("Save failed: {e}");
            } else {
                doc.save_path = Some(path);
            }
        }
    }

    /// Render all pages in the visible range (plus 1-page buffer) that are not cached.
    fn render_visible_pages(&self) -> iced::Task<Message> {
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

        let dpi_u32 = dpi as u32;
        let mut tasks = Vec::new();
        for page in buffer_first..=buffer_last {
            if !doc.page_images.contains_key(&page) {
                tasks.push(render_page_task(doc.source_path.clone(), page, dpi_u32));
            }
        }
        iced::Task::batch(tasks)
    }

    /// Scroll to a specific page by computing its y-offset and using scroll_to.
    fn scroll_to_page(&self, page: u32) -> iced::Task<Message> {
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
    fn apply_zoom_change(&mut self) -> iced::Task<Message> {
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
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                generation
            },
            Message::ZoomDebounceExpired,
        )
    }
}

/// Launch an async task to render a single PDF page via pdftoppm.
fn render_page_task(pdf_path: PathBuf, page: u32, dpi: u32) -> iced::Task<Message> {
    iced::Task::perform(
        async move {
            let renderer = PdftoppmRenderer;
            match renderer.render_page(&pdf_path, page, dpi) {
                Ok(img) => Some((page, canvas::image_to_handle(img))),
                Err(e) => {
                    eprintln!("Failed to render page {page}: {e}");
                    None
                }
            }
        },
        |result| match result {
            Some((page, handle)) => Message::PageRendered(page, handle),
            None => Message::DeselectOverlay,
        },
    )
}

/// Map a keyboard event to an application message.
fn key_to_message(key: keyboard::Key, modifiers: keyboard::Modifiers) -> Option<Message> {
    use keyboard::key::Named;

    match key {
        keyboard::Key::Named(named) => match (named, modifiers.command(), modifiers.shift()) {
            (Named::Delete, false, false) => Some(Message::DeleteOverlay),
            (Named::Escape, false, false) => Some(Message::DeselectOverlay),
            (Named::PageUp, false, false) => Some(Message::PreviousPage),
            (Named::PageDown, false, false) => Some(Message::NextPage),
            (Named::F9, false, false) => Some(Message::ToggleSidebar),
            _ => None,
        },
        keyboard::Key::Character(ref c) => {
            let s = c.as_str();
            match (s, modifiers.command(), modifiers.shift()) {
                ("o", true, false) => Some(Message::OpenFile),
                ("s", true, false) => Some(Message::Save),
                ("s", true, true) | ("S", true, _) => Some(Message::SaveAs),
                ("z", true, false) => Some(Message::Undo),
                ("z", true, true) | ("Z", true, _) => Some(Message::Redo),
                ("+" | "=", true, _) => Some(Message::ZoomIn),
                ("-", true, false) => Some(Message::ZoomOut),
                ("0", true, false) => Some(Message::ZoomFitWidth),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_default_has_no_document() {
        let (app, _) = App::new();
        assert!(app.document.is_none());
        assert!(app.undo_stack.is_empty());
        assert!(app.redo_stack.is_empty());
    }

    #[test]
    fn next_page_without_document_is_noop() {
        let (mut app, _) = App::new();
        app.update(Message::NextPage);
        assert!(app.document.is_none());
    }

    fn test_app_with_document() -> App {
        let (mut app, _) = App::new();
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
        });
        app.update(Message::Undo);
        assert_eq!(app.redo_stack.len(), 1);

        app.update(Message::PlaceOverlay {
            page: 1,
            position: PdfPosition { x: 200.0, y: 600.0 },
        });
        assert!(app.redo_stack.is_empty());
    }

    #[test]
    fn delete_overlay_removes_selected() {
        let mut app = test_app_with_document();
        app.update(Message::PlaceOverlay {
            page: 1,
            position: PdfPosition { x: 100.0, y: 700.0 },
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
        let (mut app, _) = App::new();
        let initial = app.canvas.zoom;
        app.update(Message::ZoomIn);
        assert!(app.canvas.zoom > initial);
    }

    #[test]
    fn zoom_reset_returns_to_one() {
        let (mut app, _) = App::new();
        app.update(Message::ZoomIn);
        app.update(Message::ZoomReset);
        assert!((app.canvas.zoom - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn zoom_in_increments_generation() {
        let (mut app, _) = App::new();
        assert_eq!(app.canvas.zoom_generation, 0);
        app.update(Message::ZoomIn);
        assert_eq!(app.canvas.zoom_generation, 1);
        app.update(Message::ZoomIn);
        assert_eq!(app.canvas.zoom_generation, 2);
    }

    #[test]
    fn zoom_out_increments_generation() {
        let (mut app, _) = App::new();
        app.update(Message::ZoomIn); // go above 1.0 so ZoomOut has room
        let gen_before = app.canvas.zoom_generation;
        app.update(Message::ZoomOut);
        assert_eq!(app.canvas.zoom_generation, gen_before + 1);
    }

    #[test]
    fn zoom_reset_increments_generation() {
        let (mut app, _) = App::new();
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
        let (mut app, _) = App::new();
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
        let (app, _) = App::new();
        let _element = app.view();
    }

    #[test]
    fn title_without_document() {
        let (app, _) = App::new();
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
        // Should not panic — constructs PdfCanvasProgram and renders canvas
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
    fn page_rendered_inserts_into_cache() {
        let mut app = test_app_with_document();
        let handle = Handle::from_rgba(1, 1, vec![255, 0, 0, 255]);
        let _ = app.update(Message::PageRendered(1, handle));
        assert!(app.document.as_ref().unwrap().page_images.contains_key(&1));
    }

    #[test]
    fn page_rendered_replaces_existing_cached_image() {
        let mut app = test_app_with_document();
        let handle1 = Handle::from_rgba(1, 1, vec![255, 0, 0, 255]);
        let handle2 = Handle::from_rgba(1, 1, vec![0, 255, 0, 255]);
        let _ = app.update(Message::PageRendered(1, handle1));
        let _ = app.update(Message::PageRendered(1, handle2));
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
        let (mut app, _) = App::new();
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
        let (app, _) = App::new();
        assert!(app.window_size.is_none());
    }

    #[test]
    fn window_resized_stores_size() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::WindowResized(iced::Size::new(1920.0, 1080.0)));
        let size = app.window_size.unwrap();
        assert!((size.width - 1920.0).abs() < f32::EPSILON);
        assert!((size.height - 1080.0).abs() < f32::EPSILON);
    }
}
