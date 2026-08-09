#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SCREENSHOT_DIR="$PROJECT_DIR/screenshots"

# Isolate each harness invocation so parallel worktrees don't collide on the
# IPC socket, pidfile, or Wayland display. Defaults to a short hash of
# PROJECT_DIR so a given checkout gets a stable instance across start/send/
# capture/stop calls without any setup; SPE_SCREENSHOT_INSTANCE overrides it
# (e.g. to run two instances from the same checkout). Unix socket paths have
# a 108-byte limit, so the key must stay short regardless of how long the
# worktree path is.
INSTANCE_ID="${SPE_SCREENSHOT_INSTANCE:-$(printf '%s' "$PROJECT_DIR" | sha256sum | cut -c1-8)}"

# Guard against a malicious or malformed SPE_SCREENSHOT_INSTANCE: it is
# spliced into RUNTIME_DIR below, which gets rm -rf'd on every start and
# stop. A value like '../../some-dir' would otherwise escape the
# spe-screenshot-<instance> subtree. The length bound also keeps the socket
# path under the 108-byte AF_UNIX limit.
if [[ ! "$INSTANCE_ID" =~ ^[A-Za-z0-9_-]{1,32}$ ]]; then
    echo "Invalid SPE_SCREENSHOT_INSTANCE (want 1-32 chars of [A-Za-z0-9_-])" >&2
    exit 1
fi

RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}/spe-screenshot-$INSTANCE_ID"
SOCKET_PATH="$RUNTIME_DIR/spe-ipc.sock"
PIDFILE="$RUNTIME_DIR/harness.pid"
CAGE_DISPLAY_FILE="$RUNTIME_DIR/display"

check_deps() {
    local missing=()
    command -v cage >/dev/null 2>&1 || missing+=(cage)
    command -v grim >/dev/null 2>&1 || missing+=(grim)
    command -v socat >/dev/null 2>&1 || missing+=(socat)
    command -v sha256sum >/dev/null 2>&1 || missing+=(sha256sum)
    if [[ ${#missing[@]} -gt 0 ]]; then
        echo "Missing dependencies: ${missing[*]}"
        echo "Install with: sudo pacman -S ${missing[*]}"
        exit 1
    fi
}

do_start() {
    check_deps

    # Register cleanup trap only for start
    trap 'do_stop 2>/dev/null || true' EXIT

    echo "Building app..."
    cargo build --manifest-path "$PROJECT_DIR/Cargo.toml"

    # Fresh, isolated runtime dir for this instance's socket and Wayland
    # display. Wiping it here means the wayland-* detection below never sees
    # a stale socket left over from a crashed previous run.
    rm -rf "$RUNTIME_DIR"
    mkdir -m 0700 -p "$RUNTIME_DIR"

    echo "Starting cage compositor (headless)..."
    # WLR_BACKENDS=headless: virtual display, no GPU or parent compositor needed.
    # Unset WAYLAND_DISPLAY so cage doesn't try to nest inside the host compositor.
    # XDG_RUNTIME_DIR is overridden to this instance's isolated dir, so the
    # app's IPC socket (src/ipc.rs socket_path()) and cage's Wayland socket
    # both land there instead of colliding with other running instances.
    env -u WAYLAND_DISPLAY XDG_RUNTIME_DIR="$RUNTIME_DIR" \
        WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
        cage -- "$PROJECT_DIR/target/debug/spe" --ipc &
    local cage_pid=$!
    echo "$cage_pid" > "$PIDFILE"

    echo "Waiting for IPC socket..."
    for i in {1..30}; do
        if [[ -S "$SOCKET_PATH" ]]; then
            local cage_display
            cage_display="$(ls "$RUNTIME_DIR"/wayland-* 2>/dev/null | grep -v '\.lock$' | head -1)"
            if [[ -n "$cage_display" ]]; then
                cage_display="$(basename "$cage_display")"
            else
                cage_display="wayland-0"
            fi
            echo "$cage_display" > "$CAGE_DISPLAY_FILE"
            echo "Ready (PID $cage_pid, display $cage_display)"
            # Clear the trap so the script can exit without stopping
            trap - EXIT
            return 0
        fi
        sleep 0.5
    done
    echo "Timeout waiting for IPC socket"
    kill "$cage_pid" 2>/dev/null || true
    exit 1
}

do_stop() {
    if [[ -f "$PIDFILE" ]]; then
        local pid
        pid="$(cat "$PIDFILE")"
        kill "$pid" 2>/dev/null || true
        rm -rf "$RUNTIME_DIR"
        echo "Stopped"
    else
        echo "Not running"
    fi
}

do_send() {
    if [[ -z "${1:-}" ]]; then
        echo "Usage: $0 send '<json>'"
        exit 1
    fi
    # -t 15: socat half-closes the connection this long after stdin EOF. The
    # default is 0.5s, which silently drops the app's reply whenever a command
    # takes longer than that (rendering, wait_ready, or just a loaded machine) —
    # printing nothing, which reads exactly like a successful no-op.
    echo "$1" | socat -t 15 - UNIX-CONNECT:"$SOCKET_PATH"
}

do_capture() {
    local output="${1:-$SCREENSHOT_DIR/latest.png}"
    mkdir -p "$(dirname "$output")"
    if [[ ! -f "$CAGE_DISPLAY_FILE" ]]; then
        echo "Harness not running (no display file). Run '$0 start' first."
        exit 1
    fi
    local display
    display="$(cat "$CAGE_DISPLAY_FILE")"
    XDG_RUNTIME_DIR="$RUNTIME_DIR" WAYLAND_DISPLAY="$display" grim "$output"
    echo "Captured: $output"
}

case "${1:-}" in
    start)   do_start ;;
    stop)    do_stop ;;
    send)    do_send "${2:-}" ;;
    capture) do_capture "${2:-}" ;;
    *)
        echo "Usage: $0 {start|stop|send|capture}"
        exit 1
        ;;
esac
