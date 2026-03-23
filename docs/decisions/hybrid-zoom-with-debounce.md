# Hybrid Zoom with Debounce

Decision: On zoom change, immediately scale the cached raster image for instant visual feedback, then debounce a background re-render at the new DPI after 300ms of zoom inactivity. A `zoom_generation` counter ensures only the final zoom level triggers a re-render.

Rationale: Re-rendering at every zoom step would be slow (pdftoppm subprocess per step). Scaling the cached image is instant but gets blurry at high zoom. The hybrid approach gives immediate responsiveness with eventual sharpness — the standard UX pattern used by PDF viewers like Evince and Okular.
