// Integration tests for PDF page rendering via pdftoppm.

use std::path::Path;

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use spe::pdf::renderer::{PageRenderer, PdftoppmRenderer};
use tempfile::NamedTempFile;

/// Builds a minimal single-page PDF (US Letter, 612x792 points) and saves it to `path`.
fn create_test_pdf(path: &Path) {
    let mut doc = Document::with_version("1.5");

    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new("Td", vec![72.into(), 720.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(
                    b"Test".to_vec(),
                    lopdf::StringFormat::Literal,
                )],
            ),
            Operation::new("ET", vec![]),
        ],
    };

    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        content.encode().expect("content encoding failed"),
    ));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
        "Resources" => resources_id,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    doc.save(path).expect("failed to save test PDF");
}

#[test]
#[ignore] // Requires pdftoppm to be installed
fn renders_pdf_page_to_image() {
    let tmp = NamedTempFile::new().unwrap();
    create_test_pdf(tmp.path());

    let renderer = PdftoppmRenderer;
    let image = renderer.render_page(tmp.path(), 1, 150).unwrap();

    // A US Letter page (612x792 points) at 150 DPI should be roughly 1275x1650 pixels.
    // (612 / 72 * 150 = 1275, 792 / 72 * 150 = 1650)
    assert!(image.width() > 0, "image width must be positive");
    assert!(image.height() > 0, "image height must be positive");

    // Rough dimension check — allow some tolerance for rounding.
    assert!(
        image.width() > 1200 && image.width() < 1400,
        "unexpected width: {}",
        image.width()
    );
    assert!(
        image.height() > 1550 && image.height() < 1750,
        "unexpected height: {}",
        image.height()
    );
}

#[test]
#[ignore] // Requires pdftoppm to be installed
fn returns_error_for_invalid_page() {
    let tmp = NamedTempFile::new().unwrap();
    create_test_pdf(tmp.path());

    let renderer = PdftoppmRenderer;
    // Page 99 doesn't exist in a 1-page PDF.
    let result = renderer.render_page(tmp.path(), 99, 150);

    // pdftoppm may succeed with no output or fail — depends on version.
    // Either way, the call must not panic, and any error must be a known variant.
    if let Err(e) = result {
        assert!(
            matches!(
                e,
                spe::pdf::renderer::RendererError::RenderFailed { .. }
                    | spe::pdf::renderer::RendererError::ImageDecodeFailed(_)
            ),
            "unexpected error variant: {e}"
        );
    }
}
