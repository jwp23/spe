# Architecture

## Module map

Key modules in `src/`:

| Module | Purpose |
|--------|---------|
| `app/` | Iced application state, Message enum, update/view/subscription, event handlers |
| `ui/canvas/` | PDF canvas rendering, hit testing, zoom, overlay drawing |
| `ui/sidebar.rs` | Thumbnail sidebar with drag-resize |
| `ui/toolbar.rs` | Toolbar layout, font/page state, zoom controls, page navigation |
| `ui/font_picker.rs` | Font family drop-down, each family previewed in its own typeface |
| `ui/popover.rs` | Generic anchor + floating panel widget the font picker is built on |
| `pdf/` | PDF rendering (`pdftoppm` wrapper) and writing (`lopdf` overlay embedding) |
| `overlay.rs` | Text overlay data model (position, font, text, width) |
| `coordinate.rs` | Screen-to-PDF coordinate conversion, AFM font width tables |
| `ipc.rs` | IPC protocol for the screenshot development tool |
| `command.rs` | Undo/redo command pattern, plus the finer-grained history of an open edit session |

Tests live in `tests/` (integration/E2E) and co-located `#[cfg(test)]` modules (unit). See `docs/decisions/project-directory-structure.md`.
