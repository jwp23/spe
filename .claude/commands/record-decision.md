# Record Decision

Record a technical decision that was just discussed.

## Steps

1. Ask the user: "Is this an ADR (architectural) or a decision doc (tactical)?"
2. For ADR: check `ls docs/adr/` for the next available number. Create the file using the ADR format from CLAUDE.md.
3. For decision doc: create a descriptive filename in `docs/decisions/` using the lightweight format from CLAUDE.md.
4. Present the recorded decision to the user for review.
5. Commit with message: `docs: record [ADR-NNN title | decision title]`
