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
        let mut batch = self.render_page_batch(pdf_path, page, page, dpi)?;
        // render_page_batch guarantees one entry when first_page == last_page.
        let (_page_num, img) = batch.remove(0);
        Ok(img)
    }
}

impl PdftoppmRenderer {
    /// Renders a contiguous range of pages in a single `pdftoppm` subprocess call.
    ///
    /// Returns a `Vec` of `(page_number, image)` pairs, in ascending page order.
    pub fn render_page_batch(
        &self,
        pdf_path: &Path,
        first_page: u32,
        last_page: u32,
        dpi: u32,
    ) -> Result<Vec<(u32, DynamicImage)>, RendererError> {
        if first_page == 0 || last_page == 0 || first_page > last_page {
            return Err(RendererError::RenderFailed {
                page: first_page,
                path: pdf_path.to_path_buf(),
                detail: format!(
                    "invalid page range: first_page={first_page}, last_page={last_page} \
                     (pages must be >= 1 and first_page <= last_page)"
                ),
            });
        }

        let tmp_dir = self.invoke_pdftoppm(pdf_path, first_page, last_page, dpi)?;

        // Collect all PNG files and sort by name; pdftoppm names them sequentially.
        let mut png_paths: Vec<_> = std::fs::read_dir(tmp_dir.path())
            .map_err(|e| RendererError::RenderFailed {
                page: first_page,
                path: pdf_path.to_path_buf(),
                detail: format!("failed to read temp directory: {e}"),
            })?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("png"))
            .collect();
        png_paths.sort();

        let mut results = Vec::new();
        for (i, png_path) in png_paths.into_iter().enumerate() {
            let page_num = first_page + i as u32;
            let img = image::open(&png_path)
                .map_err(|e| RendererError::ImageDecodeFailed(e.to_string()))?;
            results.push((page_num, img));
        }
        Ok(results)
    }

    /// Probes for `pdftoppm`, creates a temp directory, invokes `pdftoppm` for the
    /// given page range, and checks the exit status. Returns the live `TempDir` so
    /// the caller can read the output files before they are deleted.
    fn invoke_pdftoppm(
        &self,
        pdf_path: &Path,
        first_page: u32,
        last_page: u32,
        dpi: u32,
    ) -> Result<TempDir, RendererError> {
        let probe = Command::new("pdftoppm").arg("-v").output();
        match probe {
            Err(e) if e.kind() == ErrorKind::NotFound => return Err(RendererError::NotInstalled),
            Err(e) => {
                return Err(RendererError::RenderFailed {
                    page: first_page,
                    path: pdf_path.to_path_buf(),
                    detail: format!("failed to probe pdftoppm: {e}"),
                });
            }
            Ok(_) => {}
        }

        let tmp_dir = TempDir::new().map_err(|e| RendererError::RenderFailed {
            page: first_page,
            path: pdf_path.to_path_buf(),
            detail: format!("failed to create temp directory: {e}"),
        })?;

        let prefix = tmp_dir.path().join("page");

        // Invoke: pdftoppm -f <first> -l <last> -r <dpi> -png <pdf> <prefix>
        let output = Command::new("pdftoppm")
            .args([
                "-f",
                &first_page.to_string(),
                "-l",
                &last_page.to_string(),
                "-r",
                &dpi.to_string(),
                "-png",
            ])
            .arg(pdf_path)
            .arg(&prefix)
            .output()
            .map_err(|e| RendererError::RenderFailed {
                page: first_page,
                path: pdf_path.to_path_buf(),
                detail: format!("failed to spawn pdftoppm: {e}"),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(RendererError::RenderFailed {
                page: first_page,
                path: pdf_path.to_path_buf(),
                detail: stderr,
            });
        }

        Ok(tmp_dir)
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
    fn render_page_batch_trait_exists() {
        // Compile-time proof that the function exists on PdftoppmRenderer
        let _f: fn(
            &PdftoppmRenderer,
            &Path,
            u32,
            u32,
            u32,
        ) -> Result<Vec<(u32, DynamicImage)>, RendererError> = PdftoppmRenderer::render_page_batch;
    }

    #[test]
    fn render_page_batch_rejects_zero_first_page() {
        let renderer = PdftoppmRenderer;
        let result = renderer.render_page_batch(Path::new("/any.pdf"), 0, 1, 72);
        assert!(
            matches!(result, Err(RendererError::RenderFailed { .. })),
            "expected RenderFailed for first_page=0"
        );
    }

    #[test]
    fn render_page_batch_rejects_zero_last_page() {
        let renderer = PdftoppmRenderer;
        let result = renderer.render_page_batch(Path::new("/any.pdf"), 1, 0, 72);
        assert!(
            matches!(result, Err(RendererError::RenderFailed { .. })),
            "expected RenderFailed for last_page=0"
        );
    }

    #[test]
    fn render_page_batch_rejects_inverted_range() {
        let renderer = PdftoppmRenderer;
        let result = renderer.render_page_batch(Path::new("/any.pdf"), 5, 3, 72);
        assert!(
            matches!(result, Err(RendererError::RenderFailed { .. })),
            "expected RenderFailed for first_page > last_page"
        );
    }

    #[test]
    fn render_page_batch_validation_error_contains_range_details() {
        let renderer = PdftoppmRenderer;
        let err = renderer
            .render_page_batch(Path::new("/any.pdf"), 5, 3, 72)
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("5") && msg.contains("3"),
            "expected page numbers in error: {msg}"
        );
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
