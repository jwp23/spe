# Screenshot Tool

A development tool for Claude Code to take screenshots of the running app and drive interactions programmatically. Not a user-facing feature — requires explicit `--ipc` flag.

## Prerequisites

| Tool | Version | Install (Arch) | Purpose |
|------|---------|----------------|---------|
| cage | any | `pacman -S cage` | Headless kiosk Wayland compositor |
| grim | any | `pacman -S grim` | Wayland-native screenshot capture |
| socat | any | `pacman -S socat` | Unix socket client for IPC commands |

The harness script checks for these and prints install instructions if any are missing.

## Quick Start

```bash
# Build and start the app in a headless compositor
scripts/screenshot.sh start

# Load a PDF and wait for it to render
scripts/screenshot.sh send '{"cmd": "open", "path": "tests/fixtures/two-page.pdf"}'
scripts/screenshot.sh send '{"cmd": "wait_ready"}'

# Drive the UI
scripts/screenshot.sh send '{"cmd": "click", "page": 1, "x": 100, "y": 700}'
scripts/screenshot.sh send '{"cmd": "type", "text": "Hello world"}'
scripts/screenshot.sh send '{"cmd": "deselect"}'

# Capture and view
scripts/screenshot.sh capture screenshots/overlay-test.png

# Tear down
scripts/screenshot.sh stop
```

## Harness Script

`scripts/screenshot.sh` manages the full lifecycle.

| Command | Description |
|---------|-------------|
| `start` | Build app, start cage compositor, launch `spe --ipc`, wait for IPC socket |
| `stop` | Kill app and cage, remove IPC socket |
| `send '<json>'` | Send one IPC command over the Unix socket |
| `capture [path]` | Screenshot with grim (default: `screenshots/latest.png`) |

The `screenshots/` directory is gitignored — screenshots are ephemeral development artifacts.

## The `--ipc` Flag

Start the app with `--ipc` to enable the IPC subscription:

```bash
spe --ipc
```

Without this flag, no socket is created and no subscription runs. The harness script passes `--ipc` automatically. There is no compile-time feature gate — a single binary serves both uses.

The Unix socket is created at `$XDG_RUNTIME_DIR/spe-ipc.sock` (fallback: `/tmp/spe-ipc.sock`).

## IPC Command Protocol

Newline-delimited JSON over a Unix socket. Commands use PDF coordinates, not screen coordinates — resolution- and zoom-independent.

Every command returns a JSON response:
- `{"ok": true}` on success
- `{"ok": false, "error": "description"}` on failure

### Commands

| Command | JSON |
|---------|------|
| Open PDF | `{"cmd": "open", "path": "/path/to.pdf"}` |
| Click canvas | `{"cmd": "click", "page": 1, "x": 100.0, "y": 700.0}` |
| Drag (multiline) | `{"cmd": "drag", "page": 1, "x1": 100.0, "y1": 700.0, "x2": 300.0, "y2": 700.0}` |
| Type text | `{"cmd": "type", "text": "Hello"}` |
| Select overlay | `{"cmd": "select", "index": 0}` |
| Edit overlay | `{"cmd": "edit", "index": 0}` |
| Deselect | `{"cmd": "deselect"}` |
| Move overlay | `{"cmd": "move", "index": 0, "x": 150.0, "y": 650.0}` |
| Resize overlay | `{"cmd": "resize", "index": 0, "width": 200.0}` |
| Change font | `{"cmd": "font", "family": "Helvetica"}` |
| Change font size | `{"cmd": "font_size", "size": 14.0}` |
| Zoom in | `{"cmd": "zoom_in"}` |
| Zoom out | `{"cmd": "zoom_out"}` |
| Zoom reset | `{"cmd": "zoom_reset"}` |
| Zoom fit width | `{"cmd": "zoom_fit_width"}` |
| Wait for idle | `{"cmd": "wait_ready"}` |

### Font Family Values

Valid values for the `font` command's `family` field:

`Courier`, `CourierBold`, `CourierOblique`, `CourierBoldOblique`, `Helvetica`, `HelveticaBold`, `HelveticaOblique`, `HelveticaBoldOblique`, `TimesRoman`, `TimesBold`, `TimesItalic`, `TimesBoldItalic`, `Symbol`, `ZapfDingbats`
