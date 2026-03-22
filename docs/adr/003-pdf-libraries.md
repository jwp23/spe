# ADR-003: PDF Rendering and Writing Library Selection

## Context

The application has two distinct PDF operations:

1. **Rendering** — Convert a PDF page to a raster image for display in the GUI.
2. **Writing** — Add text at specific (x, y) coordinates on an existing PDF page and save as a new file.

These are fundamentally different operations and may use different tools.

## Decision

**Rendering: `pdftoppm`** (poppler-utils system utility, already installed).

Invoked via `std::process::Command`. Renders a specified PDF page to PPM format, which the `image` crate decodes into pixel data for Iced's image widget. Adjacent pages are pre-rendered in background threads for smooth page switching.

**Writing: `lopdf` 0.40** (Rust crate).

Opens existing PDF files, adds font resources to the page, and writes text content streams at specific coordinates. This is low-level — we work directly with PDF content stream operators (`BT`, `Tf`, `Td`, `Tj`, `ET`) — but it gives precise control over text placement without requiring an intermediate merge step.

## Trade-offs

**Rendering alternatives considered:**

- **pdfium-render** — In-process rendering via Google's PDFium engine. Faster per-page (no subprocess overhead) but requires distributing or downloading the PDFium shared library, a significant external binary dependency outside of Cargo's management.
- **mupdf-rs** — Rust bindings to MuPDF. Capable renderer but licensed under AGPL-3.0, which would impose copyleft requirements on the entire application.

**Writing alternatives considered:**

- **printpdf** — Higher-level API for text placement, but designed for creating new PDFs. Modifying existing PDFs (our core use case) would require creating overlay pages and a separate merge step.
- **pdfium-render** — Has some text creation support (added in v0.7.0+), but text writing is secondary to its rendering focus and less mature.

**Giving up:** In-process rendering speed (mitigated by background pre-rendering and caching), higher-level text writing API. **Gaining:** Zero external binary dependencies beyond standard Linux utilities, simple architecture, and the ability to modify existing PDFs directly.
