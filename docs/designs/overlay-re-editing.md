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
- box width and minimum box height (absent for unwrapped overlays)

This is exactly the data needed to reconstruct the in-memory `TextOverlay` model.

On open, if the entry exists and validates, the app strips its own overlay and
q-prefix content streams (and the metadata entry) from the in-memory document,
saves that stripped copy to a temp file, and uses the temp file as the render
and save source. The rendered page then shows only the original content while
the restored overlays sit editable on top — never both at once. Re-save just
bakes the current overlays onto the stripped source and embeds fresh metadata;
no stream removal happens at save time.

This is cheap for this codebase specifically because the writer emits overlays as
a separate content stream appended to each page's `Contents` array, never merged
into the original content — so stripping our streams back out on open is tractable.

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

Re-editing works only in this app. If another tool touches the app's own streams,
the staleness guard demotes the file to plain flat. An external edit that leaves
those streams intact (e.g. altering only the original page content) still
restores the overlays — over the edited base — which is safe because stripping
only ever removes byte-verified app streams, never the document's own content.
This is accepted: the alternative (keeping text editable in any viewer) requires
live AcroForm fields and appearance-stream generation, which is deliberately out
of scope here.

## Testing

- **Unit**: metadata round-trips (serialize → parse → identical overlay model)
  against in-memory PDFs; fingerprint mismatch detection.
- **Integration**: save → reopen → overlays restored with correct geometry and
  styling; file tampered between save and reopen → opens flat with warning.
- **E2E**: open → place text → save → reopen → edit → re-save.
