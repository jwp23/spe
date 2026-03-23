# Thumbnail Sidebar Design

This spec defines the thumbnail sidebar rendering feature, building on the existing stubbed sidebar infrastructure. The sidebar provides navigation-first page thumbnails with overlay visibility, independent scrolling, and a resizable panel.

This spec supersedes the Thumbnail Sidebar section of `2026-03-22-desktop-gui-design.md`. Key changes from the original design: DPI is now scaled to the actual display size (not fixed at 72 DPI), rendering uses batched `pdftoppm` calls (not per-page), the sidebar is resizable (not fixed width), and overlays are drawn on thumbnails. Memory estimates in this spec reflect the display-matched DPI approach.

## Design Decisions

These decisions were made during brainstorming and constrain the design:

- **Navigation-first with recognizable thumbnails.** Primary purpose is quick page jumping. Thumbnails should be detailed enough to identify page content (headings, images, layout) but speed and compactness take priority over legibility.
- **Independent scrolling.** Sidebar scrolls independently of the main canvas. No synchronization — the user may be viewing page 50's thumbnail while the canvas shows page 1.
- **Current page highlighted.** The dominant page in the main canvas viewport gets a highlight border and glow on its sidebar thumbnail, so the user can glance at the sidebar to see where they are.
- **Resizable sidebar.** Drag handle between sidebar and canvas. Width clamped to 80px–400px, default 120px. Thumbnail DPI recalculates on resize.
- **Overlays drawn in the UI layer.** Base PDF images rendered once via `pdftoppm`. Overlays drawn on top using Iced canvas drawing primitives — same pattern as the main canvas. Overlay edits cause a free redraw, not a re-render. Overlay text uses the configured overlay color.
- **Hybrid lazy loading.** Visible thumbnails render first, then remaining pages backfill in the background via batched `pdftoppm` calls (20 pages per batch). Shimmer placeholders shown for unrendered pages.
- **DPI scaled to display.** Thumbnail render DPI computed from sidebar width, page dimensions, and display scale factor. Produces images sized for the actual display, avoiding wasted memory from oversized renders.
- **Batched `pdftoppm` rendering.** Thumbnails use page-range rendering (`-f N -l M`) to amortize subprocess overhead. 20 pages per batch keeps per-call cost low while avoiding hundreds of subprocess spawns.

## State Additions

The existing `SidebarState` expands with new fields:

```
SidebarState
├── visible: bool                      // [exists] toggled by F9 or toolbar
├── thumbnails: HashMap<u32, Handle>   // [exists] base PDF thumbnail images
├── width: f32                         // current sidebar width (default 120.0)
├── scroll_y: f32                      // independent scroll offset
├── viewport_height: f32               // visible sidebar scroll area height
├── thumbnail_dpi: f32                 // computed from width + scale factor
├── dragging: bool                     // resize handle drag in progress
└── backfill_generation: u64           // incremented on file open or resize; stale backfill results ignored
```

`thumbnail_dpi` is computed as:

```
thumbnail_width_px = width - THUMBNAIL_PADDING
render_dpi = (thumbnail_width_px * scale_factor) / page_width_inches
```

Where `page_width_inches = page_width_points / 72.0` and `scale_factor` comes from Iced's window scale factor.

## New Messages

```
Message (additions)
├── SidebarScrolled(f32, f32)              // scroll_y, viewport_height
├── SidebarResized(f32)                    // new width during drag
├── SidebarResizeEnd                       // drag released — trigger re-render
├── SidebarResizeDebounceExpired(u64)      // generation-based debounce for re-render
├── SidebarPageClicked(u32)                // thumbnail clicked — jump canvas
└── ThumbnailBatchRendered(Vec<(u32, Handle)>)  // batch of thumbnails completed
```

`ThumbnailBatchRendered` replaces the single-page `ThumbnailRendered(u32, Handle)` from the desktop GUI spec. The old message variant should be removed.

`SidebarResizeDebounceExpired(u64)` carries the `backfill_generation` value — the same field used for backfill staleness. Incrementing `backfill_generation` on resize invalidates both in-flight backfill results and stale debounce expirations.

`SidebarPageClicked(page)` emits `GoToPage(page)`, which scrolls the main canvas using the existing `scrollable_id` and `scroll_to` mechanism.

## Sidebar Canvas Widget

### Structure

The sidebar view builds a `Scrollable` column where each page is a mini-canvas widget. Iced handles scroll behavior natively — no manual scroll math.

Each page entry in the column:

```
Container
├── Canvas (thumbnail_width x thumbnail_height)
│   ├── White page background
│   ├── Base thumbnail image (if cached) or shimmer placeholder
│   ├── Scaled overlays for this page (drawn in overlay color)
│   └── Highlight border if current page
└── Text label (page number, centered below)
```

### Per-Page Canvas Drawing

The `draw()` method for each thumbnail canvas:

1. Fill white page background.
2. If `thumbnails` contains an image for this page, draw it scaled to fill the canvas.
3. If no image cached, draw a shimmer placeholder (animated gradient).
4. For each overlay on this page: convert PDF coordinates to thumbnail scale, draw text in overlay color.
5. If this page is the current page, draw a highlight border (2px, accent color, with subtle glow/shadow).

### Coordinate Scaling for Overlays

Thumbnail overlay drawing uses the same coordinate conversion as the main canvas, parameterized differently:

```
thumb_scale = thumbnail_dpi / 72.0
screen_x = pdf_x * thumb_scale
screen_y = (page_height - pdf_y) * thumb_scale
font_display_size = font_size * thumb_scale
```

At thumbnail scale, text will be very small but visible as colored marks showing where overlays are placed. This is sufficient for the navigation use case.

### Click Handling

Each thumbnail canvas handles mouse press events. On click, emit `SidebarPageClicked(page_number)`. No drag handling, no text editing, no hover states.

## Shimmer Placeholder

Unrendered thumbnails show an animated shimmer effect:

- Rectangle sized to the page's aspect ratio (computed from `page_dimensions`).
- Background: subtle gradient that animates horizontally (dark → slightly lighter → dark).
- The shimmer indicates loading is in progress, distinct from an error state.
- Implemented via Iced's canvas drawing with a time-based animation offset. The `draw()` method uses `frame.fill_rectangle()` with a gradient that shifts based on a subscription tick at ~60fps (16ms interval). This uses a new `Subscription` dedicated to shimmer animation, active only while unrendered thumbnails exist.

## Rendering Pipeline

### On File Open

1. Calculate `thumbnail_dpi` from sidebar width, display scale factor, and maximum page width.
2. Determine visible sidebar pages (initially page 1 and however many fit in the viewport).
3. Fire batched render task for visible pages + 5-page buffer.
4. After visible batch completes, start backfill: render remaining pages in batches of 20, ordered outward from the visible range.

### On Sidebar Scroll

1. Update `scroll_y` and `viewport_height`.
2. Determine newly visible pages that have no cached thumbnail.
3. Fire render tasks for missing visible pages (these take priority over backfill).

### On Overlay Change

No re-render. The canvas `draw()` reads current overlays and draws them scaled on top of cached base images. Overlay edits trigger a widget redraw, not a subprocess call.

### On Sidebar Resize

1. During drag: update `width`, thumbnails stretch/shrink (Iced scales cached images).
2. On drag end: increment `backfill_generation`, schedule debounced re-render (300ms).
3. On debounce expiry: recalculate `thumbnail_dpi`, clear `thumbnails` cache, restart rendering pipeline (visible first, then backfill).

### On New Page Navigation (Canvas Scroll)

Update `current_page` tracking. Sidebar redraws to move the highlight border — no rendering needed.

## Batched pdftoppm Rendering

The existing `PdftoppmRenderer` renders one page at a time. A new function handles batch rendering:

```rust
fn render_page_batch(
    pdf_path: &Path,
    first_page: u32,
    last_page: u32,
    dpi: f32,
) -> Result<Vec<(u32, DynamicImage)>>
```

This calls `pdftoppm -f {first} -l {last} -r {dpi} -png {pdf} {prefix}`, producing multiple output files. The function reads all output PNGs and returns them as a vec of `(page_number, image)` pairs.

The batch task wrapper converts each `DynamicImage` to an Iced `Handle` and sends `ThumbnailBatchRendered(Vec<(u32, Handle)>)`.

### Backfill Strategy

1. After visible thumbnails render, determine remaining unrendered pages.
2. Order by distance from visible range (nearest pages first).
3. Fire batches of 20 pages. Wait for each batch to complete before firing the next.
4. Each `ThumbnailBatchRendered` result is checked against `backfill_generation` — stale results (from a previous file or resize) are discarded.
5. If the user scrolls to an unrendered area during backfill, those pages get priority rendering (interrupt backfill, render visible, resume).

### Concurrency

- At most 2 concurrent thumbnail batch tasks to avoid CPU saturation.
- Main canvas render tasks take priority over thumbnail tasks.
- Backfill tasks run at lower priority (yielding to visible-page requests).

## Resize Handle

### Implementation

The resize handle uses `iced::event::listen()` to capture window-level mouse events during drag. This avoids the `MouseArea` limitation where mouse move events stop when the cursor leaves the handle's bounds.

### Behavior

1. A 4–6px vertical strip renders between sidebar and canvas.
2. Mouse cursor changes to `col-resize` on hover (via `mouse::Interaction`).
3. Mouse press on the handle sets `sidebar.dragging = true` and captures the initial mouse X and sidebar width.
4. While dragging, `event::listen()` intercepts mouse move events at the app level. New width = initial width + (current_x - initial_x), clamped to 80–400px.
5. Mouse release sets `dragging = false` and fires `SidebarResizeEnd`.
6. `SidebarResizeEnd` triggers a debounced DPI recalculation and thumbnail re-render.

### Fit-to-Width Adjustment

The main canvas `fit_to_width` calculation currently subtracts the constant `SIDEBAR_WIDTH` (defined in `src/ui/sidebar.rs`). This changes to subtract `sidebar.width` (the dynamic value). The `SIDEBAR_WIDTH` constant becomes `DEFAULT_SIDEBAR_WIDTH` and is used only for initialization. When the sidebar resizes, the available canvas width changes, and if zoom is set to fit-to-width, the zoom level adjusts accordingly.

## Memory Profile

At display-matched DPI, thumbnail memory is modest:

| PDF Size | Per Thumbnail (1x) | Per Thumbnail (2x HiDPI) | Total (2x HiDPI) |
|----------|--------------------|--------------------------|--------------------|
| 20 pages | ~56 KB | ~224 KB | ~4.5 MB |
| 100 pages | ~56 KB | ~224 KB | ~22 MB |
| 200 pages | ~56 KB | ~224 KB | ~45 MB |
| 500 pages | ~56 KB | ~224 KB | ~110 MB |

All thumbnails are cleared on file open or sidebar resize. No cap is imposed — if memory becomes an issue with very large files, an LRU eviction policy can be added later.

## Testing Strategy

### Unit Tests

| Component | Coverage |
|-----------|----------|
| `thumbnail_dpi` calculation | Various sidebar widths, scale factors, page dimensions |
| Thumbnail coordinate scaling | PDF-to-thumbnail conversion at various DPI values |
| Batch page range calculation | Visible range, buffer, backfill ordering |
| Shimmer animation offset | Time-based gradient position |
| Width clamping | Values at, below, and above min/max bounds |
| Generation-based staleness | Stale backfill results discarded |

### Integration Tests

| Test | `#[ignore]`? |
|------|-------------|
| Batch `pdftoppm` render: 5 pages in one call, verify 5 images returned | Yes (needs pdftoppm) |
| Batch render at low DPI: verify image dimensions match expected | Yes (needs pdftoppm) |
| Thumbnail round-trip: render → cache → draw overlay → verify no crash | No |

### E2E Tests

| Test | Workflow |
|------|---------|
| Sidebar toggle | F9 → verify visible state → F9 → verify hidden |
| Thumbnail click navigation | Click page 3 thumbnail → verify canvas scrolled to page 3 |
| Sidebar scroll independence | Scroll sidebar → verify canvas scroll unchanged |
| Current page highlight | Navigate to page 2 → verify page 2 thumbnail highlighted |
| Sidebar resize | Drag handle → verify width changed → verify thumbnails re-render |

## File Changes

### Modified Files

- **`src/ui/sidebar.rs`** — Expand from placeholder to full thumbnail sidebar with canvas widgets, shimmer placeholders, resize handle, scroll tracking.
- **`src/app.rs`** — Add new message handlers, expand `SidebarState`, wire up `event::listen()` for resize drag, integrate batch rendering pipeline, update `fit_to_width` to use dynamic sidebar width.
- **`src/pdf/renderer.rs`** — Add `render_page_batch()` function for multi-page `pdftoppm` calls.

### New Files

None. All functionality fits within existing modules.

## What Is Not in This Design

- **Synchronized sidebar scrolling** — Sidebar and canvas scroll independently. No auto-scroll to keep current page visible in sidebar.
- **Thumbnail context menu** — No right-click menu on thumbnails.
- **Drag-to-reorder pages** — Out of scope. This is a viewer/overlay tool, not a page editor.
- **Thumbnail size slider** — Resize handle controls width; no separate zoom control for thumbnails.
- **LRU eviction** — All thumbnails cached in memory. No eviction policy until proven necessary.
- **No-document state** — When no document is loaded, the sidebar is empty (no thumbnails, no scroll content). The sidebar toggle still works but shows nothing. Sidebar state resets on file open.
