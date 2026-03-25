// Coordinate conversion between screen pixels and PDF points.

pub struct ConversionParams {
    pub zoom: f32,
    pub dpi: f32,
    pub page_height: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl ConversionParams {
    pub fn scale(&self) -> f32 {
        render_scale(self.zoom, self.dpi)
    }
}

/// Compute the combined zoom+DPI scale factor for rendering.
pub fn render_scale(zoom: f32, dpi: f32) -> f32 {
    zoom * (dpi / 72.0)
}

pub struct BoundingBox {
    pub width: f32,
    pub height: f32,
}

/// Convert screen pixel coordinates to PDF point coordinates.
pub fn screen_to_pdf(screen_x: f32, screen_y: f32, params: &ConversionParams) -> (f32, f32) {
    let scale = params.scale();
    let pdf_x = (screen_x - params.offset_x) / scale;
    let pdf_y = params.page_height - ((screen_y - params.offset_y) / scale);
    (pdf_x, pdf_y)
}

/// Convert PDF point coordinates to screen pixel coordinates.
pub fn pdf_to_screen(pdf_x: f32, pdf_y: f32, params: &ConversionParams) -> (f32, f32) {
    let scale = params.scale();
    let screen_x = pdf_x * scale + params.offset_x;
    let screen_y = (params.page_height - pdf_y) * scale + params.offset_y;
    (screen_x, screen_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_to_pdf_at_default_zoom_and_dpi() {
        let params = ConversionParams {
            zoom: 1.0,
            dpi: 72.0,
            page_height: 792.0,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        let (pdf_x, pdf_y) = screen_to_pdf(72.0, 72.0, &params);
        assert!((pdf_x - 72.0).abs() < 0.01);
        assert!((pdf_y - 720.0).abs() < 0.01);
    }

    #[test]
    fn round_trip_preserves_position() {
        let params = ConversionParams {
            zoom: 1.5,
            dpi: 150.0,
            page_height: 842.0,
            offset_x: 20.0,
            offset_y: 10.0,
        };
        let (px, py) = screen_to_pdf(300.0, 400.0, &params);
        let (sx, sy) = pdf_to_screen(px, py, &params);
        assert!((sx - 300.0).abs() < 0.01);
        assert!((sy - 400.0).abs() < 0.01);
    }

    #[test]
    fn screen_to_pdf_at_200_percent_zoom() {
        let params = ConversionParams {
            zoom: 2.0,
            dpi: 72.0,
            page_height: 792.0,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        let (pdf_x, _pdf_y) = screen_to_pdf(144.0, 0.0, &params);
        assert!((pdf_x - 72.0).abs() < 0.01);
    }

    #[test]
    fn pdf_to_screen_y_axis_flip() {
        let params = ConversionParams {
            zoom: 1.0,
            dpi: 72.0,
            page_height: 792.0,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        // PDF origin is bottom-left. PDF y=0 should map to screen y=792
        let (_sx, sy) = pdf_to_screen(0.0, 0.0, &params);
        assert!((sy - 792.0).abs() < 0.01);
        // PDF y=792 should map to screen y=0
        let (_sx, sy) = pdf_to_screen(0.0, 792.0, &params);
        assert!(sy.abs() < 0.01);
    }

    #[test]
    fn screen_to_pdf_with_offset() {
        let params = ConversionParams {
            zoom: 1.0,
            dpi: 72.0,
            page_height: 792.0,
            offset_x: 50.0,
            offset_y: 30.0,
        };
        let (pdf_x, pdf_y) = screen_to_pdf(122.0, 102.0, &params);
        assert!((pdf_x - 72.0).abs() < 0.01);
        assert!((pdf_y - 720.0).abs() < 0.01);
    }

    #[test]
    fn round_trip_at_high_dpi() {
        let params = ConversionParams {
            zoom: 1.0,
            dpi: 300.0,
            page_height: 792.0,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        let (px, py) = screen_to_pdf(1000.0, 500.0, &params);
        let (sx, sy) = pdf_to_screen(px, py, &params);
        assert!((sx - 1000.0).abs() < 0.01);
        assert!((sy - 500.0).abs() < 0.01);
    }
}
