// Reopening app-saved PDFs: validate /SPEOverlays and strip the app's own
// streams so restored overlays are edited, not shown twice (ADR-015).

use lopdf::{Document, Object};

use super::metadata::{self, OverlayMetadata, StreamFingerprint};
use crate::fonts::FontRegistry;
use crate::overlay::TextOverlay;

/// What opening a PDF found: a plain flat file, a restorable app save, or an
/// app save another tool has edited since (opened flat, with the reason).
pub enum ReopenOutcome {
    NotReEditable(Document),
    Restored(RestoredDocument),
    Stale { document: Document, reason: String },
}

/// A validated app save: the document with the app's streams and metadata
/// stripped back out, plus the overlays to restore into the editor.
pub struct RestoredDocument {
    pub document: Document,
    pub overlays: Vec<TextOverlay>,
    pub missing_fonts: Vec<String>,
}

/// Cap on the /SPEOverlays payload we'll parse: the metadata is small
/// structured JSON (positions, text, font names), so anything past this is
/// either corrupt or hostile, not a legitimate save from this app.
const MAX_METADATA_BYTES: usize = 10 * 1024 * 1024;

/// The catalog's /SPEOverlays JSON, or None when the entry is absent, not a
/// stream (a plain PDF), or too large to plausibly be ours.
///
/// WHAT: reads `stream.content` directly, with no filter decoding.
/// WHY: this app's writer always stores the metadata stream uncompressed, so
/// raw bytes are the JSON text as written. A tool that recompresses this
/// stream would make it unreadable here, which demotes the file to
/// `NotReEditable` rather than misreading it — an accepted trade-off per
/// ADR-015 (edits by other tools are already out of scope for re-editing).
fn metadata_json(doc: &Document) -> Option<String> {
    let root_id = match doc.trailer.get(b"Root").ok()? {
        Object::Reference(id) => *id,
        _ => return None,
    };
    let catalog = doc.get_dictionary(root_id).ok()?;
    let stream = match catalog.get(b"SPEOverlays").ok()? {
        Object::Reference(id) => doc.get_object(*id).ok()?.as_stream().ok()?,
        _ => return None,
    };
    if stream.content.len() > MAX_METADATA_BYTES {
        return None;
    }
    String::from_utf8(stream.content.clone()).ok()
}

/// Check every recorded stream fingerprint against the document, and every
/// overlay's page number against the document's pages, returning the first
/// mismatch as the reason the metadata cannot be trusted.
fn validate_streams(doc: &Document, meta: &OverlayMetadata) -> Result<(), String> {
    let pages = doc.get_pages();

    for record in &meta.overlays {
        if record.page == 0 || !pages.contains_key(&record.page) {
            return Err(format!(
                "an overlay names page {}, which the document does not have",
                record.page
            ));
        }
        if !meta.streams.iter().any(|s| s.page == record.page) {
            return Err(format!(
                "page {} carries an overlay but no recorded stream to strip",
                record.page
            ));
        }
    }

    let mut seen_pages = std::collections::HashSet::new();
    for record in &meta.streams {
        if !meta
            .overlays
            .iter()
            .any(|overlay| overlay.page == record.page)
        {
            return Err(format!(
                "page {} has a recorded stream but no overlay to restore",
                record.page
            ));
        }
        if !seen_pages.insert(record.page) {
            return Err(format!(
                "metadata names page {} more than once",
                record.page
            ));
        }
        let Some(&page_id) = pages.get(&record.page) else {
            return Err(format!(
                "metadata names page {}, which the document does not have",
                record.page
            ));
        };
        let content_ids = doc.get_page_contents(page_id);
        let stream_bytes = |id: lopdf::ObjectId| -> Option<Vec<u8>> {
            Some(doc.get_object(id).ok()?.as_stream().ok()?.content.clone())
        };
        let overlay_ok = content_ids
            .last()
            .and_then(|id| stream_bytes(*id))
            .is_some_and(|bytes| StreamFingerprint::of(&bytes) == record.overlay);
        if !overlay_ok {
            return Err(format!(
                "page {}'s overlay stream was changed by another tool",
                record.page
            ));
        }
        if let Some(prefix) = &record.prefix {
            let prefix_ok = content_ids
                .first()
                .and_then(|id| stream_bytes(*id))
                .is_some_and(|bytes| StreamFingerprint::of(&bytes) == *prefix);
            if !prefix_ok {
                return Err(format!(
                    "page {}'s content was changed by another tool",
                    record.page
                ));
            }
        }
    }
    Ok(())
}

/// Remove the app's overlay and prefix streams and the metadata entry,
/// leaving the document as it was before the app's save baked onto it.
/// Only called after `validate_streams` proved the layout matches; still
/// length-guards each removal rather than trusting that shape, so a page
/// whose content array doesn't hold as many streams as recorded is left
/// untouched instead of panicking.
fn strip_app_streams(doc: &mut Document, meta: &OverlayMetadata) {
    let pages = doc.get_pages();
    for record in &meta.streams {
        let Some(&page_id) = pages.get(&record.page) else {
            continue;
        };
        let content_ids = doc.get_page_contents(page_id);
        let expected_min = if record.prefix.is_some() { 2 } else { 1 };
        if content_ids.len() < expected_min {
            continue;
        }
        let mut remaining = content_ids;
        let overlay_id = remaining.pop().expect("length checked above");
        let prefix_id = record.prefix.is_some().then(|| remaining.remove(0));

        doc.objects.remove(&overlay_id);
        if let Some(prefix_id) = prefix_id {
            doc.objects.remove(&prefix_id);
        }
        let contents = match remaining.len() {
            1 => Object::Reference(remaining[0]),
            _ => Object::Array(remaining.iter().map(|id| Object::Reference(*id)).collect()),
        };
        let page_dict = doc
            .get_object_mut(page_id)
            .expect("page object must exist")
            .as_dict_mut()
            .expect("page object must be a dictionary");
        page_dict.set("Contents", contents);
    }

    if let Ok(Object::Reference(root_id)) = doc.trailer.get(b"Root").cloned()
        && let Ok(catalog) = doc
            .get_object_mut(root_id)
            .and_then(|obj| obj.as_dict_mut())
    {
        if let Ok(Object::Reference(metadata_id)) = catalog.get(b"SPEOverlays").cloned() {
            catalog.remove(b"SPEOverlays");
            doc.objects.remove(&metadata_id);
        } else {
            catalog.remove(b"SPEOverlays");
        }
    }
}

/// Classify a just-loaded document and, for a valid app save, hand back the
/// stripped document and the overlays to restore.
pub fn reopen(mut doc: Document, registry: &FontRegistry) -> ReopenOutcome {
    let Some(json) = metadata_json(&doc) else {
        return ReopenOutcome::NotReEditable(doc);
    };
    let meta = match metadata::from_json(&json) {
        Ok(meta) => meta,
        Err(e) => {
            return ReopenOutcome::Stale {
                document: doc,
                reason: e.to_string(),
            };
        }
    };
    if let Err(reason) = validate_streams(&doc, &meta) {
        return ReopenOutcome::Stale {
            document: doc,
            reason,
        };
    }
    strip_app_streams(&mut doc, &meta);
    let restored = metadata::resolve_overlays(&meta, registry);
    ReopenOutcome::Restored(RestoredDocument {
        document: doc,
        overlays: restored.overlays,
        missing_fonts: restored.missing_fonts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::FontRegistry;
    use crate::overlay::{PdfPosition, TextOverlay};
    use lopdf::content::{Content, Operation};
    use lopdf::{Document, Object, Stream, dictionary};
    use tempfile::NamedTempFile;

    fn create_test_pdf(path: &std::path::Path) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![72.into(), 720.into()]),
                Operation::new(
                    "Tj",
                    vec![Object::String(
                        b"Original".to_vec(),
                        lopdf::StringFormat::Literal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            content.encode().expect("encode"),
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
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        doc.save(path).expect("save test PDF");
    }

    /// A 3-page fixture PDF, each page with its own original content stream.
    fn create_multi_page_test_pdf(path: &std::path::Path) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let mut kids = Vec::new();
        for page_text in ["Page one", "Page two", "Page three"] {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 12.into()]),
                    Operation::new("Td", vec![72.into(), 720.into()]),
                    Operation::new(
                        "Tj",
                        vec![Object::String(
                            page_text.as_bytes().to_vec(),
                            lopdf::StringFormat::Literal,
                        )],
                    ),
                    Operation::new("ET", vec![]),
                ],
            };
            let content_id = doc.add_object(Stream::new(
                dictionary! {},
                content.encode().expect("encode"),
            ));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            });
            kids.push(Object::Reference(page_id));
        }
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => kids.clone(),
            "Count" => kids.len() as i64,
            "Resources" => resources_id,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        doc.save(path).expect("save test PDF");
    }

    /// A saved re-editable PDF and the overlays baked into it.
    fn saved_reeditable() -> (NamedTempFile, Vec<TextOverlay>, FontRegistry) {
        let registry = FontRegistry::new();
        let src = NamedTempFile::new().expect("temp file");
        create_test_pdf(src.path());
        let dst = NamedTempFile::new().expect("temp file");
        let overlays = vec![TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 650.0 },
            text: "Overlaid".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: Some(200.0),
            min_height: None,
        }];
        crate::pdf::writer::write_overlays(src.path(), dst.path(), &overlays, &registry)
            .expect("write failed");
        (dst, overlays, registry)
    }

    #[test]
    fn plain_pdf_is_not_re_editable() {
        let registry = FontRegistry::new();
        let file = NamedTempFile::new().expect("temp file");
        create_test_pdf(file.path());
        let doc = Document::load(file.path()).expect("load");
        assert!(matches!(
            reopen(doc, &registry),
            ReopenOutcome::NotReEditable(_)
        ));
    }

    #[test]
    fn saved_file_restores_overlays_and_strips_app_streams() {
        let (file, overlays, registry) = saved_reeditable();
        let doc = Document::load(file.path()).expect("load");
        let ReopenOutcome::Restored(restored) = reopen(doc, &registry) else {
            panic!("expected Restored");
        };
        assert_eq!(restored.overlays, overlays);
        assert!(restored.missing_fonts.is_empty());

        // Stripped doc must be back to the single original content stream and
        // carry no /SPEOverlays entry.
        let page_id = *restored.document.get_pages().get(&1).expect("page 1");
        let content_ids = restored.document.get_page_contents(page_id);
        assert_eq!(content_ids.len(), 1, "prefix and overlay streams stripped");
        let root_id = match restored.document.trailer.get(b"Root").expect("Root") {
            Object::Reference(id) => *id,
            other => panic!("Root must be a reference, got {other:?}"),
        };
        let catalog = restored.document.get_dictionary(root_id).expect("catalog");
        assert!(
            catalog.get(b"SPEOverlays").is_err(),
            "metadata entry removed"
        );
    }

    #[test]
    fn stripped_document_round_trips_through_save() {
        // The stripped doc must itself be saveable and re-bakeable: this is
        // the file the app renders from and re-saves onto.
        let (file, _, registry) = saved_reeditable();
        let doc = Document::load(file.path()).expect("load");
        let ReopenOutcome::Restored(mut restored) = reopen(doc, &registry) else {
            panic!("expected Restored");
        };
        let stripped = NamedTempFile::new().expect("temp file");
        restored
            .document
            .save(stripped.path())
            .expect("save stripped");
        let reloaded = Document::load(stripped.path()).expect("stripped must reload");
        assert_eq!(reloaded.get_pages().len(), 1);
    }

    #[test]
    fn tampered_overlay_stream_opens_flat_with_reason() {
        let (file, _, registry) = saved_reeditable();
        let mut doc = Document::load(file.path()).expect("load");

        // Tamper: alter the overlay stream's bytes (another tool edited it).
        let page_id = *doc.get_pages().get(&1).expect("page 1");
        let overlay_stream_id = *doc.get_page_contents(page_id).last().expect("streams");
        let stream = doc
            .get_object_mut(overlay_stream_id)
            .expect("stream obj")
            .as_stream_mut()
            .expect("stream");
        let mut tampered = stream.content.clone();
        tampered.extend_from_slice(b"\n% edited elsewhere\n");
        stream.set_content(tampered);

        let ReopenOutcome::Stale { reason, .. } = reopen(doc, &registry) else {
            panic!("expected Stale");
        };
        assert!(
            reason.contains("page 1"),
            "reason should name the mismatched page, got: {reason}"
        );
    }

    /// The document's /SPEOverlays stream, for tests that rewrite its bytes.
    fn metadata_stream_mut(doc: &mut Document) -> &mut lopdf::Stream {
        let root_id = match doc.trailer.get(b"Root").expect("Root") {
            Object::Reference(id) => *id,
            other => panic!("Root must be a reference, got {other:?}"),
        };
        let metadata_id = match doc
            .get_dictionary(root_id)
            .expect("catalog")
            .get(b"SPEOverlays")
            .expect("entry")
        {
            Object::Reference(id) => *id,
            other => panic!("expected reference, got {other:?}"),
        };
        doc.get_object_mut(metadata_id)
            .expect("metadata obj")
            .as_stream_mut()
            .expect("stream")
    }

    /// Load the metadata stream, hand its parsed form to `edit`, and write
    /// the edited JSON back onto the stream. Shared by tests that tamper
    /// with metadata contents rather than page content bytes.
    fn tamper_metadata(doc: &mut Document, edit: impl FnOnce(&mut OverlayMetadata)) {
        let stream = metadata_stream_mut(doc);
        let json = String::from_utf8(stream.content.clone()).expect("utf8");
        let mut meta: OverlayMetadata = serde_json::from_str(&json).expect("parse metadata");
        edit(&mut meta);
        let tampered = serde_json::to_string(&meta).expect("serialize metadata");
        stream.set_content(tampered.into_bytes());
    }

    #[test]
    fn duplicate_page_stream_record_opens_flat_without_panic() {
        let (file, _, registry) = saved_reeditable();
        let mut doc = Document::load(file.path()).expect("load");
        tamper_metadata(&mut doc, |meta| {
            let dup = meta.streams[0].clone();
            meta.streams.push(dup);
        });

        let page_id = *doc.get_pages().get(&1).expect("page 1");
        let original_content_ids = doc.get_page_contents(page_id);

        let ReopenOutcome::Stale { document, reason } = reopen(doc, &registry) else {
            panic!("expected Stale, not a panic");
        };
        assert!(
            reason.contains("page 1"),
            "reason should name the duplicated page, got: {reason}"
        );
        let page_id = *document.get_pages().get(&1).expect("page 1");
        assert_eq!(
            document.get_page_contents(page_id),
            original_content_ids,
            "document's streams must be unchanged"
        );
    }

    #[test]
    fn overlay_naming_an_out_of_range_page_opens_flat() {
        let (file, _, registry) = saved_reeditable();
        let mut doc = Document::load(file.path()).expect("load");
        tamper_metadata(&mut doc, |meta| {
            meta.overlays[0].page = 9999;
            meta.streams.clear();
        });

        let ReopenOutcome::Stale { reason, .. } = reopen(doc, &registry) else {
            panic!("expected Stale");
        };
        assert!(
            reason.contains("9999"),
            "reason should name the out-of-range page, got: {reason}"
        );
    }

    #[test]
    fn stream_record_naming_a_page_with_no_overlay_opens_flat() {
        let (file, _, registry) = saved_reeditable();
        let mut doc = Document::load(file.path()).expect("load");
        tamper_metadata(&mut doc, |meta| {
            // Drop the overlay but keep its still-valid-fingerprint stream
            // record: strip_app_streams must not be trusted to remove page
            // content that no overlay will restore.
            meta.overlays.clear();
        });

        let ReopenOutcome::Stale { reason, .. } = reopen(doc, &registry) else {
            panic!("expected Stale");
        };
        assert!(
            reason.contains("page 1"),
            "reason should name the orphaned stream's page, got: {reason}"
        );
    }

    #[test]
    fn oversized_metadata_payload_is_not_re_editable() {
        let (file, _, registry) = saved_reeditable();
        let mut doc = Document::load(file.path()).expect("load");
        metadata_stream_mut(&mut doc).set_content(vec![b'0'; MAX_METADATA_BYTES + 1]);

        assert!(matches!(
            reopen(doc, &registry),
            ReopenOutcome::NotReEditable(_)
        ));
    }

    #[test]
    fn metadata_naming_a_missing_page_opens_flat() {
        let (file, _, registry) = saved_reeditable();
        let mut doc = Document::load(file.path()).expect("load");
        // Rewrite the metadata to reference page 7, which does not exist.
        let stream = metadata_stream_mut(&mut doc);
        let json = String::from_utf8(stream.content.clone()).expect("utf8");
        stream.set_content(json.replace(r#""page":1"#, r#""page":7"#).into_bytes());

        assert!(matches!(
            reopen(doc, &registry),
            ReopenOutcome::Stale { .. }
        ));
    }

    #[test]
    fn metadata_cap_is_ten_megabytes() {
        // States the size independently of MAX_METADATA_BYTES. The tests
        // around the cap size their payloads from that constant, so on its own
        // a change to it moves the guard and those payloads together and no
        // assertion notices.
        assert_eq!(MAX_METADATA_BYTES, 10_485_760);
    }

    #[test]
    fn metadata_payload_exactly_at_max_size_is_still_accepted() {
        let (file, overlays, registry) = saved_reeditable();
        let mut doc = Document::load(file.path()).expect("load");
        {
            let stream = metadata_stream_mut(&mut doc);
            let mut content = stream.content.clone();
            assert!(
                content.len() <= MAX_METADATA_BYTES,
                "fixture metadata should start under the cap"
            );
            content.resize(MAX_METADATA_BYTES, b' ');
            stream.set_content(content);
        }
        assert!(
            metadata_json(&doc).is_some(),
            "a payload exactly at the cap must still parse"
        );
        let ReopenOutcome::Restored(restored) = reopen(doc, &registry) else {
            panic!("expected Restored at exactly MAX_METADATA_BYTES");
        };
        assert_eq!(restored.overlays, overlays);
    }

    #[test]
    fn overlay_naming_a_page_the_document_lacks_is_rejected_by_validate_streams() {
        let file = NamedTempFile::new().expect("temp file");
        create_test_pdf(file.path());
        let doc = Document::load(file.path()).expect("load");
        let meta = OverlayMetadata {
            version: metadata::METADATA_VERSION,
            overlays: vec![metadata::OverlayRecord {
                page: 5,
                x: 0.0,
                y: 0.0,
                text: "x".to_string(),
                font_family: "Helvetica".to_string(),
                font_size: 12.0,
                width: None,
                min_height: None,
            }],
            streams: vec![],
        };

        let err = validate_streams(&doc, &meta).expect_err("page 5 does not exist");
        assert!(
            err.contains("does not have"),
            "expected a page-existence rejection, got: {err}"
        );
        assert!(err.contains('5'), "reason should name the page, got: {err}");
    }

    /// A document with a single page whose Contents holds exactly
    /// `stream_count` dummy content streams, for `strip_app_streams` boundary
    /// tests that don't need real PDF operators.
    fn document_with_page_streams(
        stream_count: usize,
    ) -> (Document, lopdf::ObjectId, Vec<lopdf::ObjectId>) {
        let mut doc = Document::with_version("1.5");
        let content_ids: Vec<lopdf::ObjectId> = (0..stream_count)
            .map(|i| {
                doc.add_object(Stream::new(
                    dictionary! {},
                    format!("stream {i}").into_bytes(),
                ))
            })
            .collect();
        let contents = if content_ids.len() == 1 {
            Object::Reference(content_ids[0])
        } else {
            Object::Array(
                content_ids
                    .iter()
                    .map(|id| Object::Reference(*id))
                    .collect(),
            )
        };
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Contents" => contents,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        });
        if let Some(Object::Dictionary(page_dict)) = doc.objects.get_mut(&page_id) {
            page_dict.set("Parent", pages_id);
        }
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        (doc, page_id, content_ids)
    }

    #[test]
    fn strip_app_streams_with_exactly_prefix_and_overlay_leaves_empty_contents() {
        let (mut doc, page_id, content_ids) = document_with_page_streams(2);
        let meta = OverlayMetadata {
            version: metadata::METADATA_VERSION,
            overlays: vec![],
            streams: vec![metadata::PageStreams {
                page: 1,
                overlay: StreamFingerprint::of(b"overlay"),
                prefix: Some(StreamFingerprint::of(b"prefix")),
            }],
        };

        strip_app_streams(&mut doc, &meta);

        let page_dict = doc
            .get_object(page_id)
            .expect("page object")
            .as_dict()
            .expect("dict");
        assert_eq!(
            page_dict.get(b"Contents").expect("Contents"),
            &Object::Array(vec![]),
            "both app streams removed, no content left"
        );
        for id in &content_ids {
            assert!(
                !doc.objects.contains_key(id),
                "removed stream {id:?} must not remain in the document"
            );
        }
    }

    #[test]
    fn strip_app_streams_with_prefix_original_and_overlay_leaves_a_direct_reference() {
        let (mut doc, page_id, content_ids) = document_with_page_streams(3);
        let original_id = content_ids[1];
        let meta = OverlayMetadata {
            version: metadata::METADATA_VERSION,
            overlays: vec![],
            streams: vec![metadata::PageStreams {
                page: 1,
                overlay: StreamFingerprint::of(b"overlay"),
                prefix: Some(StreamFingerprint::of(b"prefix")),
            }],
        };

        strip_app_streams(&mut doc, &meta);

        let page_dict = doc
            .get_object(page_id)
            .expect("page object")
            .as_dict()
            .expect("dict");
        assert_eq!(
            page_dict.get(b"Contents").expect("Contents"),
            &Object::Reference(original_id),
            "the surviving original stream must be a direct reference, not a 1-element array"
        );
        assert!(doc.objects.contains_key(&original_id));
        assert!(!doc.objects.contains_key(&content_ids[0]));
        assert!(!doc.objects.contains_key(&content_ids[2]));
    }

    #[test]
    fn multi_page_restore_strips_streams_on_both_overlaid_pages() {
        let registry = FontRegistry::new();
        let src = NamedTempFile::new().expect("temp file");
        create_multi_page_test_pdf(src.path());
        let dst = NamedTempFile::new().expect("temp file");
        let overlays = vec![
            TextOverlay {
                page: 1,
                position: PdfPosition { x: 72.0, y: 650.0 },
                text: "Overlay one".to_string(),
                font: registry.default_font(),
                font_size: 12.0,
                width: None,
                min_height: None,
            },
            TextOverlay {
                page: 3,
                position: PdfPosition { x: 72.0, y: 650.0 },
                text: "Overlay three".to_string(),
                font: registry.default_font(),
                font_size: 12.0,
                width: None,
                min_height: None,
            },
        ];
        crate::pdf::writer::write_overlays(src.path(), dst.path(), &overlays, &registry)
            .expect("write failed");

        let doc = Document::load(dst.path()).expect("load");
        let ReopenOutcome::Restored(restored) = reopen(doc, &registry) else {
            panic!("expected Restored");
        };
        assert_eq!(restored.overlays, overlays);

        let pages = restored.document.get_pages();
        for page_num in [1u32, 3u32] {
            let page_id = *pages.get(&page_num).expect("page exists");
            let content_ids = restored.document.get_page_contents(page_id);
            assert_eq!(
                content_ids.len(),
                1,
                "page {page_num}'s overlay stream must be stripped"
            );
        }
        // Page 2 carried no overlay; it must be untouched.
        let page2_id = *pages.get(&2).expect("page 2");
        assert_eq!(restored.document.get_page_contents(page2_id).len(), 1);
    }
}
