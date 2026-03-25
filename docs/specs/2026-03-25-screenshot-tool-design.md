# Screenshot Tool Design

A development tool that lets Claude Code take screenshots of the running app to verify visual output during development. Uses a headless Wayland compositor, IPC for interaction, and Wayland-native screenshot capture.

## Problem

Claude Code writes UI code blind. There is no way to see what the Iced GUI actually renders. Visual bugs, layout issues, and z-order problems can only be caught by the human reviewing the running app. A screenshot tool closes this feedback loop.

## Architecture

Three layers:

```
Claude Code
  │  Runs harness script, reads screenshot PNGs
  │
Harness script (scripts/screenshot.sh)
  │  Manages cage compositor, app lifecycle, grim capture
  │
App (spe --ipc)
     Unix socket subscription translates JSON → Message variants
```

### Headless Compositor

The app runs inside `cage`, a kiosk Wayland compositor, on a dedicated `WAYLAND_DISPLAY` socket (`wayland-spe-test`). This renders the full Iced UI with wgpu — identical to what the user sees — without appearing on the user's screen.

System dependencies: `cage`, `grim` (both available in Arch/CachyOS repos).

### Screenshot Capture

`grim` captures the compositor output directly. This is a Wayland-native screenshot — it captures exactly what the compositor rendered, including any wgpu-specific behavior. The app does not need to participate in screenshot capture.

### IPC Channel

A Unix socket at `$XDG_RUNTIME_DIR/spe-ipc.sock` (fallback: `/tmp/spe-ipc-$UID.sock`). Activated only when the app is started with the `--ipc` CLI flag. Without the flag, no socket is created and no subscription runs.

## IPC Protocol

Newline-delimited JSON over Unix socket. Each command maps to existing `Message` variants.

### Commands

| Command | JSON | Maps to |
|---------|------|---------|
| Open PDF | `{"cmd": "open", "path": "/path/to.pdf"}` | File open flow |
| Click canvas | `{"cmd": "click", "page": 1, "x": 100.0, "y": 700.0}` | `PlaceOverlay` |
| Type text | `{"cmd": "type", "text": "Hello"}` | `OverlayTextChanged` |
| Select overlay | `{"cmd": "select", "index": 0}` | `EditOverlay` |
| Deselect | `{"cmd": "deselect"}` | `DeselectOverlay` |
| Set zoom | `{"cmd": "zoom", "level": 1.5}` | Zoom messages |
| Change font | `{"cmd": "font", "family": "Courier"}` | `FontFamilyChanged` |
| Change font size | `{"cmd": "font_size", "size": 14.0}` | `FontSizeChanged` |
| Drag (multiline) | `{"cmd": "drag", "page": 1, "x1": 100.0, "y1": 700.0, "x2": 300.0, "y2": 700.0}` | `PlaceOverlay` with width |
| Resize overlay | `{"cmd": "resize", "index": 0, "width": 200.0}` | `ResizeOverlay` |
| Move overlay | `{"cmd": "move", "index": 0, "x": 150.0, "y": 650.0}` | `MoveOverlay` |
| Wait for idle | `{"cmd": "wait_ready"}` | Blocks until no render tasks in flight |

### Responses

Every command receives a JSON response:

- `{"ok": true}` on success
- `{"ok": false, "error": "description"}` on failure

### Design Principles

- **PDF coordinates, not screen coordinates.** Commands are resolution/zoom-independent. The app translates to screen space internally.
- **Existing Message variants only.** The IPC layer is a thin translation shim. The state machine under test is identical to production.
- **Runtime activation only.** The `--ipc` flag enables the subscription. No flag means no socket, no overhead, no attack surface. No compile-time feature gate — a single binary serves both uses.

## Harness Script

`scripts/screenshot.sh` manages the full lifecycle.

### Usage

```bash
scripts/screenshot.sh start              # Build, start cage, launch app
scripts/screenshot.sh capture [path]     # Screenshot (default: screenshots/latest.png)
scripts/screenshot.sh send '<json>'      # Send IPC command
scripts/screenshot.sh stop               # Tear down everything
```

### Startup Sequence

1. Check dependencies (`cage`, `grim`) — fail with install instructions if missing
2. Build the app (`cargo build`)
3. Start `cage` with `WAYLAND_DISPLAY=wayland-spe-test`
4. Launch `spe --ipc` inside cage
5. Wait for IPC socket to appear (with timeout)
6. Print "ready"

### Teardown

Kill the app, kill cage, remove the IPC socket. Registered as a trap so Ctrl+C also cleans up.

### Screenshot Output

Default path: `screenshots/latest.png`. The `screenshots/` directory is gitignored — screenshots are ephemeral development artifacts.

## App Changes

### New Module: `src/ipc.rs`

Two components:

**Subscription:** An Iced `Subscription` that creates the Unix socket, accepts connections, reads newline-delimited JSON, parses into `IpcCommand`, maps to `Message`, and writes back the response.

**Command parsing:** A `serde::Deserialize` enum mapping the JSON protocol to typed commands:

```
IpcCommand::Open { path }
IpcCommand::Click { page, x, y }
IpcCommand::Type { text }
IpcCommand::Select { index }
IpcCommand::Deselect
IpcCommand::Zoom { level }
IpcCommand::Font { family }
IpcCommand::FontSize { size }
IpcCommand::Drag { page, x1, y1, x2, y2 }
IpcCommand::Resize { index, width }
IpcCommand::Move { index, x, y }
IpcCommand::WaitReady
```

### App State Changes

- `App.ipc_enabled: bool` — set from `--ipc` CLI arg
- `App::subscription()` — returns the IPC subscription when `ipc_enabled` is true, `Subscription::none()` otherwise

### CLI Argument

`--ipc` flag parsed in `main.rs`. No other CLI changes.

## Dependencies

### System (must be installed)

- `cage` — headless kiosk Wayland compositor
- `grim` — Wayland screenshot utility

### Rust Crates (new)

- `serde_json` — JSON parsing for IPC commands (may already be a transitive dependency)
- `clap` or manual `std::env::args` — CLI argument parsing (evaluate complexity before choosing)

## Usage Workflow

```bash
# Start harness
scripts/screenshot.sh start

# Load test PDF and wait for render
scripts/screenshot.sh send '{"cmd": "open", "path": "tests/fixtures/two-page.pdf"}'
scripts/screenshot.sh send '{"cmd": "wait_ready"}'

# Set up scenario
scripts/screenshot.sh send '{"cmd": "click", "page": 1, "x": 100, "y": 700}'
scripts/screenshot.sh send '{"cmd": "type", "text": "Hello world"}'
scripts/screenshot.sh send '{"cmd": "deselect"}'

# Capture and view
scripts/screenshot.sh capture screenshots/overlay-test.png
# Claude Code reads the PNG with its Read tool
```

## Test Fixtures

Small, predictable PDFs in `tests/fixtures/` for use with the screenshot tool. Single page, multi-page, different sizes. These may already exist from integration tests.

## What This Tool Does NOT Do

- No automated visual regression testing (no reference images, no diff comparison)
- No CI integration (local development tool only)
- No remote access (Unix socket only)
- No production use (requires explicit `--ipc` flag)
