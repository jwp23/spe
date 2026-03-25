# Signature Fonts Design

Bundled cursive signature fonts with a unified font model that replaces the current `Standard14Font`-everywhere approach with a `FontId` + `FontRegistry` architecture.

## Problem

The app currently hardcodes `Standard14Font` as the font type throughout the codebase. This has two issues:

1. **Canvas rendering is broken** — the font picker dropdown changes the data model but Iced renders all overlay text in the same default typeface. Users cannot see which font they've selected.
2. **No custom font support** — users want to digitally sign documents using a cursive/script font. Standard 14 PDF fonts are all serif/sans-serif/monospace — none look like a signature. Supporting custom fonts requires font embedding, which the current model doesn't support.

## Architecture

### Unified Font Model

A `FontRegistry` built at startup holds all known fonts. Each font is identified by a lightweight `FontId` (a `Copy + Eq + Hash` value type) used in overlays, messages, undo/redo commands, and serialization.

```
FontRegistry (owned by App)
  ├── FontEntry { id: FontId(0), name: "Helvetica", embedding: BuiltIn, ... }
  ├── FontEntry { id: FontId(1), name: "Helvetica Bold", embedding: BuiltIn, ... }
  ├── ...13 more Standard 14 entries...
  ├── FontEntry { id: FontId(14), name: "Great Vibes", embedding: TrueType { bytes }, ... }
  ├── FontEntry { id: FontId(15), name: "Dancing Script", embedding: TrueType { bytes }, ... }
  ├── FontEntry { id: FontId(16), name: "Pinyon Script", embedding: TrueType { bytes }, ... }
  └── FontEntry { id: FontId(17), name: "Pacifico", embedding: TrueType { bytes }, ... }
```

### FontEntry

Each entry carries everything needed to use the font across the app:

| Field | Type | Purpose |
|-------|------|---------|
| `id` | `FontId` | Lightweight identifier stored in overlays and messages |
| `display_name` | `String` | Shown in the font picker |
| `pdf_name` | `String` | Used in PDF font dictionaries |
| `iced_font` | `iced::Font` | Descriptor for canvas rendering |
| `embedding` | `PdfEmbedding` | How the PDF writer handles this font |
| `widths` | `WidthTable` | Per-character width data for bounding boxes and word wrap |

### PdfEmbedding

```rust
pub enum PdfEmbedding {
    BuiltIn,                          // Standard 14: reference by name, no embedding
    TrueType { bytes: &'static [u8] }, // Bundled: embed font program in PDF
}
```

### Width Tables

- **Standard 14 fonts**: Existing hardcoded AFM width tables (currently in `coordinate.rs`) move into the registry as `WidthTable` data on each entry.
- **Bundled TrueType fonts**: Width data extracted at startup from the TTF `hmtx` table using `ttf-parser` (zero-alloc, read-only crate). Parsing an in-memory font takes microseconds.

## Bundled Fonts

Four OFL-licensed cursive/script fonts from Google Fonts:

| Font | Style | Use Case |
|------|-------|----------|
| Great Vibes | Formal script | Serious signatures, legal documents |
| Dancing Script | Casual handwriting | Everyday default signature |
| Pinyon Script | Copperplate calligraphy | Elegant, traditional |
| Pacifico | Playful brush script | Informal, personality |

### Asset Pipeline

Font files are embedded at compile time using `include_bytes!()`, following the existing Phosphor icon font pattern.

```
assets/
  fonts/
    great-vibes.ttf
    dancing-script.ttf
    pinyon-script.ttf
    pacifico.ttf
    OFL.txt
  icons/
    phosphor-subset.ttf
```

Fonts are loaded into Iced's font system at startup alongside the icon font, using `iced::font::load()`.

## Canvas Font Rendering

Each `FontEntry` carries an `iced_font` descriptor. The canvas overlay renderer looks up the entry by `FontId` and passes the descriptor to Iced's text drawing.

Standard 14 fonts map to system font families:
- Helvetica variants → `Family::SansSerif` with appropriate weight/style
- Times variants → `Family::Serif` with appropriate weight/style
- Courier variants → `Family::Monospace` with appropriate weight/style
- Symbol/ZapfDingbats → `Family::SansSerif` (best-effort fallback)

Bundled fonts use `Family::Name("Great Vibes")` etc., registered via `iced::font::load()`.

This means Standard 14 fonts render as system equivalents (not exact PDF counterparts) but are visually distinct — a significant improvement over the current state where all text looks identical.

## PDF Writer Changes

The writer dispatches on `PdfEmbedding` when creating font objects:

**`BuiltIn`** (unchanged): Type1 font dictionary with `BaseFont` name. No font program.

**`TrueType`** (new):
- TrueType font dictionary: `Type: Font`, `Subtype: TrueType`, `BaseFont: <name>`
- Font descriptor object with flags, bounding box, italic angle
- Font program stream (`FontFile2`) containing the full TTF bytes
- `Widths` array and `FirstChar`/`LastChar` from the TTF cmap/hmtx tables

Existing font resource naming and deduplication logic (`F_ovl_0`, etc.) stays the same.

`ToUnicode` CMap for text extraction/copy-paste is desirable but can be a follow-on task — text renders correctly in readers without it. This should be tracked as a separate issue during planning so it doesn't get lost.

## Refactoring Scope

Modules affected by the `Standard14Font` → `FontId` + `FontRegistry` migration:

| Module | Change |
|--------|--------|
| `src/fonts.rs` (new) | `FontId`, `FontEntry`, `FontRegistry`, `PdfEmbedding`, `WidthTable` types; registry construction |
| `src/overlay.rs` | `TextOverlay.font` becomes `FontId`; `Standard14Font` moves to font module as internal type |
| `src/coordinate.rs` | `overlay_bounding_box` and `word_wrap` take registry/entry instead of `Standard14Font`; AFM tables move to font module |
| `src/app/` | `ToolbarState.font` becomes `FontId`; messages use `FontId`; `FontRegistry` owned by app |
| `src/ui/toolbar.rs` | Pick list iterates `registry.all()`; display names from `FontEntry` |
| `src/ui/canvas/` | Overlay rendering uses `entry.iced_font`; bounding box via registry |
| `src/ui/icons.rs` | `include_bytes!()` path updates for `assets/icons/` |
| `src/pdf/writer.rs` | Dispatches on `PdfEmbedding`; new TrueType embedding path |
| `src/command.rs` | `ChangeOverlayFont` stores `FontId` |
| `src/ipc.rs` | Font name → `FontId` lookup via registry |
| `tests/` | Integration and E2E tests that construct `TextOverlay` with `Standard14Font` must migrate to `FontId` |

## Styled Font Picker (Polish Pass)

After the core feature works, enhance the font picker dropdown to render each font name in its own typeface. This requires either customizing Iced's `pick_list` renderer or building a custom dropdown widget. Implemented as a separate task after the font pipeline is complete.

## Out of Scope

- Dedicated signature button/widget in the toolbar
- System font discovery via `fc-list`
- Font subsetting for PDF embedding
- Arbitrary user-provided fonts

## Implementation Order

1. **Asset reorganization** — move Phosphor to `assets/icons/`, create `assets/fonts/` structure
2. **Font model refactor** — `FontId` + `FontRegistry`, migrate all modules from `Standard14Font`
3. **Canvas font rendering fix** — Standard 14 fonts visually distinct via `iced::Font` descriptors
4. **TrueType PDF embedding** — new path in `writer.rs` for `PdfEmbedding::TrueType`
5. **Bundled cursive fonts** — download TTFs, wire into registry, end-to-end signature workflow
6. **Styled font picker** — each name rendered in its own typeface

## Dependencies

- `ttf-parser` crate (read-only TTF parsing, zero-alloc, widely used)
- Google Fonts TTF files: Great Vibes, Dancing Script, Pinyon Script, Pacifico (all SIL Open Font License)
