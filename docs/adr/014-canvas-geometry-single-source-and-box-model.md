# ADR-014: Canvas Geometry Single Source and Box Model

## Context

The canvas draws several things over an overlay — a rest/hover tint, a selection border, resize handles, drag previews — and also has to answer "is this point inside the overlay" for hit testing. Three separate bugs traced to these having grown independent, only loosely related notions of an overlay's box:

- spe-i4e/spe-ner (PR #124): the tint rectangle was anchored at the first text baseline and grew upward, while the canvas actually lays text out downward from the top, and its height ignored Iced's own line-height multiplier — the tint and the text it was supposed to sit behind disagreed about where the box was.
- spe-x2z (PR #131): selection box and resize-handle geometry were computed from a bounding box that didn't match the tint's box or the hit test's box, so selecting, dragging, and seeing the tint could each disagree about the overlay's extent.
- PR #131 also found that a single-line overlay's on-screen width, measured from the PDF's AFM width tables (the same tables the writer uses to lay out saved text), diverges from what Iced's text engine actually renders on screen — measured across the font registry, AFM widths run 0.42x to 1.65x the shaped canvas width. This is because the PDF Standard 14 fonts (Helvetica, Times, Courier, ...) map onto generic system font families for on-screen rendering, so the AFM table and the glyphs the canvas draws are never describing the same face.
- PR #140: drag-to-place kept only the drawn rectangle's start Y and treated it as a text baseline rather than a corner, so a dragged box opened floating a line high and one line tall regardless of how far the user actually dragged.

## Decision

**All canvas geometry derives from one function, `overlay_text_box` (`src/ui/canvas/mod.rs`).** Tint, selection border, hit testing (`overlay_text_box_contains_pdf`), resize handles, and drag previews all call this one function rather than each computing or caching their own notion of the overlay's box. It takes the overlay, screen offset, and scale, and returns the same `iced::Rectangle` every caller uses — a change to how the box is computed (e.g. the line-height fix) automatically applies everywhere it's drawn or tested against.

**Canvas-side text width is measured by the canvas's own text-shaping engine, not the PDF's AFM tables.** `src/ui/canvas/text_metrics.rs` measures shaped width per point of font size using Iced's own `Paragraph`/`Text` APIs (via the `advanced` feature) and caches it per `(FontId, string)`. The AFM tables remain authoritative for the *saved* PDF, where `lopdf` writes against the actual Standard 14 metrics — but the canvas draws with whatever family the system substitutes for a Standard 14 name, so no fixed padding or scale constant translating AFM widths to screen widths could be correct across the registry. Asking the same engine that will render the text is the only approach that stays correct as the substituted family changes.

**`min_height` is a floor on the box, not a saved-document property.** `TextOverlay::min_height` (`src/overlay.rs`) lets a dragged or resized box keep extra vertical space even when its text doesn't need it — `overlay_text_box` takes `text_height.max(min_height)`. The PDF writer (`src/pdf/writer.rs`) never reads `min_height`: it lays wrapped lines out downward from the first baseline and emits nothing for empty space below, so a taller box produces byte-identical saved output to a box just tall enough for its text (pinned by a writer test). The box height is purely an editing affordance.

**`TEXT_LINE_HEIGHT_RATIO` (`src/overlay.rs`, value `1.2`, matching Iced's own default relative line height) is the one constant every module agrees on.** The canvas's own text drawing, the multiline `text_editor` widget, the single-line `text_input` widget, and the PDF writer's line-spacing expectation all reference it (re-exported through `src/ui/canvas/mod.rs`), with a guard test pinning the value against Iced's actual default so an Iced upgrade that changes that default would fail the test rather than silently reintroducing the original spe-ner mismatch.

**At the page edge, size wins and position gives.** `drag_box_within` (`src/ui/canvas/mod.rs`) clamps both drag endpoints to the page bounds before computing the box, but flooring a box that ended exactly at a clamped edge can still widen it past that edge — there is no cursor position left to clamp that would prevent it. The box's minimum usable size takes priority over staying strictly inside the page bounds in that one case.

## Trade-offs

**Chosen: one geometry function, canvas-native text measurement, height as a non-persisted floor**
- A single source of truth means the tint/selection/hit-test/handle bugs (all stemming from divergent geometry) cannot recur independently of each other
- Canvas-native measurement is the only correct answer given the AFM/canvas font divergence, but it requires a cache (`text_metrics.rs`) to avoid re-shaping text every frame — accepted as a small, bounded (`MAX_CACHED_STRINGS`) cost
- Treating box height as purely visual means a user's dragged height is not information the saved PDF preserves; this is a deliberate simplification consistent with what this project does (baking text into PDFs, not laying out styled boxes) rather than an oversight

**Rejected (implicitly, by the fix history): per-feature geometry (tint's own box, selection's own box, hit test's own box)**
- This is what existed before PR #124/#131 and is what produced the three bugs above — each consumer's box could and did drift out of sync with the others
- Rejected because there is no way to keep N independent geometry computations in agreement except by construction (one function), not by convention
