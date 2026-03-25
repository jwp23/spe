// Zoom level calculations for the PDF canvas.

/// Compute the effective DPI for the current zoom level.
/// Base rendering DPI is 150; zoom scales from there.
pub fn effective_dpi(zoom: f32) -> f32 {
    150.0 * zoom
}

/// Zoom percentage for display (100% = zoom 1.0).
pub fn zoom_percent(zoom: f32) -> u32 {
    (zoom * 100.0).round() as u32
}

/// Zoom steps: 25%, 50%, 75%, 100%, 125%, 150%, 200%.
const ZOOM_STEPS: [f32; 7] = [0.25, 0.50, 0.75, 1.0, 1.25, 1.50, 2.0];

/// Compute zoom level that makes the rendered page width match the viewport width.
/// Returns a continuous value clamped to the valid zoom range.
pub fn fit_to_width_zoom(page_width_pts: f32, viewport_width: f32) -> f32 {
    if page_width_pts <= 0.0 || viewport_width <= 0.0 {
        return ZOOM_STEPS[0];
    }
    // rendered_width = page_width * zoom² * 150/72
    // Solving for zoom: zoom = sqrt(viewport_width * 72 / (page_width * 150))
    let zoom_sq = viewport_width * 72.0 / (page_width_pts * 150.0);
    let zoom = zoom_sq.sqrt();
    zoom.clamp(ZOOM_STEPS[0], *ZOOM_STEPS.last().unwrap())
}

/// Next zoom level up from current, or current if already at max.
pub fn zoom_in(current: f32) -> f32 {
    for &step in &ZOOM_STEPS {
        if step > current + 0.001 {
            return step;
        }
    }
    *ZOOM_STEPS.last().unwrap()
}

/// Next zoom level down from current, or current if already at min.
pub fn zoom_out(current: f32) -> f32 {
    for &step in ZOOM_STEPS.iter().rev() {
        if step < current - 0.001 {
            return step;
        }
    }
    ZOOM_STEPS[0]
}
