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

Both should report `0 (0)` (or, for `selected_overlay`, stay within the ~85
pixel band described below) before you copy the capture over the reference.

## Comparison Method

`compare -metric AE` (ImageMagick's Absolute Error metric — count of
differing pixels) against the checked-in reference, with a small default
tolerance (`VISUAL_REGRESSION_TOLERANCE`, default 150 pixels out of 921,600
in a 1280x720 frame — see rationale in `scripts/visual-regression.sh`).

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
diffing every pair with `compare -metric AE`:

- Most runs, across all three scenarios: 0 pixels different.
- Occasionally (roughly a quarter of runs, any scenario, not specific to
  one): a stable ~85-pixel difference (0.0093% of the frame), localized to
  antialiasing right around an overlay's tint or selection box border — not
  a content difference, verified by visual diff. Small enough that the
  default 150-pixel tolerance absorbs it without also absorbing a real
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

That said, `wait_ready`'s own precondition (`is_render_idle`, `src/app/
mod.rs`) only checks that every page number has *some* entry in
`page_images` — it does not check that entry was rendered at the current
`zoom_generation`. Structurally, `wait_ready` sent right after `zoom_reset`
could return `ok` immediately using images still cached from before the
zoom change, before the 300ms debounce ever fires — a real gap, but one
that belongs to `wait_ready`'s invariant, not to `wait_frame`'s design; it
predates spe-xqb.

Empirically tested against the running harness (open → wait_ready →
capture; zoom_reset → wait_ready → capture immediately; sleep 500ms →
wait_ready → capture again): all three captures were pixel-identical
(`compare -metric AE` = 0 for every pair). In this headless cage setup,
`window_size` never differs from what already makes
`canvas::fit_to_width_zoom` compute exactly `1.0` by the time `open` runs
(or the resize event hasn't landed yet), so `zoom_reset`'s `1.0 → 1.0`
change is a no-op here — the debounce still fires 300ms later, but
re-renders identical output. The gap is real and unaddressed, but it has
no observed effect on `scripts/visual-regression.sh`'s scenarios in this
environment. Left as documented, scoped-out follow-up rather than folded
into `wait_frame` — see below.

### Possible follow-ups (not done here)

- Close the residual compositor-presentation gap `wait_frame` cannot
  observe from inside the iced process (would need a Wayland-side signal —
  e.g. a frame callback observed by the harness itself — not something the
  app's IPC socket can report).
- Make `wait_ready`'s `is_render_idle` check `zoom_generation` freshness,
  not just key presence, to close the (currently dormant, in this
  environment) `zoom_reset`-then-`wait_ready` staleness gap described
  above. Out of scope for spe-xqb: it is a pre-existing property of
  `wait_ready`, not something the frame-presented signal introduced or is
  responsible for fixing.
- Regenerate the checked-in reference PNGs in `tests/visual/` from this
  environment, or otherwise resolve the small constant offset between them
  and this machine's font rendering, so `compare` reports MATCH instead of
  a tolerated MISMATCH.
- Wire this into CI once/if a headless-compositor-capable CI runner exists.
