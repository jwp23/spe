# Text Input: Widget Overlay on Canvas

Decision: Use Iced's `text_input` (single-line) or `text_editor` (multi-line) widget positioned on top of the canvas via `stack` layout at the overlay's screen coordinates. The canvas renders all inactive overlays; the widget handles editing for the active overlay.

Rationale: Native Iced widgets provide clipboard, selection, IME, and arrow key support for free. Positioning the widget at the overlay's screen coordinates keeps editing spatially anchored to where the text will appear. Alternatives considered: canvas-only keyboard capture (requires reimplementing a full text editor from scratch — enormous effort and bug surface), side panel (disconnected from the spatial editing metaphor). The widget overlay approach bounds complexity to coordinate math, which the project already has.
