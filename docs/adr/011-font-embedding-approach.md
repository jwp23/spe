# ADR-011: Font Embedding Approach

## Context

The application needs to support custom fonts beyond the PDF Standard 14 set — specifically, bundled cursive/script fonts for digital signatures. This requires both rendering custom fonts on the Iced canvas and embedding font programs in PDF output. The project already uses `include_bytes!()` with `iced::font::load()` for the Phosphor icon font in the toolbar.

## Decision

Embed custom font `.ttf` files at compile time using `include_bytes!()`, following the existing Phosphor icon font pattern. Fonts are registered with Iced's font system at startup and their TrueType font programs are embedded in full in the PDF output via `lopdf`. The initial set is 2-3 curated OFL-licensed cursive/script fonts bundled with the binary.

## Trade-offs

- *Runtime font directory* — Would allow users to add their own fonts and keep the binary smaller, but adds deployment complexity and runtime failure modes for only 2-3 fonts. Rejected.
- *Subset embedding in PDF* — Would produce smaller PDFs by only including used glyphs, but adds significant complexity (font subsetting library, glyf table manipulation) that's overkill for signature-length text. Rejected.
- *System font discovery (`fc-list`)* — Deferred independently. This decision covers bundled fonts only; `fc-list` support can be layered on later without changing this approach.
