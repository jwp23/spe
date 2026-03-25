# ADR-001: Haiku Subagents for Mechanical Git Operations

## Context

The main development workflow uses Opus for all git operations: creating commits, pushing branches, creating PRs, watching CI, merging PRs, and cleaning up. These operations are purely mechanical — shell commands with predictable inputs and outputs. Running them in the main Opus context wastes expensive tokens on work that requires no judgment, and blocks the main agent while waiting for CI checks.

Three areas were evaluated for subagent offloading:

1. **Creating commits** — `git add`, `git commit` with a provided message
2. **Creating PRs and watching CI** — `git push`, `gh pr create`, `gh pr checks --watch`
3. **Merging PRs and watching CI** — `gh pr merge`, `git checkout main && git pull`, `gh run watch`

## Decision

Create two Haiku-model subagents for the PR and merge workflows. Skip the commit agent — the overhead of spinning up a subagent exceeds the cost of the few tool calls needed to commit inline.

### pr-creator agent (Haiku)

- Pushes feature branch, creates PR with conventional commit title and brief body, watches CI
- Reports PR URL and CI status (pass/fail with failure details)
- Does NOT attempt to fix CI failures — reports back to the main agent
- Can run in background while the main agent continues other work

### pr-merger agent (Haiku)

- Squash merges PR with no body, checks out main, pulls, watches CI on main
- Cleans up local feature branch and worktree
- Reports merge result and CI status
- Does NOT attempt to fix CI failures — reports back to the main agent
- Can run in background

### Integration

The `finishing-a-development-branch` skill dispatches these agents instead of running git commands inline. The skill remains the orchestration layer; agents are the executors. This follows the same loose coupling pattern used with the security-reviewer agent.

### Debugging workflow on CI failure

When an agent reports CI failure, the main agent (Opus) diagnoses the issue using the systematic-debugging skill and dispatches fixes via subagent-driven-development. The CI agent never attempts repairs — it only reports.

## Trade-offs

**Chosen: Haiku subagents for PR and merge workflows**

- Token cost savings on mechanical operations
- Background execution frees the main agent during CI waits
- Clean context separation — main agent's context not polluted with git command output
- Slight latency from subagent spinup (negligible vs. CI wait time)

**Rejected: Committer agent**

- Too little work per invocation — subagent overhead exceeds inline cost
- Main agent already knows the commit message; staging and committing is 3-4 tool calls

**Rejected: Sonnet for PR creation**

- PR summaries are brief (conventional commit title + bullet points from git log)
- Haiku is sufficient for reading `git log` and producing a few bullets
- Sonnet adds cost without proportional quality gain for this task

**Rejected: Agents that self-repair on CI failure**

- Debugging requires judgment, codebase understanding, and context the agent doesn't have
- The main agent (Opus) is the right tool for diagnosis
- Keeping agents mechanical makes them predictable and trustworthy
