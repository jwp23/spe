pub struct AppConfig {
    pub overlay_color: [f32; 4], // RGBA, 0.0-1.0
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            overlay_color: [0.26, 0.53, 0.96, 1.0], // blue (#4287f5)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let config = AppConfig::default();
        assert!((config.overlay_color[0] - 0.26).abs() < f32::EPSILON);
    }

    #[test]
    fn config_fields_are_overridable() {
        let config = AppConfig {
            overlay_color: [1.0, 0.0, 0.0, 1.0],
        };
        assert!((config.overlay_color[0] - 1.0).abs() < f32::EPSILON);
    }
}
