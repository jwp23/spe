# spe — PDF Text Overlay Editor

A desktop application for Linux that opens PDF documents, renders pages visually, and lets you click anywhere on a page to place text overlays. Select font family and size, then save the result as a new PDF with text baked in.

Built with Rust and Iced, optimized for Cosmic Desktop on Wayland.

## Prerequisites

| Tool | Version | Install (Arch) | Purpose |
|------|---------|----------------|---------|
| Rust | 1.88+ | `pacman -S rust` | Build toolchain |
| pdftoppm | any | `pacman -S poppler` | PDF page rendering |
| fc-list | any | `pacman -S fontconfig` | System font discovery |

## Quick Start

```bash
git clone git@github.com:jwp23/spe.git
cd spe
cargo build
cargo run
```

## Development

```bash
cargo fmt --check       # check formatting
cargo clippy -- -D warnings  # lint
cargo test              # unit tests
cargo test -- --ignored # integration tests (requires pdftoppm, fc-list)
```

Pre-commit hooks run fmt, clippy, and tests automatically.

## Project Structure

```
src/
├── main.rs         # entry point
├── app.rs          # Iced application state and messages
├── overlay.rs      # text overlay data model
├── fonts.rs        # fc-list wrapper for font discovery
├── pdf/
│   ├── renderer.rs # pdftoppm wrapper for page rendering
│   └── writer.rs   # lopdf wrapper for text overlay writing
└── ui/
    ├── canvas.rs   # PDF page display with click-to-place
    └── toolbar.rs  # font family and size controls
```

## Architecture Decisions

Recorded in `docs/adr/`. Key decisions:

- **Rust** for zero-overhead performance and native Iced integration
- **Iced 0.14** as the GUI framework (Cosmic Desktop's native toolkit)
- **pdftoppm** for PDF rendering, **lopdf** for writing text into existing PDFs
- **Trait-based wrappers** around system utilities for testability
