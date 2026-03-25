# Testing Rules

## Test Pyramid

- **Unit tests**: Every public function and method must have tests. Test edge cases and error paths.
- **Integration tests**: Every component interaction must have tests. Mock external dependencies (system utilities, file I/O) at integration boundaries.
- **E2E tests**: Cover the core user workflow: open PDF → place text → configure font → save PDF.

## Test Organization

- Tests mirror source structure. `src/foo/bar.ext` → `tests/foo/test_bar.ext` (adjust for language conventions).
- Test filenames start with `test_` or end with `_test` per language convention.
- Each test has a descriptive name explaining what behavior is being verified.

## Visual Verification

When changes affect UI rendering (canvas drawing, overlay positioning, layout, visual states), verify visually using the screenshot tool before claiming completion.

**Prerequisites:** `sudo pacman -S cage grim socat`

**Workflow:**
```bash
# Start the headless harness
scripts/screenshot.sh start

# Load a test PDF and wait for rendering
scripts/screenshot.sh send '{"cmd": "open", "path": "tests/fixtures/single-page.pdf"}'
scripts/screenshot.sh send '{"cmd": "wait_ready"}'

# Set up the scenario you need to verify (example: place an overlay)
scripts/screenshot.sh send '{"cmd": "click", "page": 1, "x": 100, "y": 700}'
scripts/screenshot.sh send '{"cmd": "type", "text": "Hello world"}'

# Capture and inspect the screenshot
scripts/screenshot.sh capture screenshots/verify.png
# Then use the Read tool to view screenshots/verify.png

# Tear down when done
scripts/screenshot.sh stop
```

**When required:** Any change to `src/ui/canvas/`, overlay drawing, coordinate math, or visual state transitions.

**See also:** `docs/screenshot-tool.md` for the full IPC command reference.

## System Utility Tests

- Unit tests for utility wrappers must work without the utility installed (mock the subprocess call).
- Integration tests may require the utility and should be marked/tagged as such.
- Always test the error path: what happens when the utility is not installed?
