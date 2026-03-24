---
name: readme-sync
description: Use when completing code changes that affect user workflows, CLI flags, prerequisites, or project structure. Use proactively after feature implementation before session close.
context: fork
agent: general-purpose
model: sonnet
---

# README Sync

## Overview

Keep README.md current with code changes. Apply elements-of-style principles for clear, concise prose that gets users running in 5 minutes.

## When to Use

**After completing code changes, ask:**
1. Does this change what users need to install?
2. Does this change how users run commands?
3. Does this change the project structure users see?
4. Does this add a new user-facing feature?

**If YES to any:** Update README or docs.

**If NO to all:** Skip. Changelog is sufficient.

## Decision Flow

```
Change affects user workflow?
├── NO → Update changelog only, skip README
└── YES → Is it complex (50+ lines needed)?
    ├── NO → Update README directly
    └── YES → Create docs/<topic>.md, add README pointer
```

## README Update Rules

**Keep README under 400 lines.** Split to `docs/` when approaching limit.

**5-minute rule:** New user should be running tests within 5 minutes of reading Quick Start.

**Apply elements-of-style:**
- Use active voice ("Run the test" not "The test can be run")
- Omit needless words ("Use" not "You can use")
- Be specific ("v1.4.0+" not "recent version")
- Put commands in code blocks

## What Needs README Updates

| Change Type | README Section | Example |
|------------|----------------|---------|
| New prerequisite | Prerequisites table | Added Docker requirement |
| Version bump | Quick Start + Prerequisites | k6 v1.3.0 → v1.4.0 |
| New CLI flag | Relevant section | Added --verbose flag |
| New test type | Test Types or new section | Added CHS tests |
| Structure change | Code Organization | New directory added |

## What Does NOT Need README Updates

- Internal refactoring (no user-facing change)
- Bug fixes (unless workaround was documented)
- Performance improvements
- Code style changes
- Features in subsystems not covered by README
- Enhancements to self-documenting scripts (with `--help`)

**Key rule:** Do not add isolated details about features the README doesn't cover. Either document the whole subsystem or skip it. Adding one flag to an undocumented script creates confusion.

## When to Split to docs/

**Create `docs/<topic>.md` when:**
- Feature needs 50+ lines of documentation
- Feature has its own CLI with multiple flags
- Feature requires external tools (kubectl, Docker)
- Feature serves a subset of users

**Keep in README:**
- Link to the doc file
- 10-15 line summary with quick start

## Doc File Template

```markdown
# <Feature Name>

## Overview
One paragraph explaining what this does and who needs it.

## Prerequisites
| Tool | Version | Purpose |
|------|---------|---------|

## Quick Start
3-5 steps to basic usage.

## CLI Reference
Table of all flags with descriptions.

## Examples
Common workflows with commands.

## Troubleshooting
2-3 common issues with solutions.
```

## Session Close Checklist

Before marking work complete:
- [ ] Check if changes affect user workflows
- [ ] Update README or docs if needed
- [ ] Apply elements-of-style to edits
- [ ] Verify 5-minute rule still holds
