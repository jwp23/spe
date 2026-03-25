// Integration tests for PDF text overlay writing via lopdf.
//
// Note: tests marked #[ignore] require external system utilities (pdftoppm).
// Run with `cargo test -- --ignored` in CI.

use std::path::Path;

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use spe::fonts::{FontEntry, FontRegistry, PdfEmbedding, WidthTable};
use spe::overlay::{PdfPosition, TextOverlay};
use spe::pdf::writer::write_overlays;
use tempfile::NamedTempFile;

/// Builds a minimal single-page PDF and saves it to `path`.
/// This is also used to create the test fixture at `tests/fixtures/single-page.pdf`.
pub fn create_test_pdf(path: &Path) {
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
fn write_and_read_back_overlay() {
    let registry = FontRegistry::new();
    let src = NamedTempFile::new().expect("temp file");
    create_test_pdf(src.path());

    let dst = NamedTempFile::new().expect("temp file");

    let overlay = TextOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 500.0 },
        text: "Integration test overlay".to_string(),
        font: registry.find_by_name("Courier Bold").unwrap(),
        font_size: 16.0,
        width: None,
    };

    write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write_overlays failed");

    // Read back and verify.
    let doc = Document::load(dst.path()).expect("failed to load output PDF");
    let pages = doc.get_pages();
    assert_eq!(pages.len(), 1, "page count must be unchanged");

    let &page_id = pages.get(&1).expect("page 1");

    // Verify Courier-Bold font resource exists.
    let fonts = doc.get_page_fonts(page_id).expect("get_page_fonts failed");
    let has_courier_bold = fonts
        .values()
        .any(|fd| matches!(fd.get(b"BaseFont"), Ok(Object::Name(n)) if n == b"Courier-Bold"));
    assert!(has_courier_bold, "Courier-Bold font resource not found");

    // Verify the overlay text appears in content streams.
    let content_ids = doc.get_page_contents(page_id);
    let mut found_text = false;
    let target = b"Integration test overlay".to_vec();
    for id in content_ids {
        let Ok(obj) = doc.get_object(id) else {
            continue;
        };
        let Ok(stream) = obj.as_stream() else {
            continue;
        };
        let Ok(content) = stream.decode_content() else {
            continue;
        };
        for op in &content.operations {
            if op.operator == "Tj" {
                if matches!(&op.operands[0], Object::String(b, _) if *b == target) {
                    found_text = true;
                }
            }
        }
    }
    assert!(found_text, "overlay text not found in content streams");
}

#[test]
fn write_multiple_overlays_across_pages() {
    // Build a 2-page PDF.
    let src = NamedTempFile::new().expect("temp file");
    {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        let mut page_ids = vec![];
        for _ in 0..2 {
            let content = Content {
                operations: vec![Operation::new("BT", vec![]), Operation::new("ET", vec![])],
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
            page_ids.push(page_id);
        }

        let kids: Vec<Object> = page_ids.iter().map(|&id| Object::Reference(id)).collect();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => 2_i64,
                "Resources" => resources_id,
            }),
        );

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc.save(src.path()).expect("save");
    }

    let dst = NamedTempFile::new().expect("temp file");

    let registry = FontRegistry::new();
    let overlays = vec![
        TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 700.0 },
            text: "Page one text".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
        },
        TextOverlay {
            page: 2,
            position: PdfPosition { x: 72.0, y: 700.0 },
            text: "Page two text".to_string(),
            font: registry.find_by_name("Times Roman").unwrap(),
            font_size: 14.0,
            width: None,
        },
    ];

    write_overlays(src.path(), dst.path(), &overlays, &registry).expect("write_overlays failed");

    let doc = Document::load(dst.path()).expect("load output");
    let pages = doc.get_pages();
    assert_eq!(pages.len(), 2);

    // Verify each page has its overlay text.
    for (page_num, expected_text) in [(1u32, b"Page one text"), (2u32, b"Page two text")] {
        let &page_id = pages.get(&page_num).expect("page");
        let content_ids = doc.get_page_contents(page_id);
        let mut found = false;
        for id in content_ids {
            let Ok(obj) = doc.get_object(id) else {
                continue;
            };
            let Ok(stream) = obj.as_stream() else {
                continue;
            };
            let Ok(content) = stream.decode_content() else {
                continue;
            };
            for op in &content.operations {
                if op.operator == "Tj" {
                    if matches!(&op.operands[0], Object::String(b, _) if b == expected_text) {
                        found = true;
                    }
                }
            }
        }
        assert!(
            found,
            "overlay text {:?} not found on page {page_num}",
            std::str::from_utf8(expected_text).unwrap()
        );
    }
}

#[test]
fn write_and_read_back_multiline_overlay() {
    let registry = FontRegistry::new();
    let src = NamedTempFile::new().expect("temp file");
    create_test_pdf(src.path());

    let dst = NamedTempFile::new().expect("temp file");

    let overlay = TextOverlay {
        page: 1,
        position: PdfPosition { x: 72.0, y: 720.0 },
        text: "First line\nSecond line\nThird line".to_string(),
        font: registry.default_font(),
        font_size: 12.0,
        width: Some(300.0),
    };

    write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write_overlays failed");

    let doc = Document::load(dst.path()).expect("load output");
    let pages = doc.get_pages();
    assert_eq!(pages.len(), 1);

    let &page_id = pages.get(&1).expect("page 1");

    // The overlay stream is the last content stream added.
    let content_ids = doc.get_page_contents(page_id);
    let overlay_stream_id = *content_ids.last().expect("no content streams");
    let stream_obj = doc.get_object(overlay_stream_id).expect("stream obj");
    let stream = stream_obj.as_stream().expect("stream");
    let content = stream.decode_content().expect("decode");
    let ops = &content.operations;

    // 3 lines → 3 Tj operators.
    let tj_count = ops.iter().filter(|o| o.operator == "Tj").count();
    assert_eq!(tj_count, 3, "expected 3 Tj ops, got {tj_count}");

    // 3 lines → 3 Td operators (first absolute, next two relative with leading).
    let td_ops: Vec<&Operation> = ops.iter().filter(|o| o.operator == "Td").collect();
    assert_eq!(td_ops.len(), 3, "expected 3 Td ops, got {}", td_ops.len());

    // First Td: absolute position (72, 720).
    let first_x = match &td_ops[0].operands[0] {
        Object::Real(v) => *v as f64,
        Object::Integer(v) => *v as f64,
        other => panic!("unexpected type: {other:?}"),
    };
    let first_y = match &td_ops[0].operands[1] {
        Object::Real(v) => *v as f64,
        Object::Integer(v) => *v as f64,
        other => panic!("unexpected type: {other:?}"),
    };
    assert!((first_x - 72.0).abs() < 0.01, "first Td x={first_x}");
    assert!((first_y - 720.0).abs() < 0.01, "first Td y={first_y}");

    // Subsequent Td: relative offset (0, -leading) where leading = font_size * 1.2.
    let expected_leading = 12.0_f64 * 1.2;
    for (i, td) in td_ops[1..].iter().enumerate() {
        let rx = match &td.operands[0] {
            Object::Real(v) => *v as f64,
            Object::Integer(v) => *v as f64,
            other => panic!("unexpected type: {other:?}"),
        };
        let ry = match &td.operands[1] {
            Object::Real(v) => *v as f64,
            Object::Integer(v) => *v as f64,
            other => panic!("unexpected type: {other:?}"),
        };
        assert!(rx.abs() < 0.01, "Td[{i}] x should be 0, got {rx}");
        assert!(
            (ry - (-expected_leading)).abs() < 0.01,
            "Td[{i}] y should be -{expected_leading}, got {ry}"
        );
    }

    // All three line texts must appear in Tj operators.
    for expected in ["First line", "Second line", "Third line"] {
        let found = ops.iter().any(|o| {
            o.operator == "Tj"
                && matches!(&o.operands[0], Object::String(b, _) if b == expected.as_bytes())
        });
        assert!(found, "line {:?} not found in Tj operators", expected);
    }
}

#[test]
fn write_multiline_word_wrap_breaks_at_width_boundary() {
    let registry = FontRegistry::new();
    // Use Courier (monospaced, 600 units/char) at 12pt so each char = 7.2pt.
    // "AAAA BBBB" = needs ~72pt. At width=40pt "AAAA" fits (28.8pt), "BBBB" wraps.
    let src = NamedTempFile::new().expect("temp file");
    create_test_pdf(src.path());

    let dst = NamedTempFile::new().expect("temp file");

    let overlay = TextOverlay {
        page: 1,
        position: PdfPosition { x: 72.0, y: 720.0 },
        text: "AAAA BBBB".to_string(),
        font: registry.find_by_name("Courier").unwrap(),
        font_size: 12.0,
        width: Some(40.0),
    };

    write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write failed");

    let doc = Document::load(dst.path()).expect("load output");
    let pages = doc.get_pages();
    let &page_id = pages.get(&1).expect("page 1");

    let content_ids = doc.get_page_contents(page_id);
    let overlay_stream_id = *content_ids.last().expect("no content streams");
    let stream_obj = doc.get_object(overlay_stream_id).expect("stream obj");
    let stream = stream_obj.as_stream().expect("stream");
    let content = stream.decode_content().expect("decode");
    let ops = &content.operations;

    // Word wrap produces 2 lines: "AAAA" and "BBBB" → 2 Tj ops.
    let tj_count = ops.iter().filter(|o| o.operator == "Tj").count();
    assert_eq!(
        tj_count, 2,
        "expected 2 Tj ops after word wrap, got {tj_count}"
    );

    let has_aaaa = ops.iter().any(|o| {
        o.operator == "Tj" && matches!(&o.operands[0], Object::String(b, _) if b == b"AAAA")
    });
    let has_bbbb = ops.iter().any(|o| {
        o.operator == "Tj" && matches!(&o.operands[0], Object::String(b, _) if b == b"BBBB")
    });
    assert!(has_aaaa, "expected 'AAAA' in Tj ops");
    assert!(has_bbbb, "expected 'BBBB' in Tj ops");
}

#[test]
fn generate_fixture_single_page_pdf() {
    // This test generates the fixture PDF used by screenshot and other tests.
    // It creates a blank US Letter page (612x792 points) with no content.
    let fixtures_dir = std::path::PathBuf::from("tests/fixtures");
    std::fs::create_dir_all(&fixtures_dir).expect("failed to create fixtures directory");

    let fixture_path = fixtures_dir.join("single-page.pdf");
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

    // Empty content stream (no visible content)
    let content = Content { operations: vec![] };

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

    doc.save(&fixture_path).expect("failed to save fixture PDF");

    // Verify the fixture was created and is loadable.
    let loaded = Document::load(&fixture_path).expect("failed to load fixture PDF");
    let pages = loaded.get_pages();
    assert_eq!(pages.len(), 1, "fixture PDF must have exactly 1 page");
}

/// Static TTF bytes for testing TrueType embedding.
const TEST_TTF_BYTES: &[u8] = include_bytes!("../assets/icons/phosphor-subset.ttf");

/// Build a FontRegistry with a TrueType test font added alongside the Standard 14.
fn registry_with_truetype_font() -> (FontRegistry, spe::fonts::FontId) {
    let mut registry = FontRegistry::new();
    let id = registry.add_entry(FontEntry {
        id: spe::fonts::FontId::default(),
        display_name: "TestTrueType",
        pdf_name: "TestTrueType",
        iced_font: iced::Font::DEFAULT,
        embedding: PdfEmbedding::TrueType {
            bytes: TEST_TTF_BYTES,
        },
        widths: WidthTable::Monospaced(600.0),
    });
    (registry, id)
}

#[test]
fn write_truetype_overlay_embeds_font_program() {
    let (registry, tt_font_id) = registry_with_truetype_font();

    let src = NamedTempFile::new().expect("temp file");
    create_test_pdf(src.path());
    let dst = NamedTempFile::new().expect("temp file");

    let overlay = TextOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 500.0 },
        text: "TrueType test".to_string(),
        font: tt_font_id,
        font_size: 14.0,
        width: None,
    };

    write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write_overlays failed");

    let doc = Document::load(dst.path()).expect("failed to load output PDF");
    let pages = doc.get_pages();
    let &page_id = pages.get(&1).expect("page 1");

    // Find the TrueType font object among page fonts.
    let fonts = doc.get_page_fonts(page_id).expect("get_page_fonts failed");
    let tt_font_dict = fonts
        .values()
        .find(|fd| matches!(fd.get(b"BaseFont"), Ok(Object::Name(n)) if n == b"TestTrueType"))
        .expect("TestTrueType font resource not found in page fonts");

    // Verify Subtype is TrueType.
    let subtype = tt_font_dict.get(b"Subtype").expect("no Subtype");
    assert_eq!(
        subtype,
        &Object::Name(b"TrueType".to_vec()),
        "expected TrueType subtype, got {subtype:?}"
    );

    // Verify Encoding is WinAnsiEncoding.
    let encoding = tt_font_dict.get(b"Encoding").expect("no Encoding");
    assert_eq!(
        encoding,
        &Object::Name(b"WinAnsiEncoding".to_vec()),
        "expected WinAnsiEncoding, got {encoding:?}"
    );

    // Verify FirstChar and LastChar.
    let first_char = tt_font_dict.get(b"FirstChar").expect("no FirstChar");
    assert_eq!(first_char, &Object::Integer(32));
    let last_char = tt_font_dict.get(b"LastChar").expect("no LastChar");
    assert_eq!(last_char, &Object::Integer(255));

    // Verify Widths array exists and has the correct length.
    let widths_obj = tt_font_dict.get(b"Widths").expect("no Widths");
    let widths_arr = match widths_obj {
        Object::Array(arr) => arr,
        other => panic!("expected Widths array, got {other:?}"),
    };
    assert_eq!(
        widths_arr.len(),
        224, // 255 - 32 + 1
        "Widths array should have 224 entries (chars 32-255)"
    );

    // Verify FontDescriptor reference exists and points to a valid object.
    let descriptor_ref = tt_font_dict
        .get(b"FontDescriptor")
        .expect("no FontDescriptor");
    let descriptor_id = match descriptor_ref {
        Object::Reference(id) => *id,
        other => panic!("expected FontDescriptor reference, got {other:?}"),
    };
    let descriptor_obj = doc
        .get_object(descriptor_id)
        .expect("FontDescriptor object not found");
    let descriptor = descriptor_obj
        .as_dict()
        .expect("FontDescriptor should be a dictionary");

    // Verify FontDescriptor has required keys.
    assert_eq!(
        descriptor.get(b"Type").expect("no Type"),
        &Object::Name(b"FontDescriptor".to_vec())
    );
    assert_eq!(
        descriptor.get(b"FontName").expect("no FontName"),
        &Object::Name(b"TestTrueType".to_vec())
    );

    // Verify FontFile2 reference exists and contains the original TTF bytes.
    let font_file_ref = descriptor.get(b"FontFile2").expect("no FontFile2");
    let font_file_id = match font_file_ref {
        Object::Reference(id) => *id,
        other => panic!("expected FontFile2 reference, got {other:?}"),
    };
    let font_file_obj = doc
        .get_object(font_file_id)
        .expect("FontFile2 object not found");
    let font_file_stream = font_file_obj
        .as_stream()
        .expect("FontFile2 should be a stream");
    assert_eq!(
        font_file_stream.content, TEST_TTF_BYTES,
        "FontFile2 stream content must match the original TTF bytes"
    );

    // Verify Length1 in the stream dictionary matches TTF byte length.
    let length1 = font_file_stream
        .dict
        .get(b"Length1")
        .expect("no Length1 in FontFile2 stream dict");
    assert_eq!(
        length1,
        &Object::Integer(TEST_TTF_BYTES.len() as i64),
        "Length1 must equal the TTF byte length"
    );
}

#[test]
fn write_builtin_overlay_still_creates_type1_font() {
    // Regression test: BuiltIn fonts must still produce Type1 font objects.
    let registry = FontRegistry::new();

    let src = NamedTempFile::new().expect("temp file");
    create_test_pdf(src.path());
    let dst = NamedTempFile::new().expect("temp file");

    // Use Courier (a Standard 14 BuiltIn font) — the test PDF already has Helvetica
    // under "F1", so Courier will require a new font object.
    let courier_id = registry.find_by_name("Courier").unwrap();
    let overlay = TextOverlay {
        page: 1,
        position: PdfPosition { x: 100.0, y: 500.0 },
        text: "BuiltIn test".to_string(),
        font: courier_id,
        font_size: 12.0,
        width: None,
    };

    write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write_overlays failed");

    let doc = Document::load(dst.path()).expect("failed to load output PDF");
    let pages = doc.get_pages();
    let &page_id = pages.get(&1).expect("page 1");

    let fonts = doc.get_page_fonts(page_id).expect("get_page_fonts failed");
    let courier_dict = fonts
        .values()
        .find(|fd| matches!(fd.get(b"BaseFont"), Ok(Object::Name(n)) if n == b"Courier"))
        .expect("Courier font resource not found");

    // BuiltIn fonts must be Type1, not TrueType.
    let subtype = courier_dict.get(b"Subtype").expect("no Subtype");
    assert_eq!(
        subtype,
        &Object::Name(b"Type1".to_vec()),
        "BuiltIn font should have Type1 subtype, got {subtype:?}"
    );

    // BuiltIn fonts must NOT have a FontDescriptor (they're Standard 14).
    assert!(
        courier_dict.get(b"FontDescriptor").is_err(),
        "BuiltIn Type1 fonts should not have a FontDescriptor"
    );
}
