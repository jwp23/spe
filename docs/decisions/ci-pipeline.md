# CI Pipeline

Decision: GitHub Actions workflow triggered on push and pull request to `main`. Independent checks run as parallel jobs: secrets scan (betterleaks), formatting (cargo fmt), dependency audit (cargo-audit), linting (cargo clippy), unit tests (cargo test), and supply-chain checks (cargo-deny, SBOM, Grype). Integration tests (cargo test --ignored) run sequentially after unit tests pass. Rust jobs use `Swatinem/rust-cache` to offset parallel toolchain installs.

Rationale: CI must run the same checks as the pre-commit hook to prevent drift between local and CI environments. Parallel jobs reduce wall-clock time by running independent checks concurrently. Integration tests depend on unit tests because there is no value in running slow system-utility tests if fast unit tests fail. System utilities (poppler-utils, fontconfig) are only installed in the integration job since unit tests use trait-based test doubles. See ADR-008 for the parallelization decision.
