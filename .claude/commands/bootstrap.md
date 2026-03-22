# Bootstrap Project

Initiate the project bootstrapping sequence.

## Steps

1. Check if `docs/adr/` and `docs/decisions/` directories exist. Create them if not.
2. Check if any ADRs already exist (`ls docs/adr/`). If yes, summarize what has been decided so far and ask what to tackle next.
3. If no ADRs exist, propose a bootstrapping sequence. Present it as a numbered list to the user for approval. Suggested order:
   - ADR-001: Language and runtime
   - ADR-002: GUI framework
   - ADR-003: PDF rendering and writing libraries
   - ADR-004: Linux system utility strategy
   - ADR-005: Testing framework and strategy
   - ADR-006: Code style, formatting, linting
   - ADR-007: Project directory structure
   - ADR-008: Pre-commit and CI pipeline
4. Wait for user approval or modifications to the sequence.
5. Begin working through the sequence one ADR at a time. For each: present options, discuss trade-offs, get user input, then record the decision.
6. After all bootstrapping ADRs are recorded, propose the initial project structure and create skeleton files.
