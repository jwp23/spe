# Humble UI layer and mutation-scope drift detection

Decision: `src/ui/` stays humble — widget/view files build widgets and route
events; logic that computes answers (geometry, hit-testing, measurement math)
lives in modules under mutation enforcement. The `.cargo/mutants.toml`
`exclude_globs` list is the drift ledger: every entry is a standing claim that
the file contains only iced plumbing. Drift is checked at two points:

1. **Review time** — a PR that adds or changes a `pub fn` in an excluded file
   whose signature does not consume or produce iced widget/render types must
   either extract that logic to an in-scope module or remove the file from
   `exclude_globs`.
2. **Weekly triage time** — the triaging-mutation-survivors procedure includes
   an exclusion audit: skim the excluded files' diffs since the last triage and
   flag any that have accumulated extractable logic.

Rationale: the spike showed pure logic drifting under `src/ui/` (zoom, layout
math at 92–94% kill rate but unenforced), and one file (`ui/canvas/mod.rs` —
shared canvas state plus pure geometry and hit-test helpers) where the scope
question initially had no clean answer — "hard to decide whether to mutate" is
the symptom of mixed responsibilities and the trigger for this convention. Tying the audit to the existing
weekly-triage touchpoint detects recurrence without inventing a new cadence.
