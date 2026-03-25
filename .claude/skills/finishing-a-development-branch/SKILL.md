---
name: finishing-a-development-branch
description: Use when implementation is complete and all tests pass - handles the push, PR creation, and CI verification workflow
---

# Finishing a Development Branch

## Overview

Complete development work by pushing the feature branch, creating a PR, and waiting for CI to pass.

**Core principle:** Verify tests -> Push -> PR -> Wait for CI -> Clean up.

**Announce at start:** "I'm using the finishing-a-development-branch skill to complete this work."

## The Process

### Step 1: Verify Tests

**Before proceeding, verify tests pass:**

```bash
cargo test  # or project-appropriate command
```

**If tests fail:** Stop. Fix failures before proceeding.

**If tests pass:** Continue to Step 1.5.

### Step 1.5: Security Review

Run a security review of changes on this branch:

1. Determine git range:
```bash
BASE=$(git merge-base HEAD main)
```
2. Dispatch security-reviewer subagent (from `security-review/security-reviewer.md`) with the git range
3. **If Critical findings:** Stop. Fix before proceeding.
4. **If Important findings:** Fix before proceeding, or get explicit approval from Joe to defer.
5. **If Minor only or clean:** Continue to Step 2.

### Step 2: Determine Base Branch

```bash
git merge-base HEAD main 2>/dev/null || git merge-base HEAD master 2>/dev/null
```

Or ask: "This branch split from main - is that correct?"

### Step 3: Dispatch pr-creator Agent

Dispatch the `pr-creator` agent (`.claude/agents/pr-creator.md`, model: haiku) with:

- **branch**: current feature branch name
- **base**: target branch (from Step 2, usually `main`)
- **title**: conventional commit title based on the branch work

The agent pushes the branch, creates the PR with a brief summary from `git log`, and watches CI checks. It reports back with the PR URL and CI status.

**Can run in background** if you have other work to do while CI runs.

### Step 4: Handle CI Result

**If pr-creator reports PASSED:** Report PR URL and status to Joe. Done.

**If pr-creator reports FAILED:**
1. Read the failure details from the agent's report
2. Investigate the root cause (use systematic-debugging if non-obvious)
3. Fix the issue locally
4. Commit and push the fix
5. Watch CI yourself: `gh pr checks <number> --watch`
6. Repeat until all checks pass

### Step 5: Cleanup Worktree

Check if in a worktree:
```bash
git worktree list | grep $(git branch --show-current)
```

If yes:
```bash
git worktree remove <worktree-path>
```

Report: "PR ready at <URL>. All CI checks passing."

## Merging (when Joe requests)

When Joe says to merge or close a PR, dispatch the `pr-merger` agent (`.claude/agents/pr-merger.md`, model: haiku) with:

- **number**: the PR number

The agent squash merges with no body, checks out main, pulls, watches CI on the merge commit, and cleans up the local branch and worktree. It reports back with the merge SHA and CI status.

**Can run in background** if you have other work to start.

**If pr-merger reports CI FAILED on main:** Investigate and fix on a new branch.

## Quick Reference

| Step | Action |
|------|--------|
| 1. Verify | Run tests, stop if failing |
| 1.5 Security | Run security review, fix Critical/Important |
| 2. Base branch | Confirm target branch |
| 3. pr-creator | Dispatch agent: push, PR, CI watch |
| 4. CI result | If failed: debug, fix, push, re-watch |
| 5. Cleanup | Remove worktree if applicable |
| Merge | Dispatch pr-merger agent (on request) |

## Common Mistakes

**Skipping test verification**
- **Problem:** Create a failing PR
- **Fix:** Always verify tests before pushing

**Not waiting for CI checks**
- **Problem:** Report "done" before knowing CI status
- **Fix:** Always `gh pr checks --watch` and fix failures

**Pushing directly to main**
- **Problem:** Bypasses code review and CI
- **Fix:** Always use feature branches and PRs

**Including a body in squash merge commit**
- **Problem:** Clutters git log with PR details in merge commits
- **Fix:** Always use `--body ""` when merging

## Red Flags

**Never:**
- Push directly to main
- Proceed with failing tests
- Report completion before CI checks pass
- Force-push without explicit request
- Include a body in the squash merge commit

**Always:**
- Verify tests before pushing
- Create PR with a summary body
- Wait for CI checks to pass
- Fix CI failures before reporting done
- Squash merge with `--body ""` when merging
- Clean up worktree after PR is created

## Integration

**Called by:**
- **subagent-driven-development** (Step 7) - After all tasks complete
- **executing-plans** (Step 5) - After all batches complete

**Dispatches:**
- **pr-creator** agent (haiku) - Push, PR creation, CI watch (Step 3)
- **pr-merger** agent (haiku) - Squash merge, CI watch, cleanup (Merging)

**Pairs with:**
- **security-review** - Runs security audit before push (Step 1.5)
- **using-git-worktrees** - Cleans up worktree created by that skill

**Design rationale:** See `.claude/docs/adr/001-haiku-subagents-for-git-operations.md`
