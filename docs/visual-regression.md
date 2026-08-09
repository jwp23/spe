# Visual Regression Testing

A development tool for catching unintended changes to canvas rendering — in
particular, the `draw_image` z-ordering workarounds (see
`docs/tech-stack-docs.md` / ADRs on Iced wgpu canvas ordering) that can't be
unit tested because Iced's `Frame` doesn't expose its primitive list for
inspection. Built on top of `scripts/screenshot.sh`; read
`docs/screenshot-tool.md` first if you haven't already.

Not wired into CI — cage requires a real (if headless) Wayland compositor,
which CI doesn't have, matching the existing e2e-test skip pattern. This is a
local dev tool, same as `scripts/screenshot.sh` itself.

## Prerequisites

Everything `docs/screenshot-tool.md` requires (cage, grim, socat), plus
ImageMagick for `compare` — used by `compare` to diff captures against
references. `scripts/visual-regression.sh` doesn't preflight-check for it;
install it yourself before running `compare`.

Quick install (Arch): `sudo pacman -S imagemagick`

## Quick Start

```bash
# Capture a scenario to screenshots/ for manual inspection
scripts/visual-regression.sh capture committed_tint

# Compare a fresh capture against the checked-in reference
scripts/visual-regression.sh compare committed_tint

# List available scenarios
scripts/visual-regression.sh list
```

`compare` exits 0 and prints `MATCH` when the fresh capture is within
tolerance of the reference, or exits 1 and prints `MISMATCH` (plus a diff
image path) otherwise.

## Scenarios

Each scenario is a bash function (`scenario_<name>` in
`scripts/visual-regression.sh`) that drives the app via IPC from a freshly
opened fixture to the state it wants to capture. Current scenarios target the
riskiest rendering paths — the ones most likely to break silently if a
`draw_image` z-order workaround regresses:

| Scenario | What it exercises |
|----------|-------------------|
| `committed_tint` | An overlay after commit (deselected): the tint-rectangle-behind-text workaround with nothing else drawn on top |
| `selected_overlay` | An overlay with its selection box: the selection-box-over-tint layering |
| `multiline_overlay` | A drag-created multiline overlay (IPC `type` with `\n` drives multi-line text on current main): text spanning several lines within one tint rectangle |
| `editing_multiline` | The same overlay reopened in the floating editor: the editor's line height must match the canvas's, or text jumps vertically on entering edit mode |

Adding a scenario: write a new `scenario_<name>` function following the
existing ones (open + `wait_ready` + `zoom_reset` + `wait_ready` before your
scenario, `wait_ready` + `wait_frame` after it, are handled for you by
`run_scenario`), then capture and eyeball it before committing a reference
PNG.

## References

Reference PNGs live in `tests/visual/<scenario>.png`. They're committed only
because we verified they're deterministic on this machine — see "Determinism"
below. If you regenerate a reference (e.g. after an intentional rendering
change), re-run the determinism check before committing it:

```bash
for i in 1 2 3; do
  scripts/visual-regression.sh capture <scenario> /tmp/run-$i.png
done
compare -metric AE /tmp/run-1.png /tmp/run-2.png /dev/null
compare -metric AE /tmp/run-2.png /tmp/run-3.png /dev/null
```

Both should report `0 (0)` (or stay within the tolerance band described
below) before you copy the capture over the reference.

References were regenerated on 2026-08-09 (spe-h8k), on top of the spe-d3m
`wait_ready` fix: PR #137 replaced the toolbar's font `pick_list` with a
previewing picker, which changed toolbar pixel layout again after the #131
regeneration — every scenario's diff against the pre-#137 reference was
confined to the toolbar strip (font-picker widget), with zero diff pixels
anywhere in the page/overlay content below it, confirmed by inspecting each
diff image. That's the expected fallout of an intended toolbar change, not a
content regression, so the references were regenerated rather than the
tolerance widened to absorb it.

## Comparison Method

`compare -metric AE` (ImageMagick's Absolute Error metric — count of
differing pixels) against the checked-in reference, with a small default
tolerance (`VISUAL_REGRESSION_TOLERANCE`, default 40 pixels out of 921,600
in a 1280x720 frame — see rationale in `scripts/visual-regression.sh` and the
recalibration measurement below).

Why ImageMagick instead of a Rust `image`-crate test: `compare` is already
installed on this system (part of the ImageMagick package), needs no new
dependency, and a two-line shell invocation does the whole job. A Rust test
would add a test binary, an `image`-crate diff routine, and CLI plumbing to
invoke the same cage/grim harness `screenshot.sh` already drives — more
moving parts for the same result. If this tooling ever needs to run
somewhere ImageMagick isn't available, revisit with a `image`-crate
comparison (the crate is already a project dependency).

## Determinism

Confirmed by running each scenario 3+ times (10+ for `committed_tint`) and
diffing every pair with `compare -metric AE`. The measurements below predate
the spe-d3m `wait_ready` fix; see "Recalibration after the spe-d3m
`wait_ready` fix" further down for the current (zero-variance) numbers this
tolerance is actually calibrated against.

- Most runs, across all three scenarios (this project had three scenarios at
  the time): 0 pixels different.
- Occasionally (roughly a quarter of runs, any scenario, not specific to
  one): a stable ~85-pixel difference (0.0093% of the frame), localized to
  antialiasing right around an overlay's tint or selection box border — not
  a content difference, verified by visual diff. Small enough that the
  then-default 150-pixel tolerance absorbed it without also absorbing a real
  regression (a missing/mispositioned overlay showed up as 700,000+ pixels
  different in testing).

Getting to that required fixing two real timing races the harness was
hitting, not scenario-script luck:

1. **Zoom-to-fit-width races the window resize event.** On `open`, the app
   auto-zooms to fit the widest page, which reads `window_size` — set by a
   resize event whose arrival relative to document load isn't guaranteed
   under the headless compositor. Depending on which arrived first, the same
   scenario could render at 95% or 100% zoom, shifting every overlay
   position. Fixed by sending `zoom_reset` (a fixed 100%, independent of
   window size) right after `wait_ready`, instead of relying on whatever
   fit-width computed.
2. **`wait_ready` doesn't guarantee a presented frame.** It only blocks on
   page-image rendering (`pdftoppm` output being ready) — not on cage
   actually compositing and presenting a new Wayland frame that reflects the
   click/type/deselect state we just sent. Capturing immediately after the
   last command intermittently (~40% of runs) grabbed the previous frame,
   showing a blank page.

   **Fixed (spe-xqb) with a `wait_frame` IPC command**, replacing the fixed
   300ms settle sleep. The app tracks `state_generation` (bumped on every
   processed message) and `presented_generation` (set to the current
   `state_generation` whenever iced's `RedrawRequested` event fires —
   `frame_event_to_message` in `src/app/mod.rs`). `RedrawRequested` is
   broadcast to subscriptions synchronously, immediately before
   `compositor.present()` is called in the same non-yielding block of
   `iced_winit`'s event loop, so by the time the app processes the resulting
   message the frame has already been submitted. `wait_frame` blocks until
   `presented_generation` reaches the generation of the last command sent
   before it.

   Residual gap: this proves iced submitted the frame to wgpu's `present()`;
   it does not prove cage has finished compositing it and that `grim`'s
   capture lands on the composited output rather than a frame still in
   flight between GPU submission and the compositor's next repaint. No
   signal in iced 0.14 closes that last hop — there is no
   `window::frames()`-style post-present hook in this version, and no lower
   layer here to observe compositor-side presentation.

   Determinism, `committed_tint` scenario, 5 runs each, full harness
   start/stop per run (not just repeated captures in one session). Values
   are `compare -metric AE` output against the checked-in reference PNG —
   ImageMagick's Absolute Error metric, i.e. a count of differing pixels,
   not a percentage or a distance measure:

   | AE pixel count vs. reference | run 1 | run 2 | run 3 | run 4 | run 5 |
   |---|---|---|---|---|---|
   | 300ms sleep (old) | 230.103 | 230.103 | 315.492 | 315.492 | 230.103 |
   | `wait_frame` (new) | 315.492 | 315.492 | 315.492 | 315.492 | 315.492 |

   The sleep flips between two states (3/5 vs 2/5) in this environment;
   `wait_frame` is pixel-identical across all 5. Same pattern held for
   `selected_overlay` (5/5 identical under `wait_frame`). `multiline_overlay`
   showed the same ~85-pixel antialiasing variance under both methods (1-2
   runs out of 5 differ by ~85px either way) — that variance is the separate,
   pre-existing antialiasing effect described above, not the frame-presented
   race `wait_frame` targets. None of these are the 700,000+-pixel
   blank-page failures the old race produced; they're all small, localized
   diffs against the checked-in reference images, consistent with this
   environment's font rendering differing slightly from whatever machine
   generated the references — a pre-existing gap, not a regression.

### Recalibration after the spe-d3m `wait_ready` fix

spe-d3m fixed the `zoom_reset`-then-`wait_ready` staleness gap described
below (`is_render_idle` now also checks that `CanvasState::rendered_generation`
has caught up to `zoom_generation`, not just that `page_images` has an entry
per page — see `src/app/mod.rs`). Re-measured determinism afterward, 5 runs
per scenario (10 for `committed_tint`), full harness start/stop per run,
diffing every consecutive pair plus the first-vs-last pair with
`compare -metric AE`:

| Scenario | comparisons | pixels different |
|---|---|---|
| `committed_tint` | 6 (10 runs) | 0 (0) every pair |
| `selected_overlay` | 5 (5 runs) | 0 (0) every pair |
| `multiline_overlay` | 5 (5 runs) | 0 (0) every pair |
| `editing_multiline` | 5 (5 runs) | 0 (0) every pair |

25/25 comparisons pixel-identical — no ~85px antialiasing wobble and no
230-315px drift this time. Both of those were symptoms of the two races
above (fit-width-vs-resize, `wait_ready`'s pre-frame-signal blind spot, and
now the zoom-generation staleness gap); with all three closed, this
environment's variance floor measures as zero. `TOLERANCE_PIXELS` was
lowered from 150 to 40 (`scripts/visual-regression.sh`) — a margin over that
zero floor for whatever residual AA jitter a longer run might still turn up,
without weakening the check: 40 is still five orders of magnitude below the
~700k+-pixel deltas a real regression produces.

This measurement is from one machine. If variance reappears elsewhere,
that's this environment's font/GPU rendering rather than a returned race —
see "References" above for how to tell an intended-change diff (localized to
the affected widget) from noise, and regenerate references locally rather
than loosening the tolerance to paper over a cross-machine offset.

### Staleness window: which commands are safe before `wait_frame`

`IpcEvent::Command` batches the command's own task with the task that sends
the IPC reply (`Task::batch([command_task, response_task])`,
`src/app/mod.rs`). `wait_frame`'s target is a snapshot of `state_generation`
taken when it is processed, so a command whose handler chains a **trailing
task that later delivers its own `Message`** could still bump
`state_generation` after that snapshot — `wait_frame` would then resolve on
a frame that predates the trailing effect.

Audited every handler `scripts/visual-regression.sh` scenarios actually call
— `click`, `click_at`, `drag`, `type`, `select`, `deselect` — against this
risk (`src/app/handlers.rs`):

- `handle_place_overlay` (`click`/`drag`) and `handle_edit_overlay` (`edit`,
  not currently used by any scenario) return
  `Task::batch([commit_task, iced::widget::operation::focus(...)])`.
  `commit_task` is always `Task::none()` or the result of `handle_commit_text`,
  which is itself always `Task::none()`. `focus()` is `task::effect(Action::
  widget(...))` (`iced_runtime-0.14.0/src/widget/operation.rs:65`) — applied
  synchronously inside `run_action`, in the same event-loop turn as the
  command, and **never produces a `Message`**
  (`iced_winit-0.14.0/src/lib.rs:1742`, the `Action::Widget` branch calls
  `ui.operate` directly with no message channel involved). It cannot bump
  `state_generation` later, so it cannot make `wait_frame` stale.
- `handle_update_overlay_text` (`type`) and `handle_select_overlay`/
  `handle_deselect_overlay` (`select`/`deselect`) don't return a `Task` at
  all (`type`) or only ever return `Task::none()` (`select`/`deselect`,
  through the same `commit_before_targeting` → `handle_commit_text` path).

**Conclusion: none of `click` / `click_at` / `drag` / `type` / `select` /
`deselect` can leave `wait_frame` stale.** They are safe to send in any
order before a single trailing `wait_frame`, which is exactly how the
current scenario functions use them.

`open` and the `zoom_*` commands are different: `handle_file_opened` and
`apply_zoom_change` (`zoom_reset` and friends) both chain genuinely delayed
async work — `open` via a real `pdftoppm` render task delivering
`Message::PageBatchRendered`, `zoom_*` via a 300ms debounce
(`schedule_zoom_render`) delivering `Message::ZoomDebounceExpired`, which
clears and re-renders `page_images` at the new DPI. `run_scenario` already
sends `wait_ready` after both, before ever reaching a scenario function.

`wait_ready`'s precondition (`is_render_idle`, `src/app/mod.rs`) used to only
check that every page number had *some* entry in `page_images` — it did not
check that entry was rendered at the current `zoom_generation`. Structurally,
`wait_ready` sent right after `zoom_reset` (or any zoom change) could return
`ok` immediately using images still cached from before the zoom change,
before the 300ms debounce ever fired — a real gap in `wait_ready`'s
invariant, not `wait_frame`'s design; it predated spe-xqb.

This environment never observed the gap in practice: `zoom_reset`'s `1.0 →
1.0` is a no-op here (`window_size` always makes `canvas::fit_to_width_zoom`
compute exactly `1.0` by the time `open` runs), so the missing freshness
check never had a stale image to hand back. **Fixed in spe-d3m** using a
real zoom level (`zoom_in`, not the no-op `zoom_reset`) to exercise it:
`is_render_idle` now also requires `CanvasState::rendered_generation ==
CanvasState::zoom_generation`, where `rendered_generation` is bumped only
when the debounced re-render actually starts (see
`App::handle_zoom_debounce_expired`). Unit tests pin the behavior
(`app::tests::is_render_idle_false_after_zoom_before_debounce_fires`,
`app::tests::is_render_idle_true_once_rerender_catches_up_to_zoom_generation`);
verified end-to-end against the running harness too (`zoom_in` →
`wait_ready` → capture immediately vs. `zoom_in` → sleep past the debounce →
`wait_ready` → capture: both pixel-identical, `compare -metric AE` = 0,
while a before/after-`zoom_in` capture pair differs by 623 pixels, proving
the zoom itself is visible and the immediate capture correctly reflects it
rather than a stale pre-zoom frame).

### Possible follow-ups (not done here)

- Close the residual compositor-presentation gap `wait_frame` cannot
  observe from inside the iced process (would need a Wayland-side signal —
  e.g. a frame callback observed by the harness itself — not something the
  app's IPC socket can report).
- Wire this into CI once/if a headless-compositor-capable CI runner exists.
