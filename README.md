# spe — PDF Text Overlay Editor

A desktop application for Linux that opens PDF documents, renders pages visually, and lets you click anywhere on a page to place text overlays. Select font family and size, then save the result as a new PDF with text baked in.

Built with Rust and Iced, optimized for Cosmic Desktop on Wayland.

## Prerequisites

| Tool | Version | Install (Arch) | Purpose |
|------|---------|----------------|---------|
| Rust | 1.88+ | `pacman -S rust` | Build toolchain |
| pdftoppm | any | `pacman -S poppler` | PDF page rendering |

## Quick Start

```bash
git clone git@github.com:jwp23/spe.git
cd spe
cargo build
cargo run
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+O | Open file |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save as |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Ctrl+Plus | Zoom in |
| Ctrl+Minus | Zoom out |
| Delete | Delete selected overlay |
| Escape | Deselect overlay |
| Page Up / Page Down | Previous / next page |
| F9 | Toggle thumbnail sidebar |

## Development

```bash
cargo fmt --check            # check formatting
cargo clippy -- -D warnings  # lint
cargo test                   # unit + integration tests
cargo test -- --ignored      # E2E tests (requires GPU context)
```

Pre-commit hooks run secrets scanning, fmt, clippy, and tests automatically.

### Visual Debugging (Claude Code)

A screenshot tool lets Claude Code take screenshots of the running app to verify visual output. It uses `cage` (headless Wayland compositor), `grim` (screenshot capture), and `socat` (IPC), and requires starting the app with `--ipc`.

See [docs/screenshot-tool.md](docs/screenshot-tool.md) for system dependencies, harness script usage, and the full IPC command reference.

## Project Structure

Key modules in `src/`:

| Module | Purpose |
|--------|---------|
| `app/` | Iced application state, Message enum, update/view/subscription, event handlers |
| `ui/canvas/` | PDF canvas rendering, hit testing, zoom, overlay drawing |
| `ui/sidebar.rs` | Thumbnail sidebar with drag-resize |
| `ui/toolbar.rs` | Font picker, zoom controls, page navigation |
| `pdf/` | PDF rendering (`pdftoppm` wrapper) and writing (`lopdf` overlay embedding) |
| `overlay.rs` | Text overlay data model (position, font, text, width) |
| `coordinate.rs` | Screen-to-PDF coordinate conversion, AFM font width tables |
| `ipc.rs` | IPC protocol for the screenshot development tool |
| `command.rs` | Undo/redo command pattern |

Tests live in `tests/` (integration/E2E) and co-located `#[cfg(test)]` modules (unit).

## Phosphor Icon Font (contributors only)

The subsetted font is already committed — you do **not** need these tools to build or run the app. This section is only for regenerating the subset after changing which icons are included:

```bash
pip install fonttools  # or: pipx install fonttools
pyftsubset Phosphor.ttf \
  --unicodes="U+E036,U+E038,U+E08A,U+E138,U+E13A,U+E248,U+E256,U+E30C,U+E30E,U+E310,U+EAB6,U+E4A6" \
  --output-file=assets/phosphor-subset.ttf \
  --no-hinting --desubroutinize
```

## Architecture Decisions

Recorded in `docs/adr/`. Key decisions:

- **Rust** for zero-overhead performance and native Iced integration
- **Iced 0.14** as the GUI framework (Cosmic Desktop's native toolkit)
- **pdftoppm** for PDF rendering, **lopdf** for writing text into existing PDFs
- **Standard 14 PDF fonts** only (no system font embedding in v1)
- **Command pattern** for unlimited undo/redo
- **Trait-based wrappers** around system utilities for testability
