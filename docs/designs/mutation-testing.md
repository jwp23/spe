# Mutation Testing

Mutation testing (cargo-mutants) guards test quality where a weak test corrupts a
user's saved file. Three layers catch surviving mutants at increasing distance from
the keyboard: a write-time local run, a required PR check, and a weekly full-scope
sweep. The 2026-08-14 spike (`docs/spikes/2026-08-14-cargo-mutants/results.md`)
established the tool, the costs, and the run configuration this design builds on.

## Enforcement scope

The scope is defined negatively, once, in `.cargo/mutants.toml` via
`exclude_globs`. Excluded — iced view/handler plumbing where a mutation-killing
test costs more than it protects:

- `src/main.rs`
- `src/app/`
- `src/ui/`

Everything else in the crate is in scope, and every layer reads the same config,
so a new file outside the excluded paths is guarded by default and the layers
cannot drift apart. Because the excluded paths are never mutated, every survivor
any layer reports is actionable.

## Layer 1 — PR gate (required check)

A CI job runs cargo-mutants over the intersection of the PR's diff and the
enforcement scope:

- A guard step checks `git diff --name-only <base>...HEAD` against the excluded
  paths before any toolchain setup. If every changed file is excluded (or no
  `.rs` file changed), the job passes in seconds — no toolchain install, no
  baseline build. The guard is an in-job early exit, not a workflow `paths:`
  filter: a required check that is path-filtered out never reports a status and
  blocks the PR on "pending".
- With in-scope changes, `git diff <base>...HEAD` feeds `--in-diff` to restrict
  mutants to changed code; the shared config keeps excluded paths out.
- PRs that change in-scope logic mutate only their changed functions — typically a
  handful of mutants, minutes not hours on a hosted runner.

The check is **required**. A surviving mutant fails the PR: either add a test that
kills it or document it as equivalent (see Equivalence policy). If a large backend
PR makes the job slow, `--shard` splits it across parallel jobs; free on a public
repo.

Wall clock is a design constraint: the job runs in parallel with the other CI
jobs, never after them, and uses build caching to warm the baseline build.
(cargo-mutants copies a private `target/` per job, so caching mainly speeds the
baseline; measure what it buys rather than assuming.)

## Layer 2 — weekly full-scope run

A scheduled workflow (weekly) mutates the whole in-scope crate — every non-excluded
file, not just recently changed code — on GitHub-hosted runners, sharded
(`--shard k/n`) to fit hosted-runner limits. This is the backstop that catches
what `--in-diff` cannot: cross-file interactions, mutants outside any recent
diff, and anything that slipped past the PR gate.

Capture pipeline:

1. Each shard uploads its `mutants.out` as an artifact (the forensic record).
2. A summary job merges the shards' `missed.txt` files. Because excluded paths
   are never mutated and equivalent mutants are suppressed by `exclude_re`, the
   merged list needs no further filtering: it is normally empty, and anything
   present is genuinely new work.
3. If survivors exist, the workflow files a GitHub issue (or updates the existing
   open one) listing each with file, line, and mutant description, linked to the
   run.
4. Triage is manual: each real survivor becomes a bd task; each equivalent mutant
   is added to the exclusion list with justification. The issue closes when the
   next run comes back clean.

   *Future direction:* the GitHub issue is a deliberate automation seam. A later
   iteration can watch these issues with an agent that converts survivors into
   bd tasks and drives fixes — a full weekly-run → issue → bead → fix cycle.
   Manual triage is the current design; the seam exists so automating it changes
   no other layer.
5. The run summary reports the headline kill rate so trend drift is visible
   without opening artifacts.

CI never writes to bd directly: bd's dolt remote would need push credentials in a
public repo's Actions, and triage needs a judgment step between "mutant survived"
and "work item exists" — some survivors are equivalent mutants that need
documenting, not fixing.

## Layer 3 — write-time local run

Before a branch that touches in-scope files is pushed, cargo-mutants runs
locally, scoped to the changed in-scope files (`-f <files>`). On a many-core dev
machine this takes minutes and catches survivors before CI ever sees them. This
layer is process, not machine enforcement; Layers 1 and 2 back it up.

The requirement lives in this project's own tooling (a project-local skill and
project instructions), not in any shared cross-project workflow: mutation
testing is a per-project adoption, and other projects may use different tools.

## Equivalence policy

A mutant that cannot be killed by a reasonable test is excluded, never chased:

- Exclusions live in `.cargo/mutants.toml` (`exclude_re` patterns), each with a
  comment stating why the mutant is equivalent or impractical. The config file is
  chosen over `#[mutants::skip]` attributes because it needs no `mutants` crate
  dependency and keeps all exclusions in one reviewable place.
- Known accepted survivors at time of writing (documented on epic spe-26l):
  - `src/fonts.rs` `descriptor_flags` `|` → `^` — flag bits are disjoint, XOR ≡ OR.
  - `src/fonts.rs` `extract_font_descriptor` `||` → `&&` — needs a font fixture
    with a nonzero post-table italic angle but non-italic fsSelection.
  - `src/pdf/writer.rs` `MAX_FONT_FILE_SIZE` `*` → `+` — needs a >1 MB font
    fixture.
  - `src/pdf/renderer.rs` `probe_pdftoppm` → `Ok(())` — environment-equivalent on
    any machine with pdftoppm installed.
- Adding an exclusion is a reviewed code change on the PR that needs it, so the
  escape hatch is visible in review, not silent.

There is no whole-crate kill-rate target. The goal is zero unexcluded in-scope
survivors, not a percentage.

## Run configuration

Knowledge from the spike, load-bearing for any runner:

- **`TMPDIR` off tmpfs.** cargo-mutants copies the tree plus a private `target/`
  per job; on tmpfs `/tmp` a parallel run eats RAM-backed storage toward OOM.
  Point `TMPDIR` at real disk (locally: `~/.cache/cargo-mutants-tmp`).
- **Tune `-j` below the core count, generously.** Oversubscription starves
  per-mutant test runs and produces phantom timeouts that look exactly like hung
  tests. `-j8` on 20 cores was the local ceiling; hosted 4-core runners need
  `-j1`–`-j2` per shard plus a generous `--timeout-multiplier` (≥6).
- **`-- -- --include-ignored`** (doubled `--`: first cargo's, then the test
  binary's). Without it, mutants in system-utility wrapper code are judged against
  a suite missing their real tests and report false misses.
- **`-o` outside the repo** — cargo-mutants otherwise writes `mutants.out/` into
  the repo root.

Reference costs (spike, cargo-mutants 27.1.0): full crate ≈ 3 h at `-j8` on 20
cores; single file ≈ 5–15 min; baseline build ≈ 1 min; `cargo mutants --list` is
free.
