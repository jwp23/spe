# Drag State in Widget-Local ProgramState

Decision: Track overlay drag state (grab offset, current position) in the canvas `ProgramState` (widget-local, managed by Iced) rather than in App's `CanvasState`. Only publish a single `MoveOverlay` message on mouse release with the final position.

Rationale: Publishing a message per pixel of cursor movement during drag would flood the App's update loop and create an undo entry per pixel. Keeping drag state widget-local avoids this — the canvas handles visual feedback during drag internally, and the App only sees the final result. This produces a clean single-entry undo for the entire drag operation.
