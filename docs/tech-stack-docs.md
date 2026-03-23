# Tech Stack Documentation

External documentation for each dependency in the project. Versions reflect the Cargo.toml semver spec; docs.rs links resolve to the exact version in Cargo.lock at time of writing.

When docs.rs links 404 after a version bump, replace the version segment in the URL with the new version from `Cargo.lock`.

## Rust Crates

| Crate | Version | Documentation |
|-------|---------|---------------|
| iced | 0.14 | [API docs](https://docs.rs/iced/0.14.0/iced/) · [Guide (book)](https://book.iced.rs/) · [Examples](https://github.com/iced-rs/iced/tree/0.14/examples) |
| iced_test | 0.14 (dev) | [API docs](https://docs.rs/iced_test/0.14.0/iced_test/) |
| lopdf | 0.40 | [API docs](https://docs.rs/lopdf/0.40.0/lopdf/) |
| rfd | 0.17 | [API docs](https://docs.rs/rfd/0.17.0/rfd/) |
| image | 0.25 | [API docs](https://docs.rs/image/0.25.0/image/) |
| thiserror | 2 | [API docs](https://docs.rs/thiserror/2.0.18/thiserror/) |
| tempfile | 3 | [API docs](https://docs.rs/tempfile/3.27.0/tempfile/) |
| tokio | 1 | [API docs](https://docs.rs/tokio/1.50.0/tokio/) |

## System Utilities

| Utility | Package | Documentation |
|---------|---------|---------------|
| pdftoppm | poppler-utils | [Man page](https://manpages.debian.org/bookworm/poppler-utils/pdftoppm.1.en.html) · `man pdftoppm` |
| fc-list | fontconfig | [Man page](https://manpages.debian.org/bookworm/fontconfig/fc-list.1.en.html) · `man fc-list` |

## Language & Toolchain

| Tool | Documentation |
|------|---------------|
| Rust (edition 2024) | [The Rust Programming Language](https://doc.rust-lang.org/book/) · [Standard Library](https://doc.rust-lang.org/std/) · [Edition Guide](https://doc.rust-lang.org/edition-guide/) |
| Cargo | [The Cargo Book](https://doc.rust-lang.org/cargo/) |
| rustfmt | [Configuration](https://rust-lang.github.io/rustfmt/) |
| clippy | [Lint list](https://rust-lang.github.io/rust-clippy/master/) |
