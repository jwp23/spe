// IPC protocol: command parsing, command-to-Message translation, subscription.

use std::fmt;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

use crate::app::{DocumentState, Message};
use crate::fonts::FontRegistry;
use crate::overlay::PdfPosition;
use crate::ui::canvas::hit_test_pdf;

/// Errors that can occur when translating an IpcCommand to a Message.
#[derive(Debug, PartialEq)]
pub enum IpcError {
    /// The command requires a loaded document but none is present.
    NoDocument,
    /// The overlay index is out of range for the current document.
    IndexOutOfRange,
    /// The page number is out of range for the current document.
    PageOutOfRange,
    /// The command edits the overlay being worked on, but none is active.
    NoActiveOverlay,
    /// The targeted overlay has no width and cannot be resized.
    NotResizable,
    /// The font name could not be resolved in the registry.
    UnknownFont(String),
    /// There is no recorded command left to undo.
    NothingToUndo,
    /// There is no undone command left to redo.
    NothingToRedo,
}

impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IpcError::NoDocument => write!(f, "no document is loaded"),
            IpcError::IndexOutOfRange => write!(f, "overlay index is out of range"),
            IpcError::PageOutOfRange => write!(f, "page number is out of range"),
            IpcError::NoActiveOverlay => write!(f, "no overlay is active"),
            IpcError::NotResizable => write!(f, "overlay is not resizable (no width set)"),
            IpcError::UnknownFont(name) => write!(f, "unknown font: {name}"),
            IpcError::NothingToUndo => write!(f, "nothing to undo"),
            IpcError::NothingToRedo => write!(f, "nothing to redo"),
        }
    }
}

/// Read-only view of the application state that command translation consults to
/// check a command's preconditions.
///
/// Passing state in as one borrowed struct is what keeps `ipc` decoupled from
/// `App`: translation can read exactly the state a precondition needs and can
/// never mutate the application, so preconditions are checked *before* any
/// message is dispatched rather than reported after the fact.
#[derive(Default)]
pub struct CommandContext<'a> {
    /// The loaded document, if any.
    pub document: Option<&'a DocumentState>,
    /// Index of the overlay currently selected or being edited.
    pub active_overlay: Option<usize>,
    /// Whether an overlay's text is currently being edited.
    pub editing: bool,
    /// Number of commands available to undo.
    pub undo_depth: usize,
    /// Number of commands available to redo.
    pub redo_depth: usize,
}

impl<'a> CommandContext<'a> {
    /// The loaded document, or [`IpcError::NoDocument`].
    fn require_document(&self) -> Result<&'a DocumentState, IpcError> {
        self.document.ok_or(IpcError::NoDocument)
    }

    /// The loaded document, checked to actually contain `page`.
    fn require_page(&self, page: u32) -> Result<&'a DocumentState, IpcError> {
        let doc = self.require_document()?;
        if page < 1 || page > doc.page_count {
            return Err(IpcError::PageOutOfRange);
        }
        Ok(doc)
    }

    /// The loaded document, checked to actually contain overlay `index`.
    fn require_overlay(&self, index: usize) -> Result<&'a DocumentState, IpcError> {
        let doc = self.require_document()?;
        if index >= doc.overlays.len() {
            return Err(IpcError::IndexOutOfRange);
        }
        Ok(doc)
    }

    /// The index of the overlay an edit would apply to, or
    /// [`IpcError::NoActiveOverlay`] when nothing editable is selected.
    fn require_active_overlay(&self) -> Result<usize, IpcError> {
        let doc = self.require_document()?;
        match self.active_overlay {
            Some(index) if index < doc.overlays.len() => Ok(index),
            _ => Err(IpcError::NoActiveOverlay),
        }
    }
}

/// A command received over the IPC socket.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcCommand {
    Open {
        path: PathBuf,
    },
    /// Write the document with its overlays to `path`. Uses the same PDF
    /// writer as the Save As dialog; only the dialog itself is bypassed.
    Save {
        path: PathBuf,
    },
    /// Place an overlay at a PDF position, unconditionally. Bypasses the
    /// canvas hit test, so it can never select an existing overlay — use
    /// `ClickAt` to reproduce what a real mouse click would do.
    Click {
        page: u32,
        x: f32,
        y: f32,
    },
    /// Click at a PDF position the way the mouse does: commit an in-progress
    /// edit, select an overlay under the point, place a new one on empty page,
    /// or deselect when the point is off the page.
    ClickAt {
        page: u32,
        x: f32,
        y: f32,
    },
    Type {
        text: String,
    },
    Select {
        index: usize,
    },
    Edit {
        index: usize,
    },
    Deselect,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ZoomFitWidth,
    Font {
        family: String,
    },
    FontSize {
        size: f32,
    },
    Drag {
        page: u32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    Resize {
        index: usize,
        width: f32,
    },
    Move {
        index: usize,
        x: f32,
        y: f32,
    },
    Undo,
    Redo,
    WaitReady,
}

/// Upper bound on how long the IPC subscription waits for the app to answer a
/// single command. Deliberately generous: this is a wedge-breaker, not a
/// latency budget. `wait_ready` legitimately blocks until every page has
/// rendered, which can take several seconds, so the bound must stay well above
/// any real processing time. If it elapses, the command is reported to the
/// client as a timeout and the accept loop continues instead of wedging
/// forever on a command that will never respond (see spe-z6v).
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Returns the IPC socket path.
pub fn socket_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(dir).join("spe-ipc.sock")
    } else {
        PathBuf::from("/tmp/spe-ipc.sock")
    }
}

impl IpcCommand {
    /// Translate this command into the corresponding application [`Message`].
    ///
    /// Every command whose handler would silently do nothing under the current
    /// state fails here instead, so the IPC reply reports whether the action
    /// actually happened rather than merely that a message could be built.
    pub fn to_message(
        self,
        ctx: &CommandContext<'_>,
        registry: &FontRegistry,
    ) -> Result<Message, IpcError> {
        match self {
            IpcCommand::Open { path } => Ok(Message::FileOpened(path)),
            IpcCommand::Save { path } => {
                ctx.require_document()?;
                Ok(Message::SaveDestinationChosen(path))
            }
            IpcCommand::Click { page, x, y } => {
                ctx.require_page(page)?;
                Ok(Message::PlaceOverlay {
                    page,
                    position: PdfPosition { x, y },
                    width: None,
                })
            }
            IpcCommand::ClickAt { page, x, y } => {
                let doc = ctx.require_page(page)?;
                Ok(click_at_message(doc, ctx.editing, page, x, y, registry))
            }
            IpcCommand::Type { text } => {
                ctx.require_active_overlay()?;
                Ok(Message::UpdateOverlayText(text))
            }
            IpcCommand::Select { index } => {
                ctx.require_overlay(index)?;
                Ok(Message::SelectOverlay(index))
            }
            IpcCommand::Edit { index } => {
                ctx.require_overlay(index)?;
                Ok(Message::EditOverlay(index))
            }
            IpcCommand::Deselect => Ok(Message::DeselectOverlay),
            IpcCommand::ZoomIn => Ok(Message::ZoomIn),
            IpcCommand::ZoomOut => Ok(Message::ZoomOut),
            IpcCommand::ZoomReset => Ok(Message::ZoomReset),
            IpcCommand::ZoomFitWidth => {
                ctx.require_document()?;
                Ok(Message::ZoomFitWidth)
            }
            IpcCommand::Font { family } => {
                ctx.require_document()?;
                let id = registry
                    .find_by_name(&family)
                    .ok_or(IpcError::UnknownFont(family))?;
                Ok(Message::ChangeFont(id))
            }
            IpcCommand::FontSize { size } => {
                ctx.require_document()?;
                Ok(Message::ChangeFontSize(size))
            }
            IpcCommand::Drag {
                page,
                x1,
                y1,
                x2,
                y2: _,
            } => {
                ctx.require_page(page)?;
                Ok(Message::PlaceOverlay {
                    page,
                    position: PdfPosition { x: x1, y: y1 },
                    width: Some((x2 - x1).abs()),
                })
            }
            IpcCommand::Resize { index, width } => {
                let doc = ctx.require_overlay(index)?;
                let old_width = doc.overlays[index].width.ok_or(IpcError::NotResizable)?;
                Ok(Message::ResizeOverlay {
                    index,
                    old_width,
                    new_width: width,
                })
            }
            IpcCommand::Move { index, x, y } => {
                ctx.require_overlay(index)?;
                Ok(Message::MoveOverlay(index, PdfPosition { x, y }))
            }
            IpcCommand::Undo => {
                ctx.require_document()?;
                // An in-progress edit is itself undoable: undo cancels the
                // session before it reaches the command history.
                if ctx.undo_depth == 0 && !ctx.editing {
                    return Err(IpcError::NothingToUndo);
                }
                Ok(Message::Undo)
            }
            IpcCommand::Redo => {
                ctx.require_document()?;
                if ctx.redo_depth == 0 {
                    return Err(IpcError::NothingToRedo);
                }
                Ok(Message::Redo)
            }
            IpcCommand::WaitReady => Ok(Message::Noop),
        }
    }
}

/// Decide what a left click at a PDF position does, mirroring the canvas
/// program's own press handling (`OverlayCanvasProgram::handle_left_click`):
/// a click while editing commits first, a click on an overlay selects it, a
/// click on blank page area places a new overlay, and a click off the page
/// deselects. The overlay lookup is the same [`hit_test_pdf`] the mouse path
/// reaches through `hit_test`, so automation and the mouse cannot diverge.
///
/// Pages whose dimensions have not been read yet are treated as unbounded,
/// since a click cannot be shown to be off a page of unknown size.
fn click_at_message(
    doc: &DocumentState,
    editing: bool,
    page: u32,
    x: f32,
    y: f32,
    registry: &FontRegistry,
) -> Message {
    if editing {
        return Message::CommitText;
    }
    if let Some(index) = hit_test_pdf(x, y, &doc.overlays, page, registry) {
        return Message::SelectOverlay(index);
    }
    let on_page = match doc.page_dimensions.get(&page) {
        Some((w, h)) => x >= 0.0 && x <= *w && y >= 0.0 && y <= *h,
        None => true,
    };
    if on_page {
        Message::PlaceOverlay {
            page,
            position: PdfPosition { x, y },
            width: None,
        }
    } else {
        Message::DeselectOverlay
    }
}

/// Response sent from the app back to the IPC subscription.
#[derive(Debug, Clone)]
pub struct IpcResponse {
    pub ok: bool,
    pub error: Option<String>,
}

/// Wrapper around the response sender so it can be stored in App state.
/// Cloneable because Arc.
#[derive(Debug, Clone)]
pub struct ResponseSender(pub Arc<tokio::sync::Mutex<tokio::sync::mpsc::Sender<IpcResponse>>>);

/// Events yielded by the IPC subscription to the app.
#[derive(Debug, Clone)]
pub enum IpcEvent {
    /// Subscription is ready — app should store the response sender.
    Ready(ResponseSender),
    /// A parsed command from the client.
    Command(IpcCommand),
    /// A WaitReady request — app should check idle state.
    WaitReady,
}

/// Creates the IPC subscription. Returns events that the app maps to Messages.
pub fn ipc_subscription() -> iced::Subscription<IpcEvent> {
    iced::Subscription::run(ipc_stream)
}

/// Serialize a JSON value as a single newline-terminated line and write it to
/// the client. Write errors are ignored: the client may already have hung up.
async fn write_json_line<W>(writer: &mut W, value: serde_json::Value)
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    let mut line = value.to_string();
    line.push('\n');
    let _ = writer.write_all(line.as_bytes()).await;
}

/// Process one JSON line: parse command, yield event, wait for response, write reply.
/// Returns false if the response channel closed (app shut down).
async fn process_line(
    line: &str,
    output: &mut iced::futures::channel::mpsc::Sender<IpcEvent>,
    resp_rx: &mut tokio::sync::mpsc::Receiver<IpcResponse>,
    writer: &mut tokio::io::WriteHalf<tokio::net::UnixStream>,
    response_timeout: Duration,
) -> bool {
    use iced::futures::SinkExt;

    // Best-effort recovery from a previously timed-out command: discard any
    // late response it left buffered in the shared channel so it is not
    // mismatched to this command. This only covers responses that have already
    // landed; the channel carries no correlation id, so a response that arrives
    // after this drain but before our own recv can still be mismatched. Closing
    // that window fully needs the correlation/concurrency redesign deferred in
    // spe-z6v.
    while resp_rx.try_recv().is_ok() {}

    // Try to parse the command.
    let cmd: IpcCommand = match serde_json::from_str(line) {
        Ok(c) => c,
        Err(e) => {
            write_json_line(
                writer,
                serde_json::json!({
                    "ok": false,
                    "error": format!("parse error: {e}")
                }),
            )
            .await;
            return true;
        }
    };

    // Yield the appropriate event to the app.
    if matches!(cmd, IpcCommand::WaitReady) {
        let _ = output.send(IpcEvent::WaitReady).await;
    } else {
        let _ = output.send(IpcEvent::Command(cmd)).await;
    }

    // Wait for the app to process and send a response. Bounded so a command that
    // never produces a response cannot wedge the accept loop forever (spe-z6v).
    let response = match tokio::time::timeout(response_timeout, resp_rx.recv()).await {
        Ok(Some(r)) => r,
        Ok(None) => return false,
        Err(_elapsed) => {
            write_json_line(
                writer,
                serde_json::json!({
                    "ok": false,
                    "error": "timeout: app did not respond"
                }),
            )
            .await;
            return true;
        }
    };

    // Write the response back to the client.
    let resp_json = if response.ok {
        serde_json::json!({"ok": true})
    } else {
        serde_json::json!({
            "ok": false,
            "error": response.error.unwrap_or_default()
        })
    };
    write_json_line(writer, resp_json).await;
    true
}

/// Process a single IPC connection. Returns false if the app channel closed.
async fn handle_connection(
    stream: tokio::net::UnixStream,
    output: &mut iced::futures::channel::mpsc::Sender<IpcEvent>,
    resp_rx: &mut tokio::sync::mpsc::Receiver<IpcResponse>,
    response_timeout: Duration,
) -> bool {
    use tokio::io::AsyncBufReadExt;

    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = tokio::io::BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if !process_line(&line, output, resp_rx, &mut writer, response_timeout).await {
            return false;
        }
    }
    true
}

/// Bind the IPC listener and restrict the socket to its owner.
///
/// The socket is a full remote-control channel for the app, and the fallback
/// path lives in world-writable `/tmp`, so it is chmod'd to 0600 immediately
/// after bind. A failure to lock it down is fatal to the bind: the socket is
/// removed and the error propagated rather than left readable by other users.
/// (The window between `bind` and `set_permissions` is unavoidable with
/// `UnixListener::bind`; the harness mitigates it by placing the socket in a
/// 0700 per-instance directory.)
fn bind_listener(path: &std::path::Path) -> std::io::Result<tokio::net::UnixListener> {
    let listener = tokio::net::UnixListener::bind(path)?;
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        let _ = std::fs::remove_file(path);
        return Err(e);
    }
    Ok(listener)
}

fn ipc_stream() -> impl iced::futures::Stream<Item = IpcEvent> {
    iced::stream::channel(32, async |mut output| {
        use iced::futures::SinkExt;

        let path = socket_path();

        // Remove stale socket file if it exists.
        let _ = std::fs::remove_file(&path);

        let listener = match bind_listener(&path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("IPC: failed to bind {}: {e}", path.display());
                // Park forever — subscription produces no events.
                std::future::pending::<()>().await;
                unreachable!();
            }
        };

        // Create the response channel shared between subscription and app.
        let (resp_tx, mut resp_rx) = tokio::sync::mpsc::channel::<IpcResponse>(1);
        let sender = ResponseSender(Arc::new(tokio::sync::Mutex::new(resp_tx)));
        let _ = output.send(IpcEvent::Ready(sender)).await;

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    if !handle_connection(stream, &mut output, &mut resp_rx, RESPONSE_TIMEOUT).await
                    {
                        return;
                    }
                }
                Err(e) => {
                    eprintln!("IPC: accept error: {e}");
                    continue;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::app::{DocumentState, Message};
    use crate::fonts::FontRegistry;
    use crate::overlay::{PdfPosition, TextOverlay};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_registry() -> FontRegistry {
        FontRegistry::new()
    }

    fn test_document_with_overlay() -> DocumentState {
        let registry = test_registry();
        DocumentState {
            source_path: PathBuf::from("/tmp/test.pdf"),
            save_path: None,
            page_count: 1,
            current_page: 1,
            page_images: HashMap::new(),
            page_dimensions: HashMap::from([(1, (612.0, 792.0))]),
            overlays: vec![TextOverlay {
                page: 1,
                position: PdfPosition { x: 100.0, y: 700.0 },
                text: "test".to_string(),
                font: registry.default_font(),
                font_size: 12.0,
                width: Some(200.0),
            }],
        }
    }

    // --- to_message tests ---

    #[test]
    fn open_produces_file_opened() {
        let cmd = IpcCommand::Open {
            path: PathBuf::from("/tmp/test.pdf"),
        };
        let msg = cmd
            .to_message(&CommandContext::default(), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::FileOpened(p) if p == PathBuf::from("/tmp/test.pdf")));
    }

    #[test]
    fn click_produces_place_overlay_without_width() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Click {
            page: 1,
            x: 100.0,
            y: 700.0,
        };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(
            msg,
            Message::PlaceOverlay { page: 1, position: PdfPosition { x, y }, width: None }
            if (x - 100.0).abs() < f32::EPSILON && (y - 700.0).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn select_produces_select_overlay() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Select { index: 0 };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::SelectOverlay(0)));
    }

    #[test]
    fn edit_produces_edit_overlay() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Edit { index: 0 };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::EditOverlay(0)));
    }

    #[test]
    fn deselect_produces_deselect_overlay() {
        let cmd = IpcCommand::Deselect;
        let msg = cmd
            .to_message(&CommandContext::default(), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::DeselectOverlay));
    }

    #[test]
    fn zoom_in_produces_zoom_in() {
        let cmd = IpcCommand::ZoomIn;
        let msg = cmd
            .to_message(&CommandContext::default(), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::ZoomIn));
    }

    #[test]
    fn zoom_out_produces_zoom_out() {
        let cmd = IpcCommand::ZoomOut;
        let msg = cmd
            .to_message(&CommandContext::default(), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::ZoomOut));
    }

    #[test]
    fn zoom_reset_produces_zoom_reset() {
        let cmd = IpcCommand::ZoomReset;
        let msg = cmd
            .to_message(&CommandContext::default(), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::ZoomReset));
    }

    #[test]
    fn zoom_fit_width_produces_zoom_fit_width() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::ZoomFitWidth;
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::ZoomFitWidth));
    }

    #[test]
    fn font_produces_change_font() {
        let doc = test_document_with_overlay();
        let registry = test_registry();
        let courier = registry.find_by_name("Courier").unwrap();
        let cmd = IpcCommand::Font {
            family: "Courier".to_string(),
        };
        let msg = cmd
            .to_message(&context_with_document(&doc), &registry)
            .unwrap();
        assert!(matches!(msg, Message::ChangeFont(id) if id == courier));
    }

    #[test]
    fn font_unknown_name_returns_error() {
        let doc = test_document_with_overlay();
        let registry = test_registry();
        let cmd = IpcCommand::Font {
            family: "Comic Sans".to_string(),
        };
        let result = cmd.to_message(&context_with_document(&doc), &registry);
        assert!(matches!(result, Err(IpcError::UnknownFont(ref name)) if name == "Comic Sans"));
    }

    #[test]
    fn font_size_produces_change_font_size() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::FontSize { size: 18.0 };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::ChangeFontSize(s) if (s - 18.0).abs() < f32::EPSILON));
    }

    #[test]
    fn drag_produces_place_overlay_with_width() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Drag {
            page: 1,
            x1: 100.0,
            y1: 700.0,
            x2: 300.0,
            y2: 700.0,
        };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(
            msg,
            Message::PlaceOverlay { page: 1, position: PdfPosition { x, y }, width: Some(w) }
            if (x - 100.0).abs() < f32::EPSILON
                && (y - 700.0).abs() < f32::EPSILON
                && (w - 200.0).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn resize_reads_old_width_from_doc() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Resize {
            index: 0,
            width: 300.0,
        };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(
            msg,
            Message::ResizeOverlay { index: 0, old_width, new_width }
            if (old_width - 200.0).abs() < f32::EPSILON
                && (new_width - 300.0).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn resize_without_doc_returns_error() {
        let cmd = IpcCommand::Resize {
            index: 0,
            width: 300.0,
        };
        let result = cmd.to_message(&CommandContext::default(), &test_registry());
        assert!(matches!(result, Err(IpcError::NoDocument)));
    }

    #[test]
    fn resize_with_out_of_range_index_returns_error() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Resize {
            index: 99,
            width: 300.0,
        };
        let result = cmd.to_message(&context_with_document(&doc), &test_registry());
        assert!(matches!(result, Err(IpcError::IndexOutOfRange)));
    }

    #[test]
    fn resize_overlay_without_width_returns_error() {
        let mut doc = test_document_with_overlay();
        doc.overlays[0].width = None;
        let cmd = IpcCommand::Resize {
            index: 0,
            width: 300.0,
        };
        let result = cmd.to_message(&context_with_document(&doc), &test_registry());
        assert!(matches!(result, Err(IpcError::NotResizable)));
    }

    #[test]
    fn move_produces_move_overlay() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Move {
            index: 0,
            x: 150.0,
            y: 650.0,
        };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(
            msg,
            Message::MoveOverlay(0, PdfPosition { x, y })
            if (x - 150.0).abs() < f32::EPSILON && (y - 650.0).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn wait_ready_produces_noop() {
        let cmd = IpcCommand::WaitReady;
        let msg = cmd
            .to_message(&CommandContext::default(), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::Noop));
    }

    #[test]
    fn parse_open_command() {
        let json = r#"{"cmd": "open", "path": "/tmp/test.pdf"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::Open { path } if path.to_str() == Some("/tmp/test.pdf")));
    }

    #[test]
    fn parse_click_command() {
        let json = r#"{"cmd": "click", "page": 1, "x": 100.0, "y": 700.0}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(
            matches!(cmd, IpcCommand::Click { page: 1, x, y } if (x - 100.0).abs() < f32::EPSILON && (y - 700.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn parse_type_command() {
        let json = r#"{"cmd": "type", "text": "Hello"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::Type { ref text } if text == "Hello"));
    }

    #[test]
    fn parse_select_command() {
        let json = r#"{"cmd": "select", "index": 0}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::Select { index: 0 }));
    }

    #[test]
    fn parse_edit_command() {
        let json = r#"{"cmd": "edit", "index": 2}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::Edit { index: 2 }));
    }

    #[test]
    fn parse_deselect_command() {
        let json = r#"{"cmd": "deselect"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::Deselect));
    }

    #[test]
    fn parse_zoom_in_command() {
        let json = r#"{"cmd": "zoom_in"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::ZoomIn));
    }

    #[test]
    fn parse_zoom_out_command() {
        let json = r#"{"cmd": "zoom_out"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::ZoomOut));
    }

    #[test]
    fn parse_zoom_reset_command() {
        let json = r#"{"cmd": "zoom_reset"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::ZoomReset));
    }

    #[test]
    fn parse_zoom_fit_width_command() {
        let json = r#"{"cmd": "zoom_fit_width"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::ZoomFitWidth));
    }

    #[test]
    fn parse_font_command() {
        let json = r#"{"cmd": "font", "family": "Courier"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::Font { ref family } if family == "Courier"));
    }

    #[test]
    fn parse_font_size_command() {
        let json = r#"{"cmd": "font_size", "size": 14.0}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::FontSize { size } if (size - 14.0).abs() < f32::EPSILON));
    }

    #[test]
    fn parse_drag_command() {
        let json =
            r#"{"cmd": "drag", "page": 1, "x1": 100.0, "y1": 700.0, "x2": 300.0, "y2": 700.0}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::Drag { page: 1, .. }));
    }

    #[test]
    fn parse_resize_command() {
        let json = r#"{"cmd": "resize", "index": 0, "width": 200.0}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(
            matches!(cmd, IpcCommand::Resize { index: 0, width } if (width - 200.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn parse_move_command() {
        let json = r#"{"cmd": "move", "index": 0, "x": 150.0, "y": 650.0}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(
            matches!(cmd, IpcCommand::Move { index: 0, x, y } if (x - 150.0).abs() < f32::EPSILON && (y - 650.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn parse_wait_ready_command() {
        let json = r#"{"cmd": "wait_ready"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::WaitReady));
    }

    #[test]
    fn invalid_json_returns_error() {
        let result = serde_json::from_str::<IpcCommand>("not json");
        assert!(result.is_err());
    }

    #[test]
    fn unknown_command_returns_error() {
        let result = serde_json::from_str::<IpcCommand>(r#"{"cmd": "explode"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn missing_required_field_returns_error() {
        let result = serde_json::from_str::<IpcCommand>(r#"{"cmd": "click", "page": 1}"#);
        assert!(result.is_err());
    }

    #[test]
    fn socket_path_ends_with_expected_filename() {
        let path = socket_path();
        assert!(path.to_str().unwrap().ends_with("spe-ipc.sock"));
    }

    // --- precondition checks: every command reports whether it acted (spe-749) ---
    //
    // These commands used to translate unconditionally and reply ok:true while
    // the handler silently did nothing. Translation now fails fast instead.

    /// A context describing a loaded document with one overlay, nothing selected.
    fn context_with_document(doc: &DocumentState) -> CommandContext<'_> {
        CommandContext {
            document: Some(doc),
            ..CommandContext::default()
        }
    }

    #[test]
    fn click_without_document_is_rejected() {
        let cmd = IpcCommand::Click {
            page: 1,
            x: 100.0,
            y: 700.0,
        };
        let result = cmd.to_message(&CommandContext::default(), &test_registry());
        assert!(matches!(result, Err(IpcError::NoDocument)));
    }

    #[test]
    fn click_on_page_beyond_document_is_rejected() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Click {
            page: 9,
            x: 100.0,
            y: 700.0,
        };
        let result = cmd.to_message(&context_with_document(&doc), &test_registry());
        assert!(matches!(result, Err(IpcError::PageOutOfRange)));
    }

    #[test]
    fn drag_without_document_is_rejected() {
        let cmd = IpcCommand::Drag {
            page: 1,
            x1: 100.0,
            y1: 700.0,
            x2: 300.0,
            y2: 700.0,
        };
        let result = cmd.to_message(&CommandContext::default(), &test_registry());
        assert!(matches!(result, Err(IpcError::NoDocument)));
    }

    #[test]
    fn type_without_active_overlay_is_rejected() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Type {
            text: "Hello".to_string(),
        };
        let result = cmd.to_message(&context_with_document(&doc), &test_registry());
        assert!(matches!(result, Err(IpcError::NoActiveOverlay)));
    }

    #[test]
    fn type_with_stale_active_overlay_index_is_rejected() {
        let doc = test_document_with_overlay();
        let ctx = CommandContext {
            document: Some(&doc),
            active_overlay: Some(7),
            ..CommandContext::default()
        };
        let cmd = IpcCommand::Type {
            text: "Hello".to_string(),
        };
        assert!(matches!(
            cmd.to_message(&ctx, &test_registry()),
            Err(IpcError::NoActiveOverlay)
        ));
    }

    #[test]
    fn select_with_out_of_range_index_is_rejected() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Select { index: 5 };
        let result = cmd.to_message(&context_with_document(&doc), &test_registry());
        assert!(matches!(result, Err(IpcError::IndexOutOfRange)));
    }

    #[test]
    fn edit_with_out_of_range_index_is_rejected() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Edit { index: 5 };
        let result = cmd.to_message(&context_with_document(&doc), &test_registry());
        assert!(matches!(result, Err(IpcError::IndexOutOfRange)));
    }

    #[test]
    fn move_with_out_of_range_index_is_rejected() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Move {
            index: 5,
            x: 1.0,
            y: 2.0,
        };
        let result = cmd.to_message(&context_with_document(&doc), &test_registry());
        assert!(matches!(result, Err(IpcError::IndexOutOfRange)));
    }

    #[test]
    fn select_without_document_is_rejected() {
        let cmd = IpcCommand::Select { index: 0 };
        let result = cmd.to_message(&CommandContext::default(), &test_registry());
        assert!(matches!(result, Err(IpcError::NoDocument)));
    }

    #[test]
    fn font_without_document_is_rejected() {
        let cmd = IpcCommand::Font {
            family: "Courier".to_string(),
        };
        let result = cmd.to_message(&CommandContext::default(), &test_registry());
        assert!(matches!(result, Err(IpcError::NoDocument)));
    }

    #[test]
    fn font_size_without_document_is_rejected() {
        let cmd = IpcCommand::FontSize { size: 18.0 };
        let result = cmd.to_message(&CommandContext::default(), &test_registry());
        assert!(matches!(result, Err(IpcError::NoDocument)));
    }

    #[test]
    fn zoom_fit_width_without_document_is_rejected() {
        let cmd = IpcCommand::ZoomFitWidth;
        let result = cmd.to_message(&CommandContext::default(), &test_registry());
        assert!(matches!(result, Err(IpcError::NoDocument)));
    }

    #[test]
    fn type_with_active_overlay_produces_update_overlay_text() {
        let doc = test_document_with_overlay();
        let ctx = CommandContext {
            document: Some(&doc),
            active_overlay: Some(0),
            ..CommandContext::default()
        };
        let cmd = IpcCommand::Type {
            text: "Hello".to_string(),
        };
        let msg = cmd.to_message(&ctx, &test_registry()).unwrap();
        assert!(matches!(msg, Message::UpdateOverlayText(ref t) if t == "Hello"));
    }

    // --- click_at: routed through the canvas hit test (spe-7f1) ---

    #[test]
    fn click_at_on_empty_space_places_an_overlay() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::ClickAt {
            page: 1,
            x: 300.0,
            y: 300.0,
        };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(
            msg,
            Message::PlaceOverlay { page: 1, position: PdfPosition { x, y }, width: None }
            if (x - 300.0).abs() < f32::EPSILON && (y - 300.0).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn click_at_on_an_existing_overlay_selects_it() {
        let doc = test_document_with_overlay();
        // Just inside the existing overlay's bounding box at (100, 700).
        let cmd = IpcCommand::ClickAt {
            page: 1,
            x: 102.0,
            y: 705.0,
        };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::SelectOverlay(0)));
    }

    #[test]
    fn click_at_outside_the_page_deselects() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::ClickAt {
            page: 1,
            x: 900.0,
            y: 900.0,
        };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::DeselectOverlay));
    }

    #[test]
    fn click_at_on_a_page_of_unknown_size_places_instead_of_deselecting() {
        // Page dimensions are read when a document loads, but a click can
        // arrive before that; a page of unknown size cannot be shown to have
        // been missed, so the click still places rather than deselecting.
        let mut doc = test_document_with_overlay();
        doc.page_dimensions.clear();
        let cmd = IpcCommand::ClickAt {
            page: 1,
            x: 5000.0,
            y: 5000.0,
        };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(msg, Message::PlaceOverlay { page: 1, .. }));
    }

    #[test]
    fn click_at_while_editing_commits_the_text_first() {
        let doc = test_document_with_overlay();
        let ctx = CommandContext {
            document: Some(&doc),
            active_overlay: Some(0),
            editing: true,
            ..CommandContext::default()
        };
        let cmd = IpcCommand::ClickAt {
            page: 1,
            x: 300.0,
            y: 300.0,
        };
        let msg = cmd.to_message(&ctx, &test_registry()).unwrap();
        assert!(matches!(msg, Message::CommitText));
    }

    #[test]
    fn click_at_without_document_is_rejected() {
        let cmd = IpcCommand::ClickAt {
            page: 1,
            x: 1.0,
            y: 1.0,
        };
        let result = cmd.to_message(&CommandContext::default(), &test_registry());
        assert!(matches!(result, Err(IpcError::NoDocument)));
    }

    #[test]
    fn parse_click_at_command() {
        let json = r#"{"cmd": "click_at", "page": 1, "x": 100.0, "y": 700.0}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::ClickAt { page: 1, .. }));
    }

    // --- save (spe-94g) ---

    #[test]
    fn save_produces_save_destination_chosen() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Save {
            path: PathBuf::from("/tmp/out.pdf"),
        };
        let msg = cmd
            .to_message(&context_with_document(&doc), &test_registry())
            .unwrap();
        assert!(matches!(
            msg,
            Message::SaveDestinationChosen(p) if p == PathBuf::from("/tmp/out.pdf")
        ));
    }

    #[test]
    fn save_without_document_is_rejected() {
        let cmd = IpcCommand::Save {
            path: PathBuf::from("/tmp/out.pdf"),
        };
        let result = cmd.to_message(&CommandContext::default(), &test_registry());
        assert!(matches!(result, Err(IpcError::NoDocument)));
    }

    #[test]
    fn parse_save_command() {
        let json = r#"{"cmd": "save", "path": "/tmp/out.pdf"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::Save { path } if path.to_str() == Some("/tmp/out.pdf")));
    }

    // --- undo / redo (spe-0nc) ---

    #[test]
    fn undo_produces_undo() {
        let doc = test_document_with_overlay();
        let ctx = CommandContext {
            document: Some(&doc),
            undo_depth: 1,
            ..CommandContext::default()
        };
        let msg = IpcCommand::Undo.to_message(&ctx, &test_registry()).unwrap();
        assert!(matches!(msg, Message::Undo));
    }

    #[test]
    fn undo_with_empty_stack_is_rejected() {
        let doc = test_document_with_overlay();
        let result = IpcCommand::Undo.to_message(&context_with_document(&doc), &test_registry());
        assert!(matches!(result, Err(IpcError::NothingToUndo)));
    }

    #[test]
    fn undo_with_an_edit_session_is_allowed_even_with_an_empty_stack() {
        // Undo cancels an in-progress edit before it touches the command
        // history, so an edit session is on its own something to undo.
        let doc = test_document_with_overlay();
        let ctx = CommandContext {
            document: Some(&doc),
            active_overlay: Some(0),
            editing: true,
            ..CommandContext::default()
        };
        let msg = IpcCommand::Undo.to_message(&ctx, &test_registry()).unwrap();
        assert!(matches!(msg, Message::Undo));
    }

    #[test]
    fn redo_produces_redo() {
        let doc = test_document_with_overlay();
        let ctx = CommandContext {
            document: Some(&doc),
            redo_depth: 1,
            ..CommandContext::default()
        };
        let msg = IpcCommand::Redo.to_message(&ctx, &test_registry()).unwrap();
        assert!(matches!(msg, Message::Redo));
    }

    #[test]
    fn redo_with_empty_stack_is_rejected() {
        let doc = test_document_with_overlay();
        let result = IpcCommand::Redo.to_message(&context_with_document(&doc), &test_registry());
        assert!(matches!(result, Err(IpcError::NothingToRedo)));
    }

    #[test]
    fn parse_undo_command() {
        let cmd: IpcCommand = serde_json::from_str(r#"{"cmd": "undo"}"#).unwrap();
        assert!(matches!(cmd, IpcCommand::Undo));
    }

    #[test]
    fn parse_redo_command() {
        let cmd: IpcCommand = serde_json::from_str(r#"{"cmd": "redo"}"#).unwrap();
        assert!(matches!(cmd, IpcCommand::Redo));
    }

    // --- socket permissions (spe-85p) ---

    #[test]
    fn bind_listener_restricts_socket_to_owner() {
        use std::os::unix::fs::PermissionsExt;
        run_async(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("spe-ipc.sock");
            let _listener = bind_listener(&path).expect("bind should succeed in a writable dir");
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o777,
                0o600,
                "the IPC control socket must not be reachable by other users"
            );
        });
    }

    #[test]
    fn bind_listener_reports_error_when_socket_cannot_be_created() {
        run_async(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("no-such-subdir").join("spe-ipc.sock");
            let err = bind_listener(&path).expect_err("bind into a missing directory must fail");
            assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        });
    }

    // --- process_line robustness (async) ---
    //
    // These exercise the bounded-wait behavior that keeps a non-responding
    // command from wedging the accept loop (spe-z6v). Each test injects a short
    // response timeout and wraps the call in an outer timeout so a regression
    // fails the test instead of hanging the suite.
    //
    // Production drives these async fns on iced's executor; tests build a
    // current-thread tokio runtime since the IPC types need a tokio reactor.

    /// Run an async block to completion on a current-thread tokio runtime.
    fn run_async<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(fut)
    }

    /// The channels and socket halves a `process_line` call needs, in the order
    /// `(output_tx, output_rx, resp_tx, resp_rx, writer, client)`.
    type ProcessLineHarness = (
        iced::futures::channel::mpsc::Sender<IpcEvent>,
        iced::futures::channel::mpsc::Receiver<IpcEvent>,
        tokio::sync::mpsc::Sender<IpcResponse>,
        tokio::sync::mpsc::Receiver<IpcResponse>,
        tokio::io::WriteHalf<tokio::net::UnixStream>,
        tokio::net::UnixStream,
    );

    /// Build the channels and socket halves a `process_line` call needs.
    /// The caller must keep `output_rx` alive so `output.send` never fails on a
    /// closed channel; the emitted events are irrelevant to these tests.
    /// `client` is the peer socket end used to read what was written back.
    fn process_line_harness() -> ProcessLineHarness {
        let (output_tx, output_rx) = iced::futures::channel::mpsc::channel::<IpcEvent>(32);
        let (resp_tx, resp_rx) = tokio::sync::mpsc::channel::<IpcResponse>(1);
        let (client, server) = tokio::net::UnixStream::pair().unwrap();
        let (_reader, writer) = tokio::io::split(server);
        (output_tx, output_rx, resp_tx, resp_rx, writer, client)
    }

    async fn read_reply(client: &mut tokio::net::UnixStream) -> serde_json::Value {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 256];
        let n = client.read(&mut buf).await.unwrap();
        serde_json::from_slice(&buf[..n]).unwrap()
    }

    #[test]
    fn process_line_times_out_when_no_response_arrives() {
        run_async(async {
            // `_resp_tx` must stay bound: dropping it closes the channel, which
            // would send `process_line` down the `Ok(None)` shutdown path
            // instead of the timeout path this test exercises.
            let (mut output, _output_rx, _resp_tx, mut resp_rx, mut writer, mut client) =
                process_line_harness();

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                process_line(
                    r#"{"cmd":"deselect"}"#,
                    &mut output,
                    &mut resp_rx,
                    &mut writer,
                    std::time::Duration::from_millis(50),
                ),
            )
            .await
            .expect("process_line must return on timeout, not wedge forever");

            // Returning true keeps the accept loop alive for later connections.
            assert!(result);

            let reply = read_reply(&mut client).await;
            assert_eq!(reply["ok"], false);
            assert!(
                reply["error"].as_str().unwrap().contains("timeout"),
                "expected a timeout error, got: {reply}"
            );
        });
    }

    #[test]
    fn process_line_discards_stale_response_from_prior_command() {
        run_async(async {
            let (mut output, _output_rx, resp_tx, mut resp_rx, mut writer, mut client) =
                process_line_harness();

            // A late response from a previously timed-out command is buffered in
            // the shared channel. It must not be mismatched to this command.
            resp_tx
                .send(IpcResponse {
                    ok: true,
                    error: None,
                })
                .await
                .unwrap();

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                process_line(
                    r#"{"cmd":"deselect"}"#,
                    &mut output,
                    &mut resp_rx,
                    &mut writer,
                    std::time::Duration::from_millis(50),
                ),
            )
            .await
            .expect("process_line must return, not wedge");

            assert!(result);

            // The stale ok:true was drained; with no fresh response this command
            // times out rather than reporting the previous command's success.
            let reply = read_reply(&mut client).await;
            assert_eq!(
                reply["ok"], false,
                "stale response leaked to the next command: {reply}"
            );
        });
    }

    #[test]
    fn process_line_writes_ok_when_response_arrives() {
        run_async(async {
            let (mut output, _output_rx, resp_tx, mut resp_rx, mut writer, mut client) =
                process_line_harness();

            // Respond promptly, after process_line drains and sends its event.
            tokio::spawn(async move {
                resp_tx
                    .send(IpcResponse {
                        ok: true,
                        error: None,
                    })
                    .await
                    .unwrap();
            });

            let result = process_line(
                r#"{"cmd":"deselect"}"#,
                &mut output,
                &mut resp_rx,
                &mut writer,
                std::time::Duration::from_secs(5),
            )
            .await;

            assert!(result);

            let reply = read_reply(&mut client).await;
            assert_eq!(reply["ok"], true);
        });
    }
}
