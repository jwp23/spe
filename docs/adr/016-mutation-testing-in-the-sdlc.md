# ADR-016: Mutation Testing in the SDLC

## Context

The 2026-08-14 cargo-mutants spike (docs/spikes/2026-08-14-cargo-mutants/results.md)
found a 74% whole-crate kill rate with misses clustered in the PDF backend — the
code where a weak test means a corrupted saved file. Epic spe-26l closed those
gaps. The spike deferred the recurring-run design: full-crate runs cost ~3 h on 20
cores because cargo-mutants rebuilds per mutant (unlike Stryker-style mutation
switching in JS), so mutation testing cannot simply run whole-crate on every PR.
CI runs on GitHub-hosted runners; the repo is public, so runner minutes are free
but per-job wall clock still matters.

## Decision

Three layers, specified in docs/designs/mutation-testing.md:

1. **Single negative scope for all layers** — `.cargo/mutants.toml`
   `exclude_globs` excludes `src/main.rs`, `src/app/`, and `src/ui/` (iced
   plumbing); everything else is in scope, so new files are guarded by default
   and every reported survivor is actionable.
2. **Required PR check** — cargo-mutants with `--in-diff` over the PR's changes.
   A git-only guard step passes PRs touching only excluded paths in seconds,
   before any toolchain setup.
3. **Weekly scheduled full-scope run**, sharded across hosted runners.
   Survivors are auto-filed as a GitHub issue and triaged into bd by hand;
   artifacts hold the full results.
4. **Write-time local run** — the finishing-a-branch workflow runs scoped mutants
   on changed in-scope files before push.

Known-equivalent mutants are excluded via reviewed configuration changes with
justification. There is no whole-crate kill-rate target; the bar is zero
unexcluded in-scope survivors.

## Trade-offs

- **Self-hosted runner on the dev machine** (rejected): fast, but on a public
  repo fork PRs could execute arbitrary code on it, and CI would depend on a
  personal machine being awake.
- **CI writing directly to bd** (rejected): requires dolt push credentials in
  public Actions and removes the judgment step that separates equivalent mutants
  from real work. GitHub issue → manual bd triage instead.
- **Whole-crate enforcement** (rejected): the `app/`/`ui/` misses are iced
  plumbing where a killing test costs more than it protects; excluding those
  paths keeps every reported survivor meaningful and the runs cheaper.
- **Positive scope list** (rejected in favor of negative): a hardcoded
  keep-list would leave new files unguarded until someone remembered to add
  them, and would need reconciling across layers.
- **Accepted cost**: the required check adds seconds to out-of-scope PRs and
  minutes to backend PRs; equivalent mutants add occasional exclusion paperwork.
