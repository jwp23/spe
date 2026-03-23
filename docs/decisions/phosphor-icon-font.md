# Phosphor Icon Font (Subsetted)

Decision: Use Phosphor Icons for toolbar iconography, subsetted to ~12 glyphs and bundled as a TTF in the binary.

Rationale: The toolbar needs icons for file ops, undo/redo, zoom, page nav, and sidebar toggle. Phosphor has a clean geometric style popular with desktop/native apps, is MIT licensed, and supports multiple weights. Subsetting avoids bundling 9000+ icons — only the ~12 glyphs needed are extracted using a font subsetting tool (e.g., `pyftsubset`). The subset process is documented so icons can be added later.
