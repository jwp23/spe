# Overlay Visual States Design

## Problem

Canvas overlay text renders in blue (`overlay_color`: #4287f5) while the saved PDF
produces black text (PDF default fill). The floating text widget also renders in
black. This creates a visual disconnect: the user types black text, commits it,
sees blue text on the canvas, then saves a PDF with black text.

Additionally, there is no persistent visual cue to distinguish SPE-added overlays
from original PDF content. On dense documents, users can lose track of which text
is theirs.

## Solution

Render overlay text in black on the canvas (matching the PDF output) and use a
semi-transparent background tint to indicate SPE-added overlays. The tint
intensifies on hover to provide interactive feedback. This follows the Adobe
Acrobat convention for editable form fields.

## Visual States

Overlays have four visual states, mapped to existing fields plus one new field
(`hovered_overlay`):

| State    | Text Color | Background                          | Border                          | Fields                               |
|----------|-----------|-------------------------------------|---------------------------------|--------------------------------------|
| Default  | Black     | Light tint (~8% opacity)            | None                            | —                                    |
| Hovered  | Black     | Intensified tint (~15% opacity)     | Thin overlay_color border (~50%)| `hovered_overlay` (new)              |
| Selected | Black     | Light tint (same as default)        | Selection box + resize handles  | `active_overlay` (existing)          |
| Editing  | Hidden    | None (floating widget visible)      | Widget border (existing)        | `active_overlay` + `editing` (existing) |

The tint in default and selected states is identical. The selection box is the
differentiator for "selected." This avoids stacking too many visual layers.

## overlay_color Semantic Shift

The `config.overlay_color` field shifts meaning:

- **Was**: text rendering color for overlay text on the canvas
- **Becomes**: overlay chrome color (tint background, hover border, selection box,
  resize handles)

The field name, type (`[f32; 4]`), and configuration remain unchanged. Only what
it colors changes. Text rendering moves to hardcoded `Color::BLACK`, matching the
PDF writer's output.

## Code Changes

### ProgramState (canvas/mod.rs)

Add one field:

```rust
pub hovered_overlay: Option<usize>,
```

Tracks which overlay the cursor is currently over, or `None`.

### CursorMoved Handler (canvas/mod.rs — update())

On `CursorMoved` events, run `hit_test()` against the cursor position and store
the result in `hovered_overlay`. Request a redraw when `hovered_overlay` changes
so the tint updates.

The hit-test logic already exists in `mouse_interaction()`. The cursor move
handler reuses the same `hit_test()` function.

### draw() Function (canvas/mod.rs)

Three changes in the overlay drawing loop:

1. **New `draw_overlay_tint()` helper** — draws a filled rectangle behind the
   overlay text using `overlay_color` at configurable opacity. Uses the existing
   `overlay_bounding_box()` to size the rectangle. Opacity is `OVERLAY_TINT_ALPHA`
   normally, `OVERLAY_TINT_HOVER_ALPHA` when hovered.

2. **Hover border** — when `hovered_overlay == Some(i)`, draw a thin border
   around the tint rectangle at `OVERLAY_TINT_HOVER_BORDER_ALPHA`.

3. **Text color** — change from `overlay_color` to `Color::BLACK` in
   `draw_overlay_text()` calls.

### New Constants (canvas/mod.rs)

```rust
const OVERLAY_TINT_ALPHA: f32 = 0.08;
const OVERLAY_TINT_HOVER_ALPHA: f32 = 0.15;
const OVERLAY_TINT_HOVER_BORDER_ALPHA: f32 = 0.5;
```

These are opacity multipliers applied to `overlay_color`. The base RGB comes from
the configurable `overlay_color` field.

### Sidebar (sidebar.rs)

Change overlay text rendering from `overlay_color` to black. No tint on
thumbnails — the scale is too small for the tint to read well. The presence of
overlay text on the thumbnail is sufficient indication.

### Unchanged

- `TextOverlay` model — no new fields
- PDF writer — already emits black text (no color operator = PDF default fill)
- Floating text_input/text_editor widgets — already render black text
- Selection box and resize handle rendering — same visual, same code paths
- Click, drag, and editing handlers — no behavioral changes
- Config structure — `overlay_color` field unchanged

## Inline Color Constant Cleanup

As part of this work, extract existing inline `Color::from_rgb(...)` magic values
into named constants across `canvas/mod.rs`, `view.rs`, and `sidebar.rs`. This is
incremental tech debt cleanup scoped to the files being touched.

## Future Considerations

User-selectable font color is a potential future feature. When that arrives, a
`color` field on `TextOverlay` would replace the hardcoded `Color::BLACK`, and
the PDF writer would emit `rg` color operators. Nothing in this design precludes
that extension. Not implementing it now (YAGNI).
