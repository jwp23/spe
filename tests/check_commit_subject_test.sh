#!/usr/bin/env sh
# Tests for scripts/check-commit-subject.sh — the shared Conventional Commits
# validator used by the commit-msg hook and by CI's PR title check.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../scripts" && pwd)
validator="$script_dir/check-commit-subject.sh"

failures=0

accepts() {
    if "$validator" "$1" >/dev/null 2>&1; then
        printf 'ok     accepts: %s\n' "$1"
    else
        printf 'FAILED should accept: %s\n' "$1"
        failures=$((failures + 1))
    fi
}

rejects() {
    if "$validator" "$1" >/dev/null 2>&1; then
        printf 'FAILED should reject: %s\n' "$1"
        failures=$((failures + 1))
    else
        printf 'ok     rejects: %s\n' "$1"
    fi
}

accepts "feat: add font size selector to overlay toolbar"
accepts "fix: prevent crash when opening password-protected PDF"
accepts "chore: add ruff to pre-commit hooks"
accepts "docs: describe the hooks setup step"
accepts "refactor: extract overlay geometry into its own module"
accepts "test: cover the empty-overlay edge case"
accepts "perf: cache writer word-wrap line counts"
accepts "ci: pin actions to commit SHAs"
accepts "style: reformat the canvas module"
accepts "build: bump the minimum Rust version"
accepts "revert: undo the thumbnail sidebar default"
accepts "feat(canvas): size font previews by x-height"
accepts "fix(pdf/writer): escape parentheses in text strings"
accepts "feat!: drop support for the legacy overlay format"
accepts "feat(canvas)!: rename the coordinate origin"

rejects "add font size selector"
rejects "Feat: capitalised type"
rejects "feature: not a recognised type"
rejects "feat add font size selector"
rejects "feat:"
rejects "feat: "
rejects ""
rejects "wip"
rejects "Merge branch 'main' into feat/x"

if [ "$failures" -ne 0 ]; then
    printf '\n%s test(s) failed\n' "$failures" >&2
    exit 1
fi

printf '\nAll commit subject validator tests passed\n'
