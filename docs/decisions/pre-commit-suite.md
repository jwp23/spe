# Pre-commit Check Suite

Decision: Append cargo fmt check, cargo clippy, and cargo test to the existing beads-managed pre-commit hook at `.beads/hooks/pre-commit`.

Rationale: The beads issue tracker already owns `core.hooksPath` (set to `.beads/hooks`). Rather than fighting for the hooks path or creating a parallel hooks directory, we add project quality checks after the beads integration section in the same file. The checks run in order: format (fast, catches style issues), lint (medium, catches potential bugs), test (slower, catches regressions). If the full test suite exceeds 30 seconds in the future, we will switch to a fast subset.
