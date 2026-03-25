# ADR-010: Dual-Canvas Stack for Z-Order Correctness

## Context

Iced 0.14's wgpu canvas renderer draws primitives in a fixed order within each rendering layer: quads (fill_rectangle) -> meshes (stroke) -> images (draw_image) -> text (fill_text). This means fill_rectangle and stroke_rectangle always render behind draw_image regardless of draw order in code.

Our app draws PDF page images with draw_image and overlay decorations (tint rectangles, selection borders, resize handles) with fill_rectangle/stroke_rectangle. The fixed ordering caused overlays to render behind page content.

A workaround using draw_image with stretched 1x1 pixel images was in place but allocated GPU textures on every frame and obscured intent.

See: https://github.com/iced-rs/iced/issues/3017

## Decision

Split the single PdfCanvasProgram into two canvas::Program implementations rendered in a Stack widget:

- **Layer 0 (PdfPagesProgram)**: Draws canvas background, white page backgrounds, and PDF page images. No event handling.
- **Layer 1 (OverlayCanvasProgram)**: Draws overlay tints, hover borders, selection boxes, resize handles, text, and drag previews using native fill_rectangle/stroke_rectangle. Handles all mouse and keyboard events.

The Stack widget renders each child after the first in a separate wgpu layer via Renderer::with_layer. Since each layer has its own independent primitive ordering, overlay fill_rectangle calls in layer 1 render on top of page draw_image calls in layer 0.

## Trade-offs

**Chosen approach: Stack with two canvas widgets**
- Uses Iced's recommended layer mechanism (confirmed by maintainer in iced-rs/iced#3017)
- Native fill_rectangle/stroke_rectangle — no GPU texture allocation per frame
- Clean separation of page rendering and overlay interaction
- Two canvas::Program implementations share state types (ProgramState, drag states) but each has its own draw method

**Rejected: draw_image workaround (previous approach)**
- Allocated a 1x1 RGBA Handle on every draw call, every frame
- Used four image strips to simulate a stroked rectangle border
- Code obscured the intent of "draw a colored rectangle"
- Worked but was a hack exploiting the same z-ordering rule it was fighting

**Rejected: Custom widget with Renderer::with_layer**
- Full control but required reimplementing canvas Frame/geometry management
- Significantly more code and maintenance burden
- No event handling framework — would need custom implementation
