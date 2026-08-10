# Overlay Re-Editing

A PDF saved by this app can be reopened later and its overlays return as editable
overlays — move, retype, resize, restyle, delete — then re-save. To every other
viewer the saved file is a plain flat PDF.

## Mechanism

On save, alongside the baked overlay content stream the writer already emits, the
app embeds a private metadata stream in the document: an `/SPEOverlays` entry in
the document catalog holding versioned JSON. For each overlay it records:

- page number
- position (PDF points)
- text
- font family name
- font size
- box width (absent for unwrapped overlays)

This is exactly the data needed to reconstruct the in-memory `TextOverlay` model.

On open, if the entry exists and validates, the app lifts the overlays back into
the editor instead of treating them as page pixels. On re-save, it removes the
previous per-page overlay streams, then regenerates both the streams and the
metadata.

This is cheap for this codebase specifically because the writer emits overlays as
a separate content stream appended to each page's `Contents` array, never merged
into the original content — so "remove and regenerate our stream" is tractable.

## Staleness Guard

The metadata stores a fingerprint (hash) of each overlay content stream the app
wrote. On open, if a fingerprinted stream is missing or its bytes do not match —
the file was edited in another tool — the metadata is not trusted: the app opens
the file as plain flat, tells the user why, and leaves the file alone. No silent
corruption, no guessing.

## Font Edge Case

Metadata names font families; on reopen the app re-resolves them through the
existing fc-list discovery. A font that has since been uninstalled falls back to
the default family with a visible warning, the same as any missing-font path.

## Interoperability Trade-off

Re-editing works only in this app. If another tool edits the PDF in between, the
staleness guard demotes the file to plain flat. This is accepted: the alternative
(keeping text editable in any viewer) requires live AcroForm fields and
appearance-stream generation, which is deliberately out of scope here.

## Testing

- **Unit**: metadata round-trips (serialize → parse → identical overlay model)
  against in-memory PDFs; fingerprint mismatch detection.
- **Integration**: save → reopen → overlays restored with correct geometry and
  styling; file tampered between save and reopen → opens flat with warning.
- **E2E**: open → place text → save → reopen → edit → re-save.
