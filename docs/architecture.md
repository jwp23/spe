# Architecture

## Module map

Key modules in `src/`:

| Module | Purpose |
|--------|---------|
| `app/` | Iced application state, Message enum, update/view/subscription, event handlers |
| `ui/canvas/` | PDF canvas rendering, hit testing, zoom, overlay drawing |
| `ui/sidebar.rs` | Thumbnail sidebar with drag-resize |
| `ui/toolbar.rs` | Toolbar layout, font/page state, zoom controls, page navigation |
| `ui/font_picker.rs` | Font family control: a UI-font label, and a drop-down previewing each family in its own typeface |
| `ui/text_width.rs` | Shaped width of a string in any face, cached — sizes both the picker and the canvas |
| `ui/popover.rs` | Generic anchor + floating panel widget the font picker is built on |
| `pdf/` | PDF rendering (`pdftoppm` wrapper) and writing (`lopdf` overlay embedding) |
| `overlay.rs` | Text overlay data model (position, font, text, width) |
| `coordinate.rs` | Screen-to-PDF coordinate conversion, AFM font width tables |
| `ipc.rs` | IPC protocol for the screenshot development tool |
| `command.rs` | Undo/redo command pattern, plus the finer-grained history of an open edit session |

Tests live in `tests/` (integration/E2E) and co-located `#[cfg(test)]` modules (unit). See `docs/decisions/project-directory-structure.md`.

## Edit sessions and undo

Placing or editing an overlay opens an edit session, tracked in canvas state (`editing`, `active_overlay`, `edit_start_text`, `fresh_placement` in `App`/`CanvasState`). While a session is open, typing, font/size changes, moves, and resizes happen live against the document without yet being part of its undo history. A session ends by either committing (`handle_commit_text`, called on select/edit/deselect/undo/redo targeting something else, or explicitly) or cancelling (`cancel_edit_session`, which reverts a fresh placement or an established overlay's text to how the session found it).

Two histories cooperate rather than compete:

- **The document history** (`App::undo_stack`/`redo_stack`, `Command` in `src/command.rs`) holds one entry per committed, reversible change to the overlay list — place, delete, edit text, change font, change size, move, resize. Commands address overlays by raw `Vec` index; this is safe under the LIFO discipline undo/redo always use, verified by a property test in `src/app/tests.rs` that replays randomized command sequences against the live document and checks they never diverge.
- **The session history** (`SessionHistory` in `src/command.rs`) holds the finer-grained steps taken *inside* the currently open session: a coalesced run of typing is one step, and any document-changing action (font, size, move, resize) taken mid-session is already a `Command` on the document stack, so the session only records a marker pointing at it (`SessionStep::Document`).

Undo (`App::handle_undo`) walks a gradient: session steps first, then — once the session has nothing left to step back through — closing the box (itself the visible change for that keystroke), and only after that does undo reach into the document history. Once a session is committed or cancelled, its `SessionHistory` is cleared and undo/redo operate purely on the coarse, one-command-per-action document history; there is no finer-grained undo for changes made outside an open session.

An action that retargets or removes overlays (selecting or editing a different overlay, undo, redo) always commits the pending session first, so uncommitted state can never desync from what the document history describes. Undoing or redoing a command that changes the overlay count (`Command::changes_overlay_count`) clears any active selection and edit session, since the indices they hold may no longer address the same overlay; an in-place command (text/font/size/move/resize) leaves the selection and toolbar in sync with the restored state.

See ADR-012 for the reasoning and rejected alternatives behind this design.
