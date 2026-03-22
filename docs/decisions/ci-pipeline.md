# CI Pipeline

Decision: GitHub Actions workflow triggered on push and pull request to `main`. Steps: install Rust stable toolchain with rustfmt and clippy components, install system utilities (poppler-utils, fontconfig), then run cargo fmt check, cargo clippy, cargo test (unit tests), and cargo test with `--ignored` flag (integration tests requiring system utilities).

Rationale: CI must run the same checks as the pre-commit hook to prevent drift between local and CI environments. The `--ignored` integration tests are run separately because they require system utilities that may not be available in all environments. GitHub Actions Ubuntu runners provide apt for installing poppler-utils and fontconfig.
