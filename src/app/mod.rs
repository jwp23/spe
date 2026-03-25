// Iced Application: top-level state, message routing.

mod handlers;
mod view;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;

use iced::keyboard;
use iced::widget::image::Handle;

use crate::command::Command as UndoCommand;
use crate::config::AppConfig;
use crate::fonts::{FontId, FontRegistry};
use crate::overlay::{PdfPosition, TextOverlay};
use crate::ui::canvas::CanvasState;
use crate::ui::sidebar::SidebarState;
use crate::ui::toolbar::{self, ToolbarState};

/// Minimum sidebar width the user can resize to.
const MIN_SIDEBAR_WIDTH: f32 = 80.0;
/// Maximum sidebar width the user can resize to.
const MAX_SIDEBAR_WIDTH: f32 = 400.0;
/// Phase advance per shimmer tick (fraction of full cycle).
const SHIMMER_TICK_DELTA: f32 = 0.05;
/// Maximum number of thumbnail batch tasks that may run concurrently.
const MAX_CONCURRENT_THUMBNAIL_TASKS: u32 = 2;
/// Margin reserved for scrollbar width in viewport calculations.
const SCROLLBAR_MARGIN: f32 = 16.0;
/// Debounce timeout for zoom and sidebar resize operations (milliseconds).
const DEBOUNCE_MS: u64 = 300;
/// Number of pages to render in a single thumbnail batch.
const THUMBNAIL_BATCH_SIZE: usize = 20;
/// Extra pages to render above/below the visible sidebar range.
const SIDEBAR_PAGE_BUFFER: u32 = 5;

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

impl DocumentState {
    pub fn max_page_width(&self) -> f32 {
        self.page_dimensions
            .values()
            .map(|(w, _)| *w)
            .fold(0.0f32, f32::max)
    }
}

/// Top-level application state.
pub struct App {
    pub font_registry: FontRegistry,
    pub document: Option<DocumentState>,
    pub toolbar: ToolbarState,
    pub canvas: CanvasState,
    pub sidebar: SidebarState,
    pub undo_stack: Vec<UndoCommand>,
    pub redo_stack: Vec<UndoCommand>,
    pub config: AppConfig,
    pub window_size: Option<iced::Size>,
    pub scale_factor: f32,
    pub scrollable_id: iced::widget::Id,
    pub status_message: Option<(String, std::time::Instant)>,
    /// Content state for the floating multi-line text_editor (width-Some overlays).
    pub editor_content: Option<iced::widget::text_editor::Content>,
    /// Stable ID for the floating text widget, used for programmatic focus.
    pub text_input_id: iced::widget::Id,
    /// Whether the IPC socket subscription is active.
    pub ipc_enabled: bool,
    /// Sender used to deliver responses back to the IPC subscription loop.
    pub ipc_response_sender: Option<crate::ipc::ResponseSender>,
    /// A WaitReady command arrived while rendering was in progress; respond when idle.
    pub pending_ipc_wait: bool,
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
    PageBatchRendered(Vec<(u32, Handle)>),

    // Overlay editing (undoable)
    PlaceOverlay {
        page: u32,
        position: PdfPosition,
        width: Option<f32>,
    },
    UpdateOverlayText(String),
    TextEditorAction(iced::widget::text_editor::Action),
    CommitText,
    MoveOverlay(usize, PdfPosition),
    ChangeFont(FontId),
    ChangeFontSize(f32),
    DeleteOverlay,
    SelectOverlay(usize),
    EditOverlay(usize),
    DeselectOverlay,
    /// No-op: used when an async task (render, dialog) produces no actionable result.
    Noop,
    /// Dismiss the status toast if it has been visible for at least 5 seconds.
    DismissToast,

    // Canvas
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ZoomFitWidth,
    ZoomDebounceExpired(u64),
    CanvasScrolled(f32, f32),

    // Sidebar
    ToggleSidebar,
    SidebarDragStart(f32),
    ThumbnailBatchRendered(Vec<(u32, Handle)>, u64),
    SidebarScrolled(f32, f32),
    SidebarResized(f32),
    SidebarResizeEnd,
    SidebarResizeDebounceExpired(u64),
    SidebarPageClicked(u32),
    ShimmerTick,

    ResizeOverlay {
        index: usize,
        old_width: f32,
        new_width: f32,
    },

    // Undo/Redo
    Undo,
    Redo,

    // Toolbar forwarding
    Toolbar(toolbar::Message),

    // Window
    WindowResized(iced::Size),
    ScaleFactorChanged(f32),

    // Font loaded
    FontLoaded(Result<(), iced::font::Error>),

    // IPC
    Ipc(crate::ipc::IpcEvent),
}

impl App {
    pub fn new(ipc_enabled: bool) -> (Self, iced::Task<Message>) {
        let font_registry = FontRegistry::new();
        let app = Self {
            toolbar: ToolbarState::new(font_registry.default_font()),
            font_registry,
            document: None,
            canvas: CanvasState::default(),
            sidebar: SidebarState::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            config: AppConfig::default(),
            window_size: None,
            scale_factor: 1.0,
            scrollable_id: iced::widget::Id::unique(),
            status_message: None,
            editor_content: None,
            text_input_id: iced::widget::Id::unique(),
            ipc_enabled,
            ipc_response_sender: None,
            pending_ipc_wait: false,
        };
        let mut font_tasks =
            vec![iced::font::load(crate::ui::icons::font_bytes()).map(Message::FontLoaded)];
        for entry in app.font_registry.all() {
            if let crate::fonts::PdfEmbedding::TrueType { bytes } = &entry.embedding {
                font_tasks.push(iced::font::load(*bytes).map(Message::FontLoaded));
            }
        }
        let font_task = iced::Task::batch(font_tasks);
        (app, font_task)
    }

    /// Returns true when no render tasks are in flight and all pages have been rendered.
    pub fn is_render_idle(&self) -> bool {
        if self.sidebar.active_batch_tasks > 0 {
            return false;
        }
        if let Some(doc) = &self.document {
            for page in 1..=doc.page_count {
                if !doc.page_images.contains_key(&page) {
                    return false;
                }
            }
        }
        true
    }

    /// If a WaitReady response is pending and rendering is now idle, send the response.
    pub(super) fn check_ipc_wait(&mut self) -> iced::Task<Message> {
        if self.pending_ipc_wait && self.is_render_idle() {
            self.pending_ipc_wait = false;
            let response = crate::ipc::IpcResponse {
                ok: true,
                error: None,
            };
            if let Some(sender) = &self.ipc_response_sender {
                let sender = sender.clone();
                return iced::Task::perform(
                    async move {
                        let tx = sender.0.lock().await;
                        let _ = tx.send(response).await;
                    },
                    |_| Message::Noop,
                );
            }
        }
        iced::Task::none()
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

    fn execute_command(&mut self, cmd: UndoCommand) {
        if let Some(doc) = &mut self.document {
            cmd.apply(&mut doc.overlays);
            self.undo_stack.push(cmd);
            self.redo_stack.clear();
        }
    }

    fn effective_sidebar_width(&self) -> f32 {
        if self.sidebar.visible {
            self.sidebar.width
        } else {
            0.0
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
            Message::NextPage => return self.handle_next_page(),
            Message::PreviousPage => return self.handle_previous_page(),
            Message::GoToPage(page) => return self.handle_go_to_page(page),
            Message::PageBatchRendered(pages) => return self.handle_page_batch_rendered(pages),

            // --- Overlay editing (undoable) ---
            Message::PlaceOverlay {
                page,
                position,
                width,
            } => return self.handle_place_overlay(page, position, width),
            Message::UpdateOverlayText(text) => self.handle_update_overlay_text(text),
            Message::TextEditorAction(action) => self.handle_text_editor_action(action),
            Message::CommitText => {
                return self.handle_commit_text();
            }
            Message::MoveOverlay(index, new_position) => {
                self.handle_move_overlay(index, new_position);
            }
            Message::ResizeOverlay {
                index,
                old_width,
                new_width,
            } => self.handle_resize_overlay(index, old_width, new_width),
            Message::ChangeFont(font) => self.handle_change_font(font),
            Message::ChangeFontSize(size) => self.handle_change_font_size(size),
            Message::DeleteOverlay => self.handle_delete_overlay(),
            Message::SelectOverlay(index) => self.handle_select_overlay(index),
            Message::EditOverlay(index) => return self.handle_edit_overlay(index),
            Message::DeselectOverlay => return self.handle_deselect_overlay(),
            Message::Noop => {}
            Message::DismissToast => self.handle_dismiss_toast(),

            // --- Canvas (zoom with debounce) ---
            Message::ZoomIn => return self.handle_zoom_in(),
            Message::ZoomOut => return self.handle_zoom_out(),
            Message::ZoomReset => return self.handle_zoom_reset(),
            Message::ZoomFitWidth => return self.handle_zoom_fit_width(),
            Message::ZoomDebounceExpired(generation) => {
                return self.handle_zoom_debounce_expired(generation);
            }
            Message::CanvasScrolled(scroll_y, vh) => {
                return self.handle_canvas_scrolled(scroll_y, vh);
            }

            // --- Sidebar ---
            Message::ToggleSidebar => self.sidebar.visible = !self.sidebar.visible,
            Message::ThumbnailBatchRendered(batch, generation) => {
                return self.handle_thumbnail_batch_rendered(batch, generation);
            }
            Message::SidebarScrolled(scroll_y, vh) => {
                return self.handle_sidebar_scrolled(scroll_y, vh);
            }
            Message::SidebarDragStart(_) => self.handle_sidebar_drag_start(),
            Message::SidebarResized(cursor_x) => self.handle_sidebar_resized(cursor_x),
            Message::SidebarResizeEnd => return self.handle_sidebar_resize_end(),
            Message::SidebarResizeDebounceExpired(generation) => {
                return self.handle_sidebar_resize_debounce_expired(generation);
            }
            Message::SidebarPageClicked(page) => return self.handle_go_to_page(page),
            Message::ShimmerTick => {
                self.sidebar.shimmer_phase =
                    (self.sidebar.shimmer_phase + SHIMMER_TICK_DELTA) % 1.0;
            }

            // --- Undo/Redo ---
            Message::Undo => self.handle_undo(),
            Message::Redo => self.handle_redo(),

            // --- Window ---
            Message::WindowResized(size) => self.window_size = Some(size),
            Message::ScaleFactorChanged(factor) => self.scale_factor = factor,

            // --- Font loaded ---
            Message::FontLoaded(_) => {}

            // --- IPC ---
            Message::Ipc(event) => return self.handle_ipc_event(event),
        }
        iced::Task::none()
    }

    fn handle_ipc_event(&mut self, event: crate::ipc::IpcEvent) -> iced::Task<Message> {
        match event {
            crate::ipc::IpcEvent::Ready(sender) => {
                self.ipc_response_sender = Some(sender);
                iced::Task::none()
            }
            crate::ipc::IpcEvent::Command(cmd) => {
                let (response, msg_result) =
                    match cmd.to_message(self.document.as_ref(), &self.font_registry) {
                        Ok(msg) => (
                            crate::ipc::IpcResponse {
                                ok: true,
                                error: None,
                            },
                            Some(msg),
                        ),
                        Err(e) => (
                            crate::ipc::IpcResponse {
                                ok: false,
                                error: Some(e.to_string()),
                            },
                            None,
                        ),
                    };
                if let Some(msg) = msg_result {
                    let _ = self.update(msg);
                }
                if let Some(sender) = &self.ipc_response_sender {
                    let sender = sender.clone();
                    return iced::Task::perform(
                        async move {
                            let tx = sender.0.lock().await;
                            let _ = tx.send(response).await;
                        },
                        |_| Message::Noop,
                    );
                }
                iced::Task::none()
            }
            crate::ipc::IpcEvent::WaitReady => {
                if self.is_render_idle() {
                    let response = crate::ipc::IpcResponse {
                        ok: true,
                        error: None,
                    };
                    if let Some(sender) = &self.ipc_response_sender {
                        let sender = sender.clone();
                        return iced::Task::perform(
                            async move {
                                let tx = sender.0.lock().await;
                                let _ = tx.send(response).await;
                            },
                            |_| Message::Noop,
                        );
                    }
                } else {
                    self.pending_ipc_wait = true;
                }
                iced::Task::none()
            }
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        let event_sub = iced::event::listen_with(|event, status, _window| {
            // Window events are always handled, regardless of capture status.
            if let iced::Event::Window(ref win_event) = event {
                return match win_event {
                    iced::window::Event::Resized(size) => Some(Message::WindowResized(*size)),
                    iced::window::Event::Opened { size, .. } => Some(Message::WindowResized(*size)),
                    iced::window::Event::Rescaled(factor) => {
                        Some(Message::ScaleFactorChanged(*factor))
                    }
                    _ => None,
                };
            }
            // Mouse move/release events are always forwarded (regardless of
            // capture status) so the drag handler in update() can track them.
            // The handler guards on self.sidebar.dragging and ignores events
            // when no drag is active.
            if let iced::Event::Mouse(ref mouse_event) = event {
                match mouse_event {
                    iced::mouse::Event::CursorMoved { position } => {
                        return Some(Message::SidebarResized(position.x));
                    }
                    iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left) => {
                        return Some(Message::SidebarResizeEnd);
                    }
                    _ => {}
                }
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
        });

        // Tick shimmer animation only while sidebar is visible and has unrendered pages.
        let shimmer_sub = if self.sidebar.visible
            && self
                .document
                .as_ref()
                .is_some_and(|doc| doc.page_count as usize > self.sidebar.thumbnails.len())
        {
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::ShimmerTick)
        } else {
            iced::Subscription::none()
        };

        // Tick once per second to auto-dismiss the toast after 5 seconds.
        let toast_sub = if self.status_message.is_some() {
            iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::DismissToast)
        } else {
            iced::Subscription::none()
        };

        let ipc_sub = if self.ipc_enabled {
            crate::ipc::ipc_subscription().map(Message::Ipc)
        } else {
            iced::Subscription::none()
        };

        iced::Subscription::batch([event_sub, shimmer_sub, toast_sub, ipc_sub])
    }
}

/// Map a keyboard event to an application message.
fn key_to_message(key: keyboard::Key, modifiers: keyboard::Modifiers) -> Option<Message> {
    use keyboard::key::Named;

    match key {
        keyboard::Key::Named(named) => match (named, modifiers.command(), modifiers.shift()) {
            (Named::Delete, false, false) => Some(Message::DeleteOverlay),
            (Named::Escape, false, false) => Some(Message::DeselectOverlay),
            (Named::Enter, true, false) => Some(Message::CommitText),
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
