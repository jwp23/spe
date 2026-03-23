pub mod renderer;
pub mod writer;

use std::collections::HashMap;

pub fn page_dimensions(doc: &lopdf::Document) -> HashMap<u32, (f32, f32)> {
    let mut dims = HashMap::new();
    for (&page_num, &page_id) in doc.get_pages().iter() {
        if let Some((w, h)) = read_media_box(doc, page_id) {
            dims.insert(page_num, (w, h));
        }
    }
    dims
}

/// Maximum depth for walking up the page tree to find inherited MediaBox.
/// PDF page trees are typically 2-4 levels deep; this guards against
/// circular Parent references in malformed PDFs.
const MAX_PAGE_TREE_DEPTH: u32 = 16;

fn read_media_box(doc: &lopdf::Document, object_id: lopdf::ObjectId) -> Option<(f32, f32)> {
    read_media_box_bounded(doc, object_id, MAX_PAGE_TREE_DEPTH)
}

fn read_media_box_bounded(
    doc: &lopdf::Document,
    object_id: lopdf::ObjectId,
    remaining_depth: u32,
) -> Option<(f32, f32)> {
    if remaining_depth == 0 {
        return None;
    }
    let dict = doc.get_object(object_id).ok()?.as_dict().ok()?;

    // Try this node's MediaBox first
    if let Ok(media_box) = dict.get(b"MediaBox")
        && let Some(dims) = parse_media_box(media_box)
    {
        return Some(dims);
    }

    // Walk up to parent for inherited MediaBox
    if let Ok(parent_ref) = dict.get(b"Parent")
        && let Ok(parent_id) = parent_ref.as_reference()
    {
        return read_media_box_bounded(doc, parent_id, remaining_depth - 1);
    }

    None
}

fn parse_media_box(obj: &lopdf::Object) -> Option<(f32, f32)> {
    let arr = obj.as_array().ok()?;
    if arr.len() != 4 {
        return None;
    }
    let x0 = as_f32(&arr[0])?;
    let y0 = as_f32(&arr[1])?;
    let x1 = as_f32(&arr[2])?;
    let y1 = as_f32(&arr[3])?;
    Some((x1 - x0, y1 - y0))
}

fn as_f32(obj: &lopdf::Object) -> Option<f32> {
    match obj {
        lopdf::Object::Real(v) => Some(*v),
        lopdf::Object::Integer(v) => Some(*v as f32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::Content;
    use lopdf::{Document, Object, Stream, dictionary};

    fn create_single_page_pdf(width: f32, height: f32) -> Document {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = Content { operations: vec![] };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(width.into()), Object::Real(height.into())],
            "Contents" => content_id,
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
        doc
    }

    #[test]
    fn inherited_mediabox_from_parent() {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = Content { operations: vec![] };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        // Page WITHOUT its own MediaBox — inherits from parent
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let dims = page_dimensions(&doc);
        assert_eq!(dims.len(), 1);
        let (w, h) = dims[&1];
        assert!((w - 595.0).abs() < 0.01);
        assert!((h - 842.0).abs() < 0.01);
    }

    #[test]
    fn single_page_returns_correct_dimensions() {
        let doc = create_single_page_pdf(612.0, 792.0);
        let dims = page_dimensions(&doc);
        assert_eq!(dims.len(), 1);
        let (w, h) = dims[&1];
        assert!((w - 612.0).abs() < 0.01);
        assert!((h - 792.0).abs() < 0.01);
    }

    #[test]
    fn multi_page_different_sizes() {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = Content { operations: vec![] };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let content2 = Content { operations: vec![] };
        let content2_id = doc.add_object(Stream::new(dictionary! {}, content2.encode().unwrap()));
        // Page 1: US Letter
        let page1_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(612.0), Object::Real(792.0)],
            "Contents" => content_id,
        });
        // Page 2: A4
        let page2_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(595.0), Object::Real(842.0)],
            "Contents" => content2_id,
        });
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page1_id), Object::Reference(page2_id)],
            "Count" => 2_i64,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let dims = page_dimensions(&doc);
        assert_eq!(dims.len(), 2);
        let (w1, h1) = dims[&1];
        assert!((w1 - 612.0).abs() < 0.01);
        assert!((h1 - 792.0).abs() < 0.01);
        let (w2, h2) = dims[&2];
        assert!((w2 - 595.0).abs() < 0.01);
        assert!((h2 - 842.0).abs() < 0.01);
    }
}
