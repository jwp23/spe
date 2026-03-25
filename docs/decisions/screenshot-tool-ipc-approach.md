# Screenshot Tool: IPC via Unix Socket

Decision: Use a Unix socket IPC channel inside a headless Wayland compositor to enable Claude Code to take screenshots of the running app and drive interactions programmatically.

Rationale: The primary goal is closing the visual feedback loop — Claude Code makes UI changes, captures what the app renders, views the screenshot, and verifies correctness. A headless compositor (cage) provides zero-divergence rendering without appearing on the user's screen. Unix socket IPC translates JSON commands to Message variants, giving reliable, resolution-independent interaction without the coordinate fragility of mouse injection tools. Unix socket chosen over TCP because remote access is not needed and a smaller attack surface is preferred (YAGNI).

Alternatives considered:
- Shell script with input simulation only (wtype/ydotool): coordinate-fragile, breaks on layout changes
- Keyboard-only interaction: can't test canvas click/drag scenarios
- Headless software renderer: divergent render path, defeats the purpose of visual verification
- TCP socket: unnecessary network exposure for a local development tool
