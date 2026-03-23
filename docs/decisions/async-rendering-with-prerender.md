# Async Rendering with Neighbor Pre-rendering

Decision: Render PDF pages asynchronously via `iced::Task::perform()` and pre-render adjacent pages (current ± 1) in the background after the current page completes.

Rationale: pdftoppm is a subprocess call that takes 0.5-2 seconds per page. Synchronous rendering would freeze the UI. Using `iced::Task` keeps the UI responsive. Pre-rendering neighbors enables instant page navigation for the common case of sequential reading. Only pages not already cached are rendered, avoiding redundant work.
