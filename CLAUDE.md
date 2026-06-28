# PDF Text Overlay Editor

A desktop GUI application for Linux that opens PDF documents, renders pages visually, and lets users click anywhere on a page to place text overlays. Users control font and font size. The result is saved as a new PDF with text baked in.

@AGENTS.md

## Tech Stack

Rust (edition 2024) desktop GUI: Iced 0.14 (wgpu/Wayland); `pdftoppm` rendering; `lopdf` writing; `fc-list` fonts; `rfd` dialogs; `cargo test` + `rustfmt`/`clippy`; GitHub Actions CI.

Full stack detail and per-choice rationale: see `@docs/tech-stack-docs.md` and `docs/adr/`.

## Decision Recording

Recording technical decisions: see `.claude/rules/decision-recording.md`.

## What This Project Does

- Opens and renders PDF pages in a desktop GUI
- Lets users click on a rendered page to position a text cursor
- Users type text that overlays the original PDF content
- Users can select font family and font size
- Saves the result as a new PDF with overlaid text embedded

## What This Project Does NOT Do

- Does NOT edit, modify, or extract existing text in the PDF
- No annotations (highlights, sticky notes, drawing, markup)
- No multi-user or collaboration features
- No cloud storage, network features, or remote file access
- Form-filling is not in initial scope (may be evaluated later — record as ADR if pursued)

## Linux System Utilities

The project uses these system utilities instead of pure-library alternatives:

- `pdftoppm` (poppler-utils) — PDF page rasterization
- `fc-list` (fontconfig) — discover installed system fonts

Each utility has a trait-based wrapper module for testability. See ADR-004.

When calling system utilities: use `std::process::Command` (never shell). Wrap failures with clear error messages stating what tool failed and how to install it.

## Code Style

Five mandatory principles — human readable, loosely coupled, idiomatic, simple, professional. Details and anti-patterns: see `@docs/code-style-guide.md`.

When style conventions and simplicity conflict, simplicity wins.

## Testing

### Test Pyramid (Mandatory)

- **Unit tests**: Cover all public methods and functions
- **Integration tests**: Cover how components work together
- **End-to-end tests**: Cover user workflows (open PDF → place text → save)

#### When to Use Each Level

| Code Under Test | Unit Test | Integration Test | E2E Test |
|----------------|-----------|-----------------|----------|
| Pure logic (overlay model, coordinate math) | Yes | — | — |
| System utility wrapper (`fc-list`, `pdftoppm`) | Yes (trait-based test double) | Yes (`#[ignore]`, needs real utility) | — |
| PDF writing (`lopdf` operations) | Yes (in-memory PDF) | Yes (read-back written file) | — |
| Component interaction (renderer + writer) | — | Yes | — |
| User workflow (open → place → save) | — | — | Yes |

- **Unit tests** use trait-based test doubles for system boundaries. They must pass without external utilities installed.
- **Integration tests** go in `tests/`, marked `#[ignore]` when they require system utilities. CI runs them with `cargo test -- --ignored`.
- **E2E tests** exercise the full user workflow with real files and real utilities.

### TDD (Mandatory)

TDD is mandatory for every feature/bugfix — see `.claude/rules/tdd.md`, which triggers the test-driven-development skill (full RED/GREEN/REFACTOR methodology).

### Test Framework

`cargo test`. Unit tests co-located in `#[cfg(test)]` modules. See ADR-005 for full strategy.

## Git Workflow

- **Commits**: Conventional Commits, single line only. No body, no footer. Examples:
  - `feat: add font size selector to overlay toolbar`
  - `fix: prevent crash when opening password-protected PDF`
  - `chore: add ruff to pre-commit hooks`
- **Lockfiles**: Always commit lockfiles regardless of language/package manager.

See `.claude/rules/git-workflow.md` for branch naming, PR workflow, worktrees, and merge policy.

## Pre-commit & CI

Pre-commit hook (`.beads/hooks/pre-commit`) runs after beads integration:
- `betterleaks git --pre-commit --staged --redact` (hard fail; hard fail if not installed)
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo audit` (optional — warns if not installed, fails if vulnerabilities found)
- `cargo test`

GitHub Actions CI (`.github/workflows/ci.yml`) runs secrets scanning (betterleaks via Docker), the same Rust checks, plus `cargo test -- --ignored` for integration tests.

See `docs/decisions/pre-commit-suite.md` and `docs/decisions/ci-pipeline.md`.

## Reference Documents

Read these only when the trigger condition applies:

- `@docs/code-style-guide.md` — Read when writing or reviewing code. Detailed examples and anti-patterns for the five code style principles.
- `docs/adr/*.md` — Read when making decisions related to an existing ADR, or when context on a past decision is needed
- `docs/decisions/*.md` — Read when working in an area covered by an existing decision
- `docs/decision-recording.md` — Read when recording a technical decision. ADR/decision-doc formats, numbering, and classification.
- `docs/architecture.md` — Read when navigating the module map, or when modifying component boundaries or data flow
- `@docs/tech-stack-docs.md` — Read when working with a library or framework API, or when you need documentation for a dependency

## Project Structure

Module map: see `docs/architecture.md` (read when navigating or modifying modules).
