# Spike: mutation testing with cargo-mutants

**Date:** 2026-08-14 · **Question:** what does mutation testing find in spe, and what would a
regular run cost? · **Outcome:** adopt; close the `src/pdf/` + `src/fonts.rs` gaps first
(epic spe-26l), design the recurring run separately.

## Headline result

Full crate, cargo-mutants 27.1.0: **1744 mutants — 959 caught, 335 missed, 431 unviable,
19 timeouts. 74% kill rate** across viable mutants, against a suite of 844 passing tests.
Wall clock: 3 h at `-j8` on a 20-core machine.

Misses cluster hard. The pure-logic calibration slice (`overlay.rs`, `coordinate.rs`,
`fonts.rs`) scored 91%; the app/UI event-handling layer drags the average down.

## Kill rate by file

| Rate | File | Rate | File |
|---|---|---|---|
| 0% (0/2) | `main.rs`, `ui/canvas/pages.rs` | 82% | `fonts.rs` |
| 31% | `app/view.rs` | 83% | `pdf/metadata.rs` |
| 32% → 79%¹ | `pdf/renderer.rs` | 84% | `ipc.rs` |
| 41% | `ui/sidebar.rs` | 86% | `ui/canvas/mod.rs` |
| 51% | `ui/font_picker.rs` | 92% | `ui/canvas/layout.rs`, `ui/text_width.rs` |
| 61% | `pdf/writer.rs` | 94% | `ui/canvas/zoom.rs`, `pdf/mod.rs` |
| 65% | `ui/canvas/overlays.rs`, `ui/toolbar.rs` | 100% | `coordinate.rs`, `overlay.rs`, `command.rs`, `pdf/win_ansi.rs`, `ui/icons.rs` |
| 70% | `app/handlers.rs`, `app/mod.rs` | | |
| 73% | `pdf/reedit.rs` | | |

¹ 32% under plain `cargo test`; 79% when the `#[ignore]`d pdftoppm integration tests run
(`-- -- --include-ignored`). Nine of its thirteen "misses" were this config artifact.

## Principal findings (tracked in epic spe-26l)

- **`embedded_font_program_fingerprint` (`pdf/writer.rs`) — worst spot in the crate, 10
  survivors.** The whole function survives replacement with `None`; four match arms survive
  deletion; the size-guard comparison survives every operator swap. Nothing notices if
  font-program fingerprinting stops working, which is what decides re-embed vs reuse on
  re-edit.
- **`FontRegistry::word_wrap` (`fonts.rs`) — 8 survivors.** All existing tests use text whose
  wrap point is unambiguous, so they pin *that* wrapping happens, never *where*. The PDF
  writer uses this function to place line breaks in saved files.
- **Media box arithmetic (`pdf/mod.rs`)** — `x1 - x0` survives `-` → `+` because every test
  fixture has an origin-zero media box.
- **Re-edit validation boundaries (`pdf/reedit.rs`, `pdf/metadata.rs`)** — size-guard and
  length-guard comparisons untested at their boundaries; two `strip_app_streams` match arms
  deletable.
- **Two hollow tests found** (`pdf/renderer.rs`): compile-time shape assertions that verify no
  behavior. Replaced with behavioral tests in spe-26l.6.

Equivalent/impractical mutants (documented on epic spe-26l, not chased): `descriptor_flags`
`|` → `^` (disjoint bits, XOR ≡ OR), italic-detection `||` (needs a font fixture with a
nonzero post-table italic angle but non-italic fsSelection), `MAX_FONT_FILE_SIZE` const
`*` → `+` (needs a >1 MB font fixture).

## How to run cargo-mutants on this repo

```sh
TMPDIR=~/.cache/cargo-mutants-tmp \
  cargo mutants -j 8 --timeout-multiplier 6 -o <dir-outside-repo> -- -- --include-ignored
```

Every flag is load-bearing:

- **`TMPDIR` off `/tmp`.** cargo-mutants copies the source tree plus a private `target/` per
  job. On a machine where `/tmp` is tmpfs, `-j8` ate 15 GB of RAM-backed storage and was
  heading for OOM. Point it at real disk.
- **`-j8` is the ceiling here, and it's not about cores.** A `-j12` run on 20 cores starved
  the per-mutant test runs: 27% of results came back TIMEOUT, and 25 of 31 `fonts.rs`
  "timeouts" were mutants that a `-j4` run had cleanly caught in 10–14 s. Phantom timeouts
  look exactly like hung tests; they are contention. Whatever host runs this regularly needs
  parallelism tuned to it, plus a generous `--timeout-multiplier`.
- **`-- -- --include-ignored`** (the doubled `--` matters: the first set is cargo's, the
  second the test binary's). Without it, mutants in system-utility wrapper code are judged
  against a suite missing their real tests and report false misses.
- **`-o` outside the repo.** cargo-mutants otherwise writes `mutants.out/` into the repo
  root.

Costs observed: full crate ≈ 3 h at `-j8`; single file ≈ 5–15 min; baseline build ≈ 1 min.
`cargo mutants --list` is free and gives per-file mutant counts.

## Recommendation

1. Close the pdf/fonts gaps — epic spe-26l, in progress.
2. Design the recurring run as its own piece of work. Full-crate is too slow for a PR gate;
   candidates are a scheduled full run plus `--in-diff` on changed files per PR. When that
   design happens, the run-config knowledge above graduates to
   `docs/designs/mutation-testing.md`; this document stays frozen as the point-in-time
   record.
3. Don't chase the whole-crate number to 100%: the `app/`/`ui/` misses are largely iced
   view/handler plumbing where a mutation-killing test costs more than it protects. Any
   future threshold should scope to `src/pdf/`, `src/fonts.rs`, `src/coordinate.rs`.

## Raw data

Full survivor lists per target file are reproduced in the spe-26l task descriptions
(`bd show spe-26l.1` … `spe-26l.6`). The complete run output (caught/missed/unviable/timeout
lists, per-mutant logs) lived in session scratchpad only; the numbers above are the durable
record.
