// Overlay re-editing metadata: versioned JSON serialization plus content
// stream fingerprints (ADR-015, docs/designs/overlay-re-editing.md).

use serde::{Deserialize, Serialize};

use crate::fonts::FontRegistry;
use crate::overlay::TextOverlay;

/// Version of the /SPEOverlays JSON this build writes and understands.
pub const METADATA_VERSION: u32 = 1;

/// A cheap identity for a content stream: byte length plus an FNV-1a hash,
/// the same scheme the writer uses for TrueType font programs. Distinguishes
/// same-length streams a length-only check would conflate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamFingerprint {
    pub length: usize,
    pub hash: u64,
}

impl StreamFingerprint {
    pub fn of(bytes: &[u8]) -> Self {
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

/// The content streams the writer added to one page: the overlay stream and,
/// when the page had original content to isolate, the q-prefix stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageStreams {
    pub page: u32,
    pub overlay: StreamFingerprint,
    pub prefix: Option<StreamFingerprint>,
}

/// One overlay as stored in the metadata JSON. Font is stored by display
/// name (e.g. "Helvetica"), resolved back through the registry on reopen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayRecord {
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub font_family: String,
    pub font_size: f32,
    pub width: Option<f32>,
    pub min_height: Option<f32>,
}

/// The full /SPEOverlays payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayMetadata {
    pub version: u32,
    pub overlays: Vec<OverlayRecord>,
    pub streams: Vec<PageStreams>,
}

/// Serialize `overlays` and the streams the writer produced to the JSON
/// stored under /SPEOverlays.
pub fn to_json(
    overlays: &[TextOverlay],
    streams: &[PageStreams],
    registry: &FontRegistry,
) -> String {
    let records = overlays
        .iter()
        .map(|o| OverlayRecord {
            page: o.page,
            x: o.position.x,
            y: o.position.y,
            text: o.text.clone(),
            font_family: registry.get(o.font).display_name.to_string(),
            font_size: o.font_size,
            width: o.width,
            min_height: o.min_height,
        })
        .collect();
    let metadata = OverlayMetadata {
        version: METADATA_VERSION,
        overlays: records,
        streams: streams.to_vec(),
    };
    serde_json::to_string(&metadata).expect("metadata serialization cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::PdfPosition;

    fn sample_overlay(registry: &FontRegistry) -> TextOverlay {
        TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: Some(200.0),
            min_height: Some(90.0),
        }
    }

    #[test]
    fn fingerprint_distinguishes_different_bytes() {
        let a = StreamFingerprint::of(b"BT (Hello) Tj ET");
        let b = StreamFingerprint::of(b"BT (World) Tj ET");
        assert_ne!(a, b);
        assert_eq!(a, StreamFingerprint::of(b"BT (Hello) Tj ET"));
    }

    #[test]
    fn fingerprint_distinguishes_same_length_different_content() {
        let a = StreamFingerprint::of(b"aaaa");
        let b = StreamFingerprint::of(b"aaab");
        assert_eq!(a.length, b.length);
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn to_json_records_version_overlays_and_streams() {
        let registry = FontRegistry::new();
        let overlay = sample_overlay(&registry);
        let streams = vec![PageStreams {
            page: 1,
            overlay: StreamFingerprint::of(b"stream bytes"),
            prefix: Some(StreamFingerprint::of(b"q\n")),
        }];
        let json = to_json(&[overlay], &streams, &registry);

        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["version"], METADATA_VERSION);
        assert_eq!(parsed["overlays"][0]["page"], 1);
        assert_eq!(parsed["overlays"][0]["text"], "Hello");
        assert_eq!(
            parsed["overlays"][0]["font_family"],
            registry.get(registry.default_font()).display_name
        );
        assert_eq!(parsed["overlays"][0]["min_height"], 90.0);
        assert_eq!(parsed["streams"][0]["page"], 1);
        assert!(parsed["streams"][0]["prefix"].is_object());
    }

    #[test]
    fn to_json_omits_box_fields_for_single_line_overlays() {
        let registry = FontRegistry::new();
        let overlay = TextOverlay {
            width: None,
            min_height: None,
            ..sample_overlay(&registry)
        };
        let json = to_json(&[overlay], &[], &registry);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert!(parsed["overlays"][0]["width"].is_null());
        assert!(parsed["overlays"][0]["min_height"].is_null());
    }
}
