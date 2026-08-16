# Spike: running the cage-based e2e suite in CI

**Date:** 2026-08-16 · **Question:** can `tests/ipc_e2e.rs` run green on `ubuntu-latest`? ·
**Outcome:** yes, and cheaply — proceed with spe-qvm, adding `cage` to the existing `check`
job rather than a new one.

## Headline result

**All five `ipc_e2e` tests pass on a stock hosted `ubuntu-latest` runner, in 2–5 seconds, with
exactly one new package: `cage`.** No GPU, no Vulkan driver, no `XDG_RUNTIME_DIR` setup, no
change to the tests. wgpu falls back to the GL backend on llvmpipe and iced renders fine.

The recorded "CI doesn't have a compositor" blocker is moot: `WLR_BACKENDS=headless` means
wlroots never needs one. The feared blocker — wgpu failing under software rendering — did not
materialise.

Measured across five independent runner jobs — four of them on the minimal package set — for 15
`ipc_e2e` invocations plus nine combined `cargo test -- --ignored` invocations. Zero failures,
zero flakes.

## Verdicts on the five questions

| # | Question | Verdict |
|---|---|---|
| 1 | Does `cage` install, and does `cage -v` exit 0? | **Yes.** `cage 0.1.5+20240127-2build1` from Ubuntu 24.04 universe. |
| 2 | Does `cage` start headless on a runner? | **Yes.** Starts, serves `wayland-0`, exits 0. |
| 3 | Does wgpu initialise under software rendering? | **Yes.** llvmpipe, GL backend, no Vulkan needed. |
| 4 | Does the suite pass, with proof it did not skip? | **Yes**, and proven three independent ways. |
| 5 | How long, and is it stable? | **2–5 s** for the suite; stable across 5 jobs. |

### Q1 — cage installs and reports version

`sudo apt-get install -y cage` on `Ubuntu 24.04.4 LTS`:

```text
Setting up cage (0.1.5+20240127-2build1) ...
Cage version 0.1.5
cage -v exit status: 0
```

Worth noting: this is much older than a rolling distro's cage (0.3.1 locally). It is still
new enough for everything the tests do, but it is the version CI will pin to for the life of
Ubuntu 24.04.

### Q2 — cage starts headless, no compositor and no GPU

Launched exactly as `scripts/screenshot.sh:60-68` does, wrapping `/bin/sh` instead of the app:

```text
client saw WAYLAND_DISPLAY=[wayland-0]
cage exit status: 0
```

The blocker was real about the runner (there is genuinely no compositor and no GPU) and wrong
about the consequence. `WLR_BACKENDS=headless` makes wlroots allocate a virtual output and skip
DRM and libinput entirely, so there is nothing left to be missing.

wlroots still logs an error while doing this, and it is harmless — it appears in every passing
run:

```text
[ERROR] [render/wlr_renderer.c:279] Failed to find any DRM render node
```

### Q3 — wgpu initialises on llvmpipe, via GL

This was the identified real risk. It is not a problem.

Enumerated through the same wgpu version the app links (27.0.1, per `Cargo.lock`), from a
throwaway crate so nothing in the tree changed. With **only `cage` added** to the packages CI
already installs:

```text
adapters found: 1
AdapterInfo { name: "llvmpipe (LLVM 20.1.2, 256 bits)", vendor: 65541, device: 0,
              device_type: Cpu, driver: "",
              driver_info: "4.5 (Core Profile) Mesa 25.2.8-0ubuntu0.24.04.2", backend: Gl }
default request_adapter picked: <the same adapter>
```

There is no Vulkan ICD on the runner at all (`/usr/share/vulkan/icd.d/` does not exist), so wgpu
picks the GL backend on Mesa's llvmpipe software rasteriser. It needs no coaxing: no
`WGPU_BACKEND`, no `LIBGL_ALWAYS_SOFTWARE`, no `force_fallback_adapter`.

Adding `mesa-vulkan-drivers` was also tried, and it does work — it produces a second, Vulkan
adapter which `request_adapter` then prefers:

```text
adapters found: 2
AdapterInfo { name: "llvmpipe (LLVM 20.1.2, 256 bits)", ..., backend: Vulkan }
AdapterInfo { name: "llvmpipe (LLVM 20.1.2, 256 bits)", ..., backend: Gl }
default request_adapter picked: <the Vulkan one>
```

Both configurations pass all five tests. **Do not install it.** It is a second code path to
maintain for no gain, and leaving it out keeps CI on the same backend regardless of what a
future runner image happens to preinstall.

Two more errors appear in passing runs and are noise, from Xwayland and Mesa's zink driver
respectively:

```text
Failed to initialize glamor, falling back to sw
MESA: error: ZINK: failed to choose pdev
```

### Q4 — the suite really ran

The failure mode this spike had to rule out is a green result that means nothing:
`cage_available()` (`tests/ipc_e2e.rs:37`) returns false when cage is missing, and every test
then returns early and passes. Three independent checks, all enforced by the probe job:

1. **A hard gate.** `cage -v` runs as its own step before the tests and fails the job on a
   non-zero exit.
2. **A count and a skip check.** Five `#[test]` functions are declared; the job asserts five
   `test ... ok` lines, and asserts zero `SKIP ipc_` lines on stderr:

   ```text
   tests/ipc_e2e.rs declares 5 #[test] functions
   run 1: harness summary — test result: ok. 5 passed; 0 failed; 0 ignored; ... finished in 3.45s
   run 1: 5 of 5 tests reported ok, 0 self-reported skips
   ```

3. **Deliberate sabotage.** A run with `assert!(false, ...)` injected into
   `ipc_open_save_with_no_overlays_still_writes_file`, *after* its `cage_available()` guard,
   went red as required (run `31966256007`):

   ```text
   422:    assert!(false, "SPIKE SABOTAGE: this run must be RED");
   SPIKE SABOTAGE: this run must be RED
   test result: FAILED. 4 passed; 1 failed; 0 ignored; ... finished in 7.59s
   run 1: SUITE INCOMPLETE
   ##[error]Process completed with exit code 1.
   ```

   Placement matters: inject before the guard and a skipping suite would go red too, proving
   nothing. Injecting after it means only a suite that genuinely executes can fail.

Two of the five tests also assert on text extracted by `pdftotext` from a PDF the app just
rendered and saved, which cannot pass unless the whole render-place-type-save path really ran.

### Q5 — timings and stability

The suite is far cheaper than expected. Step-measured wall clock, three invocations per job:

| Job | Packages | `ipc_e2e` ×3 | combined `--ignored` ×3 |
|---|---|---|---|
| `31966504752` | full | 4 s / 3 s / 3 s | — |
| `31966632275` | minimal | 5 s / 2 s / 3 s | — |
| `31966751124` | minimal | 3 s / 3 s / 2 s | 16 s / 5 s / 5 s |
| `31966877898` | minimal | 5 s / 2 s / 3 s | 16 s / 6 s / 5 s |
| `31967008279` | minimal | 3 s / 3 s / 2 s | 15 s / 6 s / 5 s |

"full" adds `mesa-vulkan-drivers`, `libglx-mesa0`, `vulkan-tools` and `mesa-utils`; "minimal"
adds only `cage`. The package set makes no measurable difference. These are step-measured wall
clock at one-second resolution; the harness's own reported figures are slightly tighter, 2.13 s
to 4.34 s, the difference being cargo's up-to-date check.

The first invocation in each job is consistently ~1.5 s slower than subsequent ones — cold page
cache, not variance. Nothing else moves. **Zero failures and zero flakes across all of it.**

Job-level cost of the whole probe was ~2 minutes, but most of that is the throwaway wgpu crate.
The cost that matters for spe-qvm is only what gets added to the `check` job:

- `apt-get update && apt-get install cage`: **13 s** (`cage` pulls in Xwayland and wlroots).
- The tests themselves: **~0 s.** `cargo test -- --ignored` already runs in `check` today and
  already builds this binary; the five tests currently return instantly instead of running, and
  running them costs about two seconds.

Total: **roughly 15 seconds on the critical path.**

## The working job definition

The complete change to `.github/workflows/ci.yml`'s `check` job is one word:

```yaml
      - name: Install system utilities
        run: sudo apt-get update && sudo apt-get install -y poppler-utils fontconfig cage
```

That is all. Specifically, none of the following turned out to be needed:

- **No `XDG_RUNTIME_DIR` setup.** The runner already sets it (`/run/user/1001`), and
  `launch_app` overrides it per-test with its own temp directory anyway, so
  `socket_path()`'s `MissingRuntimeDir` refusal never comes into play. The design note flagged
  this as likely necessary; it is not.
- **No graphics packages.** `libgl1-mesa-dri` is preinstalled; nothing else is required.
- **No wgpu environment variables.**
- **No changes to any test.**
- **No separate job.** See below.

## One job or two?

**One — keep it in `check`.** spe-6i8 recorded GPU/compositor contention between `e2e.rs` and
`ipc_e2e.rs` under a parallel `cargo test -- --ignored`, and closed as "serialization not
needed" after a fix on developer hardware. This spike re-tested that verdict on the harsher
case — a 4-vCPU runner with no GPU — by running the full combined `cargo test -- --ignored`
nine times across three jobs. All nine were clean, 5/5 `ipc_e2e` tests passing every time, in
5–16 seconds. The verdict holds; a separate job would buy isolation nobody needs and cost a
second toolchain setup and cargo build.

## Notes for whoever implements spe-qvm

- **The skip-on-missing-cage behaviour should stay**, so the suite still degrades gracefully on
  a contributor's machine — but CI must not rely on it. Whatever job runs these tests needs a
  hard `cage -v` gate, or the protection silently evaporates the day the package name changes
  or universe is unavailable. This is the single most important thing to carry forward.
- **Never assert on a merged log.** The probe's first pass at counting passes was wrong, not the
  suite: under `--nocapture`, cage, Xwayland and the app all write to the inherited stderr and
  interleave *mid-line* with the harness's own `test ... ok` progress on stdout, breaking an
  anchored grep. Redirect the two streams to separate files and count from stdout.
- `--nocapture` is worth keeping in CI regardless. It is how the wlroots and Mesa diagnostics
  above become visible, and none of them are fatal.

## Out of scope, and still open

**`scripts/visual-regression.sh` is not covered by this result.** It shares the cage dependency,
which this spike settles, but it compares captured PNGs against committed references in
`tests/visual/` with a 40-pixel tolerance — a threshold `docs/visual-regression.md` derives from
same-machine determinism measurements. Those references were generated on a developer machine
with real GPU rendering; a runner rasterising through llvmpipe with a different font stack is
very likely to drift far past 40 pixels. That is a font-rendering and reference-management
question, not a compositor one, and it needs its own investigation. **This spike did not test
it, and nothing here should be read as evidence either way.**

## Reproducing

`.github/workflows/e2e-ci-probe.yml` (`workflow_dispatch` only, gates nothing) captures every
measurement above. Inputs: `repeats` for the timing loop, `sabotage` to re-run the
proof-of-not-skipping check — which must go **red**. Delete the file once spe-qvm lands.
