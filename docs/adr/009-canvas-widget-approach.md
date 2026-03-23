# ADR-009: Canvas Widget Approach for PDF Page Display

## Context

The application needs to display rendered PDF pages and allow users to click-to-place, drag-to-move, and visually select text overlays. Iced 0.14 offers two approaches: the `canvas` widget (custom drawing surface with full event handling via `canvas::Program` trait) or layered built-in widgets (`image` + `mouse_area` + positioned overlay elements).

## Decision

Use `iced::widget::canvas` with a `canvas::Program` implementation (`PdfCanvasProgram`) rather than layering built-in widgets.

The Program borrows App state immutably (reconstructed every `view()` call), handles mouse events in `Program::update()`, and publishes Messages back to the App via `Action::publish()`. Drawing of the PDF image, overlay text, and selection indicators all happen in a single `Program::draw()` call using `Frame::draw_image()` and `Frame::fill_text()`.

Widget-local state (`ProgramState`) tracks transient cursor position and drag state. Persistent state (overlays, zoom, selection) lives in the App and flows to the Program as borrowed data.

## Trade-offs

**Canvas widget (chosen):**
- Single widget handles image display, overlay rendering, click detection, and drag interaction
- Direct coordinate mapping between mouse events and canvas drawing surface
- Full control over z-ordering (overlays always render on top of page image)
- More code to write (manual drawing primitives)

**Layered widgets (rejected):**
- Simpler initial setup using built-in `image` widget
- No built-in absolute positioning in Iced for overlays — would need to fight the layout system
- Click coordinate mapping between `mouse_area` and the image gets complex with zoom
- Drag-to-move overlays would require complex widget interaction

**Key factor:** Iced lacks built-in absolute positioning, making precise overlay placement over an image widget impractical. The canvas widget's direct coordinate system eliminates this problem entirely.
