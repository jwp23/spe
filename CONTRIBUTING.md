# Contributing

Thanks for your interest in spe. This document covers everything you need to get a change
merged. If you use an AI coding agent, point it at [AGENTS.md](AGENTS.md) — same rules, written
as instructions.

## Reporting Bugs and Proposing Features

Open a GitHub issue. For a bug, include the PDF that triggers it if you can share it, the steps
you took, what you expected, and what happened instead. For a feature, check
[Scope](#scope) first — some things are deliberately excluded.

Please open an issue before starting substantial work, so we can agree on the approach before
you spend time on it.

## Scope

This application places new text overlays on top of existing PDF pages and saves the result as a
new file. It does **not** edit or extract the PDF's existing text, add annotations, fill forms,
or talk to a network. Those exclusions are deliberate. Proposing one means proposing an
architectural decision record — see [Recording Decisions](#recording-decisions).

## Getting Set Up

Prerequisites are listed in the [README](README.md#prerequisites). Then:

```bash
git clone https://github.com/jwp23/spe.git
cd spe
./scripts/setup-hooks.sh   # enable the project's git hooks
cargo build
cargo test
```

`scripts/setup-hooks.sh` points `core.hooksPath` at the repository's committed hooks. It is not
automatic — git will not run any hook until you do this. The hooks are the fastest way to find
out that something is wrong; CI will tell you the same thing several minutes later.

`pre-commit` runs secrets scanning, `cargo fmt`, `cargo clippy`, `cargo audit`, `cargo deny`,
and the test suites. Tools you do not have installed print a warning and are skipped locally —
CI runs all of them regardless, so a warning locally is not a free pass.

## Tests Come First

This project practises test-driven development. Write a failing test, watch it fail for the
reason you expect, then write the code that makes it pass.

A bugfix needs a test that reproduces the bug. A pull request that changes behaviour without
touching a test will be asked for one.

Where each kind of test belongs — unit, integration, end-to-end — is set out in
[AGENTS.md](AGENTS.md#the-test-pyramid). Unit tests must pass on a machine with none of the
system utilities installed; anything needing `pdftoppm` or `fc-list` is an integration test in
`tests/`, marked `#[ignore]`.

If your change affects what the canvas draws — overlay rendering, coordinate math, visual state
— verify it visually before submitting. See [docs/screenshot-tool.md](docs/screenshot-tool.md).

## Mutation Testing

Before pushing, run cargo-mutants against any Rust file your branch changed that's in
[scope](.cargo/mutants.toml). It catches tests that pass but don't actually exercise the
behaviour they claim to cover; a surviving mutant blocks the push the same way a failing test
would. See [AGENTS.md](AGENTS.md#mutation-testing) for the exact command, and
[docs/designs/mutation-testing.md](docs/designs/mutation-testing.md) for why.

## Debugging

Find the root cause before you write the fix. A change that makes a symptom disappear without
explaining why it occurred is not a fix, and describing it as one in a commit message misleads
whoever reads the history next.

## Recording Decisions

If your change makes a technical choice, write it down before you implement it:

- **Architectural decision records** go in `docs/adr/` — language, framework or major library
  selection, architecture patterns, testing strategy, CI design. Copy `docs/adr/TEMPLATE.md` and
  take the next sequential number.
- **Decision docs** go in `docs/decisions/` — tool selection, naming conventions, file
  organization, configuration, hooks. Copy `docs/decisions/TEMPLATE.md` and use a descriptive
  filename.

`docs/decision-recording.md` has the formats and the classification rules. This is the
convention most easily forgotten and the one most valuable a year later, which is why the pull
request template asks about it directly.

## Commits and Pull Requests

Work on a feature branch — `feat/`, `fix/`, `chore/`, `docs/`, `refactor/` or `test/` plus a
short description. Never commit to `main`.

Commit messages are a **single** [Conventional Commits](https://www.conventionalcommits.org)
line, with no body and no footer:

```text
feat: add font size selector to overlay toolbar
fix: prevent crash when opening password-protected PDF
```

Valid types: `feat`, `fix`, `chore`, `docs`, `refactor`, `test`, `perf`, `ci`, `style`, `build`,
`revert`. Add `!` before the colon for a breaking change. The `commit-msg` hook checks this.

**Your pull request title follows the same rule.** Pull requests are squash-merged, so the title
you write becomes the commit subject in `main`'s history — CI fails the pull request if it does
not conform. Detail belongs in the pull request description, which is why commit bodies are not
used.

Every pull request needs green CI before it can merge. CI runs secrets scanning, formatting,
lint, unit tests, integration tests, dependency audit, licence and supply-chain checks, and
static analysis.

## Code Style

Five principles: human readable, loosely coupled, idiomatic, simple, professional.
[docs/code-style-guide.md](docs/code-style-guide.md) has the detail and the anti-patterns. Where
a convention and simplicity disagree, simplicity wins.

Match the style of the file you are editing. Consistency within a file beats an external style
guide. `cargo fmt` settles the rest.

Name things for what they do in the problem domain, not for how they work internally or what
they used to be called. Comments say what and why — never what changed, which is what the git
history is for.

## Project Layout

[docs/architecture.md](docs/architecture.md) has the module map. Tests live in `tests/` for
integration and end-to-end, and in co-located `#[cfg(test)]` modules for unit tests.
