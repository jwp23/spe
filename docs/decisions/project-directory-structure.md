# Project Directory Structure

Decision: Standard Cargo binary project layout with modular `src/` organization.

Rationale: Follows Rust conventions. Modules map to concerns: `pdf/` for PDF operations (rendering and writing), `ui/` for Iced widgets (canvas and toolbar), `fonts` for font discovery, `overlay` for the data model. Unit tests are co-located in `#[cfg(test)]` modules per Rust idiom; integration tests live in `tests/` at the project root.

```
spe/
├── Cargo.toml
├── Cargo.lock
├── CLAUDE.md
├── AGENTS.md
├── .gitignore
├── .beads/
├── .claude/
├── .github/
│   └── workflows/
│       └── ci.yml
├── docs/
│   ├── adr/
│   ├── decisions/
│   ├── code-style-guide.md
│   └── architecture.md         # created after bootstrapping
├── src/
│   ├── main.rs                 # entry point, module declarations
│   ├── app.rs                  # Iced Application impl, state, messages
│   ├── overlay.rs              # text overlay data model
│   ├── fonts.rs                # fc-list wrapper for font discovery
│   ├── pdf/
│   │   ├── mod.rs
│   │   ├── renderer.rs         # pdftoppm wrapper
│   │   └── writer.rs           # lopdf text overlay writer
│   └── ui/
│       ├── mod.rs
│       ├── canvas.rs           # PDF page canvas with click handling
│       └── toolbar.rs          # font family/size controls
└── tests/
    ├── pdf_rendering.rs
    ├── pdf_writing.rs
    └── font_discovery.rs
```
