#!/usr/bin/env sh
# Enforce the project's commit message convention: one Conventional Commits
# line, no body, no footer.
#
# Invoked by .githooks/commit-msg (contributors) and .beads/hooks/commit-msg
# (maintainers running the beads issue tracker).
#
# Usage: check-commit-msg.sh <path-to-commit-message-file>
set -eu

msg_file=$1
root=$(git rev-parse --show-toplevel)

# Drop git's comment lines, then any leading blank lines.
content=$(sed -e '/^#/d' "$msg_file" | sed -e '/./,$!d')
subject=$(printf '%s\n' "$content" | head -1)

# Git generates these itself during merge, revert, and interactive rebase.
case "$subject" in
    Merge\ *|Revert\ *|fixup!\ *|squash!\ *) exit 0 ;;
esac

"$root/scripts/check-commit-subject.sh" "$subject" || exit 1

body=$(printf '%s\n' "$content" | tail -n +2 | sed -e '/^[[:space:]]*$/d')
if [ -n "$body" ]; then
    cat >&2 <<EOF
Commit message has a body. This project uses single-line commit messages only:

$body

Put the detail in the pull request description instead.
EOF
    exit 1
fi
