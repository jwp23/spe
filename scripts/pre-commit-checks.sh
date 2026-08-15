#!/usr/bin/env sh
# Project quality checks run before every commit.
#
# Invoked by .githooks/pre-commit (contributors) and by .beads/hooks/pre-commit
# (maintainers running the beads issue tracker). Both entry points call this one
# script so the checks cannot drift apart.
#
# Checks are ordered fastest and most critical first. Tools that are optional
# locally warn when missing; CI enforces them for every pull request.

if command -v betterleaks >/dev/null 2>&1; then
    if ! betterleaks git --pre-commit --staged --redact; then
        echo >&2 "pre-commit: betterleaks detected secrets in staged changes."
        exit 1
    fi
else
    echo >&2 "pre-commit: WARNING: betterleaks not installed — staged changes were not scanned for secrets. Install from https://github.com/betterleaks/betterleaks"
fi

if ! cargo fmt --check 2>/dev/null; then
    echo >&2 "pre-commit: cargo fmt check failed. Run 'cargo fmt' to fix."
    exit 1
fi

if ! cargo clippy --all-targets -- -D warnings 2>/dev/null; then
    echo >&2 "pre-commit: clippy found warnings."
    exit 1
fi

if command -v cargo-audit >/dev/null 2>&1; then
    if ! cargo audit 2>/dev/null; then
        echo >&2 "pre-commit: cargo audit found vulnerabilities."
        exit 1
    fi
else
    echo >&2 "pre-commit: WARNING: cargo-audit not installed. Run 'cargo install cargo-audit' for dependency vulnerability scanning."
fi

if command -v cargo-deny >/dev/null 2>&1; then
    if ! cargo deny check 2>/dev/null; then
        echo >&2 "pre-commit: cargo deny check failed."
        exit 1
    fi
else
    echo >&2 "pre-commit: WARNING: cargo-deny not installed. Run 'cargo install cargo-deny' for license and supply-chain checks."
fi

if ! cargo test 2>/dev/null; then
    echo >&2 "pre-commit: tests failed."
    exit 1
fi

if ! cargo test -- --ignored 2>/dev/null; then
    echo >&2 "pre-commit: integration tests failed."
    exit 1
fi
