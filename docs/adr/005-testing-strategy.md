# ADR-005: Testing Framework and Strategy

## Context

The project requires a testing strategy that supports mandatory TDD (red/green/refactor), covers the test pyramid (unit, integration, E2E), and handles system utility dependencies gracefully in test environments.

## Decision

**Framework:** Rust's built-in test framework (`cargo test`). No additional test runner needed.

**Unit tests:** Co-located with source code in `#[cfg(test)]` modules, following idiomatic Rust convention. Cover all public functions and methods. System utility wrappers are tested via trait-based test doubles — unit tests verify the wrapper logic without requiring the actual utility to be installed.

**Integration tests:** Located in the `tests/` directory at the project root. Test component interactions with real system utilities. Tests requiring `pdftoppm` or `fc-list` are marked with `#[ignore]` so they don't run by default but can be invoked explicitly with `cargo test -- --ignored`. CI runs both default and ignored tests.

**E2E tests:** Cover the full pipeline — render a PDF page, create text overlays, write the result to a new PDF, and verify the output. These live in `tests/` and exercise the real system utilities. They do not test the GUI layer directly (Iced lacks a test harness), but they verify every step of the data pipeline that the GUI orchestrates.

**TDD workflow:** Every new function follows red/green/refactor. Write the test, run it, confirm it fails. Write the minimum implementation. Refactor. No exceptions.

**Coverage target:** All public functions and methods must have tests. No specific percentage target — 100% of the public API surface is the goal.

## Trade-offs

**Considered: nextest** — Parallel test runner with better output formatting. Unnecessary for the current project size; can be adopted later if the test suite becomes slow.

**Considered: proptest** — Property-based testing. Valuable for coordinate math and PDF content stream generation, but adds complexity. Will evaluate when those modules are implemented.

**Giving up:** Parallel test execution (nextest), property-based testing (proptest), GUI-level testing. **Gaining:** Zero additional dependencies for testing, idiomatic Rust test organization, and a clear separation between tests that need system utilities and those that don't.
