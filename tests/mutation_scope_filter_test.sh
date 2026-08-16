#!/usr/bin/env bash
# Tests for scripts/mutation-scope-filter.sh — the pure stdin -> stdout
# filter mutation-pr-gate.yml's scope guard uses to reduce a changed-file
# list to the subset cargo-mutants would mutate.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../scripts" && pwd)
fixtures_dir=$(CDPATH= cd -- "$(dirname -- "$0")/fixtures/mutation-scope-filter" && pwd)
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
filter="$script_dir/mutation-scope-filter.sh"
real_config="$repo_root/.cargo/mutants.toml"

failures=0

# expect_output DESC INPUT CONFIG EXPECTED_STDOUT
expect_output() {
    desc="$1" input="$2" config="$3" expected="$4"
    actual=$(printf '%s' "$input" | "$filter" "$config")
    if [ "$actual" = "$expected" ]; then
        printf 'ok     %s\n' "$desc"
    else
        printf 'FAILED %s\n' "$desc"
        printf '       expected: %s\n' "$expected"
        printf '       actual:   %s\n' "$actual"
        failures=$((failures + 1))
    fi
}

# expect_failure DESC INPUT CONFIG
expect_failure() {
    desc="$1" input="$2" config="$3"
    if printf '%s' "$input" | "$filter" "$config" >/dev/null 2>&1; then
        printf 'FAILED %s (expected non-zero exit)\n' "$desc"
        failures=$((failures + 1))
    else
        printf 'ok     %s\n' "$desc"
    fi
}

# --- against the real, checked-in .cargo/mutants.toml ---

expect_output "drops a non-.rs file" \
    "README.md" "$real_config" ""

expect_output "drops an exact-path exclusion (src/main.rs)" \
    "src/main.rs" "$real_config" ""

expect_output "drops a file directly under a dir/** exclusion (src/app/**)" \
    "src/app/foo.rs" "$real_config" ""

expect_output "drops a file nested deeper under a dir/** exclusion (src/app/**)" \
    "src/app/deep/nested/foo.rs" "$real_config" ""

expect_output "drops a file directly under a dir/** exclusion (src/ui/**)" \
    "src/ui/widget.rs" "$real_config" ""

expect_output "drops a file nested deeper under a dir/** exclusion (src/ui/**)" \
    "src/ui/deep/nested/widget.rs" "$real_config" ""

expect_output "keeps a path that shares a prefix with an excluded dir but is not under it" \
    "src/appfoo.rs" "$real_config" "src/appfoo.rs"

expect_output "keeps an in-scope file" \
    "src/pdf/writer.rs" "$real_config" "src/pdf/writer.rs"

expect_output "empty input yields empty output" \
    "" "$real_config" ""

expect_output "mixed input keeps only the in-scope subset" \
    "$(printf 'README.md\nsrc/main.rs\nsrc/appfoo.rs\nsrc/pdf/writer.rs\nsrc/app/foo.rs\n')" \
    "$real_config" \
    "$(printf 'src/appfoo.rs\nsrc/pdf/writer.rs')"

# --- error handling ---

expect_failure "a config that fails to parse (no exclude_globs array)" \
    "src/foo.rs" "$fixtures_dir/unparsable.toml"

expect_failure "a glob shape the matcher cannot faithfully honour (src/*.rs)" \
    "src/foo.rs" "$fixtures_dir/unsupported-glob.toml"

if [ "$failures" -ne 0 ]; then
    printf '\n%s test(s) failed\n' "$failures" >&2
    exit 1
fi

printf '\nAll mutation-scope filter tests passed\n'
