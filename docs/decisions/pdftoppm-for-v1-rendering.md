# pdftoppm for v1 Rendering

Decision: Continue using `pdftoppm` (poppler-utils) for PDF page rasterization in v1. Defer `pdfium-render` as a potential future upgrade path.

Rationale: No GPU-accelerated PDF renderer exists in the Rust ecosystem. `pdfium-render` wraps Google's PDFium (CPU-based, C++ dependency) and would eliminate subprocess overhead and temp files, but adds a large native dependency. `pdftoppm` is already implemented, tested, and widely available on Linux. With lazy thumbnail loading and debounced zoom re-rendering, subprocess overhead is manageable for a desktop app.
