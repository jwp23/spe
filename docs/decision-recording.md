# Decision Recording

Reference for formats, numbering, and classification rules. Read when recording a technical decision.

## Locations

- **ADRs** — `docs/adr/` — architectural and major library decisions
- **Decision docs** — `docs/decisions/` — tactical, tool, and convention decisions

## ADR Format

```
# ADR-NNN: Title

## Context
What situation or problem prompted this decision?

## Decision
What was decided and why?

## Trade-offs
What alternatives were considered? What are we giving up?
```

## Decision Doc Format

```
# Title
Decision: [what was decided]
Rationale: [why, in 1-3 sentences]
```

## ADR Numbering

- Three-digit sequential numbering: `001`, `002`, `003`
- Check existing ADRs before assigning a number: `ls docs/adr/`
- Filename format: `NNN-short-description.md` (lowercase, hyphens)

## Decision Doc Naming

- Descriptive filenames: `use-pdftoppm-for-rendering.md`
- Lowercase, hyphens, no numbers unless needed for ordering

## What Requires an ADR

- Language or runtime selection
- Framework or major library selection
- Architecture patterns (component structure, data flow)
- Testing strategy
- CI/CD pipeline design

## What Requires a Decision Doc

- Specific tool or utility selection
- Naming conventions
- File organization choices
- Configuration decisions
- Pre-commit hook setup
