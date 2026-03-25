// IPC protocol: command parsing, command-to-Message translation.

use std::path::PathBuf;

use serde::Deserialize;

use crate::overlay::Standard14Font;

/// A command received over the IPC socket.
#[derive(Debug, Deserialize)]
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
        family: Standard14Font,
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(matches!(
            cmd,
            IpcCommand::Font {
                family: crate::overlay::Standard14Font::Courier
            }
        ));
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
}
