#!/usr/bin/env bash
# Tests for scripts/mutation-weekly-summary.sh — the weekly full-scope
# mutation run's shard merge, kill-rate computation, and summary/issue-body
# rendering (mutation-weekly.yml's `summary` job).
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../scripts" && pwd)
script="$script_dir/mutation-weekly-summary.sh"

failures=0
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

fail() {
    printf 'FAILED %s\n' "$1"
    failures=$((failures + 1))
}

ok() {
    printf 'ok     %s\n' "$1"
}

# assert_eq DESC ACTUAL EXPECTED
assert_eq() {
    if [ "$2" = "$3" ]; then
        ok "$1"
    else
        fail "$1 (expected [$3], got [$2])"
    fi
}

# assert_contains DESC HAYSTACK_FILE NEEDLE
assert_contains() {
    if grep -qF -- "$3" "$2"; then
        ok "$1"
    else
        fail "$1 (did not find [$3] in $2)"
    fi
}

# write_shard DIR SHARD_INDEX EXIT_CODE CAUGHT MISSED TIMEOUT UNVIABLE
# Writes a shard's mutants.out contents. Any of CAUGHT/MISSED/TIMEOUT/UNVIABLE
# may be "-" to mean "no such file" rather than an empty one.
write_shard() {
    local dir="$1" shard="$2" exit_code="$3" caught="$4" missed="$5" timeout="$6" unviable="$7"
    local shard_dir="$dir/mutants-out-$shard"
    mkdir -p "$shard_dir"
    if [ "$exit_code" != "-" ]; then
        printf 'shard=%s\nexit_code=%s\n' "$shard" "$exit_code" > "$shard_dir/status.txt"
    fi
    # write_field FILE CONTENT — trailing newline on non-empty content,
    # matching how cargo-mutants itself writes these files. Required so
    # concatenating two shards' missed.txt files below doesn't join the
    # last line of one with the first line of the next.
    write_field() {
        if [ -n "$2" ]; then
            printf '%s\n' "$2" > "$1"
        else
            : > "$1"
        fi
    }
    # `if`, not `[ ... ] && write_field ...`: under `set -e`, a false test
    # as the last statement of a function call would make the whole
    # statement (and the script) exit right here when the condition is
    # false — the exact pitfall mutation-weekly-summary.sh's own comments
    # warn about.
    if [ "$caught" != "-" ]; then write_field "$shard_dir/caught.txt" "$caught"; fi
    if [ "$missed" != "-" ]; then write_field "$shard_dir/missed.txt" "$missed"; fi
    if [ "$timeout" != "-" ]; then write_field "$shard_dir/timeout.txt" "$timeout"; fi
    if [ "$unviable" != "-" ]; then write_field "$shard_dir/unviable.txt" "$unviable"; fi
}

# run_summary CASE_DIR SHARD_COUNT
# Runs the script against $CASE_DIR/shards and captures its outputs under
# $CASE_DIR: step-summary, github-output, merged-missed.txt, issue-body.md.
# Returns the script's own exit code.
run_summary() {
    local case_dir="$1" shard_count="$2"
    : > "$case_dir/step-summary"
    : > "$case_dir/github-output"
    SHARDS_DIR="$case_dir/shards" \
        SHARD_COUNT="$shard_count" \
        MERGED_MISSED="$case_dir/merged-missed.txt" \
        ISSUE_BODY="$case_dir/issue-body.md" \
        GITHUB_STEP_SUMMARY="$case_dir/step-summary" \
        GITHUB_OUTPUT="$case_dir/github-output" \
        GITHUB_SERVER_URL="https://github.example" \
        GITHUB_REPOSITORY="acme/widgets" \
        GITHUB_RUN_ID="12345" \
        "$script"
}

output_value() {
    grep "^$2=" "$1/github-output" | tail -n1 | cut -d= -f2-
}

# --- case: all clean (single shard, no survivors) ---
case_dir="$work/all-clean"
mkdir -p "$case_dir"
write_shard "$case_dir/shards" 0 0 "$(printf 'a\nb\nc')" "" "" ""
if run_summary "$case_dir" 1 > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    ok "all-clean: script exits 0"
else
    fail "all-clean: script exited $? (expected 0)"
fi
assert_eq "all-clean: missed-count is 0" "$(output_value "$case_dir" missed-count)" "0"
assert_eq "all-clean: crashed-count is 0" "$(output_value "$case_dir" crashed-count)" "0"
assert_contains "all-clean: kill rate is 100.0%" "$case_dir/step-summary" "100.0%"
assert_contains "all-clean: issue body reports 0 survivors" "$case_dir/issue-body.md" "Survivors (0)"

# --- case: survivors present (kill-rate computation, exit 0) ---
case_dir="$work/survivors"
mkdir -p "$case_dir"
write_shard "$case_dir/shards" 0 2 \
    "$(printf 'a\nb\nc')" \
    "$(printf 'src/x.rs:1:1: replace + with - in foo\nsrc/y.rs:2:2: replace * with / in bar')" \
    "" ""
if run_summary "$case_dir" 1 > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    ok "survivors: script exits 0 even with survivors (Layer 2 captures, never fails on a survivor)"
else
    fail "survivors: script exited $? (expected 0)"
fi
assert_eq "survivors: missed-count is 2" "$(output_value "$case_dir" missed-count)" "2"
assert_eq "survivors: crashed-count is 0" "$(output_value "$case_dir" crashed-count)" "0"
# 3 caught / 5 viable = 60.0%
assert_contains "survivors: kill rate is 60.0%" "$case_dir/step-summary" "60.0%"
assert_contains "survivors: issue body lists a survivor" "$case_dir/issue-body.md" "src/x.rs:1:1: replace + with - in foo"

# --- case: multi-shard merge (three shards, sorted, disjoint mutant sets —
# --shard k/n always partitions the mutant space, so no two shards ever
# report the same survivor in real operation) ---
case_dir="$work/multi-shard"
mkdir -p "$case_dir"
write_shard "$case_dir/shards" 0 0 "$(printf 'a\nb')" "$(printf 'm3')" "" ""
write_shard "$case_dir/shards" 1 0 "$(printf 'c')" "$(printf 'm1')" "" ""
write_shard "$case_dir/shards" 2 0 "$(printf 'd\ne')" "" "" ""
if run_summary "$case_dir" 3 > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    ok "multi-shard: script exits 0"
else
    fail "multi-shard: script exited $? (expected 0)"
fi
# caught: 2 + 1 + 2 = 5; missed: m1, m3 = 2
assert_eq "multi-shard: missed-count sums across all three shards" "$(output_value "$case_dir" missed-count)" "2"
assert_eq "multi-shard: merged-missed.txt is sorted across shards" \
    "$(cat "$case_dir/merged-missed.txt")" "$(printf 'm1\nm3')"
# 5 caught / (5 + 2) viable = 71.4%
assert_contains "multi-shard: kill rate is 71.4%" "$case_dir/step-summary" "71.4%"

# --- case: duplicate missed entries across shards are deduplicated for the
# reported survivor count and list, but NOT for the kill-rate denominator,
# which sums each shard's raw missed count. This is the script's actual,
# documented behaviour (real --shard k/n runs partition the mutant space, so
# this scenario is not expected in practice; it exercises the distinction
# directly rather than relying on it). ---
case_dir="$work/duplicate-missed"
mkdir -p "$case_dir"
write_shard "$case_dir/shards" 0 0 "$(printf 'a\nb\nc')" "$(printf 'm1')" "" ""
write_shard "$case_dir/shards" 1 0 "" "$(printf 'm1')" "" ""
if run_summary "$case_dir" 2 > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    ok "duplicate-missed: script exits 0"
else
    fail "duplicate-missed: script exited $? (expected 0)"
fi
assert_eq "duplicate-missed: reported missed-count is deduplicated to 1" "$(output_value "$case_dir" missed-count)" "1"
# kill rate denominator uses the raw (non-deduplicated) missed count: 3 caught / (3 + 2) = 60.0%
assert_contains "duplicate-missed: kill rate uses the raw missed count, not the deduplicated one" \
    "$case_dir/step-summary" "60.0%"

# --- case: crashed shard vs. a shard that ran and found nothing ---
case_dir="$work/crashed-vs-clean"
mkdir -p "$case_dir"
# shard 0: ran to completion (exit 0), found nothing.
write_shard "$case_dir/shards" 0 0 "$(printf 'a')" "" "" ""
# shard 1: ran but crashed (a usage/filter error, exit code 1).
write_shard "$case_dir/shards" 1 1 "-" "-" "-" "-"
if run_summary "$case_dir" 2 > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    ok "crashed-vs-clean: script exits 0"
else
    fail "crashed-vs-clean: script exited $? (expected 0)"
fi
assert_eq "crashed-vs-clean: crashed-count is 1 (only shard 1)" "$(output_value "$case_dir" crashed-count)" "1"
assert_contains "crashed-vs-clean: warns about shard 1's exit code" "$case_dir/step-summary" "shard 1: cargo-mutants exit code 1"
assert_contains "crashed-vs-clean: shards-completed reports 1 / 2" "$case_dir/step-summary" "1 / 2"

# --- case: missing artifact entirely (shard directory absent) ---
case_dir="$work/missing-artifact"
mkdir -p "$case_dir/shards"
write_shard "$case_dir/shards" 0 0 "$(printf 'a')" "" "" ""
# shard 1's directory is never created at all.
if run_summary "$case_dir" 2 > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    ok "missing-artifact: script exits 0"
else
    fail "missing-artifact: script exited $? (expected 0)"
fi
assert_eq "missing-artifact: crashed-count is 1" "$(output_value "$case_dir" crashed-count)" "1"
assert_contains "missing-artifact: warns artifact missing" "$case_dir/step-summary" "shard 1: artifact missing (shard never uploaded results)"

# --- case: SHARD_COUNT is not a positive integer -> fails loudly ---
case_dir="$work/bad-shard-count"
mkdir -p "$case_dir/shards"
if run_summary "$case_dir" "not-a-number" > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    fail "bad-shard-count: script should have exited non-zero"
else
    ok "bad-shard-count: script exits non-zero"
fi
assert_contains "bad-shard-count: error message names the bad value" "$case_dir/stderr" "SHARD_COUNT is not a positive integer: 'not-a-number'"

# --- case: no caught and no missed at all -> kill rate is n/a ---
case_dir="$work/no-viable"
mkdir -p "$case_dir"
write_shard "$case_dir/shards" 0 0 "" "" "$(printf 't1')" "$(printf 'u1\nu2')"
if run_summary "$case_dir" 1 > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    ok "no-viable: script exits 0"
else
    fail "no-viable: script exited $? (expected 0)"
fi
assert_contains "no-viable: kill rate is n/a with zero viable mutants" "$case_dir/step-summary" "| n/a |"

if [ "$failures" -ne 0 ]; then
    printf '\n%s test(s) failed\n' "$failures" >&2
    exit 1
fi

printf '\nAll mutation-weekly-summary tests passed\n'
