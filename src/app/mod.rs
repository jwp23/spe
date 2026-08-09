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
    /// Counts every message this app has processed. Bumped once at the top of
    /// `update`, so it is a monotonic clock over the app's own state changes —
    /// the basis `wait_frame` uses to know which mutations a redraw must
    /// reflect before the wait can resolve (spe-xqb).
    pub state_generation: u64,
    /// The `state_generation` value as of the most recent completed redraw
    /// (see [`Message::FramePresented`]). `wait_frame` resolves once this
    /// reaches the generation captured when the wait was requested.
    pub presented_generation: u64,
    /// A `wait_frame` command arrived before a redraw had caught up to the
    /// generation it needs to observe; holds that target generation until
    /// [`App::check_ipc_frame_wait`] can resolve it.
    pub pending_frame_wait: Option<u64>,
    /// Failure recorded by the handler of the IPC command currently running.
    ///
    /// Preconditions are checked before dispatch (see [`crate::ipc::CommandContext`]),
    /// but some commands can only fail while doing their work — opening a PDF,
    /// writing one. Those handlers record the reason here and the response path
    /// consumes it, so no command needs a field of its own.
    pub last_command_error: Option<String>,
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
    /// The Open or Save As file dialog was closed without picking a path.
    /// Distinct from `Noop` so canceling can safely refocus an in-progress
    /// overlay edit — `Noop` also fires for the font-size arrow-key's
    /// unfocused case, where refocusing would yank the cursor away from
    /// in-editor arrow-key navigation.
    DialogDismissed,

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
    /// An ArrowUp (`true`) or ArrowDown (`false`) key was pressed. Only
    /// affects the font size when the toolbar's font-size input is
    /// focused, so this queries that focus state before acting.
    FontSizeArrowPressed(bool),
    /// The focus query from [`Message::FontSizeArrowPressed`] resolved with
    /// the font-size input focused: step the size up (`true`) or down
    /// (`false`).
    FontSizeArrowKeyResult(bool),
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

    /// A redraw completed: iced submitted a frame reflecting `state_generation`
    /// as of when this message is processed. See [`App::presented_generation`].
    FramePresented,
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
            state_generation: 0,
            presented_generation: 0,
            pending_frame_wait: None,
            last_command_error: None,
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

    /// Build a task that delivers an IPC response to the waiting client. Yields
    /// an empty task when no IPC client is connected.
    fn send_ipc_response(&self, response: crate::ipc::IpcResponse) -> iced::Task<Message> {
        let Some(sender) = &self.ipc_response_sender else {
            return iced::Task::none();
        };
        let sender = sender.clone();
        iced::Task::perform(deliver_ipc_response(sender, response), |_| Message::Noop)
    }

    /// Fold any failure the command's handler recorded into its response.
    /// `to_message` only proves a command translated into a Message; handlers
    /// that do fallible work set `last_command_error` synchronously, so the
    /// reply reports what actually happened (spe-6vq, spe-749).
    fn command_response(&mut self, base: crate::ipc::IpcResponse) -> crate::ipc::IpcResponse {
        match self.last_command_error.take() {
            Some(error) => crate::ipc::IpcResponse {
                ok: false,
                error: Some(error),
            },
            None => base,
        }
    }

    /// The read-only view of app state that IPC command translation inspects to
    /// decide whether a command can actually do anything.
    fn ipc_context(&self) -> crate::ipc::CommandContext<'_> {
        crate::ipc::CommandContext {
            document: self.document.as_ref(),
            active_overlay: self.canvas.active_overlay,
            editing: self.canvas.editing,
            undo_depth: self.undo_stack.len(),
            redo_depth: self.redo_stack.len(),
        }
    }

    /// Run one IPC command and report whether it actually happened, along with
    /// the follow-up task its update produced.
    ///
    /// The follow-up task (e.g. the page-render task from opening a document)
    /// must be kept and returned: discarding it strands rendering and wedges
    /// `wait_ready` (spe-dr0).
    pub(super) fn run_ipc_command(
        &mut self,
        cmd: crate::ipc::IpcCommand,
    ) -> (crate::ipc::IpcResponse, iced::Task<Message>) {
        // Any error left by an earlier command was already reported; clearing it
        // here keeps a stale failure from being blamed on this command.
        self.last_command_error = None;

        let msg = match cmd.to_message(&self.ipc_context(), &self.font_registry) {
            Ok(msg) => msg,
            Err(e) => {
                return (
                    crate::ipc::IpcResponse {
                        ok: false,
                        error: Some(e.to_string()),
                    },
                    iced::Task::none(),
                );
            }
        };
        let task = self.update(msg);
        let response = self.command_response(crate::ipc::IpcResponse {
            ok: true,
            error: None,
        });
        (response, task)
    }

    /// If a WaitReady response is pending and rendering is now idle, send the response.
    pub(super) fn check_ipc_wait(&mut self) -> iced::Task<Message> {
        if self.pending_ipc_wait && self.is_render_idle() {
            self.pending_ipc_wait = false;
            return self.send_ipc_response(crate::ipc::IpcResponse {
                ok: true,
                error: None,
            });
        }
        iced::Task::none()
    }

    /// If a WaitFrame response is pending and a redraw has caught up to the
    /// generation it needs to observe, send the response.
    pub(super) fn check_ipc_frame_wait(&mut self) -> iced::Task<Message> {
        if let Some(target) = self.pending_frame_wait
            && self.presented_generation >= target
        {
            self.pending_frame_wait = None;
            return self.send_ipc_response(crate::ipc::IpcResponse {
                ok: true,
                error: None,
            });
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
        self.state_generation += 1;
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
                return self.handle_save_destination(path);
            }
            Message::DialogDismissed => return self.refocus_editing_widget(),

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
            Message::ChangeFont(font) => return self.handle_change_font(font),
            Message::ChangeFontSize(size) => return self.handle_change_font_size(size),
            Message::FontSizeArrowPressed(increment) => {
                return self.handle_font_size_arrow_pressed(increment);
            }
            Message::FontSizeArrowKeyResult(increment) => {
                return self.handle_font_size_arrow_key_result(increment);
            }
            Message::DeleteOverlay => return self.handle_delete_overlay(),
            Message::SelectOverlay(index) => return self.handle_select_overlay(index),
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
            Message::ToggleSidebar => return self.handle_toggle_sidebar(),
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
            Message::Redo => return self.handle_redo(),

            // --- Window ---
            Message::WindowResized(size) => self.window_size = Some(size),
            Message::ScaleFactorChanged(factor) => self.scale_factor = factor,

            // --- Font loaded ---
            Message::FontLoaded(_) => {}

            // --- IPC ---
            Message::Ipc(event) => return self.handle_ipc_event(event),
            Message::FramePresented => {
                self.presented_generation = self.state_generation;
                return self.check_ipc_frame_wait();
            }
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
                // command_task and response_task run concurrently, so a
                // command whose handler chains a *trailing* task that later
                // delivers its own Message could still bump state_generation
                // after a wait_frame sent right after this reply — wait_frame
                // would then resolve on a frame that predates that trailing
                // effect. None of click/click_at/drag/type/select/deselect do
                // this (their only trailing task is a synchronous,
                // message-less widget focus effect); open and the zoom_*
                // commands do (a real render task / a debounced re-render)
                // and need their own wait_ready before anything else runs.
                // Full audit: "Staleness window" in docs/visual-regression.md.
                let (response, command_task) = self.run_ipc_command(cmd);
                let response_task = self.send_ipc_response(response);
                iced::Task::batch([command_task, response_task])
            }
            crate::ipc::IpcEvent::WaitReady => {
                if self.is_render_idle() {
                    self.send_ipc_response(crate::ipc::IpcResponse {
                        ok: true,
                        error: None,
                    })
                } else {
                    self.pending_ipc_wait = true;
                    iced::Task::none()
                }
            }
            crate::ipc::IpcEvent::WaitFrame => {
                // Target the generation as of the command that preceded this
                // one, not this WaitFrame message's own bump.
                //
                // A self-inclusive target would NOT hang: iced's AboutToWait
                // handler unconditionally requests a redraw for every window
                // whenever the message queue was non-empty, regardless of
                // whether the view actually changed (`iced_winit-0.14.0/
                // src/lib.rs:1211-1249`), so even WaitFrame's own bump gets a
                // follow-up RedrawRequested and would eventually resolve.
                // Verified empirically against the running harness: a
                // self-inclusive target resolved in ~20ms, same order as
                // self-exclusive, never hit the 30s timeout.
                //
                // Excluding WaitFrame's own bump is still the better choice:
                // it gives idle-resolve semantics matching wait_ready — if
                // the preceding command's frame is already presented,
                // wait_frame returns immediately instead of always paying for
                // one needless extra redraw round-trip.
                let target = self.state_generation.saturating_sub(1);
                if self.presented_generation >= target {
                    self.send_ipc_response(crate::ipc::IpcResponse {
                        ok: true,
                        error: None,
                    })
                } else {
                    self.pending_frame_wait = Some(target);
                    iced::Task::none()
                }
            }
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        let event_sub = iced::event::listen_with(event_to_message);

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

        // Only subscribe while a wait_frame is pending. iced's AboutToWait
        // handler unconditionally requests another redraw whenever the
        // message queue was non-empty (see the WaitFrame comment above), so
        // an unconditional subscription here would turn every RedrawRequested
        // into a new Message::FramePresented and never let the event loop go
        // idle — a perpetual redraw loop.
        let frame_sub = if self.pending_frame_wait.is_some() {
            iced::event::listen_raw(frame_event_to_message)
        } else {
            iced::Subscription::none()
        };

        iced::Subscription::batch([event_sub, shimmer_sub, toast_sub, ipc_sub, frame_sub])
    }
}

/// Deliver an IPC response over the response channel. Separated from
/// [`App::send_ipc_response`] so delivery can be tested without the iced runtime.
async fn deliver_ipc_response(
    sender: crate::ipc::ResponseSender,
    response: crate::ipc::IpcResponse,
) {
    let tx = sender.0.lock().await;
    let _ = tx.send(response).await;
}

/// Map an iced event to an application message, filtering by event type and capture status.
fn event_to_message(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    // Window events are always handled, regardless of capture status.
    if let iced::Event::Window(ref win_event) = event {
        return window_event_to_message(win_event);
    }
    // Mouse move/release events are always forwarded (regardless of capture status)
    // so the drag handler in update() can track them.
    if let iced::Event::Mouse(ref mouse_event) = event
        && let Some(msg) = mouse_event_to_message(mouse_event)
    {
        return Some(msg);
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
}

/// Map a raw runtime event to [`Message::FramePresented`] when it is a
/// completed redraw, discarding everything else.
///
/// `RedrawRequested` is broadcast to subscriptions synchronously *before*
/// `compositor.present()` is called in the same, non-yielding block of the
/// winit event loop (see `iced_winit::run_instance`, the branch handling
/// `WindowEvent::RedrawRequested`: the broadcast happens, then `present()` is
/// called with no `.await` between them). So by the time this message reaches
/// [`App::update`], the frame for the current state has already been
/// submitted — this is the closest signal iced 0.14 exposes to "a frame
/// reflecting the latest state has been presented"; there is no
/// `window::frames()`-style post-present hook in this version. It is filtered
/// out of the ordinary [`iced::event::listen_with`] subscription (see
/// `event_to_message`), so `wait_frame` uses [`iced::event::listen_raw`]
/// instead, restricted here to just this one event to avoid the flood
/// `listen_raw` would otherwise deliver.
fn frame_event_to_message(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Window(iced::window::Event::RedrawRequested(_)) => {
            Some(Message::FramePresented)
        }
        _ => None,
    }
}

/// Map a window event to an application message.
fn window_event_to_message(event: &iced::window::Event) -> Option<Message> {
    match event {
        iced::window::Event::Resized(size) => Some(Message::WindowResized(*size)),
        iced::window::Event::Opened { size, .. } => Some(Message::WindowResized(*size)),
        iced::window::Event::Rescaled(factor) => Some(Message::ScaleFactorChanged(*factor)),
        _ => None,
    }
}

/// Map a mouse event to an application message for sidebar drag tracking.
fn mouse_event_to_message(event: &iced::mouse::Event) -> Option<Message> {
    match event {
        iced::mouse::Event::CursorMoved { position } => Some(Message::SidebarResized(position.x)),
        iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left) => {
            Some(Message::SidebarResizeEnd)
        }
        _ => None,
    }
}

/// Resolve the `is_focused` query [`App::handle_font_size_arrow_pressed`]
/// dispatches: only step the font size (`FontSizeArrowKeyResult`) when the
/// font-size input was actually focused, otherwise no-op.
fn arrow_key_result(focused: bool, increment: bool) -> Message {
    if focused {
        Message::FontSizeArrowKeyResult(increment)
    } else {
        Message::Noop
    }
}

/// Map a keyboard event to an application message.
fn key_to_message(key: keyboard::Key, modifiers: keyboard::Modifiers) -> Option<Message> {
    use keyboard::key::Named;

    match key {
        keyboard::Key::Named(named) => match (named, modifiers.command(), modifiers.shift()) {
            (Named::Delete, false, false) => Some(Message::DeleteOverlay),
            // Ctrl+Enter commits multi-line edits the same way Escape does:
            // both funnel through DeselectOverlay, which commits (if editing)
            // and then clears the selection.
            (Named::Escape, false, false) | (Named::Enter, true, false) => {
                Some(Message::DeselectOverlay)
            }
            (Named::PageUp, false, false) => Some(Message::PreviousPage),
            (Named::PageDown, false, false) => Some(Message::NextPage),
            (Named::F9, false, false) => Some(Message::ToggleSidebar),
            (Named::ArrowUp, false, false) if modifiers.is_empty() => {
                Some(Message::FontSizeArrowPressed(true))
            }
            (Named::ArrowDown, false, false) if modifiers.is_empty() => {
                Some(Message::FontSizeArrowPressed(false))
            }
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
