# Testing Rules

## TDD Is Mandatory

Every new function or method follows red/green/refactor:

1. Write the test. Run it. Confirm it FAILS.
2. Write the minimum implementation to make it pass.
3. Refactor. Tests must stay green.

If a test passes on first run without new implementation code, the test is wrong. Fix the test.

## Test Pyramid

- **Unit tests**: Every public function and method must have tests. Test edge cases and error paths.
- **Integration tests**: Every component interaction must have tests. Mock external dependencies (system utilities, file I/O) at integration boundaries.
- **E2E tests**: Cover the core user workflow: open PDF → place text → configure font → save PDF.

## Test Organization

- Tests mirror source structure. `src/foo/bar.ext` → `tests/foo/test_bar.ext` (adjust for language conventions).
- Test filenames start with `test_` or end with `_test` per language convention.
- Each test has a descriptive name explaining what behavior is being verified.

## System Utility Tests

- Unit tests for utility wrappers must work without the utility installed (mock the subprocess call).
- Integration tests may require the utility and should be marked/tagged as such.
- Always test the error path: what happens when the utility is not installed?
