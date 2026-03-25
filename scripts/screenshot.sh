#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
WAYLAND_DISPLAY_NAME="wayland-spe-test"
SOCKET_PATH="${XDG_RUNTIME_DIR:-/tmp}/spe-ipc.sock"
SCREENSHOT_DIR="$PROJECT_DIR/screenshots"
PIDFILE="/tmp/spe-screenshot-harness.pid"

check_deps() {
    local missing=()
    command -v cage >/dev/null 2>&1 || missing+=(cage)
    command -v grim >/dev/null 2>&1 || missing+=(grim)
    command -v socat >/dev/null 2>&1 || missing+=(socat)
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

    # Clean up stale socket
    rm -f "$SOCKET_PATH"

    echo "Starting cage compositor..."
    WAYLAND_DISPLAY="$WAYLAND_DISPLAY_NAME" cage -- "$PROJECT_DIR/target/debug/spe" --ipc &
    local cage_pid=$!
    echo "$cage_pid" > "$PIDFILE"

    echo "Waiting for IPC socket..."
    for i in {1..30}; do
        if [[ -S "$SOCKET_PATH" ]]; then
            echo "Ready (PID $cage_pid)"
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
        rm -f "$PIDFILE"
        rm -f "$SOCKET_PATH"
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
    echo "$1" | socat - UNIX-CONNECT:"$SOCKET_PATH"
}

do_capture() {
    local output="${1:-$SCREENSHOT_DIR/latest.png}"
    mkdir -p "$(dirname "$output")"
    WAYLAND_DISPLAY="$WAYLAND_DISPLAY_NAME" grim "$output"
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
