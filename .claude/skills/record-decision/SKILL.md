---
name: record-decision
description: Use when a technical decision is being discussed or has been made - choosing between approaches, selecting libraries, establishing conventions, or making trade-offs
---

# Record Decision

## Overview

Record technical decisions as they happen. Architectural decisions go in `docs/adr/`, tactical decisions in `docs/decisions/`.

**Core principle:** Decisions not recorded are decisions forgotten. Record before implementing.

## When to Use

**Trigger when you notice:**
- Choosing between multiple approaches ("X vs Y")
- Selecting a library, tool, or pattern
- Establishing a project convention
- Making a trade-off ("we're giving up X for Y")
- Someone asking "why did we do it this way?"

**Do NOT trigger for:**
- Routine implementation choices (variable names, loop vs iterator)
- Choices already recorded in an existing ADR or decision doc
- Choices dictated by an existing decision (follow the decision, don't re-record it)

## Classification

Ask Joe if uncertain: "Should I record this as an ADR or decision doc?"

| ADR (`docs/adr/`) | Decision Doc (`docs/decisions/`) |
|--------------------|----------------------------------|
| Language/runtime selection | Specific tool choice |
| Framework/major library | Naming conventions |
| Architecture patterns | File organization |
| Testing strategy | Configuration choices |
| CI/CD pipeline design | Pre-commit hook setup |

## The Process

1. **Detect** — Recognize a decision is being made or was just made
2. **Ask** — "Should I record this as an ADR or decision doc?"
3. **Number/Name**
   - ADR: `ls docs/adr/` → next three-digit number → `NNN-short-description.md`
   - Decision doc: descriptive filename → `short-description.md`
4. **Write** — Use format from CLAUDE.md
   - ADR: Context / Decision / Trade-offs
   - Decision doc: Decision / Rationale
5. **Review** — Present to Joe for approval before committing
6. **Commit** — `docs: record ADR-NNN title` or `docs: record title`

## Quick Reference

| | ADR | Decision Doc |
|-|-----|-------------|
| Location | `docs/adr/NNN-title.md` | `docs/decisions/title.md` |
| Numbering | Three-digit sequential | None |
| Format | Context / Decision / Trade-offs | Decision / Rationale |
| Commit | `docs: record ADR-NNN title` | `docs: record title` |

## Common Mistakes

**Recording too late**
- Decision buried in code, rationale lost
- Fix: Record during or immediately after discussion, before implementing

**Wrong classification**
- Tactical choice recorded as ADR, or vice versa
- Fix: Ask Joe. When uncertain, err toward decision doc (lighter weight)

**Skipping the review step**
- Recorded decision doesn't match what was actually decided
- Fix: Always present to Joe before committing
