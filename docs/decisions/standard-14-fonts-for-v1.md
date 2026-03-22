# Standard 14 Fonts for v1

Decision: Use only PDF Standard 14 built-in fonts for text overlays in v1. Defer system font discovery (`fc-list`) and font embedding to future work.

Rationale: Standard 14 fonts are guaranteed available in every PDF reader without embedding, which avoids the complexity of parsing `fc-list` output, resolving font files, and embedding font programs into the PDF. The overlay writer already supports Standard 14 fonts via `lopdf`. A hardcoded dropdown in the toolbar is sufficient for v1.
