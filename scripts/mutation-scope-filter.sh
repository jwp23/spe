#!/usr/bin/env bash
# Reduce a changed-file list to the subset cargo-mutants would mutate.
#
# A pure stdin -> stdout filter used by mutation-pr-gate.yml's scope guard
# and covered by tests/mutation_scope_filter_test.sh. Reads a newline-
# separated list of changed files on stdin and writes the in-scope subset to
# stdout: non-.rs files are dropped, files matching .cargo/mutants.toml's
# exclude_globs are dropped, everything else is kept.
#
# Deliberately reads exclude_globs out of .cargo/mutants.toml rather than
# hardcoding a second copy: the design defines enforcement scope once,
# negatively, in that file, and every layer of the mutation-testing pipeline
# reads it (docs/designs/mutation-testing.md, "Enforcement scope").
#
# A genuinely empty input, or an input with no .rs files, short-circuits
# before the config is even read: an out-of-scope PR must never be blocked
# by an unrelated config problem, only a PR that actually has .rs files to
# judge.
#
# Usage: <changed-files, one per line> | mutation-scope-filter.sh [path-to-mutants.toml]
#   (default: .cargo/mutants.toml, relative to the current directory)
set -euo pipefail

config="${1:-.cargo/mutants.toml}"

rs_files=""
while IFS= read -r f || [ -n "$f" ]; do
    [ -z "$f" ] && continue
    case "$f" in
        *.rs) rs_files="${rs_files}${f}"$'\n' ;;
    esac
done

if [ -z "$rs_files" ]; then
    exit 0
fi

# This is a purpose-built line scanner, not a TOML parser. It assumes
# exclude_globs is a bracketed array with one double-quoted glob per line,
# e.g.:
#   exclude_globs = [
#       "src/main.rs",
#       "src/app/**",
#   ]
# If that shape changes (single line, single quotes, a different key order),
# this scanner needs updating alongside it.
exclude_globs=$(awk '/^exclude_globs[[:space:]]*=[[:space:]]*\[/{flag=1; next} /^\]/{flag=0} flag' "$config" | sed -E 's/^[[:space:]]*"(.*)",?[[:space:]]*$/\1/')

if [ -z "$exclude_globs" ]; then
    # Fail loudly rather than guessing. Treating a parse failure as "nothing
    # excluded" would silently run mutation testing over the whole crate
    # (very slow, but not wrong); treating it as "everything excluded"
    # would silently stop enforcing the gate entirely. The second failure
    # mode is the dangerous one — it can't be told apart from a
    # legitimately out-of-scope PR — so this refuses to guess in either
    # direction and fails instead.
    echo "::error::Failed to parse exclude_globs from $config — refusing to guess mutation-testing scope." >&2
    exit 1
fi

# Only two glob shapes are matched faithfully against globset semantics (the
# matcher cargo-mutants itself uses), where a single `*` does not cross `/`:
# an exact path, or a `dir/**` recursive-directory prefix. Anything else
# (`src/*.rs`, a mid-path `**`, multiple wildcards) is rejected loudly
# rather than matched approximately — bash's `case` `*` crosses `/`, unlike
# globset's, so an approximate match can silently over- or under-exclude a
# pattern this filter was never taught to handle.
match_glob() {
    local f="$1" pattern="$2" prefix
    case "$pattern" in
        */'**')
            prefix="${pattern%/**}"
            case "$prefix" in
                *'*'*) return 2 ;;
            esac
            case "$f" in
                "$prefix"/*) return 0 ;;
                *) return 1 ;;
            esac
            ;;
        *'*'*)
            return 2
            ;;
        *)
            [ "$f" = "$pattern" ] && return 0 || return 1
            ;;
    esac
}

is_excluded() {
    local f="$1" pattern rc
    while IFS= read -r pattern; do
        [ -z "$pattern" ] && continue
        match_glob "$f" "$pattern"
        rc=$?
        if [ "$rc" -eq 0 ]; then
            return 0
        elif [ "$rc" -eq 2 ]; then
            echo "::error::exclude_globs entry '$pattern' is not a glob shape this filter can faithfully match (supported: an exact path, or 'dir/**'). Update the filter's matcher before adding this pattern." >&2
            exit 1
        fi
    done <<< "$exclude_globs"
    return 1
}

while IFS= read -r f; do
    [ -z "$f" ] && continue
    if ! is_excluded "$f"; then
        printf '%s\n' "$f"
    fi
done <<< "$rs_files"
