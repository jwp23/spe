#!/usr/bin/env bash
# Merge the weekly full-scope mutation run's shard results, compute the
# headline kill rate, and render the GitHub Actions job summary plus the
# tracking-issue body.
#
# Used by .github/workflows/mutation-weekly.yml's `summary` job. See
# docs/designs/mutation-testing.md, "Layer 2 - weekly full-scope run" for the
# capture pipeline this implements.
#
# Reads (env):
#   SHARDS_DIR     - directory holding one subdir per shard artifact, named
#                     mutants-out-<k>, each containing that shard's
#                     mutants.out contents (caught.txt, missed.txt,
#                     timeout.txt, unviable.txt, status.txt).
#   SHARD_COUNT    - total number of shards expected (0..SHARD_COUNT-1).
#   MERGED_MISSED  - path to write the deduplicated, sorted merged
#                     missed.txt to.
#   ISSUE_BODY     - path to write the tracking-issue markdown body to.
#   GITHUB_STEP_SUMMARY, GITHUB_OUTPUT, GITHUB_SERVER_URL, GITHUB_REPOSITORY,
#   GITHUB_RUN_ID  - standard GitHub Actions step environment.
#
# Writes (GITHUB_OUTPUT):
#   missed-count   - number of distinct survivors in the merged list.
#   crashed-count  - number of shards that did not run to completion.
set -euo pipefail

# Counts lines in a file whether or not it exists or ends in a trailing
# newline (a file with content but no final newline still has that content
# counted, unlike `wc -l`).
count_lines() {
    local file="$1"
    if [ -f "$file" ]; then
        grep -c '' "$file" 2>/dev/null || true
    else
        echo 0
    fi
}

caught_total=0
missed_total_raw=0
timeout_total=0
unviable_total=0
declare -a crashed_shards=()
declare -a ok_shards=()

: > "$MERGED_MISSED"

for i in $(seq 0 $((SHARD_COUNT - 1))); do
    dir="$SHARDS_DIR/mutants-out-$i"

    if [ ! -d "$dir" ]; then
        crashed_shards+=("shard $i: artifact missing (shard never uploaded results)")
        continue
    fi

    status_file="$dir/status.txt"
    if [ ! -f "$status_file" ]; then
        crashed_shards+=("shard $i: status.txt missing from artifact")
    else
        exit_code=$(grep '^exit_code=' "$status_file" | cut -d= -f2 || echo "unknown")
        case "$exit_code" in
            # 0 (Success), 2 (FoundProblems), 3 (Timeout): the run reached
            # completion and its result files reflect the whole shard. Every
            # other cargo-mutants exit code - 1/5/6 (usage/filter errors), 4
            # (baseline already failing), 70 (internal error), or "unknown"
            # (the run step never wrote its output) - means this shard's
            # data is partial or absent.
            0|2|3)
                ok_shards+=("$i")
                ;;
            *)
                crashed_shards+=("shard $i: cargo-mutants exit code $exit_code")
                ;;
        esac
    fi

    caught_total=$((caught_total + $(count_lines "$dir/caught.txt")))
    missed_total_raw=$((missed_total_raw + $(count_lines "$dir/missed.txt")))
    timeout_total=$((timeout_total + $(count_lines "$dir/timeout.txt")))
    unviable_total=$((unviable_total + $(count_lines "$dir/unviable.txt")))

    if [ -s "$dir/missed.txt" ]; then
        cat "$dir/missed.txt" >> "$MERGED_MISSED"
    fi
done

sort -u -o "$MERGED_MISSED" "$MERGED_MISSED"
missed_total=$(count_lines "$MERGED_MISSED")
crashed_count=${#crashed_shards[@]}

# Kill rate over viable mutants (caught + missed), matching the convention
# the 2026-08-14 spike used: unviable mutants (dead code the compiler itself
# rejects) and timeouts are excluded from the denominator, since neither is
# a real pass/fail signal on the test suite.
if [ $((caught_total + missed_total_raw)) -gt 0 ]; then
    kill_rate="$(awk -v c="$caught_total" -v m="$missed_total_raw" 'BEGIN { printf "%.1f", (c / (c + m)) * 100 }')%"
else
    kill_rate="n/a"
fi

run_url="${GITHUB_SERVER_URL}/${GITHUB_REPOSITORY}/actions/runs/${GITHUB_RUN_ID}"

{
    echo "## Weekly mutation testing"
    echo
    echo "Run: $run_url"
    echo
    echo "| Metric | Count |"
    echo "|---|---|"
    echo "| Shards completed | ${#ok_shards[@]} / $SHARD_COUNT |"
    echo "| Caught | $caught_total |"
    echo "| Missed (survivors) | $missed_total |"
    echo "| Timeouts | $timeout_total |"
    echo "| Unviable | $unviable_total |"
    echo "| Kill rate (caught / viable) | ${kill_rate} |"
    echo
    if [ "$crashed_count" -gt 0 ]; then
        echo "**Warning: $crashed_count shard(s) did not run to completion. The counts above are incomplete.**"
        echo
        for reason in "${crashed_shards[@]}"; do
            echo "- $reason"
        done
        echo
    fi
} >> "$GITHUB_STEP_SUMMARY"

{
    echo "Run: $run_url"
    echo
    echo "Kill rate: ${kill_rate} ($caught_total caught / $((caught_total + missed_total_raw)) viable)"
    echo
    if [ "$crashed_count" -gt 0 ]; then
        echo "> **Warning:** $crashed_count of $SHARD_COUNT shard(s) did not run to completion this run:"
        echo ">"
        for reason in "${crashed_shards[@]}"; do
            echo "> - $reason"
        done
        echo ">"
        echo "> This survivor list may be incomplete."
        echo
    fi
    echo "## Survivors ($missed_total)"
    echo
    if [ "$missed_total" -gt 0 ]; then
        while IFS= read -r line; do
            [ -z "$line" ] && continue
            echo "- \`$line\`"
        done < "$MERGED_MISSED"
    fi
} > "$ISSUE_BODY"

{
    echo "missed-count=$missed_total"
    echo "crashed-count=$crashed_count"
} >> "$GITHUB_OUTPUT"
