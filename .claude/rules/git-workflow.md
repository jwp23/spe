# Git Workflow Rules

## Feature Branches Only

Never commit directly to main. All changes go through feature branches and pull requests.

Branch naming: `feat/`, `fix/`, `chore/`, `docs/`, `refactor/`, `test/` + short description.

## Worktrees

For extensive changes, use git worktrees in `.worktrees/` (project-local, hidden). Always use this location — do not ask.

## Completing Work

When implementation is complete and tests pass:

1. Push feature branch: `git push -u origin <branch>`
2. Create PR: `gh pr create --title "<conventional-commit-title>" --body "<summary>"`
3. Wait for CI checks: `gh pr checks <number> --watch`
4. If checks fail: investigate, fix, commit, push, wait again. Repeat until green.
5. Report PR URL and check status to Joe.

Work is complete when the PR is open and CI is green.

## Merging PRs

When Joe says to merge or close a PR:

1. Squash merge with no body: `gh pr merge <number> --squash --body "" --delete-branch`
2. Switch to main and pull: `git checkout main && git pull`
3. Wait for CI checks on main: `gh run watch` (the merge commit triggers a new CI run)
4. If checks fail: investigate and fix on a new branch
5. Delete local feature branch: `git branch -d <branch>`
6. Remove worktree if applicable: `git worktree remove <path>`

## Session Completion

This workflow supersedes the beads-generated "Landing the Plane" section in AGENTS.md. Do not push directly to main. Always use the PR-based workflow above.
