# ADR-006: Security Review System

## Status

Partly superseded. **The Decision and Trade-offs sections below are a historical record, not a
description of how the project works today.**

What changed since:

- The agent-driven review skill no longer lives in this repository. It was harness-specific
  tooling rather than a project asset, and the project's instructions no longer depend on any
  particular agent harness. The 8-point checklist it encoded survives only as guidance.
- Automated enforcement, which this ADR explicitly declined to build, now exists in CI:
  betterleaks secrets scanning, `cargo audit`, `cargo deny`, and Semgrep.
- SAST was rejected below and later adopted anyway, as `.github/workflows/semgrep.yml`.
- SBOM generation, deferred below, was adopted in ADR-007 and runs in the `supply-chain` job.
- `cargo-deny` was deferred below in favour of `cargo-audit`; both now run.

Coverage caveat: the `sonarqube` job does not run for Dependabot pull requests, which have no
`SONAR_TOKEN`. The other checks do run for them.

## Context

The project invokes system utilities via subprocess (`pdftoppm`) and processes user-provided PDF files. While the current codebase follows good security practices (typed paths, safe `Command::new` usage, `tempfile` crate, enum-based font selection), there is no systematic process to verify security properties as the codebase evolves. Additionally, dependency supply-chain attacks are an industry-wide concern that warrants automated scanning.

## Decision

**Security review skill:** A platform-agnostic security review skill is added to the project at `.claude/skills/security-review/`. It defines an 8-point security checklist covering: command execution, dependency vulnerabilities, input validation, path handling, secrets scanning, unsafe code, error information disclosure, and temp file handling. The skill detects the tech stack at runtime and applies language-specific checks.

**Integration:** The security review runs automatically before PR creation via the `finishing-a-development-branch` skill (Step 1.5). It can also be invoked on-demand via `/security-review`. A subagent prompt (`security-reviewer.md`) enables fresh-context review without polluting the development session.

**Dependency auditing:** `cargo audit` is added to CI via the `rustsec/audit-check` GitHub Action and to the pre-commit hook with warn-if-missing behavior. It runs against the RustSec Advisory Database to detect known vulnerabilities in dependencies.

**Severity model:** Findings are classified as Critical (must fix), Important (should fix), or Minor (informational), matching the existing code review severity model.

**Future consideration:** SBOM (Software Bill of Materials) generation in CI will be evaluated separately when compliance or release distribution requirements arise.

## Trade-offs

**Considered: Global skill (`~/.claude/skills/`)** — Would enable cross-project reuse without duplication. Rejected because version control and team sharing are more valuable for this project's workflow. The skill is platform-agnostic by design and can be copied to other projects.

**Considered: `cargo-deny` instead of `cargo-audit`** — `cargo-deny` is more comprehensive (license checking, banned crates). `cargo-audit` is simpler and focused on the immediate need (vulnerability scanning). `cargo-deny` can be evaluated later if license auditing becomes relevant.

**Considered: Automated SAST tools (e.g., semgrep)** — Would provide deeper static analysis. Adds CI complexity and maintenance burden. The agent-driven review approach is more flexible and covers patterns that rule-based tools miss. SAST tools can supplement the process later if needed.

**Considered: Security rules in `.claude/rules/`** — Rules fire on every interaction. Security review is a periodic activity, not a per-interaction constraint. A skill with structured checklist and subagent dispatch is more appropriate.

**Giving up:** Automated enforcement (the skill is guidance, not a gate), cross-project reuse without copying. **Gaining:** Version-controlled security process, platform-agnostic coverage, integration with the existing agent-driven development workflow, automated dependency vulnerability scanning in CI.
