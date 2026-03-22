---
name: finishing-a-development-branch
description: Use when implementation is complete and all tests pass - pushes feature branch, creates PR, waits for CI checks to pass, and cleans up worktree
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

**If tests pass:** Continue to Step 2.

### Step 2: Determine Base Branch

```bash
git merge-base HEAD main 2>/dev/null || git merge-base HEAD master 2>/dev/null
```

Or ask: "This branch split from main - is that correct?"

### Step 3: Push and Create PR

```bash
# Push feature branch
git push -u origin <feature-branch>

# Create PR with summary body
gh pr create --title "<conventional-commit-title>" --body "$(cat <<'EOF'
## Summary
<2-3 bullets of what changed>

## Test Plan
<verification steps>
EOF
)"
```

The PR title should follow conventional commit format based on the branch work.

### Step 4: Wait for CI Checks

```bash
gh pr checks <number> --watch
```

**If checks fail:**
1. Investigate the failure output
2. Fix the issue locally
3. Commit and push the fix
4. Wait again: `gh pr checks <number> --watch`
5. Repeat until all checks pass

**If checks pass:** Report PR URL and status.

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

When Joe says to merge or close a PR, squash merge with no body:

```bash
gh pr merge <number> --squash --body "" --delete-branch
git checkout main && git pull
git branch -d <feature-branch>
```

Clean up worktree if applicable.

## Quick Reference

| Step | Action |
|------|--------|
| 1. Verify | Run tests, stop if failing |
| 2. Base branch | Confirm target branch |
| 3. Push + PR | Push branch, create PR with summary |
| 4. CI | Watch checks, fix failures |
| 5. Cleanup | Remove worktree if applicable |
| Merge | Squash merge, no body (on request) |

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

**Pairs with:**
- **using-git-worktrees** - Cleans up worktree created by that skill
