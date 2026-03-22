// PDF page rendering via pdftoppm.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use image::DynamicImage;
use tempfile::TempDir;
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

/// Renders PDF pages by invoking the system `pdftoppm` utility.
pub struct PdftoppmRenderer;

impl PageRenderer for PdftoppmRenderer {
    fn render_page(
        &self,
        pdf_path: &Path,
        page: u32,
        dpi: u32,
    ) -> Result<DynamicImage, RendererError> {
        // Verify pdftoppm is available.
        let probe = Command::new("pdftoppm").arg("-v").output();
        match probe {
            Err(e) if e.kind() == ErrorKind::NotFound => return Err(RendererError::NotInstalled),
            Err(e) => {
                return Err(RendererError::RenderFailed {
                    page,
                    path: pdf_path.to_path_buf(),
                    detail: format!("failed to probe pdftoppm: {e}"),
                });
            }
            Ok(_) => {}
        }

        // Create a temp directory; pdftoppm writes output files here.
        let tmp_dir = TempDir::new().map_err(|e| RendererError::RenderFailed {
            page,
            path: pdf_path.to_path_buf(),
            detail: format!("failed to create temp directory: {e}"),
        })?;

        let prefix = tmp_dir.path().join("page");

        // Invoke: pdftoppm -f <page> -l <page> -r <dpi> -png <pdf> <prefix>
        let output = Command::new("pdftoppm")
            .args([
                "-f",
                &page.to_string(),
                "-l",
                &page.to_string(),
                "-r",
                &dpi.to_string(),
                "-png",
            ])
            .arg(pdf_path)
            .arg(&prefix)
            .output()
            .map_err(|e| RendererError::RenderFailed {
                page,
                path: pdf_path.to_path_buf(),
                detail: format!("failed to spawn pdftoppm: {e}"),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(RendererError::RenderFailed {
                page,
                path: pdf_path.to_path_buf(),
                detail: stderr,
            });
        }

        // Find the PNG file pdftoppm wrote. It names files like `<prefix>-<N>.png`
        // where N is zero-padded based on total page count. Glob for any .png in the dir.
        let png_path = std::fs::read_dir(tmp_dir.path())
            .map_err(|e| RendererError::RenderFailed {
                page,
                path: pdf_path.to_path_buf(),
                detail: format!("failed to read temp directory: {e}"),
            })?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|p| p.extension().and_then(|e| e.to_str()) == Some("png"))
            .ok_or_else(|| RendererError::RenderFailed {
                page,
                path: pdf_path.to_path_buf(),
                detail: "pdftoppm produced no PNG output".to_string(),
            })?;

        // Decode the PNG into a DynamicImage.
        image::open(&png_path).map_err(|e| RendererError::ImageDecodeFailed(e.to_string()))
    }
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
    fn pdftoppm_renderer_is_constructible() {
        // Compile-time proof that PdftoppmRenderer exists and implements PageRenderer.
        let _r: &dyn PageRenderer = &PdftoppmRenderer;
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
