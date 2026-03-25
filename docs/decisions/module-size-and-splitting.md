# Module Size and Splitting Convention

Decision: When a file exceeds ~500 lines of production code or ~1000 total lines (including tests), split it into a directory module with sibling files organized by concern.

Rationale: Large files degrade both human readability and AI-assisted development. When an LLM agent needs to work on a handler, it shouldn't have to read 250 lines of view code to find it. Smaller, focused files let agents (and humans) read a complete concern in one pass and reason about it with full context. The split-by-concern pattern (handlers, view, pure functions, tests) aligns with Rust's support for multiple `impl` blocks across files within the same module.
