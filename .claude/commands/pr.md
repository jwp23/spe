# Create Pull Request

Create a PR for the current branch.

## Steps

1. Run `git status` to confirm there are no uncommitted changes. If there are, ask the user whether to commit or stash them.
2. Run the full pre-commit check suite (lint, format, type-check, tests). If any check fails, fix it before proceeding.
3. Run `git log main..HEAD --oneline` to summarize commits on this branch.
4. Generate a PR title using conventional commit format based on the branch work.
5. Generate a PR body with:
   - Summary of changes (2-3 sentences)
   - List of commits
   - Any new decisions recorded (link to ADR/decision doc files)
   - Testing notes
6. Create PR using `gh pr create --title "..." --body "..."`.
