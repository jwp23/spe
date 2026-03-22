// PDF text overlay writing via lopdf.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use thiserror::Error;

use crate::overlay::TextOverlay;

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

/// Write `overlays` onto the PDF at `source`, saving the result to `destination`.
pub fn write_overlays(
    source: &Path,
    destination: &Path,
    overlays: &[TextOverlay],
) -> Result<(), WriterError> {
    let mut doc = Document::load(source).map_err(WriterError::OpenFailed)?;

    let pages = doc.get_pages();

    for overlay in overlays {
        let &page_id = pages.get(&overlay.page).ok_or(WriterError::PageNotFound {
            requested: overlay.page,
            total: pages.len() as u32,
        })?;

        // Add the font as an indirect object.
        let font_obj_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => Object::Name(overlay.font.pdf_name().as_bytes().to_vec()),
        });

        // Ensure the page has its own Resources dict with a Font sub-dict.
        // Setting Resources directly on the Page overrides the inherited parent dict (PDF spec §7.8.3).
        {
            let page_dict = doc
                .get_object_mut(page_id)
                .expect("page object must exist")
                .as_dict_mut()
                .expect("page object must be a dictionary");

            if !page_dict.has(b"Resources") {
                page_dict.set("Resources", dictionary! {});
            }

            let resources = page_dict
                .get_mut(b"Resources")
                .expect("Resources just set")
                .as_dict_mut()
                .expect("Resources must be a dictionary");

            if !resources.has(b"Font") {
                resources.set("Font", dictionary! {});
            }

            resources
                .get_mut(b"Font")
                .expect("Font just set")
                .as_dict_mut()
                .expect("Font must be a dictionary")
                .set("F_overlay0", font_obj_id);
        }

        // Build the content stream for this overlay.
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new(
                    "Tf",
                    vec![
                        Object::Name(b"F_overlay0".to_vec()),
                        Object::Real(overlay.font_size),
                    ],
                ),
                Operation::new(
                    "Td",
                    vec![
                        Object::Real(overlay.position.x),
                        Object::Real(overlay.position.y),
                    ],
                ),
                Operation::new(
                    "Tj",
                    vec![Object::String(
                        overlay.text.as_bytes().to_vec(),
                        lopdf::StringFormat::Literal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ],
        };

        let content_bytes = content.encode().map_err(|e| WriterError::SaveFailed {
            path: destination.to_path_buf(),
            source: e,
        })?;

        let stream_id = doc.add_object(Stream::new(dictionary! {}, content_bytes));

        // Append the new stream to the page's Contents.
        let page_dict = doc
            .get_object_mut(page_id)
            .expect("page object must exist")
            .as_dict_mut()
            .expect("page object must be a dictionary");

        match page_dict.get(b"Contents") {
            Ok(Object::Reference(existing_id)) => {
                let existing_id = *existing_id;
                page_dict.set(
                    "Contents",
                    vec![Object::Reference(existing_id), Object::Reference(stream_id)],
                );
            }
            Ok(Object::Array(arr)) => {
                let mut new_arr = arr.clone();
                new_arr.push(Object::Reference(stream_id));
                page_dict.set("Contents", Object::Array(new_arr));
            }
            _ => {
                page_dict.set("Contents", stream_id);
            }
        }
    }

    doc.save(destination).map_err(|e| WriterError::SaveFailed {
        path: destination.to_path_buf(),
        source: lopdf::Error::IO(e),
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn write_single_overlay_adds_font_resource() {
        use crate::overlay::{PdfPosition, Standard14Font, TextOverlay};

        let src = NamedTempFile::new().expect("failed to create temp file");
        create_test_pdf(src.path());

        let dst = NamedTempFile::new().expect("failed to create temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: Standard14Font::Helvetica,
            font_size: 12.0,
        };

        write_overlays(src.path(), dst.path(), &[overlay]).expect("write_overlays failed");

        let doc = Document::load(dst.path()).expect("failed to re-open output PDF");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1 not found");

        let font_names = collect_page_font_names(&doc, page_id);
        assert!(
            font_names.iter().any(|n| n == "Helvetica"),
            "expected Helvetica in font resources, got: {font_names:?}"
        );
    }

    #[test]
    fn write_single_overlay_adds_content_stream() {
        use crate::overlay::{PdfPosition, Standard14Font, TextOverlay};

        let src = NamedTempFile::new().expect("failed to create temp file");
        create_test_pdf(src.path());

        let dst = NamedTempFile::new().expect("failed to create temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: Standard14Font::Helvetica,
            font_size: 12.0,
        };

        write_overlays(src.path(), dst.path(), &[overlay]).expect("write_overlays failed");

        let doc = Document::load(dst.path()).expect("failed to re-open output PDF");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1 not found");

        let ops = collect_page_operations(&doc, page_id);
        let op_names: Vec<&str> = ops.iter().map(|o| o.operator.as_str()).collect();

        // The overlay stream must contain BT / Tf / Td / Tj / ET.
        assert!(op_names.contains(&"BT"), "missing BT in ops: {op_names:?}");
        assert!(op_names.contains(&"Tf"), "missing Tf in ops: {op_names:?}");
        assert!(op_names.contains(&"Td"), "missing Td in ops: {op_names:?}");
        assert!(op_names.contains(&"Tj"), "missing Tj in ops: {op_names:?}");
        assert!(op_names.contains(&"ET"), "missing ET in ops: {op_names:?}");

        // Verify one of the Tj operands contains our overlay text "Hello".
        let hello_bytes = b"Hello".to_vec();
        let has_hello = ops.iter().any(|o| {
            o.operator == "Tj"
                && matches!(&o.operands[0], Object::String(b, _) if b == &hello_bytes)
        });
        assert!(has_hello, "no Tj with text 'Hello' found in ops: {ops:?}");

        // Find the Td immediately before the Tj containing "Hello" and verify its coordinates.
        let ops_slice = ops.as_slice();
        let td_op = ops_slice
            .windows(2)
            .find(|w| {
                w[0].operator == "Td"
                    && w[1].operator == "Tj"
                    && matches!(&w[1].operands[0], Object::String(b, _) if b == &hello_bytes)
            })
            .map(|w| &w[0])
            .expect("Td before Hello Tj not found");

        let x = match &td_op.operands[0] {
            Object::Real(v) => *v as f64,
            Object::Integer(v) => *v as f64,
            other => panic!("expected numeric x in Td, got {other:?}"),
        };
        let y = match &td_op.operands[1] {
            Object::Real(v) => *v as f64,
            Object::Integer(v) => *v as f64,
            other => panic!("expected numeric y in Td, got {other:?}"),
        };
        assert!((x - 72.0_f64).abs() < 0.01, "Td x mismatch: {x}");
        assert!((y - 720.0_f64).abs() < 0.01, "Td y mismatch: {y}");
    }

    // --- Test helpers ---

    /// Collects all BaseFont names reachable from the font resources of `page_id`.
    fn collect_page_font_names(doc: &Document, page_id: lopdf::ObjectId) -> Vec<String> {
        let Ok(fonts) = doc.get_page_fonts(page_id) else {
            return vec![];
        };
        fonts
            .values()
            .filter_map(|fd| {
                if let Ok(Object::Name(base)) = fd.get(b"BaseFont") {
                    std::str::from_utf8(base).ok().map(str::to_string)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Decodes all content streams for `page_id` and returns the flattened list of operations.
    fn collect_page_operations(doc: &Document, page_id: lopdf::ObjectId) -> Vec<Operation> {
        let content_ids = doc.get_page_contents(page_id);
        let mut ops = Vec::new();
        for id in content_ids {
            let Ok(stream_obj) = doc.get_object(id) else {
                continue;
            };
            let Ok(stream) = stream_obj.as_stream() else {
                continue;
            };
            let Ok(content) = stream.decode_content() else {
                continue;
            };
            ops.extend(content.operations);
        }
        ops
    }
}
