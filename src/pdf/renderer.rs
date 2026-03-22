#![allow(dead_code)]
// PDF page rendering via pdftoppm.

use std::path::{Path, PathBuf};

use image::DynamicImage;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RendererError {
    #[error("pdftoppm not found. Install with: sudo pacman -S poppler")]
    NotInstalled,

    #[error("pdftoppm failed for page {page} of {}: {detail}", path.display())]
    RenderFailed {
        page: u32,
        path: PathBuf,
        detail: String,
    },

    #[error("failed to decode rendered image: {0}")]
    ImageDecodeFailed(String),
}

/// Renders a single page of a PDF to a raster image.
pub trait PageRenderer {
    fn render_page(
        &self,
        pdf_path: &Path,
        page: u32,
        dpi: u32,
    ) -> Result<DynamicImage, RendererError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_installed_error_includes_install_instructions() {
        let err = RendererError::NotInstalled;
        let msg = err.to_string();
        assert!(msg.contains("pacman"), "expected 'pacman' in: {msg}");
        assert!(msg.contains("poppler"), "expected 'poppler' in: {msg}");
    }

    #[test]
    fn render_failed_error_includes_context() {
        let err = RendererError::RenderFailed {
            page: 3,
            path: PathBuf::from("/tmp/test.pdf"),
            detail: "exit status 1".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("3"), "expected page number in: {msg}");
        assert!(msg.contains("test.pdf"), "expected path in: {msg}");
        assert!(msg.contains("exit status 1"), "expected detail in: {msg}");
    }

    #[test]
    fn image_decode_error_includes_reason() {
        let reason = "unexpected EOF";
        let err = RendererError::ImageDecodeFailed(reason.to_string());
        let msg = err.to_string();
        assert!(msg.contains(reason), "expected reason in: {msg}");
    }

    #[test]
    fn mock_renderer_implements_trait() {
        struct MockRenderer;

        impl PageRenderer for MockRenderer {
            fn render_page(
                &self,
                _pdf_path: &Path,
                _page: u32,
                _dpi: u32,
            ) -> Result<DynamicImage, RendererError> {
                Err(RendererError::NotInstalled)
            }
        }

        let renderer = MockRenderer;
        let result = renderer.render_page(Path::new("/any.pdf"), 1, 150);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RendererError::NotInstalled));
    }
}
