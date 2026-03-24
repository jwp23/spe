# Text Input and Save: Design Spec

Inline text editing for placed overlays, multi-line support via click-and-drag, undoable text edits, and save feedback via toast notifications.

## Background

The backend modules (overlay model, PDF writer, PDF renderer) and the GUI shell (canvas, toolbar, sidebar, zoom, page navigation, undo/redo) are complete. The missing piece is the ability to actually type text into placed overlays and get feedback when saving. This spec covers that last-mile work.

## Overlay Model Changes

`TextOverlay` gains a `width` field:

```rust
pub struct TextOverlay {
    pub page: u32,
    pub position: PdfPosition,
    pub text: String,
    pub font: Standard14Font,
    pub font_size: f32,
    pub width: Option<f32>,  // None = single-line, Some(pts) = multi-line wrap width
}
```

- `width: None` -- single-line overlay, placed by click. Text is a single line in the PDF.
- `width: Some(pts)` -- multi-line overlay, placed by click-and-drag. The value is the wrap width in PDF points. Text wraps at this boundary. The user can resize the width after placement by dragging the right edge.

A new `Command` variant supports undoable resize. The inner `f32` values are the unwrapped width — this command only applies to multi-line overlays (`width: Some`):

```rust
Command::ResizeOverlay {
    index: usize,
    old_width: f32,
    new_width: f32,
}
```

## Canvas Interaction: Click vs Drag

Placement is deferred from mouse-down to mouse-up to distinguish click from drag.

1. **Mouse down** on blank page area: record start position and page number. No message emitted yet.
2. **Mouse move**: if cursor moves >10px from start, enter drag mode. Draw a dashed rectangle preview from start position to current cursor position.
3. **Mouse up**:
   - Drag distance <10px: single-line placement. Emit `Message::PlaceOverlay { page, position, width: None }`.
   - Drag distance >=10px: multi-line placement. Convert drag rectangle width to PDF points. Emit `Message::PlaceOverlay { page, position, width: Some(width_pts) }`.

The existing `Message::PlaceOverlay` variant gains a `width: Option<f32>` field. In the message handler, this is used to construct the full `TextOverlay` (combining the placement info with the current toolbar font and font size), which is then passed to `Command::PlaceOverlay { overlay }` for the undo stack — same as today, just with the new `width` field populated on the overlay.

The 10px threshold prevents accidental micro-drags from creating multi-line boxes.

Existing overlay hit-test (select + drag-to-move) is checked first and takes priority. The click-vs-drag detection only applies to clicks on blank page areas.

## Floating Text Input Widget

When an overlay enters editing mode, a native Iced text widget appears on top of the canvas at the overlay's screen position.

### Widget type

- Single-line (`width: None`): `text_input` widget. Auto-grows horizontally based on text content. Minimum width of 80px so the empty input is visible and clickable.
- Multi-line (`width: Some`): `text_editor` widget. Fixed width matching the overlay's width (converted to screen pixels at current zoom/DPI). Height grows with content.

### Positioning

The widget is rendered in a `stack` layer on top of the scrollable canvas. Screen position is computed via `pdf_to_screen()` using the overlay's PDF coordinates, current zoom, scroll offset, and page layout.

The widget repositions when:
- The canvas scrolls (recompute from same PDF coordinates)
- Zoom changes (recompute position and scale multi-line widget width)
- If the overlay scrolls out of the viewport, hide the widget but keep editing state. It reappears when scrolled back into view.

### Font rendering

The text input widget uses the system default font (Iced's default). This is consistent with existing canvas overlay rendering, which already uses `iced::Font::default()`. The Standard 14 font selection only affects PDF output.

### Entering edit mode

- **New overlay**: automatically enters edit mode after placement (click or drag).
- **Existing overlay**: double-click to re-enter edit mode. Single-click selects without editing.

### Font and size changes during editing

If the user changes font or font size in the toolbar while an overlay is in edit mode, the change applies to the active overlay immediately (same as current behavior for selected overlays). These are separate undo commands (`ChangeOverlayFont`, `ChangeOverlayFontSize`) — they are not bundled with the text edit. The editing session continues; only the text commit produces an `EditText` command.

### Committing (exiting edit mode)

- Click anywhere outside the widget
- Press Escape
- Navigate to a different page

On commit, if the text changed, an `EditText` command is pushed to the undo stack.

## Multi-line PDF Writing

The PDF writer must handle line breaks for multi-line overlays.

### Pipeline

Raw overlay text -> split on `\n` (explicit line breaks) -> word-wrap each line at `width` using AFM character widths -> emit PDF operators.

### Word wrapping

For overlays with `width: Some(pts)`, each line is word-wrapped at the width boundary. Word measurement uses the AFM-based `char_width` functions already in `coordinate.rs`. Words are broken at whitespace boundaries.

### Position semantics

`TextOverlay.position.y` is the **first line's baseline** in PDF coordinates (Y increases upward, origin bottom-left). This is consistent with how existing single-line overlays work. For multi-line overlays, subsequent lines are offset downward (negative Y direction) from this baseline.

### PDF operator output

Each line gets its own `Td`/`Tj` pair. Line spacing (leading) is `font_size * 1.2`. Subsequent lines use relative `Td` offsets with negative Y to move the text cursor down the page.

Example for `"123 Main St\nApt 4\nSpringfield"` at (72, 720), font size 12:

```
BT
  /F1 12 Tf
  72 720 Td
  (123 Main St) Tj
  0 -14.4 Td
  (Apt 4) Tj
  0 -14.4 Td
  (Springfield) Tj
ET
```

Single-line overlays (`width: None`) continue to use the existing single `Tj` path. The writer checks `overlay.width` to decide which path to take.

## Undo for Text Edits

`CanvasState` gains a new field:

```rust
pub edit_start_text: Option<String>,
```

When entering edit mode, snapshot the overlay's current text into `edit_start_text`. When committing:

- Compare `edit_start_text` to the overlay's current text.
- If different, push `Command::EditText { index, old_text, new_text }` to the undo stack and clear the redo stack.
- Clear `edit_start_text`.

Individual keystrokes are not undoable. Only the complete edit session (enter edit mode to commit) is a single undo step.

## Save Feedback

### Toast notification

`App` gains a status message field:

```rust
pub status_message: Option<(String, std::time::Instant)>,
```

- Save success: `"Saved to <filename>"`
- Save failure: `"Save failed: <error>"`

The toast renders as a small overlay bar at the bottom of the window (or below the toolbar). It auto-dismisses after 5 seconds via a subscription that checks the timestamp.

### Save remains synchronous

The `write_overlays` call stays on the main thread. The Standard 14 fonts path is fast (no font embedding). If save becomes noticeably slow on large PDFs, we can move it to a background task later.

## Multi-line Resize Handle

When a multi-line overlay (`width: Some`) is selected, its right edge shows a drag handle. Dragging the handle emits `ResizeOverlay` with the new width in PDF points. The resize is undoable.

Single-line overlays do not show a resize handle. Users adjust font size and position to fit text into form fields.

## Deferred to Future Sessions

- **Signature font**: requires font embedding pipeline (bundling a cursive font file, TrueType embedding via lopdf). Separate brainstorm session.
- **Text alignment**: left/center/right within multi-line boxes. Only relevant once multi-line is working.

## Testing Strategy

### Unit tests

- Overlay model: `width` field construction, default None
- Command: `ResizeOverlay` apply/reverse round-trip
- Word wrap: line breaking at width boundary, explicit `\n` handling, empty text, single word wider than width
- PDF operator generation: single-line vs multi-line output, leading calculation

### Integration tests

- PDF write + read-back: multi-line overlay produces correct number of `Tj` operators with correct line offsets
- PDF write + read-back: single-line overlay unchanged from current behavior

### E2E tests

- Single-line workflow: construct a `TextOverlay` with `width: None`, write to PDF via `write_overlays`, read back and verify the text appears as a single `Tj` at the correct position
- Multi-line workflow: construct a `TextOverlay` with `width: Some`, text containing `\n` and lines that exceed the wrap width, write to PDF, read back and verify correct number of `Tj` operators with correct leading offsets between lines

### Manual testing

- Click to place single-line, type, commit, verify on canvas
- Click-and-drag to place multi-line, type with wrapping, commit
- Resize multi-line box, verify text reflows
- Double-click to re-edit existing overlay
- Undo/redo text edits
- Save and verify toast appears
- Save failure (e.g., read-only path) shows error toast
