# Git Workflow Rules

## Feature Branches Only

Never commit directly to main. All changes go through feature branches and pull requests.

Branch naming: `feat/`, `fix/`, `chore/`, `docs/`, `refactor/`, `test/` + short description.

## Worktrees

For extensive changes, use git worktrees in `.worktrees/` (project-local, hidden). Always use this location — do not ask.

## Completing Work

When implementation is complete and tests pass, YOU MUST invoke the `finishing-a-development-branch` skill via the Skill tool — it dispatches the pr-creator agent for push/PR/CI; the pr-merger agent handles approved merges.

## Merge Policy

Merges use squash with no body, and delete the branch on merge.

## Session Completion

Use the PR-based workflow above; never push directly to main. Work is complete only when the PR is open and CI is green; PRs never merge without passing CI.
