# ADR-008: CI Pipeline Parallelization

## Context

The CI pipeline ran all checks sequentially in a single `check` job: secrets scanning, Rust toolchain install, dependency audit, formatting, linting, unit tests, and integration tests. This meant every step waited on all previous steps, even when there were no real dependencies between them. Slow steps like `cargo install cargo-audit`, `cargo clippy`, and `cargo test` blocked each other unnecessarily, increasing wall-clock time for every PR.

Separately, GitHub's default CodeQL analysis was enabled in the repository security settings. CodeQL performs deep dataflow and taint analysis, which is valuable for web-facing applications with user-controlled inputs flowing through complex code paths. However, this is a desktop PDF editor with no network-facing attack surface. Semgrep (pattern-based SAST) and `cargo audit` / `cargo-deny` already cover the realistic vulnerability surface. CodeQL added significant CI time for minimal additional security value in this context.

## Decision

Split the monolithic `check` job into independent parallel jobs:

- **secrets**: betterleaks scan (no Rust toolchain needed)
- **fmt**: `cargo fmt --check`
- **audit**: `cargo install cargo-audit && cargo audit`
- **lint**: `cargo clippy -- -D warnings`
- **test**: `cargo test` (unit tests)
- **integration**: `cargo test -- --ignored` (depends on `test` passing first)

The `supply-chain` job was already independent and remains unchanged.

Add `Swatinem/rust-cache` to all Rust jobs to offset the cost of each job installing the toolchain and building dependencies independently.

Remove CodeQL from the repository security settings in favor of the existing Semgrep + `cargo audit` + `cargo-deny` coverage.

## Trade-offs

**Parallel jobs (chosen)** vs. **single sequential job (status quo)**:
- Faster wall-clock time per PR at the cost of more total runner minutes
- Each job installs the Rust toolchain independently; mitigated by `rust-cache`
- More granular failure reporting: a lint failure doesn't hide behind a slow audit step
- Slightly more complex workflow file

**Removing CodeQL** vs. **keeping CodeQL alongside Semgrep**:
- Loses deep taint-tracking analysis, which is low-value for a desktop app with no network inputs
- Eliminates the slowest CI check
- Semgrep + `cargo audit` + `cargo-deny` provide sufficient SAST and dependency vulnerability coverage for this project's threat model
- If the project later adds network-facing features (e.g., remote file access), this decision should be revisited
