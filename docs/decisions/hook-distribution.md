# Git Hook Distribution

Decision: Commit the hooks to `.githooks/` and enable them with `scripts/setup-hooks.sh`, which
sets `core.hooksPath` to a relative path. The checks themselves live in
`scripts/pre-commit-checks.sh` and `scripts/check-commit-msg.sh`; every hook — including the
beads-managed ones in `.beads/hooks/` — is a one-line call to those scripts. No pre-commit
framework is adopted.

Rationale: `core.hooksPath` is not shared by a clone, so before this the project's checks ran
only on machines that had been configured by hand. A framework such as `pre-commit` or
`cargo-husky` would install hooks more smoothly but wants to own the hooks directory, which
`bd` already owns on maintainer machines, and `pre-commit` would put a Python dependency in
front of building a Rust application. Keeping the check bodies in `scripts/` lets both hook
directories share one definition, so they cannot drift.

`betterleaks`, `cargo-audit`, and `cargo-deny` warn rather than fail when missing locally. CI
runs all three on every pull request, so the local hook is defence in depth and a missing tool
should not stop a contributor from committing.
