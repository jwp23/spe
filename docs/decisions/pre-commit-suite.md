# Pre-commit Check Suite

Decision: Append project quality checks to the existing beads-managed pre-commit hook at `.beads/hooks/pre-commit`. Checks run in order after the beads integration section.

Rationale: The beads issue tracker already owns `core.hooksPath` (set to `.beads/hooks`). Rather than fighting for the hooks path or creating a parallel hooks directory, we add project quality checks after the beads integration section in the same file.

Check order (fastest and most critical first):
1. `betterleaks git --pre-commit --staged --redact` — secrets detection (hard fail; hard fail if not installed)
2. `cargo fmt --check` — formatting (fast, catches style issues)
3. `cargo clippy -- -D warnings` — lint (medium, catches potential bugs)
4. `cargo audit` — dependency vulnerabilities (optional, warns if cargo-audit not installed)
5. `cargo test` — tests (slower, catches regressions)

Betterleaks runs first because it is fast (no compilation), catches the most critical class of defect (leaked secrets), and a commit containing secrets should be blocked before any other analysis. Unlike cargo-audit, betterleaks is a hard requirement — the hook fails if the tool is not installed.

If the full test suite exceeds 30 seconds in the future, we will switch to a fast subset.
