# ADR-015: Overlay Re-Editing via Embedded Private Metadata

## Context

A saved PDF bakes overlays into page content streams and keeps no record of what
was an overlay. Reopening a saved file treats prior overlays as page pixels, so
fixing a typo later means covering it with a new overlay or redoing the file.
Re-editability is also a prerequisite for the roadmap ahead (implied-field
filling of flattened forms, then AcroForm support): filled fields are far more
valuable if they can be reopened and corrected.

## Decision

Keep saving the same flat, viewer-friendly PDF, and additionally embed a private
metadata stream (an `/SPEOverlays` catalog entry holding versioned JSON) that
records each overlay's page, position, text, font family, size, and box width —
enough to reconstruct the in-memory `TextOverlay` model. On open, valid metadata
lifts overlays back into editable state; on re-save, the app removes its previous
per-page overlay streams and regenerates streams and metadata. The metadata
fingerprints (hashes) each overlay stream the app wrote; a missing or altered
stream means the file was edited elsewhere, so the app distrusts the metadata,
opens the file as plain flat, and says why.

This is cheap here because the writer already emits overlays as a separate
content stream appended to each page's `Contents` array, never merged into the
original content.

## Trade-offs

- **Re-edit works only in this app.** Rejected alternative: keep text in live
  AcroForm fields (`/V` values), which any PDF tool can edit but requires
  appearance-stream generation — notoriously flaky across viewers and contrary
  to the app's bake-text-in model.
- **Bake-only status quo rejected:** maximally simple and interoperable, but
  permanently flattens every save.
- Edits made by other tools demote the file to plain flat (by design, via the
  fingerprint guard) — prior overlays become uneditable pixels again.
- Saved files carry a small amount of extra private data; other viewers ignore
  it.
