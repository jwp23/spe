# Code Style Guide

Read this when writing or reviewing code. These expand on the principles in CLAUDE.md.

## Human Readable

Names are documentation. A reader should understand what code does without reading its implementation.

- Name functions as verb phrases describing what they do: `render_page`, `place_text_overlay`, `discover_system_fonts`. Not `process`, `handle`, `do_thing`.
- Name variables as nouns describing what they hold: `page_image`, `font_path`, `overlay_position`. Not `data`, `temp`, `result`.
- Name booleans as questions: `is_valid`, `has_overlays`, `can_render`. Not `flag`, `check`, `status`.
- Avoid abbreviations unless universally understood in the domain (`pdf`, `gui`, `dpi` are fine; `pg`, `fnt`, `pos` are not).
- Limit function length to what fits in one screen (~30 lines). If longer, extract a well-named helper.
- Limit function parameters to 4. If more are needed, group related parameters into a data structure.
- Limit module size to ~500 lines of production code or ~1000 total lines (including tests). When a file exceeds this, split it into a directory module (`foo.rs` → `foo/mod.rs` + sibling files). Extract by concern — view logic, handlers, pure functions — into sibling files with separate `impl` blocks. Move tests into a `tests.rs` submodule when they exceed ~500 lines. Each file should represent a single coherent concern readable in one pass.
- Write comments only to explain WHY, never WHAT. The code explains what. If code needs a WHAT comment, the code is unclear — rewrite the code.

## Loosely Coupled

Each component should be replaceable without rewriting its consumers.

- Define clear boundaries: UI rendering, PDF operations, font management, file I/O, and system utility wrappers are separate concerns.
- Depend on abstractions (interfaces, protocols, type signatures) not implementations. If the PDF library changes, only the PDF module should change.
- No module should import from more than one layer away. UI code does not call system utilities directly — it goes through an intermediate layer.
- Pass dependencies in rather than reaching out for them. Functions accept what they need as parameters rather than importing globals or singletons.
- Side effects (file I/O, subprocess calls, GUI updates) live at the edges. Core logic is pure and testable without mocking.

## Idiomatic

Using the language the way its community uses it reduces cognitive load for any developer familiar with that language.

- Follow the language's official style guide as the baseline.
- Use the language's standard library before reaching for third-party alternatives.
- Use the language's native error handling pattern (exceptions, Result types, error returns — whatever is standard).
- Use the language's standard project layout (not a custom structure unless the ADR justifies it).
- When in doubt, look at how the language's standard library or most respected framework solves the same problem.
- Record specific idiom decisions (e.g., "use dataclasses over dicts for structured data") in `docs/decisions/`.

## Simple — Do Not Inherit a Ball of Mud

Complexity is the enemy. Every abstraction has a cost. Only add abstractions that pay for themselves.

Anti-patterns to refuse:
- Deep inheritance hierarchies. Prefer composition: small objects that hold references to each other.
- God objects that know everything and do everything. Split by responsibility.
- Premature abstraction. Do not create an interface until there are two implementations. Do not create a factory until there are two creation paths.
- Framework worship. Do not adopt a pattern because a framework uses it if the project doesn't need it.
- Abstraction layers that only delegate. If a function's entire body is calling one other function, it probably shouldn't exist.

Decision test: "If I delete this abstraction and inline its logic, does the code get harder or easier to understand?" If easier, delete the abstraction.

## Professional Engineering Standards

- No dead code. If code is not called, delete it. Version control remembers.
- No commented-out code. Same reason.
- No TODO without a tracked issue or decision doc. If it's worth doing later, record it properly.
- No copy-paste duplication. Extract shared logic into a well-named function.
- No magic numbers or strings. Use named constants with descriptive names.
- Error messages must be actionable. Tell the user what went wrong AND what to do about it. "Failed to render PDF page 3: pdftoppm not found. Install with: sudo apt install poppler-utils"
- Log at appropriate levels: ERROR for things that need immediate attention, WARN for recoverable issues, INFO for significant operations, DEBUG for tracing.
- Handle resource cleanup explicitly (file handles, subprocess pipes, GUI resources). Use the language's idiom for this (context managers, defer, RAII, try-finally).
