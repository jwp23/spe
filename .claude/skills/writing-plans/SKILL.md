---
name: writing-plans
description: Use when you have a spec or requirements for a multi-step task, before touching code
---

# Writing Plans

## Overview

Decompose features into detailed, bite-sized implementation tasks tracked in bd (beads). Each task contains everything an engineer needs: which files to touch, code, testing steps, how to verify. DRY. YAGNI. TDD. Frequent commits.

Assume the implementing engineer is skilled but knows almost nothing about our toolset or problem domain. Assume they don't know good test design very well.

**Announce at start:** "I'm using the writing-plans skill to create the implementation plan."

**Context:** This should be run in a dedicated worktree (created by brainstorming skill).

## Input

This skill receives an **epic ID** from the brainstorming skill. The epic already has feature/bug children representing components or subsystems.

If starting without an epic (e.g., ad-hoc planning), create one first:
```bash
bd create "<project name>" -t epic --description="<summary>" --json
```

Review the existing hierarchy before creating tasks:
```bash
bd children <epic-id> --json
```

## Scope Check

If the epic covers multiple independent subsystems, it should have been broken into sub-project epics during brainstorming. If it wasn't, suggest breaking this into separate epics — one per subsystem. Each epic should produce working, testable software on its own.

## File Structure

Before defining tasks, map out which files will be created or modified and what each one is responsible for. This is where decomposition decisions get locked in.

- Design units with clear boundaries and well-defined interfaces. Each file should have one clear responsibility.
- You reason best about code you can hold in context at once, and your edits are more reliable when files are focused. Prefer smaller, focused files over large ones that do too much.
- Files that change together should live together. Split by responsibility, not by technical layer.
- In existing codebases, follow established patterns. If the codebase uses large files, don't unilaterally restructure - but if a file you're modifying has grown unwieldy, including a split in the plan is reasonable.

This structure informs the task decomposition. Each task should produce self-contained changes that make sense independently.

## Bite-Sized Task Granularity

**Each step is one action (2-5 minutes):**
- "Write the failing test" - step
- "Run it to make sure it fails" - step
- "Implement the minimal code to make the test pass" - step
- "Run the tests and make sure they pass" - step
- "Commit" - step

## Creating Tasks in bd

For each feature/bug child of the epic, create task issues as children. Use `--description` for summary and acceptance criteria, `--design` for detailed TDD steps with code.

```bash
bd create "<task title>" -t task \
  --parent <feature-id> \
  --description="<summary>. Acceptance: <criteria>" \
  --design="$(cat <<'EOF'
## Files
- Create: `exact/path/to/file.ext`
- Modify: `exact/path/to/existing.ext:123-145`
- Test: `tests/exact/path/to/test.ext`

## Steps

### Step 1: Write the failing test

```lang
def test_specific_behavior():
    result = function(input)
    assert result == expected
```

### Step 2: Run test to verify it fails

Run: `test-command tests/path/test.ext::test_name -v`
Expected: FAIL with "function not defined"

### Step 3: Write minimal implementation

```lang
def function(input):
    return expected
```

### Step 4: Run test to verify it passes

Run: `test-command tests/path/test.ext::test_name -v`
Expected: PASS

### Step 5: Commit

```bash
git add tests/path/test.ext src/path/file.ext
git commit -m "feat: add specific feature"
```
EOF
)" --json
```

### Task Dependencies

Set inter-task dependencies when one task must complete before another can start:

```bash
bd dep <blocker-task-id> --blocks <blocked-task-id>
```

## Remember
- Exact file paths always
- Complete code in task designs (not "add validation")
- Exact commands with expected output
- Reference relevant skills with @ syntax
- DRY, YAGNI, TDD, frequent commits

## Plan Review Loop

After creating all tasks:

1. Dispatch a single plan-document-reviewer subagent (see plan-document-reviewer-prompt.md) with the epic ID and spec path. The reviewer examines the bd hierarchy — not a plan file.
2. If Issues Found: fix the tasks (`bd update <id> --design="..."` or `bd delete <id>` and recreate), re-dispatch reviewer
3. If Approved: proceed to execution handoff

**Review loop guidance:**
- Same agent that created the tasks fixes them (preserves context)
- If loop exceeds 3 iterations, surface to human for guidance
- Reviewers are advisory — explain disagreements if you believe feedback is incorrect

## Execution Handoff

After all tasks are created and reviewed, offer execution choice:

**"Tasks created under epic `<epic-id>`. Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?"**

**If Subagent-Driven chosen:**
- **REQUIRED SUB-SKILL:** Use subagent-driven-development
- Fresh subagent per task + two-stage review

**If Inline Execution chosen:**
- **REQUIRED SUB-SKILL:** Use executing-plans
- Batch execution with checkpoints for review
