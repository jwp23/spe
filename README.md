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
git clone https://github.com/jwp23/spe.git
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

### Git Hooks

```bash
./scripts/setup-hooks.sh
```

This points `core.hooksPath` at the repository's committed hooks. Git runs no hook until you do
this, so it is the first step after cloning if you plan to submit a change.

`pre-commit` runs secrets scanning, formatting, lint, dependency audit, licence checks, and the
test suites. `commit-msg` checks the commit message convention.

### Development Dependencies

| Tool | Version | Install (Arch) | Purpose |
|------|---------|----------------|---------|
| betterleaks | any | [install guide](https://github.com/betterleaks/betterleaks) | Secrets scanning in the pre-commit hook |
| cargo-audit | any | `cargo install cargo-audit` | Dependency vulnerability scanning |
| cargo-deny | any | `cargo install cargo-deny` | Licence and supply-chain checks |
| cage | any | `pacman -S cage` | Headless Wayland compositor for screenshot harness |
| grim | any | `pacman -S grim` | Wayland-native screenshot capture |
| socat | any | `pacman -S socat` | Unix socket client for IPC commands |

None of these are needed to build or run the app. The first three are used by the pre-commit
hook, which warns and skips a check when its tool is missing — CI runs all of them on every pull
request regardless. The last three are for visual debugging with the screenshot tool.

### Commands

```bash
cargo fmt --check            # check formatting
cargo clippy --all-targets -- -D warnings  # lint
cargo test                   # unit + integration tests
cargo test -- --ignored      # E2E tests (requires GPU context)
```

Once enabled, the pre-commit hook runs these automatically.

### Visual Debugging

A screenshot tool takes screenshots of the running app to verify visual output — useful by hand and for AI coding agents. It uses `cage` (headless Wayland compositor), `grim` (screenshot capture), and `socat` (IPC), and requires starting the app with `--ipc`.

See [docs/screenshot-tool.md](docs/screenshot-tool.md) for system dependencies, harness script usage, and the full IPC command reference.

## Project Structure

The app is organized into `src/` modules for the UI, PDF rendering/writing, overlay model, and IPC — see [docs/architecture.md](docs/architecture.md) for the module map.

Tests live in `tests/` (integration/E2E) and co-located `#[cfg(test)]` modules (unit).

## Phosphor Icon Font (contributors only)

The subsetted font is already committed — you do **not** need these tools to build or run the app. This section is only for regenerating the subset after changing which icons are included.

Source: [phosphor-icons/web](https://github.com/phosphor-icons/web) (MIT licensed), `src/regular/Phosphor.ttf` at commit [`3d40a3e`](https://github.com/phosphor-icons/web/commit/3d40a3eaa73b25d20d263f0e1e55c9fed3e66809). Codepoints come from that commit's `src/regular/selection.json`.

```bash
pipx install fonttools  # provides pyftsubset; do not `pip install` system-wide

curl -sLO https://raw.githubusercontent.com/phosphor-icons/web/3d40a3eaa73b25d20d263f0e1e55c9fed3e66809/src/regular/Phosphor.ttf

pyftsubset Phosphor.ttf \
  --unicodes="U+E036,U+E038,U+E08A,U+E136,U+E138,U+E13A,U+E13C,U+E248,U+E256,U+E30C,U+E30E,U+E310,U+EAB6,U+E4A6" \
  --output-file=assets/icons/phosphor-subset.ttf \
  --no-hinting --desubroutinize
```

## Contributing

Bug reports, feature proposals, and pull requests are welcome — see
[CONTRIBUTING.md](CONTRIBUTING.md) for setup, conventions, and what the project does and does
not aim to do. If you work with an AI coding agent, [AGENTS.md](AGENTS.md) carries the same
rules in instruction form.

## Architecture Decisions

Recorded in `docs/adr/`. Key decisions:

- **Rust** for zero-overhead performance and native Iced integration
- **Iced 0.14** as the GUI framework (Cosmic Desktop's native toolkit)
- **pdftoppm** for PDF rendering, **lopdf** for writing text into existing PDFs
- **Standard 14 PDF fonts** only (no system font embedding in v1)
- **Command pattern** for unlimited undo/redo
- **Trait-based wrappers** around system utilities for testability
