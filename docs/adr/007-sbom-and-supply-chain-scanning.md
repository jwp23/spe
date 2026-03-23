# ADR-007: SBOM Generation and Supply-Chain Scanning

## Context

ADR-006 established dependency auditing via `cargo audit` and deferred SBOM generation for when compliance requirements arose. While the project has no formal compliance requirement, adding SBOM generation and multi-database vulnerability scanning is a best-practice discipline that complements the existing security tooling. The existing `cargo audit` checks only the RustSec Advisory Database; vulnerabilities published only to NVD, OSV, or GitHub Advisory Database are not caught. License compliance is also unchecked.

## Decision

**SBOM generation:** `syft` via `anchore/sbom-action` generates a CycloneDX JSON SBOM from `Cargo.lock` and uploads it as a CI workflow artifact. Syft is a pre-built Go binary that installs in seconds and reads Cargo.lock v4 natively.

**Vulnerability scanning:** `grype` via `anchore/scan-action` consumes the SBOM and checks against NVD, GitHub Advisory Database, and OSV. Only `critical` severity findings fail the build; lower severities are reported but do not gate. This threshold can be tightened after observing signal quality.

**License and advisory compliance:** `cargo-deny` via `EmbarkStudios/cargo-deny-action` checks license compatibility, crate bans, source restrictions, and RustSec advisories. Configuration lives in `deny.toml` at the project root.

**CI architecture:** A `supply-chain` job runs in parallel with the existing `check` job, sharing the same triggers (push to main, PRs against main). This adds no wall-clock time to PR feedback.

**Pre-commit:** `cargo-deny` is added as an optional check (warn-if-missing pattern) alongside `cargo audit`.

**Existing tooling preserved:** `cargo audit` via `rustsec/audit-check` remains in the `check` job. Both tools check RustSec, but `cargo-audit` has smarter reachability analysis. The overlap is harmless.

## Trade-offs

**Considered: `cargo-cyclonedx` for SBOM generation** — Rust-native, CycloneDX output. Requires compiling a Rust binary in CI (slow) or managing pre-built binary distribution. Syft is simpler to integrate and has broader ecosystem support.

**Considered: SPDX format** — More common in enterprise license compliance. CycloneDX has broader tooling support for vulnerability scanning workflows and handles both concerns adequately. For a project focused on security discipline rather than regulatory compliance, CycloneDX is the pragmatic choice.

**Considered: Replacing `cargo-audit` with `cargo-deny`** — `cargo-deny` is a superset for advisory checking. However, `cargo-audit` handles reachability differently and is already stable in the pipeline. Removing it is a separate future decision.

**Considered: GitHub dependency submission API** — Would feed the Dependency Graph tab but doesn't produce a downloadable SBOM artifact. Can be enabled later via `dependency-snapshot: true` on sbom-action.

**Giving up:** Simplicity of a single CI job. **Gaining:** Multi-database vulnerability scanning, license compliance checking, standard SBOM artifact, and supply-chain source verification.
