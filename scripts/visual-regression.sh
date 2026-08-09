#!/usr/bin/env bash
# Visual regression capture and comparison, built on scripts/screenshot.sh.
#
# Each scenario is a scripted IPC sequence against a fresh app instance:
# open the fixture, drive the UI into a specific rendering state, capture a
# PNG. Every scenario starts and stops its own harness instance so captures
# never see state left over from a previous scenario or a previous run.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SCREENSHOT_SH="$SCRIPT_DIR/screenshot.sh"
REFERENCE_DIR="$PROJECT_DIR/tests/visual"
FIXTURE="$PROJECT_DIR/tests/fixtures/single-page.pdf"

# Comparison tolerance: count of differing pixels (ImageMagick's AE metric)
# allowed before a comparison is reported as a mismatch. Captures are
# pixel-exact most of the time (see docs/visual-regression.md), but any
# scenario occasionally shows a stable ~85-pixel difference — small,
# localized antialiasing variance around an overlay's border, not a content
# difference. 150 comfortably covers that while staying far below the
# ~700k+ pixel deltas a real rendering regression (e.g. a missing overlay)
# produces.
TOLERANCE_PIXELS="${VISUAL_REGRESSION_TOLERANCE:-150}"

send() { "$SCREENSHOT_SH" send "$1" >/dev/null; }

# Each scenario function drives the app from a freshly opened fixture to the
# state it wants to capture. Keep them small and additive so new scenarios
# are cheap to write.

scenario_committed_tint() {
    send '{"cmd": "click", "page": 1, "x": 100, "y": 700}'
    send '{"cmd": "type", "text": "Hello world"}'
    send '{"cmd": "deselect"}'
}

scenario_selected_overlay() {
    send '{"cmd": "click", "page": 1, "x": 100, "y": 700}'
    send '{"cmd": "type", "text": "Hello world"}'
    send '{"cmd": "deselect"}'
    send '{"cmd": "select", "index": 0}'
}

scenario_multiline_overlay() {
    send '{"cmd": "drag", "page": 1, "x1": 100, "y1": 500, "x2": 300, "y2": 500}'
    send '{"cmd": "type", "text": "Line one\nLine two\nLine three"}'
    send '{"cmd": "deselect"}'
}

list_scenario_names() {
    declare -F | awk '{print $3}' | grep '^scenario_' | sed 's/^scenario_//'
}

validate_scenario() {
    local name="$1"
    local fn="scenario_$name"
    if ! declare -F "$fn" >/dev/null; then
        echo "Unknown scenario: $name" >&2
        echo "Available scenarios: $(list_scenario_names | tr '\n' ' ')" >&2
        exit 1
    fi
}

run_scenario() {
    local name="$1" output="$2"
    validate_scenario "$name"
    # validate_scenario's copy is local to it, so resolve the function name
    # here too rather than reaching for a variable that is out of scope.
    local fn="scenario_$name"

    "$SCREENSHOT_SH" start >&2
    trap '"$SCREENSHOT_SH" stop >&2 || true' EXIT

    send '{"cmd": "open", "path": "'"$FIXTURE"'"}'
    send '{"cmd": "wait_ready"}'
    # Pin zoom to a fixed 100% instead of leaving it at whatever
    # zoom-to-fit-width computed: fit-width depends on window_size, which is
    # set by a resize event that races the document load under the headless
    # compositor, so its result (and therefore overlay screen position) is
    # not reproducible across runs without this.
    send '{"cmd": "zoom_reset"}'
    send '{"cmd": "wait_ready"}'
    "$fn"
    send '{"cmd": "wait_ready"}'
    # wait_ready only guards page-image rendering (pdftoppm), not the
    # compositor actually presenting a frame that reflects our state
    # mutations. wait_frame closes that gap: the app tracks a generation
    # counter bumped on every processed message, and records the generation
    # as of each completed redraw (iced's RedrawRequested event, which fires
    # synchronously just before the frame is submitted to the compositor —
    # see frame_event_to_message in src/app/mod.rs). wait_frame blocks until
    # a redraw has been observed at or after the generation of every command
    # sent before it, so the reply proves the click/type/deselect above has
    # actually been drawn and submitted.
    #
    # Residual gap: this proves iced submitted the frame to wgpu's present();
    # it does not prove the Wayland compositor (cage here) has finished
    # compositing and grim's capture has landed on the composited output
    # rather than a frame still in flight. That residual race is far smaller
    # than the "no signal at all" gap the old 300ms sleep covered, so no
    # sleep remains here — see 5x-determinism results in spe-xqb.
    send '{"cmd": "wait_frame"}'

    mkdir -p "$(dirname "$output")"
    "$SCREENSHOT_SH" capture "$output" >&2

    "$SCREENSHOT_SH" stop >&2
    trap - EXIT
}

do_capture() {
    local name="${1:?Usage: $0 capture <scenario> [output]}"
    local output="${2:-$PROJECT_DIR/screenshots/visual-$name.png}"
    run_scenario "$name" "$output"
    echo "$output"
}

do_compare() {
    local name="${1:?Usage: $0 compare <scenario> [reference]}"
    validate_scenario "$name"
    local reference="${2:-$REFERENCE_DIR/$name.png}"

    if [[ ! -f "$reference" ]]; then
        echo "No reference image at $reference" >&2
        exit 1
    fi

    local candidate
    candidate="$(mktemp --suffix=.png)"
    run_scenario "$name" "$candidate" >/dev/null

    # compare's AE output is "<count> (<normalized fraction>)"; keep just the
    # count, which can be fractional (partial pixel weighting) as well as
    # integral. compare exits 1 whenever the images differ at all (even
    # within our tolerance), so `|| true` is required under set -e/pipefail
    # or any nonzero-but-tolerable diff would abort the script before the
    # tolerance check below ever runs.
    local diff
    diff="$(compare -metric AE "$reference" "$candidate" /dev/null 2>&1 | awk '{print $1}' || true)"

    if [[ "$diff" =~ ^[0-9]+(\.[0-9]+)?$ ]] && awk -v d="$diff" -v t="$TOLERANCE_PIXELS" 'BEGIN{exit !(d<=t)}'; then
        echo "MATCH: $name ($diff pixels different, tolerance $TOLERANCE_PIXELS)"
        rm -f "$candidate"
        exit 0
    fi

    local diff_image="$PROJECT_DIR/screenshots/visual-$name-diff.png"
    mkdir -p "$(dirname "$diff_image")"
    compare "$reference" "$candidate" "$diff_image" 2>/dev/null || true
    echo "MISMATCH: $name ($diff pixels different, tolerance $TOLERANCE_PIXELS)" >&2
    echo "Candidate saved to: $candidate" >&2
    echo "Diff image saved to: $diff_image" >&2
    exit 1
}

do_list() {
    list_scenario_names
}

case "${1:-}" in
    capture) do_capture "${2:-}" "${3:-}" ;;
    compare) do_compare "${2:-}" "${3:-}" ;;
    list)    do_list ;;
    *)
        echo "Usage: $0 {capture|compare|list} [scenario] [path]" >&2
        echo "Available scenarios: $(list_scenario_names | tr '\n' ' ')" >&2
        exit 1
        ;;
esac
