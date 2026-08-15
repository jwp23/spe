#!/usr/bin/env sh
# Point git at the repository's committed hooks.
#
# Maintainers running the beads issue tracker keep core.hooksPath set to
# .beads/hooks, which runs the same checks plus beads integration. This script
# is for everyone else.
set -eu

cd "$(git rev-parse --show-toplevel)"

current=$(git config --get core.hooksPath || true)
case "$current" in
    *.beads/hooks)
        echo "core.hooksPath already points at $current (beads-managed) — leaving it alone."
        echo "Those hooks run the same project checks."
        exit 0
        ;;
esac

git config core.hooksPath .githooks
chmod +x .githooks/* scripts/*.sh

echo "Hooks enabled. core.hooksPath is now .githooks"
echo "pre-commit runs formatting, lint, and tests; commit-msg checks the commit convention."
