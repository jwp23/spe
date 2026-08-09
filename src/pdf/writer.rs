// PDF text overlay writing via lopdf.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use thiserror::Error;

use crate::fonts::{FontDescriptorInfo, FontId, FontRegistry, PdfEmbedding};
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

/// Maps FontIds to PDF resource names, tracking which font objects need to be added to the page.
struct FontMapping {
    resource_names: HashMap<FontId, String>,
    new_font_objects: Vec<(String, lopdf::ObjectId)>,
}

/// Collect unique font IDs from a set of overlays, preserving first-seen order.
fn collect_unique_fonts(overlays: &[&TextOverlay]) -> Vec<FontId> {
    let mut seen = std::collections::HashSet::new();
    overlays
        .iter()
        .filter_map(|o| {
            if seen.insert(o.font) {
                Some(o.font)
            } else {
                None
            }
        })
        .collect()
}

/// Build a mapping from FontId to PDF resource name for a page.
///
/// Reuses existing resource names where the BaseFont already matches the needed font.
/// Creates new font objects (Type1 or TrueType) for fonts not already present on the page.
fn build_font_mapping(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    needed_fonts: &[FontId],
    registry: &FontRegistry,
) -> FontMapping {
    // Build a map from resource name -> BaseFont for the page's existing fonts.
    // Uses lopdf's get_page_fonts which resolves inherited resources from parent nodes.
    let existing: HashMap<Vec<u8>, Vec<u8>> = doc
        .get_page_fonts(page_id)
        .map(|fonts| {
            fonts
                .into_iter()
                .filter_map(|(key, fd)| {
                    if let Ok(Object::Name(base)) = fd.get(b"BaseFont") {
                        Some((key, base.clone()))
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let existing_names: std::collections::HashSet<Vec<u8>> = existing.keys().cloned().collect();
    let mut resource_names: HashMap<FontId, String> = HashMap::new();
    let mut new_font_objects: Vec<(String, lopdf::ObjectId)> = Vec::new();

    for font in needed_fonts {
        let entry = registry.get(*font);
        let base_font_bytes = entry.pdf_name.as_bytes();

        // Check if any existing resource already maps to this BaseFont.
        let reuse_name = existing
            .iter()
            .find(|(_, base)| base.as_slice() == base_font_bytes)
            .map(|(key, _)| String::from_utf8_lossy(key).into_owned());

        if let Some(name) = reuse_name {
            resource_names.insert(*font, name);
        } else {
            // Generate a fresh name, skipping any that already exist.
            let new_name = (0..)
                .map(|i| format!("F_ovl_{i}"))
                .find(|candidate| {
                    !existing_names.contains(candidate.as_bytes())
                        && !new_font_objects.iter().any(|(n, _)| n == candidate)
                })
                .expect("infinite iterator always finds a free name");

            let font_obj_id = match &entry.embedding {
                PdfEmbedding::BuiltIn => doc.add_object(dictionary! {
                    "Type" => "Font",
                    "Subtype" => "Type1",
                    "BaseFont" => Object::Name(base_font_bytes.to_vec()),
                }),
                PdfEmbedding::TrueType { bytes } => {
                    create_truetype_font_object(doc, entry, base_font_bytes, bytes)
                }
            };
            new_font_objects.push((new_name.clone(), font_obj_id));
            resource_names.insert(*font, new_name);
        }
    }

    FontMapping {
        resource_names,
        new_font_objects,
    }
}

/// Create a TrueType font object with embedded font program and descriptor.
fn create_truetype_font_object(
    doc: &mut Document,
    entry: &crate::fonts::FontEntry,
    base_font_bytes: &[u8],
    ttf_bytes: &[u8],
) -> lopdf::ObjectId {
    let font_file_stream = Stream::new(
        dictionary! {
            "Length1" => Object::Integer(ttf_bytes.len() as i64),
        },
        ttf_bytes.to_vec(),
    );
    let font_file_id = doc.add_object(font_file_stream);

    // Use real descriptor values when available; fall back to safe defaults.
    let default_desc = FontDescriptorInfo {
        ascent: 800,
        descent: -200,
        cap_height: 700,
        italic_angle: 0.0,
        flags: 32,
        bbox: [0, 0, 1000, 1000],
        stem_v: 80,
    };
    let desc = entry.descriptor.as_ref().unwrap_or(&default_desc);
    let descriptor = dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(base_font_bytes.to_vec()),
        "Flags" => Object::Integer(desc.flags),
        "FontBBox" => vec![
            Object::Integer(desc.bbox[0]),
            Object::Integer(desc.bbox[1]),
            Object::Integer(desc.bbox[2]),
            Object::Integer(desc.bbox[3]),
        ],
        "ItalicAngle" => Object::Real(desc.italic_angle),
        "Ascent" => Object::Integer(desc.ascent),
        "Descent" => Object::Integer(desc.descent),
        "CapHeight" => Object::Integer(desc.cap_height),
        "StemV" => Object::Integer(desc.stem_v),
        "FontFile2" => Object::Reference(font_file_id),
    };
    let descriptor_id = doc.add_object(descriptor);

    let first_char = 32_i64;
    let last_char = 255_i64;
    let widths: Vec<Object> = (first_char..=last_char)
        .map(|c| {
            let w = entry.widths.char_width(c as u8 as char);
            Object::Integer(w.round() as i64)
        })
        .collect();

    let to_unicode_id = doc.add_object(Stream::new(
        dictionary! {},
        win_ansi_to_unicode_cmap().into_bytes(),
    ));

    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => Object::Name(base_font_bytes.to_vec()),
        "FirstChar" => Object::Integer(first_char),
        "LastChar" => Object::Integer(last_char),
        "Widths" => Object::Array(widths),
        "FontDescriptor" => Object::Reference(descriptor_id),
        "Encoding" => "WinAnsiEncoding",
        "ToUnicode" => Object::Reference(to_unicode_id),
    })
}

/// The 27 WinAnsiEncoding codes in 0x80-0x9F whose Unicode values differ from
/// Latin-1. The remaining codes in that block are undefined and left unmapped.
const WIN_ANSI_HIGH_CONTROL_BLOCK: &[(u8, u32)] = &[
    (0x80, 0x20AC),
    (0x82, 0x201A),
    (0x83, 0x0192),
    (0x84, 0x201E),
    (0x85, 0x2026),
    (0x86, 0x2020),
    (0x87, 0x2021),
    (0x88, 0x02C6),
    (0x89, 0x2030),
    (0x8A, 0x0160),
    (0x8B, 0x2039),
    (0x8C, 0x0152),
    (0x8E, 0x017D),
    (0x91, 0x2018),
    (0x92, 0x2019),
    (0x93, 0x201C),
    (0x94, 0x201D),
    (0x95, 0x2022),
    (0x96, 0x2013),
    (0x97, 0x2014),
    (0x98, 0x02DC),
    (0x99, 0x2122),
    (0x9A, 0x0161),
    (0x9B, 0x203A),
    (0x9C, 0x0153),
    (0x9E, 0x017E),
    (0x9F, 0x0178),
];

/// Build a ToUnicode CMap (PDF 32000-1 §9.10.3) mapping WinAnsiEncoding character
/// codes to Unicode, so readers can extract, copy and search text shown in an
/// embedded TrueType font instead of treating it as unmappable glyphs.
///
/// The ASCII and Latin-1 stretches are emitted as ranges; only WinAnsi's
/// 0x80-0x9F block, which diverges from Latin-1, needs per-code entries.
fn win_ansi_to_unicode_cmap() -> String {
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n\
         /CMapType 2 def\n\
         1 begincodespacerange\n\
         <20> <FF>\n\
         endcodespacerange\n\
         2 beginbfrange\n\
         <20> <7E> <0020>\n\
         <A0> <FF> <00A0>\n\
         endbfrange\n",
    );

    cmap.push_str(&format!(
        "{} beginbfchar\n",
        WIN_ANSI_HIGH_CONTROL_BLOCK.len()
    ));
    for &(code, unicode) in WIN_ANSI_HIGH_CONTROL_BLOCK {
        cmap.push_str(&format!("<{code:02X}> <{unicode:04X}>\n"));
    }
    cmap.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap
}

/// Add new font objects to the page's Resources/Font dictionary.
///
/// Ensures the page has its own Resources dict with a Font sub-dict, then inserts each
/// new font object. Setting Resources directly on the Page overrides the inherited parent
/// dict (PDF spec 7.8.3).
fn install_page_fonts(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    new_font_objects: Vec<(String, lopdf::ObjectId)>,
) {
    if new_font_objects.is_empty() {
        return;
    }

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

    let font_dict = resources
        .get_mut(b"Font")
        .expect("Font just set")
        .as_dict_mut()
        .expect("Font must be a dictionary");

    for (name, obj_id) in new_font_objects {
        font_dict.set(name, obj_id);
    }
}

/// Build PDF content stream operations (BT/Tf/Td/Tj/ET) for a set of overlays.
fn build_overlay_operations(
    page_overlays: &[&TextOverlay],
    font_resource_names: &HashMap<FontId, String>,
    registry: &FontRegistry,
) -> Vec<Operation> {
    let mut operations: Vec<Operation> = Vec::new();
    for overlay in page_overlays {
        let resource_name = font_resource_names
            .get(&overlay.font)
            .expect("all fonts mapped above");
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![
                Object::Name(resource_name.as_bytes().to_vec()),
                Object::Real(overlay.font_size),
            ],
        ));

        let lines = if let Some(width) = overlay.width {
            registry.word_wrap(&overlay.text, overlay.font, overlay.font_size, width)
        } else {
            vec![overlay.text.clone()]
        };

        let leading = overlay.font_size * 1.2;
        for (i, line) in lines.iter().enumerate() {
            if i == 0 {
                operations.push(Operation::new(
                    "Td",
                    vec![
                        Object::Real(overlay.position.x),
                        Object::Real(overlay.position.y),
                    ],
                ));
            } else {
                operations.push(Operation::new(
                    "Td",
                    vec![Object::Real(0.0), Object::Real(-leading)],
                ));
            }
            operations.push(Operation::new(
                "Tj",
                vec![Object::String(
                    line.as_bytes().to_vec(),
                    lopdf::StringFormat::Literal,
                )],
            ));
        }

        operations.push(Operation::new("ET", vec![]));
    }
    operations
}

/// Create a content stream from raw bytes and append it to the page's Contents.
///
/// Existing page content is wrapped in q/Q (a `q` stream before it, `Q` at the start
/// of the overlay stream) so graphics-state changes it leaves active — e.g. the
/// top-down CTM flip Skia/Google Docs emits — cannot affect the overlay. Assumes
/// the original content has balanced q/Q pairs, as the spec requires.
fn embed_content_stream(doc: &mut Document, page_id: lopdf::ObjectId, content_bytes: Vec<u8>) {
    let existing = {
        let page_dict = doc
            .get_object(page_id)
            .expect("page object must exist")
            .as_dict()
            .expect("page object must be a dictionary");
        match page_dict.get(b"Contents") {
            // A Reference may point to a stream or (per spec) to an array of
            // stream references; splice the latter to keep Contents flat.
            Ok(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Array(arr)) => arr.clone(),
                _ => vec![Object::Reference(*id)],
            },
            Ok(Object::Array(arr)) => arr.clone(),
            _ => vec![],
        }
    };

    let contents = if existing.is_empty() {
        let stream_id = doc.add_object(Stream::new(dictionary! {}, content_bytes));
        Object::Reference(stream_id)
    } else {
        let prefix_id = doc.add_object(Stream::new(dictionary! {}, b"q\n".to_vec()));
        // Leading whitespace ensures the `Q` cannot merge with the final token of the
        // preceding stream when readers concatenate array-referenced content streams
        // (e.g. a content stream ending in `f` with no trailing whitespace would
        // otherwise combine with `Q` into the invalid token `fQ`).
        let mut overlay_bytes = b"\nQ\n".to_vec();
        overlay_bytes.extend(content_bytes);
        let stream_id = doc.add_object(Stream::new(dictionary! {}, overlay_bytes));

        let mut arr = vec![Object::Reference(prefix_id)];
        arr.extend(existing);
        arr.push(Object::Reference(stream_id));
        Object::Array(arr)
    };

    let page_dict = doc
        .get_object_mut(page_id)
        .expect("page object must exist")
        .as_dict_mut()
        .expect("page object must be a dictionary");
    page_dict.set("Contents", contents);
}

/// Write `overlays` onto the PDF at `source`, saving the result to `destination`.
pub fn write_overlays(
    source: &Path,
    destination: &Path,
    overlays: &[TextOverlay],
    registry: &FontRegistry,
) -> Result<(), WriterError> {
    let mut doc = Document::load(source).map_err(WriterError::OpenFailed)?;

    if overlays.is_empty() {
        // Save must always produce a file: write a plain copy of the source
        // through the same load/save path used for baking overlays, so an
        // empty-overlay save is never silently a no-op.
        return doc
            .save(destination)
            .map(|_| ())
            .map_err(|e| WriterError::SaveFailed {
                path: destination.to_path_buf(),
                source: lopdf::Error::IO(e),
            });
    }

    let pages = doc.get_pages();

    // Validate all page references before mutating anything.
    for overlay in overlays {
        if !pages.contains_key(&overlay.page) {
            return Err(WriterError::PageNotFound {
                requested: overlay.page,
                total: pages.len() as u32,
            });
        }
    }

    // Group overlays by page number so each page gets a single content stream.
    let mut overlays_by_page: HashMap<u32, Vec<&TextOverlay>> = HashMap::new();
    for overlay in overlays {
        overlays_by_page
            .entry(overlay.page)
            .or_default()
            .push(overlay);
    }

    for (page_num, page_overlays) in &overlays_by_page {
        let &page_id = pages.get(page_num).expect("validated above");

        let needed_fonts = collect_unique_fonts(page_overlays);
        let mapping = build_font_mapping(&mut doc, page_id, &needed_fonts, registry);
        install_page_fonts(&mut doc, page_id, mapping.new_font_objects);
        let operations = build_overlay_operations(page_overlays, &mapping.resource_names, registry);
        let content_bytes =
            Content { operations }
                .encode()
                .map_err(|e| WriterError::SaveFailed {
                    path: destination.to_path_buf(),
                    source: e,
                })?;
        embed_content_stream(&mut doc, page_id, content_bytes);
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

    /// Builds a single-page PDF whose content stream flips the CTM top-down
    /// (like Skia/Google Docs output) without restoring it. Saves to `path`.
    fn create_flipped_ctm_test_pdf(path: &Path) {
        let mut doc = Document::with_version("1.5");

        let pages_id = doc.new_object_id();

        let content = Content {
            operations: vec![
                // Vertical flip left active for the rest of the page — no enclosing q/Q.
                Operation::new(
                    "cm",
                    vec![
                        1.into(),
                        0.into(),
                        0.into(),
                        (-1).into(),
                        0.into(),
                        792.into(),
                    ],
                ),
                Operation::new("re", vec![10.into(), 10.into(), 100.into(), 50.into()]),
                Operation::new("f", vec![]),
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
    fn write_overlays_isolates_original_graphics_state() {
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        let src = NamedTempFile::new().expect("temp file");
        create_flipped_ctm_test_pdf(src.path());
        let dst = NamedTempFile::new().expect("temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Upright".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
            min_height: None,
        };

        write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write failed");

        let doc = Document::load(dst.path()).expect("load failed");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1");

        // The original content must be wrapped in q/Q so its CTM changes (e.g. the
        // Skia top-down flip) cannot affect the overlay: the first content stream
        // must be a lone `q`, and the overlay stream must begin with `Q`.
        let content_ids = doc.get_page_contents(page_id);
        assert_eq!(
            content_ids.len(),
            3,
            "expected q-prefix, original, and overlay streams, got {} streams",
            content_ids.len()
        );

        let decode = |id: lopdf::ObjectId| {
            doc.get_object(id)
                .expect("stream obj")
                .as_stream()
                .expect("stream")
                .decode_content()
                .expect("decode")
                .operations
        };

        let first_ops = decode(content_ids[0]);
        assert_eq!(
            first_ops.len(),
            1,
            "prefix stream must contain exactly one op, got {first_ops:?}"
        );
        assert_eq!(first_ops[0].operator, "q", "prefix stream must be `q`");

        // Content streams are concatenated by readers; the split must fall on a
        // token boundary, so the prefix stream must end with whitespace.
        let prefix_bytes = &doc
            .get_object(content_ids[0])
            .expect("stream obj")
            .as_stream()
            .expect("stream")
            .content;
        assert!(
            prefix_bytes.last().is_some_and(|b| b.is_ascii_whitespace()),
            "prefix stream must end with whitespace, got {prefix_bytes:?}"
        );

        let overlay_stream_id = *content_ids.last().expect("no streams");
        let overlay_ops = decode(overlay_stream_id);
        assert_eq!(
            overlay_ops
                .first()
                .map(|o| o.operator.as_str())
                .unwrap_or(""),
            "Q",
            "overlay stream must start with `Q` to restore the default graphics state"
        );

        // The original content stream above ends in `f` with no trailing whitespace
        // (see create_flipped_ctm_test_pdf), exactly the case where concatenation
        // could merge the final token with a leading `Q` into the invalid token `fQ`.
        // The overlay stream must therefore start with whitespace so the split
        // always falls on a token boundary.
        let overlay_bytes = &doc
            .get_object(overlay_stream_id)
            .expect("stream obj")
            .as_stream()
            .expect("stream")
            .content;
        assert!(
            overlay_bytes
                .first()
                .is_some_and(|b| b.is_ascii_whitespace()),
            "overlay stream must start with whitespace, got {overlay_bytes:?}"
        );
    }

    #[test]
    fn write_overlays_handles_contents_reference_to_array() {
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        // Build a PDF whose Contents is an indirect reference to an ARRAY of
        // stream references (permitted by the spec) rather than to a stream.
        let src = NamedTempFile::new().expect("temp file");
        {
            let mut doc = Document::with_version("1.5");
            let pages_id = doc.new_object_id();

            let content = Content {
                operations: vec![
                    Operation::new("re", vec![10.into(), 10.into(), 100.into(), 50.into()]),
                    Operation::new("f", vec![]),
                ],
            };
            let stream_id = doc.add_object(Stream::new(
                dictionary! {},
                content.encode().expect("encode"),
            ));
            let array_id = doc.add_object(Object::Array(vec![Object::Reference(stream_id)]));

            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => Object::Reference(array_id),
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            });
            let pages = dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1_i64,
            };
            doc.objects.insert(pages_id, Object::Dictionary(pages));
            let catalog_id = doc.add_object(dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
            });
            doc.trailer.set("Root", catalog_id);
            doc.save(src.path()).expect("save");
        }

        let dst = NamedTempFile::new().expect("temp file");
        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
            min_height: None,
        };
        write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write failed");

        let doc = Document::load(dst.path()).expect("load failed");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1");

        // Every entry in the resulting Contents must resolve to a decodable
        // stream — no array nested inside the Contents array.
        let content_ids = doc.get_page_contents(page_id);
        assert_eq!(
            content_ids.len(),
            3,
            "expected q-prefix, original, and overlay streams, got {} streams",
            content_ids.len()
        );
        for id in content_ids {
            let obj = doc.get_object(id).expect("content entry must resolve");
            assert!(
                obj.as_stream().is_ok(),
                "Contents entry {id:?} must be a stream, got {obj:?}"
            );
        }
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
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        let src = NamedTempFile::new().expect("failed to create temp file");
        create_test_pdf(src.path());

        let dst = NamedTempFile::new().expect("failed to create temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
            min_height: None,
        };

        write_overlays(src.path(), dst.path(), &[overlay], &registry)
            .expect("write_overlays failed");

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
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        let src = NamedTempFile::new().expect("failed to create temp file");
        create_test_pdf(src.path());

        let dst = NamedTempFile::new().expect("failed to create temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
            min_height: None,
        };

        write_overlays(src.path(), dst.path(), &[overlay], &registry)
            .expect("write_overlays failed");

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

    #[test]
    fn write_overlays_reuses_existing_font() {
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        // The test PDF already has Helvetica registered as "F1".
        let src = NamedTempFile::new().expect("failed to create temp file");
        create_test_pdf(src.path());

        let dst = NamedTempFile::new().expect("failed to create temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Reuse".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
            min_height: None,
        };

        write_overlays(src.path(), dst.path(), &[overlay], &registry)
            .expect("write_overlays failed");

        let doc = Document::load(dst.path()).expect("failed to re-open output PDF");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1 not found");

        // There should be exactly one font resource with BaseFont=Helvetica, not two.
        let font_names = collect_page_font_names(&doc, page_id);
        let helvetica_count = font_names
            .iter()
            .filter(|n| n.as_str() == "Helvetica")
            .count();
        assert_eq!(
            helvetica_count, 1,
            "expected exactly 1 Helvetica font resource, got {helvetica_count}: {font_names:?}"
        );

        // The content stream must reference the EXISTING resource name "F1", not a new F_ovl_N.
        let ops = collect_page_operations(&doc, page_id);
        let tf_ops: Vec<&Operation> = ops.iter().filter(|o| o.operator == "Tf").collect();
        let uses_f1 = tf_ops
            .iter()
            .any(|op| matches!(&op.operands[0], Object::Name(n) if n == b"F1"));
        assert!(
            uses_f1,
            "expected Tf to reference existing 'F1', got: {tf_ops:?}"
        );
    }

    #[test]
    fn write_overlays_multiple_fonts_get_unique_names() {
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        let src = NamedTempFile::new().expect("failed to create temp file");
        create_test_pdf(src.path());

        let dst = NamedTempFile::new().expect("failed to create temp file");

        let overlays = vec![
            TextOverlay {
                page: 1,
                position: PdfPosition { x: 72.0, y: 720.0 },
                text: "Helvetica text".to_string(),
                font: registry.default_font(),
                font_size: 12.0,
                width: None,
                min_height: None,
            },
            TextOverlay {
                page: 1,
                position: PdfPosition { x: 72.0, y: 700.0 },
                text: "Courier text".to_string(),
                font: registry.find_by_name("Courier").unwrap(),
                font_size: 12.0,
                width: None,
                min_height: None,
            },
        ];

        write_overlays(src.path(), dst.path(), &overlays, &registry)
            .expect("write_overlays failed");

        let doc = Document::load(dst.path()).expect("failed to re-open output PDF");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1 not found");

        // Build a map from resource name → BaseFont for the page.
        let Ok(fonts) = doc.get_page_fonts(page_id) else {
            panic!("could not get page fonts");
        };
        let resource_to_basefont: std::collections::HashMap<Vec<u8>, Vec<u8>> = fonts
            .iter()
            .filter_map(|(key, fd)| {
                if let Ok(Object::Name(base)) = fd.get(b"BaseFont") {
                    Some((key.clone(), base.clone()))
                } else {
                    None
                }
            })
            .collect();

        // Both Helvetica and Courier must appear.
        assert!(
            resource_to_basefont.values().any(|b| b == b"Helvetica"),
            "Helvetica missing from font resources: {resource_to_basefont:?}"
        );
        assert!(
            resource_to_basefont.values().any(|b| b == b"Courier"),
            "Courier missing from font resources: {resource_to_basefont:?}"
        );

        // Parse the overlay-only content stream: the NEW stream added by write_overlays.
        // We expect a single new stream containing both overlays.
        let content_ids = doc.get_page_contents(page_id);
        // The last stream is the overlay stream (original PDF has 1 stream, we add 1).
        let overlay_stream_id = *content_ids.last().expect("no content streams");
        let stream_obj = doc.get_object(overlay_stream_id).expect("stream not found");
        let stream = stream_obj.as_stream().expect("expected stream");
        let content = stream.decode_content().expect("failed to decode content");

        // Walk through ops: each BT block should have a Tf op whose resource name
        // maps to the correct BaseFont.
        // Op sequence: BT Tf Td Tj ET  BT Tf Td Tj ET
        let ops = &content.operations;

        // Find Tf operand immediately after first BT → should resolve to Helvetica.
        let first_tf = ops
            .iter()
            .skip_while(|o| o.operator != "BT")
            .skip(1) // skip the BT itself
            .find(|o| o.operator == "Tf")
            .expect("no Tf after first BT");

        let first_resource = match &first_tf.operands[0] {
            Object::Name(n) => n.clone(),
            other => panic!("expected Name in Tf operand, got {other:?}"),
        };
        let first_basefont = resource_to_basefont
            .get(&first_resource)
            .unwrap_or_else(|| panic!("resource {first_resource:?} not in font dict"));
        assert_eq!(
            first_basefont, b"Helvetica",
            "first overlay Tf should map to Helvetica, resource {:?} maps to {:?}",
            first_resource, first_basefont
        );

        // Find Tf operand in the second BT block → should resolve to Courier.
        let second_tf = ops
            .iter()
            .skip_while(|o| o.operator != "ET") // skip past first ET
            .skip(1)
            .skip_while(|o| o.operator != "BT") // find second BT
            .skip(1)
            .find(|o| o.operator == "Tf")
            .expect("no Tf after second BT");

        let second_resource = match &second_tf.operands[0] {
            Object::Name(n) => n.clone(),
            other => panic!("expected Name in Tf operand, got {other:?}"),
        };
        let second_basefont = resource_to_basefont
            .get(&second_resource)
            .unwrap_or_else(|| panic!("resource {second_resource:?} not in font dict"));
        assert_eq!(
            second_basefont, b"Courier",
            "second overlay Tf should map to Courier, resource {:?} maps to {:?}",
            second_resource, second_basefont
        );

        // The two resource names must be different.
        assert_ne!(
            first_resource, second_resource,
            "Helvetica and Courier overlays must use different resource names"
        );
    }

    #[test]
    fn write_overlays_multiple_overlays_same_page_single_stream() {
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        let src = NamedTempFile::new().expect("failed to create temp file");
        create_test_pdf(src.path());

        let dst = NamedTempFile::new().expect("failed to create temp file");

        let overlays = vec![
            TextOverlay {
                page: 1,
                position: PdfPosition { x: 72.0, y: 720.0 },
                text: "First".to_string(),
                font: registry.default_font(),
                font_size: 12.0,
                width: None,
                min_height: None,
            },
            TextOverlay {
                page: 1,
                position: PdfPosition { x: 72.0, y: 700.0 },
                text: "Second".to_string(),
                font: registry.default_font(),
                font_size: 12.0,
                width: None,
                min_height: None,
            },
        ];

        // Count content streams BEFORE writing.
        let doc_before = Document::load(src.path()).expect("failed to open source PDF");
        let pages_before = doc_before.get_pages();
        let &page_id_before = pages_before.get(&1).expect("page 1 not found");
        let streams_before = doc_before.get_page_contents(page_id_before).len();

        write_overlays(src.path(), dst.path(), &overlays, &registry)
            .expect("write_overlays failed");

        let doc = Document::load(dst.path()).expect("failed to re-open output PDF");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1 not found");
        let streams_after = doc.get_page_contents(page_id).len();

        // Two overlays on the same page → exactly TWO new streams: the q-prefix
        // wrapping the original content, and ONE overlay stream for both overlays.
        assert_eq!(
            streams_after,
            streams_before + 2,
            "expected {} content streams after writing 2 overlays on 1 page, got {}",
            streams_before + 2,
            streams_after
        );

        // The NEW stream (last content stream) must contain TWO BT/ET pairs.
        let content_ids = doc.get_page_contents(page_id);
        let overlay_stream_id = *content_ids.last().expect("no content streams");
        let stream_obj = doc.get_object(overlay_stream_id).expect("stream not found");
        let stream = stream_obj.as_stream().expect("expected stream");
        let content = stream.decode_content().expect("failed to decode content");
        let bt_count = content
            .operations
            .iter()
            .filter(|o| o.operator == "BT")
            .count();
        assert_eq!(
            bt_count, 2,
            "expected 2 BT blocks (one per overlay) in the overlay stream, got {bt_count}"
        );
    }

    #[test]
    fn write_overlays_empty_slice_still_writes_destination() {
        use crate::fonts::FontRegistry;
        let registry = FontRegistry::new();
        let src = NamedTempFile::new().expect("failed to create temp file");
        create_test_pdf(src.path());

        let dst_path = src.path().with_extension("output.pdf");

        write_overlays(src.path(), &dst_path, &[], &registry).expect("write_overlays failed");

        assert!(
            dst_path.exists(),
            "Save must always produce a file, even with no overlays placed"
        );
        let src_doc = Document::load(src.path()).expect("source PDF must still load");
        let dst_doc = Document::load(&dst_path).expect("destination must be a loadable PDF");
        assert_eq!(
            dst_doc.get_pages().len(),
            src_doc.get_pages().len(),
            "destination page count must match the source"
        );

        let _ = std::fs::remove_file(&dst_path);
    }

    #[test]
    fn write_overlays_invalid_page_returns_page_not_found() {
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        let src = NamedTempFile::new().expect("failed to create temp file");
        create_test_pdf(src.path());

        let dst = NamedTempFile::new().expect("failed to create temp file");

        let overlay = TextOverlay {
            page: 99,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Ghost".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
            min_height: None,
        };

        let result = write_overlays(src.path(), dst.path(), &[overlay], &registry);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(
                err,
                WriterError::PageNotFound {
                    requested: 99,
                    total: 1
                }
            ),
            "expected PageNotFound for page 99, got: {err}"
        );
    }

    #[test]
    fn write_multiline_overlay_produces_multiple_tj_operators() {
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        let src = NamedTempFile::new().expect("temp file");
        create_test_pdf(src.path());
        let dst = NamedTempFile::new().expect("temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Line 1\nLine 2\nLine 3".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: Some(200.0),
            min_height: None,
        };

        write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write failed");

        let doc = Document::load(dst.path()).expect("load failed");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1");

        // Inspect only the overlay stream (the last content stream added by write_overlays).
        let content_ids = doc.get_page_contents(page_id);
        let overlay_stream_id = *content_ids.last().expect("no content streams");
        let stream_obj = doc.get_object(overlay_stream_id).expect("stream obj");
        let stream = stream_obj.as_stream().expect("stream");
        let content = stream.decode_content().expect("decode");
        let ops = &content.operations;

        // Should have 3 Tj operators (one per line)
        let tj_count = ops.iter().filter(|o| o.operator == "Tj").count();
        assert_eq!(tj_count, 3, "expected 3 Tj ops for 3 lines, got {tj_count}");

        // Should have 3 Td operators, one per line.
        let td_ops: Vec<&Operation> = ops.iter().filter(|o| o.operator == "Td").collect();
        assert_eq!(td_ops.len(), 3, "expected 3 Td ops, got {}", td_ops.len());

        // Verify leading offset for the second Td: (0, -(12.0 * 1.2)) = (0, -14.4)
        let leading = 12.0_f64 * 1.2;
        let second_td = td_ops[1];
        let x = match &second_td.operands[0] {
            Object::Real(v) => *v as f64,
            Object::Integer(v) => *v as f64,
            other => panic!("expected numeric x in second Td, got {other:?}"),
        };
        let y = match &second_td.operands[1] {
            Object::Real(v) => *v as f64,
            Object::Integer(v) => *v as f64,
            other => panic!("expected numeric y in second Td, got {other:?}"),
        };
        assert!(x.abs() < 0.01, "second Td x should be 0, got {x}");
        assert!(
            (y - (-leading)).abs() < 0.01,
            "second Td y should be -{leading}, got {y}"
        );
    }

    #[test]
    fn write_single_line_overlay_width_none_unchanged() {
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        // Confirm the single-line (width: None) path still emits exactly 1 Tj.
        let src = NamedTempFile::new().expect("temp file");
        create_test_pdf(src.path());
        let dst = NamedTempFile::new().expect("temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Single line".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
            min_height: None,
        };

        write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write failed");

        let doc = Document::load(dst.path()).expect("load failed");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1");

        // The original test PDF has 1 Tj ("Test"), plus 1 from the overlay = 2 total.
        let overlay_stream_id = *doc.get_page_contents(page_id).last().expect("stream");
        let stream_obj = doc.get_object(overlay_stream_id).expect("obj");
        let stream = stream_obj.as_stream().expect("stream");
        let content = stream.decode_content().expect("decode");
        let tj_in_overlay = content
            .operations
            .iter()
            .filter(|o| o.operator == "Tj")
            .count();
        assert_eq!(
            tj_in_overlay, 1,
            "width:None should produce exactly 1 Tj, got {tj_in_overlay}"
        );
    }

    #[test]
    fn write_truetype_overlay_creates_truetype_font_object() {
        use crate::fonts::{FontEntry, FontRegistry, PdfEmbedding, WidthTable};
        use crate::overlay::{PdfPosition, TextOverlay};

        static TEST_TTF: &[u8] = include_bytes!("../../assets/icons/phosphor-subset.ttf");

        let mut registry = FontRegistry::new();
        let tt_id = registry.add_entry(FontEntry {
            id: crate::fonts::FontId::default(),
            display_name: "UnitTestTT",
            pdf_name: "UnitTestTT",
            iced_font: iced::Font::DEFAULT,
            embedding: PdfEmbedding::TrueType { bytes: TEST_TTF },
            widths: WidthTable::Monospaced(600.0),
            descriptor: None,
        });

        let src = NamedTempFile::new().expect("temp file");
        create_test_pdf(src.path());
        let dst = NamedTempFile::new().expect("temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: tt_id,
            font_size: 12.0,
            width: None,
            min_height: None,
        };

        write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write failed");

        let doc = Document::load(dst.path()).expect("load failed");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1");

        // Find the TrueType font among page fonts.
        let fonts = doc.get_page_fonts(page_id).expect("get_page_fonts");
        let tt_dict = fonts
            .values()
            .find(|fd| matches!(fd.get(b"BaseFont"), Ok(Object::Name(n)) if n == b"UnitTestTT"))
            .expect("UnitTestTT not found in page fonts");

        // Must be TrueType subtype.
        assert_eq!(
            tt_dict.get(b"Subtype").expect("no Subtype"),
            &Object::Name(b"TrueType".to_vec())
        );

        // Must have FontDescriptor.
        assert!(
            matches!(tt_dict.get(b"FontDescriptor"), Ok(Object::Reference(_))),
            "TrueType font must have a FontDescriptor reference"
        );
    }

    /// Resolve `code` through a ToUnicode CMap by parsing its bfchar and bfrange
    /// sections, so tests assert on what a PDF reader would actually resolve
    /// rather than on the literal text of the stream.
    fn cmap_lookup(cmap: &str, code: u8) -> Option<u32> {
        let hex =
            |s: &str| u32::from_str_radix(s.trim_start_matches('<').trim_end_matches('>'), 16);

        let mut in_bfchar = false;
        let mut in_bfrange = false;
        for line in cmap.lines() {
            let line = line.trim();
            match line {
                "endbfchar" => in_bfchar = false,
                "endbfrange" => in_bfrange = false,
                _ if line.ends_with("beginbfchar") => in_bfchar = true,
                _ if line.ends_with("beginbfrange") => in_bfrange = true,
                _ if in_bfchar => {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() == 2
                        && let (Ok(src), Ok(dst)) = (hex(parts[0]), hex(parts[1]))
                        && src == u32::from(code)
                    {
                        return Some(dst);
                    }
                }
                _ if in_bfrange => {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() == 3
                        && let (Ok(lo), Ok(hi), Ok(dst)) =
                            (hex(parts[0]), hex(parts[1]), hex(parts[2]))
                        && (lo..=hi).contains(&u32::from(code))
                    {
                        return Some(dst + u32::from(code) - lo);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// The entry count a `N begin<kind>` header declares, paired with the number
    /// of entry lines actually present before the matching `end<kind>`.
    fn section_entry_counts(cmap: &str, kind: &str) -> (usize, usize) {
        let mut declared = 0;
        let mut actual = 0;
        let mut in_section = false;
        for line in cmap.lines() {
            let line = line.trim();
            if line == format!("end{kind}") {
                in_section = false;
            } else if let Some(count) = line.strip_suffix(&format!(" begin{kind}")) {
                declared = count.parse().expect("section header must declare a count");
                in_section = true;
            } else if in_section {
                actual += 1;
            }
        }
        (declared, actual)
    }

    #[test]
    fn tounicode_cmap_has_required_cmap_structure() {
        let cmap = win_ansi_to_unicode_cmap();

        for required in [
            "/CIDInit /ProcSet findresource begin",
            "begincmap",
            "/CMapName /Adobe-Identity-UCS def",
            "/CMapType 2 def",
            "begincodespacerange",
            "<20> <FF>",
            "endcodespacerange",
            "endcmap",
        ] {
            assert!(
                cmap.contains(required),
                "ToUnicode CMap must contain `{required}`, got:\n{cmap}"
            );
        }

        // A declared count that disagrees with the entries present is the mistake a
        // hand-formatted CMap is most likely to make, and readers trust the header.
        for (kind, expected) in [("codespacerange", 1), ("bfrange", 2), ("bfchar", 27)] {
            let (declared, actual) = section_entry_counts(&cmap, kind);
            assert_eq!(
                declared, expected,
                "`{kind}` header should declare {expected} entries"
            );
            assert_eq!(
                actual, expected,
                "`{kind}` section should contain {expected} entry lines"
            );
        }
    }

    #[test]
    fn tounicode_cmap_maps_win_ansi_codes_to_unicode() {
        let cmap = win_ansi_to_unicode_cmap();

        // ASCII range maps to identical codepoints.
        assert_eq!(cmap_lookup(&cmap, b' '), Some(0x0020));
        assert_eq!(cmap_lookup(&cmap, b'H'), Some(0x0048));
        assert_eq!(cmap_lookup(&cmap, b'~'), Some(0x007E));
        // Latin-1 upper range maps to identical codepoints.
        assert_eq!(cmap_lookup(&cmap, 0xA9), Some(0x00A9)); // copyright
        assert_eq!(cmap_lookup(&cmap, 0xFF), Some(0x00FF)); // y with diaeresis
        // WinAnsi's 0x80-0x9F block differs from Latin-1.
        assert_eq!(cmap_lookup(&cmap, 0x80), Some(0x20AC)); // euro
        assert_eq!(cmap_lookup(&cmap, 0x92), Some(0x2019)); // right single quote
        assert_eq!(cmap_lookup(&cmap, 0x9F), Some(0x0178)); // Y with diaeresis
        // Codes WinAnsi leaves undefined have no mapping.
        for code in [0x81, 0x8D, 0x8F, 0x90, 0x9D] {
            assert_eq!(cmap_lookup(&cmap, code), None);
        }
    }

    #[test]
    fn write_truetype_overlay_adds_tounicode_cmap() {
        use crate::fonts::{FontEntry, FontRegistry, PdfEmbedding, WidthTable};
        use crate::overlay::{PdfPosition, TextOverlay};

        static TEST_TTF: &[u8] = include_bytes!("../../assets/icons/phosphor-subset.ttf");

        let mut registry = FontRegistry::new();
        let tt_id = registry.add_entry(FontEntry {
            id: crate::fonts::FontId::default(),
            display_name: "UnitTestTT",
            pdf_name: "UnitTestTT",
            iced_font: iced::Font::DEFAULT,
            embedding: PdfEmbedding::TrueType { bytes: TEST_TTF },
            widths: WidthTable::Monospaced(600.0),
            descriptor: None,
        });

        let src = NamedTempFile::new().expect("temp file");
        create_test_pdf(src.path());
        let dst = NamedTempFile::new().expect("temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: tt_id,
            font_size: 12.0,
            width: None,
            min_height: None,
        };

        write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write failed");

        let doc = Document::load(dst.path()).expect("load failed");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1");

        let fonts = doc.get_page_fonts(page_id).expect("get_page_fonts");
        let tt_dict = fonts
            .values()
            .find(|fd| matches!(fd.get(b"BaseFont"), Ok(Object::Name(n)) if n == b"UnitTestTT"))
            .expect("UnitTestTT not found in page fonts");

        let Ok(Object::Reference(cmap_id)) = tt_dict.get(b"ToUnicode") else {
            panic!("TrueType font must reference a ToUnicode CMap stream");
        };
        let stream = doc
            .get_object(*cmap_id)
            .expect("ToUnicode object missing")
            .as_stream()
            .expect("ToUnicode must be a stream");
        let cmap = String::from_utf8(
            stream
                .decompressed_content()
                .unwrap_or(stream.content.clone()),
        )
        .expect("CMap must be valid UTF-8");

        // Every character actually shown must resolve back to its own codepoint.
        for c in "Hello".chars() {
            assert_eq!(
                cmap_lookup(&cmap, c as u8),
                Some(c as u32),
                "CMap must map `{c}` back to U+{:04X}",
                c as u32
            );
        }
    }

    #[test]
    fn write_builtin_overlay_has_no_tounicode_cmap() {
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        let src = NamedTempFile::new().expect("temp file");
        create_test_pdf(src.path());
        let dst = NamedTempFile::new().expect("temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Standard".to_string(),
            font: registry.find_by_name("Helvetica").unwrap(),
            font_size: 12.0,
            width: None,
            min_height: None,
        };

        write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write failed");

        let doc = Document::load(dst.path()).expect("load failed");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1");
        let fonts = doc.get_page_fonts(page_id).expect("get_page_fonts");
        let helvetica = fonts
            .values()
            .find(|fd| matches!(fd.get(b"BaseFont"), Ok(Object::Name(n)) if n == b"Helvetica"))
            .expect("Helvetica not found");

        assert!(
            helvetica.get(b"ToUnicode").is_err(),
            "Standard 14 fonts have known encodings and need no ToUnicode CMap"
        );
    }

    #[test]
    fn write_builtin_overlay_still_produces_type1() {
        use crate::fonts::FontRegistry;
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        let src = NamedTempFile::new().expect("temp file");
        create_test_pdf(src.path());
        let dst = NamedTempFile::new().expect("temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Regression".to_string(),
            font: registry.find_by_name("Courier").unwrap(),
            font_size: 12.0,
            width: None,
            min_height: None,
        };

        write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write failed");

        let doc = Document::load(dst.path()).expect("load failed");
        let pages = doc.get_pages();
        let &page_id = pages.get(&1).expect("page 1");

        let fonts = doc.get_page_fonts(page_id).expect("get_page_fonts");
        let courier = fonts
            .values()
            .find(|fd| matches!(fd.get(b"BaseFont"), Ok(Object::Name(n)) if n == b"Courier"))
            .expect("Courier not found");

        assert_eq!(
            courier.get(b"Subtype").expect("no Subtype"),
            &Object::Name(b"Type1".to_vec()),
            "BuiltIn font must remain Type1"
        );
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
