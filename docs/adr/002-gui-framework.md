# ADR-002: GUI Framework Selection

## Context

The application needs a GUI framework that works well on Cosmic Desktop (Wayland), can display raster images (rendered PDF pages), supports mouse click handling with pixel-level coordinates for text placement, and provides text input and font selection controls.

## Decision

Use **Iced 0.14** with features: `image`, `canvas`, `tokio`.

Iced is the GUI framework that Cosmic Desktop itself is built with. It is GPU-accelerated via wgpu, Wayland-native, and designed for reactive, 60fps rendering. The `canvas` widget supports custom drawing and mouse event handling with coordinates — exactly what we need for click-to-place text. The `image` widget can display rendered PDF pages. The `tokio` feature enables async background tasks for pre-rendering PDF pages.

File dialogs are handled by the `rfd` crate, which integrates with Iced's Task system and uses XDG Desktop Portal on Linux (native to Cosmic/Wayland).

## Trade-offs

**Considered: GTK4 via gtk4-rs** — Mature, well-documented, strong Linux support. But GTK is not the native toolkit for Cosmic Desktop, adds a heavy C dependency chain, and the Rust bindings add a layer of indirection over the C API.

**Considered: egui** — Immediate-mode, simple, very responsive. But less suitable for document-oriented UIs, doesn't integrate with Cosmic Desktop's theming, and lacks native file dialog integration.

**Giving up:** GTK4's maturity and extensive widget library. Iced is pre-1.0 and its API may change between releases. Documentation is thinner than GTK4's. **Gaining:** Native Cosmic Desktop integration, GPU-accelerated rendering, idiomatic Rust API, and the same toolkit the desktop environment uses.
