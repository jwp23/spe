# Run Tests

Run the project test suite.

## Steps

1. Check `docs/adr/` for the testing framework ADR. If none exists, tell the user: "No testing framework has been selected yet. Run /bootstrap to set up the project first."
2. Read the testing ADR to determine the test command.
3. Run the full test suite.
4. Report results: total tests, passed, failed, coverage (if configured).
5. If any tests fail, summarize which tests failed and offer to investigate.
