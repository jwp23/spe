# Decision Recording Rules

## Before Making Any Technical Decision

1. Identify whether this is an architectural decision (ADR) or a tactical decision (decision doc).
2. Ask the user: "Should I record this in `docs/adr/` or `docs/decisions/`?"
3. Wait for confirmation before recording.
4. Never make a significant technical choice without recording it.

## ADR Numbering

- Use three-digit sequential numbering: `001`, `002`, `003`
- Check existing ADRs before assigning a number: `ls docs/adr/`
- Filename format: `NNN-short-description.md` (lowercase, hyphens)

## Decision Doc Naming

- Use descriptive filenames: `use-pdftoppm-for-rendering.md`
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
