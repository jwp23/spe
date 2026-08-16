---
name: mutation-testing
description: Use before pushing a branch that touched an in-scope Rust file (anything not excluded by .cargo/mutants.toml) — a required local mutation-testing run catches survivors before Layer 1 (the PR gate) or Layer 2 (the weekly sweep) ever see them
---

# Mutation Testing

Before pushing a branch that touches an in-scope Rust file, run cargo-mutants locally scoped
to what you changed.

For the command, the run configuration, and what to do with a surviving mutant, see AGENTS.md's
"Mutation Testing" section — that is the canonical, single copy of this rule. This file only
points to it.
