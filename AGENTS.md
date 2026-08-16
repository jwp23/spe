# Agent Instructions

Instructions for AI coding agents working in this repository. Human contributors want
[CONTRIBUTING.md](CONTRIBUTING.md), which covers the same ground in less imperative language.

These rules belong to the project, not to any one tool. They assume no particular agent harness,
plugin, or slash command.

## What This Project Is

A desktop GUI application for Linux that opens PDF documents, renders pages visually, and lets
users click anywhere on a page to place text overlays. Users control font and font size. The
result is saved as a new PDF with the text baked in.

### In Scope

- Opening and rendering PDF pages in a desktop GUI
- Clicking a rendered page to position a text cursor
- Typing text that overlays the original PDF content
- Selecting font family and font size
- Saving the result as a new PDF with the overlaid text embedded
- Re-editing overlays in PDFs this application previously saved

### Out of Scope

Do not add these without an accepted ADR first:

- Editing, modifying, or extracting the PDF's existing text
- Annotations — highlights, sticky notes, drawing, markup
- Multi-user or collaboration features
- Cloud storage, networking, or remote file access
- Form filling

## Tech Stack

Rust (edition 2024) desktop GUI: Iced 0.14 (wgpu/Wayland); `pdftoppm` rendering; `lopdf`
writing; `fc-list` font discovery; `rfd` dialogs; `cargo test` with `rustfmt` and `clippy`;
GitHub Actions CI.

Per-choice rationale lives in `docs/adr/`. Library API documentation is indexed in
`docs/tech-stack-docs.md` — read it when working against a dependency's API.

## Linux System Utilities

This project shells out to system utilities rather than using pure-library equivalents:

- `pdftoppm` (poppler-utils) — PDF page rasterization
- `fc-list` (fontconfig) — installed font discovery

Each has a trait-based wrapper module so it can be tested without the utility present. See
ADR-004.

YOU MUST use `std::process::Command` to invoke them, never a shell. Wrap failures in an error
that names the tool that failed and how to install it.

## Test-Driven Development

YOU MUST write a failing test before writing implementation code, for every feature and every
bugfix. Run it, watch it fail for the reason you expect, then make it pass.

A bugfix without a test that reproduces the bug is incomplete.

## Testing

### The Test Pyramid

| Code under test | Unit | Integration | E2E |
|-----------------|------|-------------|-----|
| Pure logic (overlay model, coordinate math) | Yes | — | — |
| System utility wrapper (`fc-list`, `pdftoppm`) | Yes, via trait-based test double | Yes, `#[ignore]`, needs the real utility | — |
| PDF writing (`lopdf` operations) | Yes, in-memory PDF | Yes, read back the written file | — |
| Component interaction (renderer + writer) | — | Yes | — |
| User workflow (open, place text, save) | — | — | Yes |

- Unit tests cover every public function and method, including edge cases and error paths, and
  MUST pass without any system utility installed.
- Integration tests live in `tests/`, marked `#[ignore]` when they need a system utility. CI
  runs them with `cargo test -- --ignored`.
- E2E tests exercise the real workflow with real files and real utilities.

### Rules

- Tests mirror the source structure. Unit tests are co-located in `#[cfg(test)]` modules.
- Name each test for the behaviour it verifies.
- NEVER write a test that asserts on mocked behaviour rather than real logic.
- NEVER mock anything in an E2E test.
- Test output MUST be clean. When a test deliberately triggers an error, capture that output and
  assert on it rather than letting it print.
- Always test the failure path: what happens when a system utility is missing?

### Visual Verification

Changes to `src/ui/canvas/`, overlay drawing, coordinate math, or visual state transitions MUST
be verified visually before you call them done. See `docs/screenshot-tool.md`.

## Code Style

Five principles: human readable, loosely coupled, idiomatic, simple, professional. Details and
anti-patterns are in `docs/code-style-guide.md` — read it before writing code.

When a style convention and simplicity conflict, simplicity wins.

Name things for what they do in the domain, not for how they are implemented or what they used
to be. Comments explain what and why, never what changed.

## Debugging

Find the root cause. A change that suppresses a symptom without explaining the cause is not a
fix, and NEVER belongs in a commit described as one.

## Recording Decisions

A contribution that makes a technical choice MUST record it, *before* implementing rather than
after:

- **ADR** — `docs/adr/NNN-short-description.md` — language, framework, or major library
  selection; architecture patterns; testing strategy; CI/CD design. Copy
  `docs/adr/TEMPLATE.md`.
- **Decision doc** — `docs/decisions/short-description.md` — tool selection, naming conventions,
  file organization, configuration, hook setup. Copy `docs/decisions/TEMPLATE.md`.

Formats, numbering, and the full classification rules: `docs/decision-recording.md`.

## Git Workflow

- NEVER commit directly to `main`. Every change goes through a feature branch and a pull
  request.
- Branch names: `feat/`, `fix/`, `chore/`, `docs/`, `refactor/`, `test/` plus a short
  description.
- Commit messages are a single Conventional Commits line — no body, no footer:
  - `feat: add font size selector to overlay toolbar`
  - `fix: prevent crash when opening password-protected PDF`
- Pull request titles follow the same convention. Merges are squashed, so the title becomes the
  commit subject on `main`. CI rejects a title that does not conform.
- Commit lockfiles.
- Make the smallest reasonable change that achieves the outcome.
- NEVER discard or rewrite an existing implementation without asking first.

## Local Checks

`scripts/setup-hooks.sh` enables the committed hooks. `pre-commit` runs secrets scanning,
`cargo fmt`, `cargo clippy`, `cargo audit`, `cargo deny`, and the test suites; `commit-msg`
checks the commit convention.

A missing `betterleaks`, `cargo-audit`, or `cargo-deny` warns and skips that check; CI runs all
three regardless. Formatting, lint, and test failures always block the commit.

NEVER skip, evade, or disable a hook.

## Mutation Testing

Before pushing a branch that touches an in-scope Rust file, run cargo-mutants locally against
what you changed. This is a process check: Layer 1 (the required PR check) and Layer 2 (the
weekly full-scope run) back it up, but both run after the push — this is what catches a
survivor before either of them sees it. Full design: `docs/designs/mutation-testing.md`.

Enforcement scope lives once, negatively, in `.cargo/mutants.toml`'s `exclude_globs`. You do not
need to check your changed files against it yourself — `-f` respects the config's exclusions
even when a file is passed explicitly, so an excluded file in your diff costs nothing:
cargo-mutants finds zero mutants there and moves on.

```sh
CHANGED_RS=$(git diff --name-only main...HEAD -- '*.rs')
[ -z "$CHANGED_RS" ] && echo "No changed Rust files." || \
TMPDIR=~/.cache/cargo-mutants-tmp cargo mutants \
  $(printf -- '-f %s ' $CHANGED_RS) \
  -j 8 --timeout-multiplier 6 \
  -o ~/.cache/cargo-mutants-out \
  -- -- --include-ignored
```

`TMPDIR` must be real disk, never tmpfs `/tmp` — cargo-mutants copies the source tree plus a
private `target/` per job, and a parallel run on tmpfs risks OOM. `-o` writes outside the repo
(any directory works; `~/.cache/cargo-mutants-out` mirrors the `TMPDIR` convention above) so
`mutants.out/` never lands in your working tree. `-j8` was the ceiling on a 20-core machine;
oversubscription produces phantom timeouts, not speed, so tune it down on a smaller machine and
keep `--timeout-multiplier` at 6 or higher regardless. The doubled `-- --` is deliberate: the
first is cargo's own separator, the second passes `--include-ignored` to the test binary itself
— drop it and mutants in system-utility wrapper code are judged against a suite missing their
`#[ignore]`d tests, and report false misses.

Expect minutes, not seconds: the spike measured 5–15 minutes for a single file
(`docs/spikes/2026-08-14-cargo-mutants/results.md`). A branch touching several in-scope files
takes proportionally longer. Never run this over the whole crate locally to check a branch —
that was ~3 hours in the spike; Layer 2's weekly workflow does that job on hosted runners.

A surviving mutant blocks the push: kill it with a new test, or, if it is genuinely equivalent
or impractical to kill, exclude it in `.cargo/mutants.toml` with a justification (see
`docs/designs/mutation-testing.md`, "Equivalence policy").

## Issue Tracking

Report bugs and propose features as GitHub issues. Work that changes behaviour should trace back
to one.

## Non-Interactive Shell Commands

`cp`, `mv`, and `rm` are aliased to interactive mode on some systems, which will hang an agent
waiting for input that never comes. Always pass the non-interactive flag: `cp -f`, `mv -f`,
`rm -f`, `rm -rf`, `cp -rf`. Likewise `apt-get -y`, and `-o BatchMode=yes` for `ssh` and `scp`.

## Reference Documents

Read these when the trigger applies, not before:

- `docs/code-style-guide.md` — writing or reviewing code
- `docs/architecture.md` — navigating the module map, or changing component boundaries
- `docs/tech-stack-docs.md` — working against a dependency's API
- `docs/decision-recording.md` — recording a decision
- `docs/designs/mutation-testing.md` — pushing a branch that touched an in-scope Rust file, or
  documenting a mutant as equivalent
- `docs/adr/*.md` — a decision touches an existing ADR
- `docs/decisions/*.md` — working in an area an existing decision covers
- `docs/screenshot-tool.md` — verifying UI changes visually
