// PDF text overlay writing via lopdf.
#![allow(dead_code)]

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WriterError {
    #[error("failed to open PDF: {0}")]
    OpenFailed(lopdf::Error),

    #[error("page {requested} not found in PDF (document has {total} pages)")]
    PageNotFound { requested: u32, total: u32 },

    #[error("failed to save PDF to {}: {source}", path.display())]
    SaveFailed {
        path: PathBuf,
        #[source]
        source: lopdf::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{Document, Object, Stream, dictionary};
    use std::path::Path;
    use tempfile::NamedTempFile;

    /// Builds a minimal single-page PDF and saves it to `path`.
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
    fn create_test_pdf_produces_valid_single_page_pdf() {
        let tmp = NamedTempFile::new().expect("failed to create temp file");
        let path = tmp.path();

        create_test_pdf(path);

        let doc = Document::load(path).expect("lopdf failed to re-open written PDF");
        assert_eq!(
            doc.get_pages().len(),
            1,
            "expected 1 page, got {}",
            doc.get_pages().len()
        );
    }

    #[test]
    fn writer_error_open_failed_display() {
        let inner = lopdf::Error::CharacterEncoding;
        let err = WriterError::OpenFailed(inner);
        let msg = err.to_string();
        assert!(
            msg.starts_with("failed to open PDF:"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn writer_error_page_not_found_display() {
        let err = WriterError::PageNotFound {
            requested: 5,
            total: 2,
        };
        let msg = err.to_string();
        assert_eq!(msg, "page 5 not found in PDF (document has 2 pages)");
    }

    #[test]
    fn writer_error_save_failed_display() {
        let path = PathBuf::from("/tmp/out.pdf");
        let err = WriterError::SaveFailed {
            path: path.clone(),
            source: lopdf::Error::CharacterEncoding,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("/tmp/out.pdf"),
            "expected path in message: {msg}"
        );
        assert!(
            msg.starts_with("failed to save PDF to"),
            "unexpected message: {msg}"
        );
    }
}
