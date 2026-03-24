# Text Input: Click vs Drag Gesture

Decision: Single-click places a single-line auto-growing text input. Click-and-drag creates a fixed-width multi-line text box where the drag width defines the wrap boundary.

Rationale: The two dominant text overlay use cases (short labels vs multi-line blocks like addresses) need different input behaviors. An implicit gesture — click for single-line, drag for multi-line — is natural, discoverable, and matches conventions from tools like Acrobat and Xournal++. Alternatives considered: explicit toolbar toggle (adds UI state to manage), shift-click modifier (less discoverable). The gesture approach requires no mode switching and maps cleanly to the underlying data model.
