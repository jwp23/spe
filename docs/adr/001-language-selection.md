# ADR-001: Language Selection

## Context

This project is a desktop GUI application for Linux that renders PDF pages, allows click-to-place text overlays, and saves the result as a new PDF. The target environment is Cosmic Desktop on CachyOS (Arch-based), running Wayland. A key goal is butter-smooth UI performance — zero perceptible lag during page rendering, scrolling, and text placement.

The application needs to: call system utilities (pdftoppm, fc-list) via subprocess, render raster images in a GUI, handle mouse click coordinates for text placement, and write text into existing PDF files.

## Decision

Use **Rust** (1.94.0, system-installed via pacman on Arch Linux).

Rust provides zero-overhead event dispatch with no garbage collector or interpreter in the hot path. It enables native integration with Iced, the GUI framework that powers Cosmic Desktop. RAII ensures clean resource management for file handles, subprocess pipes, and image buffers. The type system catches integration errors at compile time rather than runtime.

## Trade-offs

**Considered: Python** — Faster prototyping, richer PDF library ecosystem (PyMuPDF handles both rendering and writing ergonomically), mature TDD tooling (pytest). However, Python's GIL can block the event loop during CPU-bound work, the interpreter adds overhead to every event callback, and Iced bindings would be second-class compared to the native Rust API.

**Considered: Go** — Strong concurrency model and fast compilation, but the Linux GUI ecosystem is weak (Fyne and Gio lack the polish needed for a document-oriented application).

**Giving up:** Rapid prototyping speed, dynamic typing flexibility, and access to Python's ergonomic PDF libraries. **Gaining:** Zero-overhead performance, native Iced API, compile-time safety, and tight integration with the Cosmic Desktop ecosystem.
