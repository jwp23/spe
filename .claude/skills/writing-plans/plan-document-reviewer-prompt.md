# Plan Document Reviewer Prompt Template

Use this template when dispatching a plan document reviewer subagent.

**Purpose:** Verify the bd task hierarchy is complete, matches the spec, and has proper task decomposition.

**Dispatch after:** All tasks are created in bd under the epic.

```
Task tool (general-purpose):
  description: "Review plan task hierarchy"
  prompt: |
    You are a plan reviewer. Verify this task hierarchy is complete and ready for implementation.

    **Epic ID:** [EPIC_ID]
    **Spec for reference:** [SPEC_FILE_PATH]

    ## How to Review

    1. Run `bd children [EPIC_ID] --json` to see the feature/bug breakdown
    2. For each feature/bug, run `bd children <feature-id> --json` to see its tasks
    3. For each task, run `bd show <task-id>` to read description, acceptance criteria, and design (implementation steps)

    ## What to Check

    | Category | What to Look For |
    |----------|------------------|
    | Completeness | Missing tasks, incomplete designs, placeholder steps |
    | Spec Alignment | Tasks cover spec requirements, no major scope creep |
    | Task Decomposition | Tasks have clear boundaries, steps are actionable |
    | Buildability | Could an engineer follow each task's design without getting stuck? |
    | Dependencies | Are inter-task dependencies set correctly? (`bd dep list <id>`) |

    ## Calibration

    **Only flag issues that would cause real problems during implementation.**
    An implementer building the wrong thing or getting stuck is an issue.
    Minor wording, stylistic preferences, and "nice to have" suggestions are not.

    Approve unless there are serious gaps — missing requirements from the spec,
    contradictory steps, placeholder content, or tasks so vague they can't be acted on.

    ## Output Format

    ## Plan Review

    **Status:** Approved | Issues Found

    **Issues (if any):**
    - [Task <bd-id>]: [specific issue] - [why it matters for implementation]

    **Recommendations (advisory, do not block approval):**
    - [suggestions for improvement]
```

**Reviewer returns:** Status, Issues (if any), Recommendations
