# Pre-commit Check Suite

Decision: The project's quality checks live in `scripts/pre-commit-checks.sh`, invoked by every
`pre-commit` hook the repository ships. Checks run in order, fastest and most critical first.

Rationale: A single script keeps the contributor hooks in `.githooks/` and the beads-managed
hooks in `.beads/hooks/` from drifting apart. See `hook-distribution.md` for why there are two
hook directories.

Check order:
1. `betterleaks git --pre-commit --staged --redact` — secrets detection
2. `cargo fmt --check` — formatting (fast, catches style issues)
3. `cargo clippy --all-targets -- -D warnings` — lint (medium, catches potential bugs)
4. `cargo audit` — dependency vulnerabilities
5. `cargo deny check` — licences and supply chain
6. `cargo test` — unit and integration tests
7. `cargo test -- --ignored` — tests requiring system utilities

Betterleaks runs first because it needs no compilation and catches the most serious class of
defect: a commit containing a secret should be stopped before anything else is analysed.

Tools that are not installed produce a warning and are skipped. CI enforces all of them on every
pull request, so a locally missing tool delays the failure rather than hiding it.

If the full test suite exceeds 30 seconds in the future, we will switch to a fast subset.
