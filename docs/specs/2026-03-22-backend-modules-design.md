# Backend Modules Design: Overlay Model, PDF Writer, PDF Renderer

This spec defines the three backend modules that form the core data pipeline: a text overlay data model, a PDF writer that bakes overlays into existing PDFs, and a PDF renderer that rasterizes pages for display.

## Design Decisions

These decisions were made during brainstorming and constrain the design:

- **Standard 14 fonts only.** No font embedding. The 14 built-in PDF fonts (Helvetica, Times-Roman, Courier, and variants) are referenced by name. Font discovery (`fc-list`) is not needed and its beads issue (`spe-6zr`) is deleted.
- **PDF points coordinate system.** Overlay positions are stored in PDF coordinate space: points (1/72 inch), origin at the bottom-left corner of the page. The UI layer converts screen coordinates to PDF points before creating overlays.
- **`thiserror` for error types.** Each module defines a typed error enum. Callers can match on specific failure modes.
- **Trait abstraction for system utilities only.** The renderer wraps `pdftoppm` (a subprocess) and gets a trait per ADR-004. The writer uses `lopdf` (a library) and does not need a trait — it is tested with real PDF data.
- **Implementation order:** overlay model → writer → renderer. The model has no dependencies. The writer consumes the model. The renderer is independent.

## Module 1: Overlay Model

**File:** `src/overlay.rs`
**Beads:** `spe-31e`

### Types

**`Standard14Font`** — An enum with all 14 PDF built-in fonts:

| Variant | `pdf_name()` |
|---------|-------------|
| `Helvetica` | `"Helvetica"` |
| `HelveticaBold` | `"Helvetica-Bold"` |
| `HelveticaOblique` | `"Helvetica-Oblique"` |
| `HelveticaBoldOblique` | `"Helvetica-BoldOblique"` |
| `TimesRoman` | `"Times-Roman"` |
| `TimesBold` | `"Times-Bold"` |
| `TimesItalic` | `"Times-Italic"` |
| `TimesBoldItalic` | `"Times-BoldItalic"` |
| `Courier` | `"Courier"` |
| `CourierBold` | `"Courier-Bold"` |
| `CourierOblique` | `"Courier-Oblique"` |
| `CourierBoldOblique` | `"Courier-BoldOblique"` |
| `Symbol` | `"Symbol"` |
| `ZapfDingbats` | `"ZapfDingbats"` |

Derives: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

**`PdfPosition`** — A point on a PDF page.

| Field | Type | Description |
|-------|------|-------------|
| `x` | `f32` | Horizontal offset from left edge, in points |
| `y` | `f32` | Vertical offset from bottom edge, in points |

Derives: `Debug`, `Clone`, `Copy`, `PartialEq`.

**`TextOverlay`** — A text overlay placed on a PDF page.

| Field | Type | Description |
|-------|------|-------------|
| `page` | `u32` | 1-indexed page number (matches lopdf) |
| `position` | `PdfPosition` | Where the text baseline starts |
| `text` | `String` | The text content |
| `font` | `Standard14Font` | Which font to use |
| `font_size` | `f32` | Font size in points |

Derives: `Debug`, `Clone`, `PartialEq`.

### Responsibilities

This module defines data structures only. It does not validate positions against page dimensions, convert coordinates, or perform any I/O.

### Testing

Unit tests co-located in `#[cfg(test)]`. Cover:
- Construction of all types
- `pdf_name()` returns the correct string for every `Standard14Font` variant
- Derive behavior (clone, equality, debug formatting)

## Module 2: PDF Writer

**File:** `src/pdf/writer.rs`
**Beads:** `spe-sjk`

### Error Type

`WriterError` via `thiserror::Error`:

| Variant | Fields | When |
|---------|--------|------|
| `OpenFailed` | `#[from] lopdf::Error` | Source PDF cannot be parsed |
| `PageNotFound` | `requested: u32, total: u32` | Overlay references a nonexistent page |
| `SaveFailed` | `path: PathBuf, source: io::Error` | Output file cannot be written |

### Public API

```rust
pub fn write_overlays(
    source: &Path,
    destination: &Path,
    overlays: &[TextOverlay],
) -> Result<(), WriterError>
```

A free function. No struct, no trait. Opens the source PDF, applies all overlays, and saves to destination.

### Algorithm

1. Load the source PDF with `lopdf::Document::load(source)`.
2. Group overlays by page number.
3. For each page that has overlays:
   a. Read the page's existing font resources to avoid naming collisions.
   b. For each unique `Standard14Font` used on this page, add a font resource entry (e.g., `/F1` → `/Helvetica`). Choose names that do not collide with existing resources.
   c. Build a PDF content stream with text-showing operators:
      - `BT` (begin text)
      - `/{name} {size} Tf` (select font)
      - `{x} {y} Td` (position)
      - `({text}) Tj` (show text)
      - `ET` (end text)
   d. Add the content stream as a new stream object and append its reference to the page's `Contents` array. This preserves the page's existing content.
4. Save the modified document to `destination`.

### Edge Cases

- **Empty overlays slice:** Return `Ok(())` immediately. No destination file is created — the caller must not assume the destination exists after a successful return with zero overlays.
- **Multiple overlays on the same page with different fonts:** Each unique font gets its own resource entry. A single content stream per page contains all overlays for that page.
- **Special characters in text:** PDF string encoding. For Standard 14 fonts, text uses PDFDocEncoding (a superset of Latin-1). Characters outside this range are not supported in the initial implementation.

### Testing

Unit and integration tests create real PDFs with `lopdf`:
- Create a minimal single-page PDF in memory
- Write overlays to a temp file
- Re-open the output with lopdf
- Verify: font resources exist, content streams contain expected operators, page count unchanged
- Error paths: nonexistent source file, invalid page number

## Module 3: PDF Renderer

**File:** `src/pdf/renderer.rs`
**Beads:** `spe-oqx`

### Error Type

`RendererError` via `thiserror::Error`:

| Variant | Fields | When |
|---------|--------|------|
| `NotInstalled` | — | `pdftoppm` binary not found. Display message includes install instructions. |
| `RenderFailed` | `page: u32, path: PathBuf, detail: String` | `pdftoppm` exits non-zero. `detail` is captured stderr. |
| `ImageDecodeFailed` | `String` | Output image cannot be decoded by the `image` crate. |

### Trait

```rust
pub trait PageRenderer {
    fn render_page(
        &self,
        pdf_path: &Path,
        page: u32,
        dpi: u32,
    ) -> Result<DynamicImage, RendererError>;
}
```

This trait exists so that callers (the app layer) can use a test double in unit tests without requiring `pdftoppm` to be installed.

### Production Implementation: `PdftoppmRenderer`

An empty struct implementing `PageRenderer`.

**`render_page` algorithm:**

1. Check that `pdftoppm` is available (e.g., `which pdftoppm` or attempt to run with `--help`). Fail fast with `RendererError::NotInstalled` if missing.
2. Create a temporary file for output.
3. Invoke: `pdftoppm -f {page} -l {page} -r {dpi} {pdf_path} {temp_prefix}`
   - `-f` / `-l`: first and last page (same value for single-page render)
   - `-r`: resolution in DPI
   - Output is PPM format (pdftoppm's default)
4. Check exit status. On non-zero, capture stderr and return `RenderFailed`.
5. Locate the output file (pdftoppm appends page number to the prefix).
6. Decode the PPM file with the `image` crate into a `DynamicImage`.
7. Clean up the temporary file.
8. Return the image.

**Why temp files:** pdftoppm's stdout behavior varies across poppler versions. Writing to a temp file is reliable everywhere.

### Testing

- **Unit tests** (`#[cfg(test)]`): Test error type construction and display messages. A `MockRenderer` struct (defined in tests, not production code) implements `PageRenderer` for testing callers.
- **Integration tests** (`tests/pdf_rendering.rs`, `#[ignore]`): Use `PdftoppmRenderer` with a real test PDF fixture. Verify the returned image has valid dimensions and pixel data.

## What Is Not in This Design

- **Font discovery (`fonts.rs`)** — Not needed for Standard 14 fonts. The beads issue `spe-6zr` is deleted.
- **Background pre-rendering and caching** — App layer concern. The renderer is a synchronous, single-page operation.
- **GUI modules** (`app.rs`, `ui/canvas.rs`, `ui/toolbar.rs`) — Separate design cycle.
- **Coordinate conversion** — UI layer responsibility. The overlay model stores PDF points; the UI converts screen clicks using DPI and page dimensions.
- **Page dimension queries** — Will emerge during writer or app layer work. Not a separate module.

## Dependencies

Add to `Cargo.toml`:
- `thiserror` in `[dependencies]`
- `tempfile` in `[dependencies]` (used by the renderer's production code for temp file management)

Note: `tempfile` is a regular dependency, not dev-only, because `PdftoppmRenderer::render_page` creates temporary files as part of its normal operation.

## Verification

After all three modules are implemented:

1. `cargo fmt --check` — no formatting issues
2. `cargo clippy -- -D warnings` — no lint warnings
3. `cargo test` — all unit tests pass (no system utilities required)
4. `cargo test -- --ignored` — integration tests pass (requires `pdftoppm`)
5. Manual verification: create a test PDF with overlays, open in a PDF viewer, confirm text appears at expected positions
