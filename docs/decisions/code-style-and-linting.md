# Code Style and Linting

Decision: Use rustfmt for formatting and clippy (deny all warnings) for linting.

Rationale: Both are the Rust ecosystem standards and are already installed system-wide (rustfmt 1.8.0, clippy 0.1.94 via pacman). rustfmt uses default settings — no custom `rustfmt.toml` needed, as defaults match Rust community conventions. clippy runs with `-D warnings` to prevent lint warnings from accumulating. The five code style principles in `docs/code-style-guide.md` (human readable, loosely coupled, idiomatic, simple, professional) apply on top of these automated tools.
