# ADR-004: Linux System Utility Integration Strategy

## Context

The application depends on two Linux system utilities:

- `pdftoppm` (poppler-utils) — PDF page rasterization
- `fc-list` (fontconfig) — System font discovery

Both are standard on Linux desktop installations and are already present on the target system (CachyOS with Cosmic Desktop).

## Decision

**Invocation:** Use `std::process::Command` directly. Never invoke commands through a shell interpreter (`sh -c`). Pass arguments as separate parameters to `Command::arg()`.

**Wrapper design:** Each utility gets a dedicated wrapper module (`pdf/renderer.rs` for pdftoppm, `fonts.rs` for fc-list) with a trait-based abstraction. The trait defines the operation's interface; the production implementation calls the real utility. This enables unit tests to substitute a test double without requiring the utility to be installed.

**Error handling:** Every wrapper checks that the utility exists before invoking it (fail fast). On failure, capture stderr and return an error that names the tool, describes what went wrong, and explains how to install it (e.g., "pdftoppm not found. Install with: sudo pacman -S poppler").

**Background execution:** PDF page rendering via pdftoppm runs in background threads. When the user views page N, pages N-1 and N+1 are pre-rendered asynchronously. Rendered pages are cached in memory to avoid redundant subprocess calls.

## Trade-offs

**Considered: Pure-library alternatives** — Using pdfium-render or mupdf-rs instead of pdftoppm would eliminate subprocess overhead entirely. However, both introduce significant external dependencies (binary shared libraries or AGPL licensing). The subprocess approach is simpler, uses proven tools, and the overhead is manageable with background pre-rendering.

**Giving up:** In-process rendering speed, tighter error recovery (a subprocess crash is harder to diagnose than a library error). **Gaining:** Zero binary dependency management, battle-tested rendering quality from poppler, and clean separation between the application and its system dependencies.
