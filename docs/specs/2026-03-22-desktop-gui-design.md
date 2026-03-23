# Desktop GUI Design: PDF Text Overlay Editor

This spec defines the desktop GUI layer that wires the existing backend modules (overlay model, PDF writer, PDF renderer) into an interactive Iced application.

## Design Decisions

These decisions were made during brainstorming and constrain the design:

- **Elm-style architecture with sub-modules.** Single `App` struct owns all state. Single `Message` enum and `update()` function. Sub-modules handle view logic for toolbar, canvas, and sidebar. Idiomatic Iced, and undo/redo benefits from centralized state mutation.
- **Click-then-type interaction.** User clicks on the canvas to place a cursor, then types directly. Sensible defaults (Helvetica 12pt). Each overlay has independent font/size.
- **Top toolbar, single row.** File ops, undo/redo, font controls, zoom, and page nav in one horizontal bar. Minimum window width ~750px.
- **Collapsible thumbnail sidebar.** Lazy-loaded with placeholders. Toggled via toolbar icon and F9.
- **Standard 14 fonts only.** Font dropdown lists all 14 variants. No system font discovery.
- **Phosphor icon font (subsetted).** ~12 glyphs extracted from the full font. Bundled in the binary.
- **Save + Save-as.** First save prompts for destination. Subsequent saves are silent. Save-as always available.
- **Unlimited undo/redo.** Command pattern. Survives saves. Clears on file close.
- **Configurable overlay color.** Hardcoded sensible default (blue). Configurable via config. No system accent color integration in v1.
- **pdftoppm for rendering.** CPU-based subprocess. Lazy thumbnail loading. Debounced re-render on zoom.

## Application State Model

The central `App` struct owns all state. Sub-modules read slices of it for rendering but all mutations go through `App::update()`.

```
App
├── document: Option<DocumentState>
│   ├── source_path: PathBuf
│   ├── save_path: Option<PathBuf>
│   ├── page_count: u32
│   ├── current_page: u32                  // 1-indexed
│   ├── page_images: HashMap<u32, Handle>  // Rendered pages (Iced image handles)
│   ├── page_dimensions: HashMap<u32, (f32, f32)>  // Width x height in PDF points
│   └── overlays: Vec<TextOverlay>
├── toolbar: ToolbarState
│   ├── font: Standard14Font               // Default: Helvetica
│   └── font_size: f32                     // Default: 12.0
├── canvas: CanvasState
│   ├── zoom: f32                          // 1.0 = fit-to-width
│   ├── active_overlay: Option<usize>      // Index into overlays vec
│   ├── editing: bool                      // True when typing into active overlay
│   └── dragging: Option<DragState>
├── sidebar: SidebarState
│   ├── visible: bool
│   └── thumbnails: HashMap<u32, Handle>
├── undo_stack: Vec<Command>
├── redo_stack: Vec<Command>
└── overlay_color: Color
```

**`DragState`** tracks an in-progress overlay drag:

```
DragState
├── overlay_index: usize      // Which overlay is being dragged
├── initial_position: PdfPosition  // Overlay's position when drag started
└── grab_offset: (f32, f32)   // Offset between mouse click and overlay origin (preserves grab point)
```

**`Handle`** refers to `iced::widget::image::Handle`. The async task callbacks convert `DynamicImage` (from the renderer) to `Handle` via `Handle::from_rgba()` before sending the `PageRendered` or `ThumbnailRendered` message.

**`Color`** refers to `iced::Color`.

**Page dimensions** are read from `lopdf` on file open. A new helper function in `src/pdf/mod.rs` extracts the `MediaBox` from each page dictionary:

```rust
pub fn page_dimensions(doc: &lopdf::Document) -> HashMap<u32, (f32, f32)>
```

This function iterates `doc.get_pages()`, reads each page's `MediaBox` array `[x0, y0, x1, y1]`, and returns `(x1 - x0, y1 - y0)` as `(width, height)` in PDF points. It handles inherited `MediaBox` from parent `Pages` nodes. This is a synchronous, in-memory operation (no rendering) used by both the canvas (coordinate conversion) and the sidebar (placeholder sizing).

The `TextOverlay` type from `overlay.rs` is reused directly. The `active_overlay` index ties the canvas selection to the overlays vec. When an overlay is selected, the toolbar reflects that overlay's font/size; changes apply to it immediately. When no overlay is selected, toolbar values set the defaults for the next placed overlay.

## Message Types

All user actions and async events flow through a single `Message` enum. Overlay editing messages push onto the undo stack.

```
Message
├── File Operations
│   ├── OpenFile
│   ├── FileOpened(PathBuf)
│   ├── Save
│   ├── SaveAs
│   └── SaveDestinationChosen(PathBuf)
│
├── Page Navigation
│   ├── GoToPage(u32)
│   ├── NextPage
│   ├── PreviousPage
│   └── PageRendered(u32, Handle)
│
├── Overlay Editing (undoable)
│   ├── PlaceOverlay(PdfPosition)
│   ├── UpdateOverlayText(String)
│   ├── CommitText
│   ├── MoveOverlay(usize, PdfPosition)
│   ├── ChangeFont(Standard14Font)
│   ├── ChangeFontSize(f32)
│   ├── DeleteOverlay(usize)
│   ├── SelectOverlay(usize)
│   └── DeselectOverlay
│
├── Canvas
│   ├── ZoomIn
│   ├── ZoomOut
│   ├── ZoomReset
│   ├── ScrollZoom(f32)
│   └── DragStarted(usize, Point)
│
├── Sidebar
│   ├── ToggleSidebar
│   └── ThumbnailRendered(u32, Handle)
│
├── Undo/Redo
│   ├── Undo
│   └── Redo
│
└── Keyboard
    └── KeyPressed(KeyEvent)
```

Undo granularity: `UpdateOverlayText` fires on every keystroke but is not individually recorded. `CommitText` captures the full text change as a single undoable command. `MoveOverlay` records only the final position after drag-end.

## Undo/Redo System

The command pattern records every undoable action as a `Command` that knows how to apply and reverse itself.

### Command Variants

| Variant | Undo | Redo |
|---------|------|------|
| `PlaceOverlay { overlay }` | Remove from vec | Re-insert |
| `DeleteOverlay { overlay, index }` | Re-insert at index | Remove |
| `MoveOverlay { index, from, to }` | Set position to `from` | Set position to `to` |
| `EditText { index, old_text, new_text }` | Set text to `old_text` | Set text to `new_text` |
| `ChangeOverlayFont { index, old_font, new_font }` | Set font to `old_font` | Set font to `new_font` |
| `ChangeOverlayFontSize { index, old_size, new_size }` | Set size to `old_size` | Set size to `new_size` |

### Rules

- New undoable action clears the redo stack.
- Undo pops from undo stack, applies reverse, pushes onto redo stack.
- Redo pops from redo stack, applies forward, pushes onto undo stack.
- Unlimited depth. Clears when a new file is opened or the current file is closed.
- History survives saves — saving does not reset the stacks.
- Non-undoable actions: save, zoom, page navigation, sidebar toggle.

### Index Stability

Commands reference overlays by index into `Vec<TextOverlay>`. Undo/redo executes in strict LIFO order, so indices remain valid. `PlaceOverlay` undo removes the last-added overlay. `DeleteOverlay` undo re-inserts at the recorded index, shifting subsequent entries.

## Canvas: PDF Rendering and Coordinate Conversion

### Three Coordinate Spaces

```
Screen pixels (canvas widget coordinates)
    ↕  zoom factor + offset (pan/scroll position)
Display points (rendered image coordinates)
    ↕  DPI scale factor (render DPI / 72)
PDF points (overlay model, origin bottom-left)
```

### Conversion Functions

Screen to PDF (on click/placement):
```
pdf_x = (screen_x - offset_x) / zoom / (dpi / 72.0)
pdf_y = page_height - ((screen_y - offset_y) / zoom / (dpi / 72.0))
```

PDF to Screen (for drawing overlays):
```
screen_x = pdf_x * (dpi / 72.0) * zoom + offset_x
screen_y = (page_height - pdf_y) * (dpi / 72.0) * zoom + offset_y
```

The Y-axis flip accounts for PDF origin at bottom-left vs. screen origin at top-left. These conversions are pure functions in `coordinate.rs`, parameterized by zoom, DPI, page dimensions, and offset.

### Hit Testing

On click, convert screen coordinates to PDF space, then check if the click lands within any existing overlay's bounding box. If yes, select that overlay. If no, place a new overlay.

### Overlay Bounding Boxes

Standard 14 fonts have defined glyph widths in the PDF spec (Adobe Font Metrics / AFM data). A per-font width table is embedded in `coordinate.rs`, mapping each ASCII character to its width in units of 1/1000 of the font size. The bounding box for an overlay is computed by summing character widths for the text string and using the font size as height. This data is sourced from the AFM files published by Adobe for the Standard 14 fonts.

This approach gives accurate hit testing for proportional fonts (Helvetica) and monospaced fonts (Courier) alike. Pixel-perfect accuracy is not required — this is for click targeting and drag handles.

### Overlay Visual Feedback

- Unselected overlays: render in the configured overlay color with a dashed bounding box.
- Selected overlay: solid border + drag handles at corners/edges.
- Active text entry: blinking cursor at insertion point.
- Overlay color is configurable with a sensible default. Does not affect saved PDF output (overlays bake in as black text).

## Thumbnail Sidebar

### Layout

Vertical scrollable panel on the left side of the canvas area. Toggled via toolbar icon and F9 keyboard shortcut. When hidden, the canvas gets the full width.

### Lazy Loading

1. On file open, read page count and dimensions from `lopdf` (no rendering).
2. Show placeholder rectangles sized proportionally to actual page dimensions.
3. Track which thumbnails are in the visible scroll viewport.
4. Fire async `Task` renders for visible pages + 2-page buffer, at ~72 DPI.
5. `ThumbnailRendered(page, handle)` messages replace placeholders with images.
6. On scroll, queue renders for newly visible pages.

### Interaction

- Click a thumbnail to navigate to that page.
- Current page thumbnail gets a highlight border (overlay color).
- Thumbnail width fixed (~120px), height scales proportionally.

### Cache

- Thumbnails rendered once and cached for the session.
- Opening a new file clears the cache.
- At 72 DPI, ~1.9MB per US Letter page. A 100-page document uses ~190MB. Acceptable for a desktop app.

## Toolbar

### Layout

Single horizontal row:

```
[Sidebar icon] | [Open] [Save] [Save As] | [Undo] [Redo] | [Font dropdown] [Size input] | [Zoom-] [100%] [Zoom+] [Reset] | [< ] [Page / total] [ >]
```

### Icons

Phosphor Icons, subsetted to ~12 glyphs: folder-open, floppy-disk, floppy-disk-plus (save-as), sidebar, magnifying-glass-plus, magnifying-glass-minus, magnifying-glass (reset), caret-left, caret-right, arrow-counter-clockwise (undo), arrow-clockwise (redo), trash (delete). Font file subset committed to the repository.

### Controls

- **Sidebar toggle:** Phosphor sidebar icon. Toggles thumbnail sidebar visibility. Matches F9.
- **Open / Save / Save As:** Icon buttons. Save disabled until document loaded. First save behaves as save-as (no `save_path` yet). Subsequent saves are silent.
- **Undo / Redo:** Icon buttons. Disabled when respective stack is empty.
- **Font dropdown:** `PickList` with all 14 Standard 14 fonts. Defaults to Helvetica. Reflects active overlay's font when selected.
- **Size input:** Numeric text input. Defaults to 12. Reflects active overlay's size when selected. Accepts decimals.
- **Zoom controls:** Minus, percentage display, plus, reset. The percentage display shows the zoom relative to fit-to-width (100% = fit-to-width, 200% = twice that size). Steps in 25% increments (25%, 50%, 75%, 100%, 125%, 150%, 200%). Reset returns to 100% (fit-to-width).
- **Page nav:** Previous arrow, editable page number, "/ {total}" label, next arrow. Arrows disabled at boundaries.

### State Interaction

- No overlay selected: font/size show defaults for next overlay.
- Overlay selected: font/size reflect that overlay's properties. Changes modify the overlay immediately (undoable).
- No document loaded: everything except Open is disabled.

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
| Escape | Deselect overlay / cancel text entry |
| Page Up | Previous page |
| Page Down | Next page |
| F9 | Toggle thumbnail sidebar |

## Async Task Pipeline

### Page Rendering

1. Navigate to page → check `page_images` cache.
2. Cache miss → spawn async `Task` calling `PdftoppmRenderer::render_page()`.
3. Canvas shows loading indicator while waiting.
4. `PageRendered(page, Handle)` stores handle, canvas refreshes.

### Zoom Re-rendering

On zoom change, scale the cached image immediately (responsive but blurry at high zoom). After ~300ms debounce with no further zoom changes, fire a re-render at the target DPI. Swap in the crisp image when it arrives.

### Thumbnail Rendering

Visible thumbnails + 2-page buffer rendered in background. Up to 2-3 concurrent thumbnail tasks to avoid CPU saturation.

### Concurrency

- One main canvas render task at a time. Cancel in-flight task if user navigates away.
- Thumbnail tasks run in parallel (2-3 max).

## File Structure

```
src/
├── main.rs              # Entry point: iced::application() launch
├── app.rs               # App struct, Message enum, update(), view()
├── overlay.rs           # [exists] TextOverlay, PdfPosition, Standard14Font
├── command.rs           # [new] Command enum for undo/redo
├── coordinate.rs        # [new] Screen <-> PDF coordinate conversion
├── config.rs            # [new] Overlay color, defaults, window constraints
├── pdf/
│   ├── mod.rs           # [exists]
│   ├── renderer.rs      # [exists] PageRenderer trait, PdftoppmRenderer
│   └── writer.rs        # [exists] write_overlays()
└── ui/
    ├── mod.rs           # [exists]
    ├── canvas.rs         # Page image + overlay drawing + mouse handling
    ├── toolbar.rs        # Icons, font picker, zoom, page nav
    ├── sidebar.rs        # [new] Thumbnail sidebar
    └── icons.rs          # [new] Phosphor icon constants and helpers
```

### Module Responsibilities

- **app.rs** — Owns all state. `update()` handles every `Message`, including undo/redo interception. `view()` composes toolbar + sidebar + canvas.
- **command.rs** — `Command` enum with `apply()` and `reverse()` methods. Pure data, no side effects.
- **coordinate.rs** — Pure functions for coordinate conversion. Parameterized by zoom, DPI, page dimensions, offset.
- **config.rs** — Configurable values (overlay color, default font/size, minimum window size). Hardcoded defaults, overridable.
- **ui/canvas.rs** — Canvas widget. Draws page image + overlays, reports mouse events as messages.
- **ui/toolbar.rs** — Toolbar row. Reads toolbar state, returns interaction messages.
- **ui/sidebar.rs** — Thumbnail sidebar. Reads cache + current page, reports page selection.
- **ui/icons.rs** — Phosphor font loading and glyph constants.

## Testing Strategy

### Unit Tests

| Module | Coverage |
|--------|----------|
| `coordinate.rs` | Screen↔PDF conversions at various zoom/DPI/page sizes. Y-axis flip. Edge cases (zero zoom, fractional coords). |
| `command.rs` | Each variant's `apply()` and `reverse()`. Round-trip correctness. Stack push/pop/clear behavior. |
| `config.rs` | Default values. Custom value overrides. |
| `ui/icons.rs` | Icon constants valid. Font loading doesn't panic. |
| `overlay.rs` | [exists] Already covered. |
| `pdf/writer.rs` | [exists] Already covered. |
| `pdf/renderer.rs` | [exists] Already covered. |

### Integration Tests

| Test | `#[ignore]`? |
|------|-------------|
| Load PDF → pdftoppm → verify image dimensions | Yes (needs pdftoppm) |
| Overlay write round-trip: create → write → re-read → verify | No |
| Coordinate round-trip: screen→PDF→screen within tolerance | No |
| Undo/redo full cycle: place→move→edit→delete→undo all→redo all | No |

### E2E Tests (iced_test Simulator)

| Test | Workflow |
|------|---------|
| Open file | Simulate open → verify document state populated |
| Place and edit text | Click canvas → type → commit → verify overlay in state |
| Font/size change | Select overlay → change font → verify overlay updated |
| Move overlay | Select → drag → verify new position |
| Page navigation | Next/prev, type page number → verify current page |
| Save flow | First save triggers save-as → subsequent save is silent |
| Undo/redo | Place → undo → verify removed → redo → verify restored |
| Sidebar toggle | F9 → verify visible → F9 → verify hidden |
| Zoom | Ctrl+Plus → verify increased → reset → verify 1.0 |
| Delete overlay | Select → Delete key → verify removed |

Note: `iced_test` Simulator may require a software rendering context (Mesa `llvmpipe`) in CI. If GitHub Actions runners cannot provide this, E2E tests run locally only (`#[ignore]` in CI).

### Automated Verification (Bot)

- Saved PDF opens without errors (lopdf re-read)
- Overlay text, font, size, position correct in saved PDF (content stream inspection)
- Rendered output has non-white pixels in expected overlay regions (pdftoppm + pixel sampling, `#[ignore]`)
- Page count preserved after save
- Multiple overlays on different pages all present

### Visual Verification (Joe)

- Overlay text renders at visually correct position
- Overlay color and dashed bounding box visible and distinct from PDF content
- Selected overlay shows solid border + drag handles
- Drag feels responsive, overlay tracks mouse smoothly
- Thumbnails appear as placeholders then fill in on scroll
- Current page thumbnail has highlight border
- Zoom in/out looks smooth (scale then sharpen on debounce)
- Toolbar Phosphor icons render correctly (not empty boxes)
- Half-screen tiling: toolbar usable, sidebar collapses cleanly

## What Is Not in This Design

- **System font discovery / embedding** — Standard 14 only. See `docs/decisions/standard-14-fonts-for-v1.md`.
- **GPU-accelerated PDF rendering** — No Rust-native solution exists. pdftoppm with lazy loading is sufficient.
- **Cosmic Desktop theme integration** — Overlay color is configurable but not auto-detected from system accent color.
- **Multi-document** — One PDF open at a time.
- **Form filling** — Out of scope per project definition.
- **Text editing of existing PDF content** — Out of scope per project definition.
- **Screenshot-based testing** — Deferred unless headless state tests prove insufficient.

## Dependencies

Existing in `Cargo.toml`:
- `iced` 0.14 (features: `image`, `canvas`, `tokio`)
- `lopdf` 0.40
- `rfd` 0.17
- `image` 0.25
- `thiserror` 2
- `tempfile` 3

New:
- `iced_test` (dev-dependency, for E2E tests)

Assets:
- Subsetted Phosphor Icons TTF (~12 glyphs), committed to repository
