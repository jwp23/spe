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

## Project Structure

```
src/
├── main.rs           # entry point — launches Iced application
├── app.rs            # App struct, Message enum, update/view/subscription
├── command.rs        # undo/redo Command enum with apply/reverse
├── config.rs         # AppConfig with overlay color, font/size defaults
├── coordinate.rs     # screen <-> PDF coordinate conversion, AFM width tables
├── overlay.rs        # TextOverlay, PdfPosition, Standard14Font
├── pdf/
│   ├── mod.rs        # page_dimensions() helper
│   ├── renderer.rs   # pdftoppm wrapper for page rendering
│   └── writer.rs     # lopdf wrapper for text overlay writing
└── ui/
    ├── canvas.rs     # canvas state, hit testing, zoom helpers
    ├── icons.rs      # Phosphor icon font constants and loading
    ├── sidebar.rs    # thumbnail sidebar state and helpers
    └── toolbar.rs    # toolbar view with font picker, zoom, page nav
assets/
└── phosphor-subset.ttf  # subsetted Phosphor Icons (~12 glyphs, 3KB)
tests/
├── e2e.rs            # E2E tests with iced_test Simulator
├── pdf_rendering.rs  # integration tests for pdftoppm rendering
└── pdf_writing.rs    # integration tests for PDF overlay writing
```

## Phosphor Icon Font

The toolbar uses a subsetted [Phosphor Icons](https://phosphoricons.com/) TTF bundled at `assets/phosphor-subset.ttf`. To regenerate after changing which icons are included:

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
