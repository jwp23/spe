# PDF Text Overlay Editor

A desktop GUI application for Linux that opens PDF documents, renders pages visually, and lets users click anywhere on a page to place text overlays. Users control font and font size. The result is saved as a new PDF with text baked in.

@AGENTS.md

## Project Bootstrapping

**This is a discovery-driven project.** The language, GUI framework, PDF libraries, and tooling are NOT pre-selected. Claude's first task is to propose and execute a bootstrapping sequence.

### First Session Workflow

1. Propose a bootstrapping sequence as the first action. Present it to the user for approval before proceeding.
2. Evaluate each decision through discussion with the user.
3. Record every decision using the formats below.
4. Do NOT write application code until the bootstrapping sequence is complete and recorded.

### Suggested Bootstrapping ADRs (propose order, user approves)

- Language and runtime selection
- GUI framework selection
- PDF rendering and writing library selection
- Linux system utility integration strategy
- Testing framework and tooling
- Code style, formatting, and linting conventions
- Project directory structure
- Pre-commit check suite

## Decision Recording

### Architectural Decision Records — `docs/adr/`

Use sequential numbering: `001-language-selection.md`, `002-gui-framework.md`, etc.

Format:
```
# ADR-NNN: Title

## Context
What situation or problem prompted this decision?

## Decision
What was decided and why?

## Trade-offs
What alternatives were considered? What are we giving up?
```

### Decision Docs — `docs/decisions/`

For smaller, tactical decisions. Use descriptive filenames: `use-pdftoppm-for-rendering.md`.

Format:
```
# Title
Decision: [what was decided]
Rationale: [why, in 1-3 sentences]
```

### When to Record

- **ADR**: Language, framework, architecture, library choices, testing strategy, CI pipeline design
- **Decision doc**: Specific tool selection, naming conventions, file organization choices, utility preferences
- **Always ask the user**: "Should I record this in `docs/adr/` or `docs/decisions/`?" before recording

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

Prefer common Linux system utilities when they simplify the codebase over pure-library solutions. Check for commonly installed tools first:

- `pdftoppm` (poppler-utils) — PDF page rasterization
- `fc-list` (fontconfig) — discover installed system fonts
- `qpdf` — PDF optimization, linearization, repair
- `gs` (Ghostscript) — PDF post-processing

If a utility that is not commonly pre-installed would meaningfully reduce code complexity, propose it to the user and record the decision in `docs/decisions/`. Include installation instructions.

When calling system utilities from code: use the language's standard subprocess/exec mechanism with error checking enabled. Never invoke commands through a shell interpreter. Wrap failures with clear error messages stating what tool failed and how to install it.

## Code Style

These principles are mandatory regardless of language or framework. See `@docs/code-style-guide.md` for detailed examples and anti-patterns.

- **Human readable** — Code is read far more than it is written. Optimize for the next developer. Descriptive names, clear control flow, no clever tricks.
- **Loosely coupled** — Components communicate through well-defined interfaces. No module should need to know another's internals. Changing one component must not cascade through the codebase.
- **Idiomatic** — Use the conventions of the chosen language and framework. Do not import patterns from other ecosystems. Claude records language-specific idiom decisions in `docs/decisions/`.
- **Simple** — Do not inherit a ball of mud. Prefer composition over inheritance. Prefer flat over nested. Prefer explicit over implicit. If a pattern adds indirection without clear value, do not use it.
- **Professional** — Write like a senior engineer shipping to production. No TODO-driven development, no dead code, no commented-out blocks, no "temporary" hacks without a tracked issue.

When style conventions and simplicity conflict, simplicity wins.

## Testing

### Test Pyramid (Mandatory)

- **Unit tests**: Cover all public methods and functions
- **Integration tests**: Cover how components work together
- **End-to-end tests**: Cover user workflows (open PDF → place text → save)

### TDD Workflow (Mandatory)

1. **RED** — Write a failing test first. Verify it actually fails before proceeding.
2. **GREEN** — Write the minimum code to make the test pass.
3. **REFACTOR** — Clean up while keeping tests green.

Never skip the RED step. If a test passes immediately, the test is wrong or the code already existed.

### Test Framework

Claude selects the testing framework based on the language chosen. Record as an ADR including: framework choice, test organization structure, coverage targets, and mocking approach.

## Git Workflow

- **Commits**: Conventional Commits, single line only. No body, no footer. Examples:
  - `feat: add font size selector to overlay toolbar`
  - `fix: prevent crash when opening password-protected PDF`
  - `chore: add ruff to pre-commit hooks`
- **Branches**: Feature branches with PRs. Claude proposes branch naming convention and records in `docs/decisions/`.
- **Lockfiles**: Always commit lockfiles regardless of language/package manager.
- **CI**: GitHub Actions. PRs cannot merge without passing CI. No exceptions.

## Pre-commit & CI

Claude must define and record a pre-commit check suite once tooling is selected. The suite MUST include:

- Lint
- Format check
- Type check (if the language supports it)
- Full test suite (or fast subset if full suite exceeds 30 seconds)

Record the pre-commit configuration in `docs/decisions/pre-commit-suite.md`.

GitHub Actions CI must run the same checks. Record the pipeline design in `docs/decisions/ci-pipeline.md`.

## Reference Documents

Read these only when the trigger condition applies:

- `@docs/code-style-guide.md` — Read when writing or reviewing code. Detailed examples and anti-patterns for the five code style principles.
- `docs/adr/*.md` — Read when making decisions related to an existing ADR, or when context on a past decision is needed
- `docs/decisions/*.md` — Read when working in an area covered by an existing decision
- `docs/architecture.md` — Read when modifying component boundaries or data flow (create after bootstrapping)

## Project Structure

Finalized during bootstrapping. Claude proposes and records as a decision doc. Expected top-level:

```
├── CLAUDE.md
├── AGENTS.md
├── .claude/
├── docs/
│   ├── adr/
│   ├── decisions/
│   ├── code-style-guide.md
│   └── architecture.md
├── src/ or pkg/ or cmd/     # depends on language
├── tests/
└── [language-specific config files]
```
