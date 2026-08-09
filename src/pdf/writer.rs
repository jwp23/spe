// PDF text overlay writing via lopdf.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use thiserror::Error;

use super::win_ansi;
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

/// What a completed save is worth telling the user about.
#[derive(Debug, Default, PartialEq)]
pub struct SaveReport {
    /// Characters WinAnsiEncoding cannot represent, each listed once in
    /// first-seen order. They were written as `?`.
    pub unencodable_chars: Vec<char>,
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
    type FontIdentity = (Vec<u8>, Option<Vec<u8>>, Option<FontProgramFingerprint>);
    let existing: HashMap<Vec<u8>, FontIdentity> = doc
        .get_page_fonts(page_id)
        .map(|fonts| {
            fonts
                .into_iter()
                .filter_map(|(key, fd)| {
                    if let Ok(Object::Name(base)) = fd.get(b"BaseFont") {
                        let encoding = match fd.get(b"Encoding") {
                            Ok(Object::Name(name)) => Some(name.clone()),
                            _ => None,
                        };
                        let program = embedded_font_program_fingerprint(doc, fd);
                        Some((key, (base.clone(), encoding, program)))
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

        // Reuse an existing resource only when it names the same font *and*
        // reads bytes the way the overlay writes them; a font left on the
        // default StandardEncoding would show the wrong glyphs for our bytes.
        let wanted_encoding = win_ansi_encoding_for(entry.pdf_name);
        // A page font can share our BaseFont name yet embed a different font
        // program (two documents both naming a custom font the same thing).
        // Standard 14 fonts have no embedded program to diverge, so only
        // TrueType entries need this check.
        let wanted_program = match &entry.embedding {
            PdfEmbedding::TrueType { bytes } => Some(FontProgramFingerprint::of(bytes)),
            PdfEmbedding::BuiltIn => None,
        };
        let reuse_name = existing
            .iter()
            .find(|(_, (base, encoding, program))| {
                base.as_slice() == base_font_bytes
                    && encoding.as_deref() == wanted_encoding
                    && match &wanted_program {
                        Some(wanted) => program.as_ref() == Some(wanted),
                        None => true,
                    }
            })
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
                PdfEmbedding::BuiltIn => {
                    let mut font = dictionary! {
                        "Type" => "Font",
                        "Subtype" => "Type1",
                        "BaseFont" => Object::Name(base_font_bytes.to_vec()),
                    };
                    if let Some(encoding) = wanted_encoding {
                        font.set("Encoding", Object::Name(encoding.to_vec()));
                    }
                    doc.add_object(font)
                }
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

/// Vertical spacing between overlay lines, as a multiple of font size.
///
/// Defined *as* `crate::overlay::TEXT_LINE_HEIGHT_RATIO` rather than merely
/// equal to it: `ui::canvas` lays overlay lines out at that ratio while
/// editing, so the saved PDF's `Td` leading must reproduce the same spacing or
/// a multiline overlay's lines land at different offsets when the file is
/// reopened elsewhere. Both layers read the one constant from the shared
/// overlay data model rather than the backend depending on the presentation
/// layer (spe-5xe).
const LINE_SPACING_RATIO: f32 = crate::overlay::TEXT_LINE_HEIGHT_RATIO;

/// The `/Encoding` a Standard 14 font must declare so its bytes are read as
/// WinAnsi, or `None` for the two symbolic fonts (Symbol and ZapfDingbats),
/// whose own built-in encodings WinAnsiEncoding would override with the wrong
/// glyphs.
fn win_ansi_encoding_for(pdf_name: &str) -> Option<&'static [u8]> {
    match pdf_name {
        "Symbol" | "ZapfDingbats" => None,
        _ => Some(b"WinAnsiEncoding"),
    }
}

/// A cheap identity for a TrueType font program: byte length plus an FNV-1a
/// hash of its contents. Cheaper than comparing the bytes directly (no need
/// to hold both font programs in memory at once) while still distinguishing
/// same-length font files, which a length-only check would conflate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FontProgramFingerprint {
    length: usize,
    hash: u64,
}

impl FontProgramFingerprint {
    fn of(bytes: &[u8]) -> Self {
        const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x100_0000_01b3;
        let mut hash = FNV_OFFSET_BASIS;
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        Self {
            length: bytes.len(),
            hash,
        }
    }
}

/// The identity of the TrueType font program a page font dictionary embeds,
/// or `None` when it has no `FontFile2` to check (a Standard 14 font, or a
/// TrueType font PDF spec allows without an embedded program).
fn embedded_font_program_fingerprint(
    doc: &Document,
    font_dict: &lopdf::Dictionary,
) -> Option<FontProgramFingerprint> {
    const MAX_FONT_FILE_SIZE: usize = 50 * 1024 * 1024; // 50 MB

    let descriptor = match font_dict.get(b"FontDescriptor").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(dict) => dict,
        _ => return None,
    };
    let font_file = match descriptor.get(b"FontFile2").ok()? {
        Object::Reference(id) => doc.get_object(*id).ok()?.as_stream().ok()?,
        Object::Stream(stream) => stream,
        _ => return None,
    };
    let content = font_file.decompressed_content().ok()?;

    // Enforce maximum decompressed size to prevent denial of service.
    if content.len() > MAX_FONT_FILE_SIZE {
        return None;
    }

    Some(FontProgramFingerprint::of(&content))
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
    // Each code is a byte the content stream can emit; resolve it through
    // WinAnsiEncoding rather than treating the byte as its own codepoint, or
    // the codes in the 0x80-0x9F block (euro sign, em dash, etc.) get the
    // width of an unrelated C1 control character instead.
    let widths: Vec<Object> = (first_char..=last_char)
        .map(|c| {
            // A handful of codes in that block (0x81, 0x8D, 0x8F, 0x90, 0x9D)
            // are undefined in WinAnsiEncoding; fall back to space's width.
            let ch = win_ansi::decode(c as u8).unwrap_or(' ');
            let w = entry.widths.char_width(ch);
            Object::Integer(w.round() as i64)
        })
        .collect();

    let to_unicode_id = doc.add_object(Stream::new(
        dictionary! {},
        win_ansi::to_unicode_cmap().into_bytes(),
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

/// A page's overlay content stream operations, with whatever the encoding lost.
struct OverlayContent {
    operations: Vec<Operation>,
    unencodable: Vec<char>,
}

/// Build PDF content stream operations (BT/Tf/Td/Tj/ET) for a set of overlays.
fn build_overlay_operations(
    page_overlays: &[&TextOverlay],
    font_resource_names: &HashMap<FontId, String>,
    registry: &FontRegistry,
) -> OverlayContent {
    let mut operations: Vec<Operation> = Vec::new();
    let mut unencodable: Vec<char> = Vec::new();
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

        let leading = overlay.font_size * LINE_SPACING_RATIO;
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
            // A PDF shows text as bytes read through the font's /Encoding, so
            // the line must be encoded to WinAnsi rather than emitted as UTF-8.
            let encoded = win_ansi::encode(line);
            win_ansi::merge_unencodable(&mut unencodable, encoded.unencodable);
            operations.push(Operation::new(
                "Tj",
                vec![Object::String(encoded.bytes, lopdf::StringFormat::Literal)],
            ));
        }

        operations.push(Operation::new("ET", vec![]));
    }
    OverlayContent {
        operations,
        unencodable,
    }
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
) -> Result<SaveReport, WriterError> {
    let mut doc = Document::load(source).map_err(WriterError::OpenFailed)?;

    if overlays.is_empty() {
        // Save must always produce a file: write a plain copy of the source
        // through the same load/save path used for baking overlays, so an
        // empty-overlay save is never silently a no-op.
        return doc
            .save(destination)
            .map(|_| SaveReport::default())
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
    // Ordered by page so the save report reads the same way on every run.
    let mut overlays_by_page: BTreeMap<u32, Vec<&TextOverlay>> = BTreeMap::new();
    for overlay in overlays {
        overlays_by_page
            .entry(overlay.page)
            .or_default()
            .push(overlay);
    }

    let mut report = SaveReport::default();
    for (page_num, page_overlays) in &overlays_by_page {
        let &page_id = pages.get(page_num).expect("validated above");

        let needed_fonts = collect_unique_fonts(page_overlays);
        let mapping = build_font_mapping(&mut doc, page_id, &needed_fonts, registry);
        install_page_fonts(&mut doc, page_id, mapping.new_font_objects);
        let content = build_overlay_operations(page_overlays, &mapping.resource_names, registry);
        win_ansi::merge_unencodable(&mut report.unencodable_chars, content.unencodable);
        let content_bytes = Content {
            operations: content.operations,
        }
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

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    /// The writer's line spacing must stay in lockstep with the canvas's, or a
    /// saved PDF's lines land at different offsets than they were edited at
    /// (spe-5xe). `LINE_SPACING_RATIO` is defined *as* the shared overlay
    /// constant rather than merely equal to it, so this test can only ever
    /// fail if that direct link is ever replaced with an independent literal.
    #[test]
    fn line_spacing_ratio_is_the_overlay_text_line_height_ratio() {
        assert_eq!(LINE_SPACING_RATIO, crate::overlay::TEXT_LINE_HEIGHT_RATIO);
    }

    /// Builds a minimal single-page PDF and saves it to `path`. Its Helvetica
    /// page font declares WinAnsiEncoding, as real-world text PDFs do.
    fn create_test_pdf(path: &Path) {
        create_test_pdf_with_font_encoding(path, Some("WinAnsiEncoding"));
    }

    /// Builds the same PDF with `encoding` on the page's Helvetica font, or no
    /// `/Encoding` entry at all when `None` (leaving it on StandardEncoding).
    fn create_test_pdf_with_font_encoding(path: &Path, encoding: Option<&str>) {
        let mut doc = Document::with_version("1.5");

        let pages_id = doc.new_object_id();

        let mut font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
        if let Some(encoding) = encoding {
            font.set("Encoding", Object::Name(encoding.as_bytes().to_vec()));
        }
        let font_id = doc.add_object(font);

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
            },
            TextOverlay {
                page: 1,
                position: PdfPosition { x: 72.0, y: 700.0 },
                text: "Courier text".to_string(),
                font: registry.find_by_name("Courier").unwrap(),
                font_size: 12.0,
                width: None,
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
            },
            TextOverlay {
                page: 1,
                position: PdfPosition { x: 72.0, y: 700.0 },
                text: "Second".to_string(),
                font: registry.default_font(),
                font_size: 12.0,
                width: None,
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

        // Verify leading offset for the second Td: (0, -(12.0 * LINE_SPACING_RATIO)).
        let leading = 12.0_f64 * f64::from(LINE_SPACING_RATIO);
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

        // The stream must be the WinAnsi CMap itself, whose contents the
        // `win_ansi` module tests in detail.
        assert_eq!(cmap, win_ansi::to_unicode_cmap());
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

    #[test]
    fn write_overlay_emits_non_ascii_text_as_win_ansi_bytes() {
        let registry = FontRegistry::new();
        let (_, doc) = write_one_overlay("café — €5", registry.default_font(), &registry);

        assert_eq!(
            overlay_tj_strings(&doc),
            vec![b"caf\xE9 \x97 \x805".to_vec()],
            "text must be shown in the encoding the font declares, not UTF-8"
        );
    }

    #[test]
    fn write_overlay_substitutes_question_mark_for_unencodable_characters() {
        let registry = FontRegistry::new();
        let (_, doc) = write_one_overlay("a中b", registry.default_font(), &registry);

        assert_eq!(overlay_tj_strings(&doc), vec![b"a?b".to_vec()]);
    }

    #[test]
    fn write_overlays_reports_characters_it_could_not_encode() {
        let registry = FontRegistry::new();
        let (report, _) = write_one_overlay("中 and 😀", registry.default_font(), &registry);

        assert_eq!(report.unencodable_chars, vec!['中', '😀']);
    }

    #[test]
    fn write_overlays_reports_nothing_when_all_text_is_encodable() {
        let registry = FontRegistry::new();
        let (report, _) = write_one_overlay("café — €5", registry.default_font(), &registry);

        assert!(report.unencodable_chars.is_empty());
    }

    #[test]
    fn write_builtin_overlay_declares_win_ansi_encoding() {
        let registry = FontRegistry::new();
        let font = registry.find_by_name("Helvetica").expect("Helvetica");
        let (_, doc) = write_one_overlay("Standard", font, &registry);

        assert_eq!(
            page_font_dict(&doc, b"Helvetica").get(b"Encoding").ok(),
            Some(&Object::Name(b"WinAnsiEncoding".to_vec())),
            "a text font must declare the encoding its bytes are written in"
        );
    }

    #[test]
    fn write_overlays_does_not_reuse_a_page_font_with_a_different_encoding() {
        use crate::overlay::{PdfPosition, TextOverlay};
        let registry = FontRegistry::new();

        // The page's Helvetica is on StandardEncoding, which would read the
        // overlay's WinAnsi bytes as the wrong glyphs.
        let src = NamedTempFile::new().expect("temp file");
        create_test_pdf_with_font_encoding(src.path(), None);
        let dst = NamedTempFile::new().expect("temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "café".to_string(),
            font: registry.find_by_name("Helvetica").expect("Helvetica"),
            font_size: 12.0,
            width: None,
        };
        write_overlays(src.path(), dst.path(), &[overlay], &registry).expect("write failed");

        let doc = Document::load(dst.path()).expect("load failed");
        let &page_id = doc.get_pages().get(&1).expect("page 1");
        let helvetica_count = collect_page_font_names(&doc, page_id)
            .iter()
            .filter(|n| n.as_str() == "Helvetica")
            .count();
        assert_eq!(
            helvetica_count, 2,
            "the overlay needs its own WinAnsi-encoded Helvetica alongside the page's"
        );
    }

    /// A page font can share our BaseFont name and WinAnsiEncoding yet embed a
    /// completely different font program — e.g. two documents both naming a
    /// custom font "UnitTestTT" but shipping different TrueType files.
    /// Reusing that resource would show our overlay text in the wrong glyphs
    /// (or a font with none of the glyphs we need). The reuse predicate must
    /// check the embedded font program's identity, not just its name (spe-adl).
    #[test]
    fn write_overlays_does_not_reuse_a_same_named_font_with_a_different_program() {
        use crate::fonts::{FontEntry, FontId, FontRegistry, PdfEmbedding, WidthTable};
        use crate::overlay::{PdfPosition, TextOverlay};

        static FONT_A: &[u8] = include_bytes!("../../assets/icons/phosphor-subset.ttf");
        static FONT_B: &[u8] = include_bytes!("../../assets/fonts/dancing-script.ttf");

        let font_entry = |bytes: &'static [u8]| FontEntry {
            id: FontId::default(),
            display_name: "UnitTestTT",
            pdf_name: "UnitTestTT",
            iced_font: iced::Font::DEFAULT,
            embedding: PdfEmbedding::TrueType { bytes },
            widths: WidthTable::Monospaced(600.0),
            descriptor: None,
        };

        // Write once with FONT_A to produce a document whose page already
        // embeds a "UnitTestTT" font backed by FONT_A's bytes.
        let mut registry_a = FontRegistry::new();
        let font_a = registry_a.add_entry(font_entry(FONT_A));
        let plain = NamedTempFile::new().expect("temp file");
        create_test_pdf(plain.path());
        let with_font_a = NamedTempFile::new().expect("temp file");
        write_overlays(
            plain.path(),
            with_font_a.path(),
            &[TextOverlay {
                page: 1,
                position: PdfPosition { x: 72.0, y: 720.0 },
                text: "Hello".to_string(),
                font: font_a,
                font_size: 12.0,
                width: None,
            }],
            &registry_a,
        )
        .expect("first write failed");

        // Now write an overlay needing "UnitTestTT" backed by FONT_B's
        // (different) bytes onto that same document.
        let mut registry_b = FontRegistry::new();
        let font_b = registry_b.add_entry(font_entry(FONT_B));
        let with_font_b = NamedTempFile::new().expect("temp file");
        write_overlays(
            with_font_a.path(),
            with_font_b.path(),
            &[TextOverlay {
                page: 1,
                position: PdfPosition { x: 72.0, y: 700.0 },
                text: "World".to_string(),
                font: font_b,
                font_size: 12.0,
                width: None,
            }],
            &registry_b,
        )
        .expect("second write failed");

        let doc = Document::load(with_font_b.path()).expect("load failed");
        let &page_id = doc.get_pages().get(&1).expect("page 1");

        let unit_test_tt_count = collect_page_font_names(&doc, page_id)
            .iter()
            .filter(|n| n.as_str() == "UnitTestTT")
            .count();
        assert_eq!(
            unit_test_tt_count, 2,
            "a same-named font with a different program must not be reused; \
             the overlay needs its own FONT_B-backed resource alongside FONT_A's"
        );
    }

    #[test]
    fn write_symbolic_builtin_overlay_keeps_its_own_encoding() {
        let registry = FontRegistry::new();
        let font = registry.find_by_name("Symbol").expect("Symbol");
        let (_, doc) = write_one_overlay("abg", font, &registry);

        assert!(
            page_font_dict(&doc, b"Symbol").get(b"Encoding").is_err(),
            "Symbol has its own built-in encoding; WinAnsiEncoding would garble it"
        );
    }

    #[test]
    fn truetype_widths_are_indexed_by_the_winansi_character_not_the_raw_byte() {
        let registry = FontRegistry::new();
        let font = registry.find_by_name("Great Vibes").expect("Great Vibes");
        let (_, doc) = write_one_overlay("x", font, &registry);

        let font_dict = page_font_dict(&doc, b"GreatVibes-Regular");
        let widths = font_dict
            .get(b"Widths")
            .expect("Widths")
            .as_array()
            .expect("array");
        let first_char = font_dict
            .get(b"FirstChar")
            .expect("FirstChar")
            .as_i64()
            .expect("int");
        let width_at = |code: u8| {
            widths[(code as i64 - first_char) as usize]
                .as_i64()
                .expect("int")
        };

        let entry = registry.get(font);
        assert_eq!(
            width_at(0x80),
            entry.widths.char_width('\u{20AC}').round() as i64,
            "byte 0x80 is EURO SIGN under WinAnsiEncoding, not U+0080"
        );
        assert_eq!(
            width_at(0x97),
            entry.widths.char_width('\u{2014}').round() as i64,
            "byte 0x97 is EM DASH under WinAnsiEncoding, not U+0097"
        );

        // Great Vibes has real glyphs for both characters; a width equal to
        // the table's fallback would mean the advance never made it into the
        // font's WidthTable, not just a coincidental match with char_width.
        let fallback = entry.widths.char_width('\u{4E2D}').round() as i64;
        assert_ne!(
            width_at(0x80),
            fallback,
            "euro sign width must be its real glyph advance, not the fallback"
        );
        assert_ne!(
            width_at(0x97),
            fallback,
            "em dash width must be its real glyph advance, not the fallback"
        );
    }

    // --- Test helpers ---

    /// Write one single-line overlay in `font` through the real writer and
    /// return the save report alongside the reloaded document.
    fn write_one_overlay(
        text: &str,
        font: crate::fonts::FontId,
        registry: &FontRegistry,
    ) -> (SaveReport, Document) {
        use crate::overlay::{PdfPosition, TextOverlay};

        let src = NamedTempFile::new().expect("temp file");
        create_test_pdf(src.path());
        let dst = NamedTempFile::new().expect("temp file");

        let overlay = TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: text.to_string(),
            font,
            font_size: 12.0,
            width: None,
        };
        let report =
            write_overlays(src.path(), dst.path(), &[overlay], registry).expect("write failed");
        (report, Document::load(dst.path()).expect("load failed"))
    }

    /// The font dictionary on page 1 whose BaseFont is `base_font`.
    fn page_font_dict<'a>(doc: &'a Document, base_font: &[u8]) -> lopdf::Dictionary {
        let &page_id = doc.get_pages().get(&1).expect("page 1");
        doc.get_page_fonts(page_id)
            .expect("get_page_fonts")
            .values()
            .find(|fd| matches!(fd.get(b"BaseFont"), Ok(Object::Name(n)) if n == base_font))
            .map(|fd| (*fd).clone())
            .unwrap_or_else(|| {
                panic!(
                    "{} not found in page fonts",
                    String::from_utf8_lossy(base_font)
                )
            })
    }

    /// The operand bytes of every `Tj` in the overlay content stream (the last
    /// stream on page 1), which is what a reader actually shows.
    fn overlay_tj_strings(doc: &Document) -> Vec<Vec<u8>> {
        let &page_id = doc.get_pages().get(&1).expect("page 1");
        let overlay_stream_id = *doc.get_page_contents(page_id).last().expect("stream");
        let stream_obj = doc.get_object(overlay_stream_id).expect("obj");
        let content = stream_obj
            .as_stream()
            .expect("stream")
            .decode_content()
            .expect("decode");
        content
            .operations
            .iter()
            .filter(|o| o.operator == "Tj")
            .map(|o| match o.operands.first() {
                Some(Object::String(bytes, _)) => bytes.clone(),
                other => panic!("Tj operand must be a string, got {other:?}"),
            })
            .collect()
    }

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
