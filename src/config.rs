use crate::overlay::Standard14Font;

pub struct AppConfig {
    pub overlay_color: [f32; 4], // RGBA, 0.0-1.0
    pub default_font: Standard14Font,
    pub default_font_size: f32,
    pub min_window_width: u32,
    pub min_window_height: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            overlay_color: [0.26, 0.53, 0.96, 1.0], // blue (#4287f5)
            default_font: Standard14Font::Helvetica,
            default_font_size: 12.0,
            min_window_width: 750,
            min_window_height: 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let config = AppConfig::default();
        assert_eq!(config.default_font, Standard14Font::Helvetica);
        assert!((config.default_font_size - 12.0).abs() < f32::EPSILON);
        assert_eq!(config.min_window_width, 750);
        assert_eq!(config.min_window_height, 500);
    }

    #[test]
    fn config_fields_are_overridable() {
        let config = AppConfig {
            overlay_color: [1.0, 0.0, 0.0, 1.0],
            default_font: Standard14Font::Courier,
            default_font_size: 14.0,
            ..AppConfig::default()
        };
        assert_eq!(config.default_font, Standard14Font::Courier);
        assert!((config.default_font_size - 14.0).abs() < f32::EPSILON);
    }
}
