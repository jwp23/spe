// IPC protocol: command parsing, command-to-Message translation, subscription.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;

use crate::app::{DocumentState, Message};
use crate::fonts::FontRegistry;
use crate::overlay::PdfPosition;

/// Errors that can occur when translating an IpcCommand to a Message.
#[derive(Debug, PartialEq)]
pub enum IpcError {
    /// The command requires a loaded document but none is present.
    NoDocument,
    /// The overlay index is out of range for the current document.
    IndexOutOfRange,
    /// The targeted overlay has no width and cannot be resized.
    NotResizable,
    /// The font name could not be resolved in the registry.
    UnknownFont(String),
}

impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IpcError::NoDocument => write!(f, "no document is loaded"),
            IpcError::IndexOutOfRange => write!(f, "overlay index is out of range"),
            IpcError::NotResizable => write!(f, "overlay is not resizable (no width set)"),
            IpcError::UnknownFont(name) => write!(f, "unknown font: {name}"),
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
    Click {
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
    WaitReady,
}

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
    /// `doc` must be `Some` for commands that need to read current overlay state
    /// (e.g. `Resize`, which reads the old width from the document).
    pub fn to_message(
        self,
        doc: Option<&DocumentState>,
        registry: &FontRegistry,
    ) -> Result<Message, IpcError> {
        match self {
            IpcCommand::Open { path } => Ok(Message::FileOpened(path)),
            IpcCommand::Click { page, x, y } => Ok(Message::PlaceOverlay {
                page,
                position: PdfPosition { x, y },
                width: None,
            }),
            IpcCommand::Type { text } => Ok(Message::UpdateOverlayText(text)),
            IpcCommand::Select { index } => Ok(Message::SelectOverlay(index)),
            IpcCommand::Edit { index } => Ok(Message::EditOverlay(index)),
            IpcCommand::Deselect => Ok(Message::DeselectOverlay),
            IpcCommand::ZoomIn => Ok(Message::ZoomIn),
            IpcCommand::ZoomOut => Ok(Message::ZoomOut),
            IpcCommand::ZoomReset => Ok(Message::ZoomReset),
            IpcCommand::ZoomFitWidth => Ok(Message::ZoomFitWidth),
            IpcCommand::Font { family } => {
                let id = registry
                    .find_by_name(&family)
                    .ok_or(IpcError::UnknownFont(family))?;
                Ok(Message::ChangeFont(id))
            }
            IpcCommand::FontSize { size } => Ok(Message::ChangeFontSize(size)),
            IpcCommand::Drag {
                page,
                x1,
                y1,
                x2,
                y2: _,
            } => Ok(Message::PlaceOverlay {
                page,
                position: PdfPosition { x: x1, y: y1 },
                width: Some((x2 - x1).abs()),
            }),
            IpcCommand::Resize { index, width } => {
                let doc = doc.ok_or(IpcError::NoDocument)?;
                let overlay = doc.overlays.get(index).ok_or(IpcError::IndexOutOfRange)?;
                let old_width = overlay.width.ok_or(IpcError::NotResizable)?;
                Ok(Message::ResizeOverlay {
                    index,
                    old_width,
                    new_width: width,
                })
            }
            IpcCommand::Move { index, x, y } => {
                Ok(Message::MoveOverlay(index, PdfPosition { x, y }))
            }
            IpcCommand::WaitReady => Ok(Message::Noop),
        }
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

fn ipc_stream() -> impl iced::futures::Stream<Item = IpcEvent> {
    iced::stream::channel(32, async |mut output| {
        use iced::futures::SinkExt;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::UnixListener;

        let path = socket_path();

        // Remove stale socket file if it exists.
        let _ = std::fs::remove_file(&path);

        let listener = match UnixListener::bind(&path) {
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
            let (stream, _addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("IPC: accept error: {e}");
                    continue;
                }
            };

            let (reader, mut writer) = tokio::io::split(stream);
            let mut lines = BufReader::new(reader).lines();

            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                // Try to parse the command.
                let cmd: IpcCommand = match serde_json::from_str(&line) {
                    Ok(c) => c,
                    Err(e) => {
                        let err_json = serde_json::json!({
                            "ok": false,
                            "error": format!("parse error: {e}")
                        });
                        let mut resp = err_json.to_string();
                        resp.push('\n');
                        let _ = writer.write_all(resp.as_bytes()).await;
                        continue;
                    }
                };

                // Yield the appropriate event to the app.
                let is_wait_ready = matches!(cmd, IpcCommand::WaitReady);
                if is_wait_ready {
                    let _ = output.send(IpcEvent::WaitReady).await;
                } else {
                    let _ = output.send(IpcEvent::Command(cmd)).await;
                }

                // Wait for the app to process and send a response.
                let response = match resp_rx.recv().await {
                    Some(r) => r,
                    None => {
                        // Channel closed — app shut down.
                        return;
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
                let mut resp_str = resp_json.to_string();
                resp_str.push('\n');
                let _ = writer.write_all(resp_str.as_bytes()).await;
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
            page_dimensions: HashMap::new(),
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
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(msg, Message::FileOpened(p) if p == PathBuf::from("/tmp/test.pdf")));
    }

    #[test]
    fn click_produces_place_overlay_without_width() {
        let cmd = IpcCommand::Click {
            page: 1,
            x: 100.0,
            y: 700.0,
        };
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(
            msg,
            Message::PlaceOverlay { page: 1, position: PdfPosition { x, y }, width: None }
            if (x - 100.0).abs() < f32::EPSILON && (y - 700.0).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn type_produces_update_overlay_text() {
        let cmd = IpcCommand::Type {
            text: "Hello".to_string(),
        };
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(msg, Message::UpdateOverlayText(ref t) if t == "Hello"));
    }

    #[test]
    fn select_produces_select_overlay() {
        let cmd = IpcCommand::Select { index: 2 };
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(msg, Message::SelectOverlay(2)));
    }

    #[test]
    fn edit_produces_edit_overlay() {
        let cmd = IpcCommand::Edit { index: 3 };
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(msg, Message::EditOverlay(3)));
    }

    #[test]
    fn deselect_produces_deselect_overlay() {
        let cmd = IpcCommand::Deselect;
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(msg, Message::DeselectOverlay));
    }

    #[test]
    fn zoom_in_produces_zoom_in() {
        let cmd = IpcCommand::ZoomIn;
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(msg, Message::ZoomIn));
    }

    #[test]
    fn zoom_out_produces_zoom_out() {
        let cmd = IpcCommand::ZoomOut;
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(msg, Message::ZoomOut));
    }

    #[test]
    fn zoom_reset_produces_zoom_reset() {
        let cmd = IpcCommand::ZoomReset;
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(msg, Message::ZoomReset));
    }

    #[test]
    fn zoom_fit_width_produces_zoom_fit_width() {
        let cmd = IpcCommand::ZoomFitWidth;
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(msg, Message::ZoomFitWidth));
    }

    #[test]
    fn font_produces_change_font() {
        let registry = test_registry();
        let courier = registry.find_by_name("Courier").unwrap();
        let cmd = IpcCommand::Font {
            family: "Courier".to_string(),
        };
        let msg = cmd.to_message(None, &registry).unwrap();
        assert!(matches!(msg, Message::ChangeFont(id) if id == courier));
    }

    #[test]
    fn font_unknown_name_returns_error() {
        let registry = test_registry();
        let cmd = IpcCommand::Font {
            family: "Comic Sans".to_string(),
        };
        let result = cmd.to_message(None, &registry);
        assert!(matches!(result, Err(IpcError::UnknownFont(ref name)) if name == "Comic Sans"));
    }

    #[test]
    fn font_size_produces_change_font_size() {
        let cmd = IpcCommand::FontSize { size: 18.0 };
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(msg, Message::ChangeFontSize(s) if (s - 18.0).abs() < f32::EPSILON));
    }

    #[test]
    fn drag_produces_place_overlay_with_width() {
        let cmd = IpcCommand::Drag {
            page: 1,
            x1: 100.0,
            y1: 700.0,
            x2: 300.0,
            y2: 700.0,
        };
        let msg = cmd.to_message(None, &test_registry()).unwrap();
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
        let msg = cmd.to_message(Some(&doc), &test_registry()).unwrap();
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
        let result = cmd.to_message(None, &test_registry());
        assert!(matches!(result, Err(IpcError::NoDocument)));
    }

    #[test]
    fn resize_with_out_of_range_index_returns_error() {
        let doc = test_document_with_overlay();
        let cmd = IpcCommand::Resize {
            index: 99,
            width: 300.0,
        };
        let result = cmd.to_message(Some(&doc), &test_registry());
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
        let result = cmd.to_message(Some(&doc), &test_registry());
        assert!(matches!(result, Err(IpcError::NotResizable)));
    }

    #[test]
    fn move_produces_move_overlay() {
        let cmd = IpcCommand::Move {
            index: 1,
            x: 150.0,
            y: 650.0,
        };
        let msg = cmd.to_message(None, &test_registry()).unwrap();
        assert!(matches!(
            msg,
            Message::MoveOverlay(1, PdfPosition { x, y })
            if (x - 150.0).abs() < f32::EPSILON && (y - 650.0).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn wait_ready_produces_noop() {
        let cmd = IpcCommand::WaitReady;
        let msg = cmd.to_message(None, &test_registry()).unwrap();
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
}
