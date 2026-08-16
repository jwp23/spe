# Mutation testing: measured impact

Point-in-time record of what the mutation-testing workstream found and fixed,
2026-08-14 through 2026-08-16. The spike itself is documented in
[results.md](results.md); the recurring pipeline in
`docs/designs/mutation-testing.md` and ADR 016. Numbers below come from the
spe-26l / spe-ji7 bd task records and the merged diffs, not estimates.

## Headline

Because of mutation testing we found **51 potential bugs the test suite would
not have caught**, **1 line of dead production code**, and **2 tests that
tested nothing at all** — fixed by adding **35 new tests**. The enforced-scope
kill rate went from **74%** (spike, full crate, 2026-08-14) to **97.0%** (first
weekly run, 2026-08-16, 359 caught / 370 viable), and every survivor from that
run was triaged to zero within a day.

Framing caveat: mutation testing does not find bugs *in* shipped code — it
proves where a bug *could* land undetected. Each of the 51 is a specific,
verified one-token change (a flipped comparison, a deleted match arm, a
function returning a dummy value) that the suite demonstrably let through, and
each now has a test that catches it.

## The 51 test gaps

| Round | Gaps killed | Detail |
|---|---|---|
| Spike round (PR #155, epic spe-26l) | 43 | Font-program fingerprinting: 12 — the function deciding font re-embed on re-edit could be disabled entirely unnoticed. `word_wrap`: 8 — tests proved text wraps, never *where*. Re-edit validation boundaries: 12. Media-box arithmetic: 4. Writer misc: 3. Renderer probe: 4. |
| First weekly run (PR #164, spe-ji7.10/.11) | 8 | All in `ipc.rs`: page/overlay index boundary checks, the click-hit `&&` conditions, connection-handling return values. |

## The other findings

- **Dead code: 1 line** — the redundant zero-length match arm in
  `strip_app_streams` (PR #163). The mutant was unkillable *because* the code
  was redundant; deleting the line removed both. Small in lines, but it is the
  category proving the tool distinguishes "untested" from
  "untestable-for-a-reason".
- **Hollow tests: 2** — `pdftoppm_renderer_is_constructible` and
  `render_page_batch_trait_exists` in `renderer.rs` compiled things and
  asserted no behavior; both replaced with behavioral tests (spe-26l.6).
- **Testability refactors driven: 2** — `probe_command` extracted from
  `probe_pdftoppm` (killed 3 previously untestable mutants at the cost of 1
  environment-equivalent one), and `serve_connections` extracted in `ipc.rs`
  (PR #164).
- **Tests added: 35** (24 in #155, 11 in #164), growing the lib suite
  844 → 870 (the delta is smaller than 35 because the hollow tests were
  replaced and other commits landed between counts).
- **Proven-equivalent or intentionally excluded mutants: 7** dispositioned with
  written justifications (`.cargo/mutants.toml` `exclude_re`, or code deletion).
  Some are proven equivalent by construction (e.g. `descriptor_flags`,
  `default_font`, `probe_pdftoppm`); others are excluded because killing them
  needs a costly fixture (e.g. `extract_font_descriptor`,
  `MAX_FONT_FILE_SIZE`) rather than because they are equivalent — the audit
  trail that keeps the 97% kill rate meaningful instead of padded.

## Kill-rate movement

| Metric | 2026-08-14 (spike) | 2026-08-16 (first weekly run) |
|---|---|---|
| Kill rate | 74% full crate (1744 mutants, 959 caught / ~1294 viable) | 97.0% enforced scope (359 / 370) |
| Untriaged survivors | 335 | 0 |
| Lib tests | 844 | 870 |
| Wall clock | ~3 h local `-j8` | 11.5 min sharded CI |

The 74% → 97% comparison overstates the pure test-quality gain slightly: the
weekly run's scope excludes the UI layer that dragged the spike average down
(see `docs/designs/mutation-testing.md`, "Enforcement scope"). The
like-for-like movement is the enforced backend files going from 61–94% each to
100% each.

## Ongoing guard

The durable part is not the one-time cleanup: every PR now gets its diff
mutation-tested before merge (required check), a weekly full-scope run files
survivors as a GitHub issue, and a write-time local run catches them before
push — so the 97% cannot quietly erode. Scope-drift follow-ups are tracked on
epic spe-ji7 (.18 narrow the ui exclusion, .19 canvas/mod.rs into scope, .20
drift-detection convention).
