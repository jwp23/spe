// Overlay re-editing metadata: versioned JSON serialization plus content
// stream fingerprints (ADR-015, docs/designs/overlay-re-editing.md).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::fonts::FontRegistry;
use crate::overlay::{PdfPosition, TextOverlay};

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

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("overlay metadata is not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error(
        "overlay metadata version {found} is newer than this build understands ({METADATA_VERSION})"
    )]
    UnsupportedVersion { found: u32 },

    #[error("overlay metadata has an invalid value: {0}")]
    InvalidValue(String),
}

/// Reject non-finite positions/sizes and non-positive font sizes: a value a
/// legitimate save from this app could never produce, so JSON carrying one
/// is either corrupt or crafted, not trustworthy metadata.
fn validate_values(metadata: &OverlayMetadata) -> Result<(), MetadataError> {
    for record in &metadata.overlays {
        if !record.x.is_finite() || !record.y.is_finite() {
            return Err(MetadataError::InvalidValue(format!(
                "overlay on page {} has a non-finite position",
                record.page
            )));
        }
        if !record.font_size.is_finite() || record.font_size <= 0.0 {
            return Err(MetadataError::InvalidValue(format!(
                "overlay on page {} has an invalid font size",
                record.page
            )));
        }
        if record.width.is_some_and(|w| !w.is_finite() || w <= 0.0) {
            return Err(MetadataError::InvalidValue(format!(
                "overlay on page {} has an invalid width",
                record.page
            )));
        }
        if record.min_height.is_some_and(|h| !h.is_finite() || h < 0.0) {
            return Err(MetadataError::InvalidValue(format!(
                "overlay on page {} has an invalid min height",
                record.page
            )));
        }
    }
    Ok(())
}

/// Parse /SPEOverlays JSON, rejecting versions newer than this build writes
/// and values a legitimate save could never contain.
pub fn from_json(json: &str) -> Result<OverlayMetadata, MetadataError> {
    let metadata: OverlayMetadata = serde_json::from_str(json)?;
    if metadata.version > METADATA_VERSION {
        return Err(MetadataError::UnsupportedVersion {
            found: metadata.version,
        });
    }
    validate_values(&metadata)?;
    Ok(metadata)
}

/// Overlays reconstructed from metadata, plus the font families that are no
/// longer installed and fell back to the default font (deduplicated, in
/// first-seen order).
pub struct RestoredOverlays {
    pub overlays: Vec<TextOverlay>,
    pub missing_fonts: Vec<String>,
}

/// Resolve overlay records back into the in-memory model, substituting the
/// default font for families the registry no longer knows.
pub fn resolve_overlays(metadata: &OverlayMetadata, registry: &FontRegistry) -> RestoredOverlays {
    let mut missing_fonts: Vec<String> = Vec::new();
    let overlays = metadata
        .overlays
        .iter()
        .map(|r| {
            let font = registry.find_by_name(&r.font_family).unwrap_or_else(|| {
                if !missing_fonts.contains(&r.font_family) {
                    missing_fonts.push(r.font_family.clone());
                }
                registry.default_font()
            });
            TextOverlay {
                page: r.page,
                position: PdfPosition { x: r.x, y: r.y },
                text: r.text.clone(),
                font,
                font_size: r.font_size,
                width: r.width,
                min_height: r.min_height,
            }
        })
        .collect();
    RestoredOverlays {
        overlays,
        missing_fonts,
    }
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

    #[test]
    fn json_round_trips_to_an_identical_overlay_model() {
        let registry = FontRegistry::new();
        let overlays = vec![
            sample_overlay(&registry),
            TextOverlay {
                page: 2,
                position: PdfPosition { x: 10.5, y: 400.25 },
                text: "Second".to_string(),
                font: registry.find_by_name("Courier").expect("Courier exists"),
                font_size: 9.5,
                width: None,
                min_height: None,
            },
        ];
        let streams = vec![PageStreams {
            page: 1,
            overlay: StreamFingerprint::of(b"ops"),
            prefix: None,
        }];

        let json = to_json(&overlays, &streams, &registry);
        let metadata = from_json(&json).expect("round-trip parse");
        assert_eq!(metadata.version, METADATA_VERSION);
        assert_eq!(metadata.streams, streams);

        let restored = resolve_overlays(&metadata, &registry);
        assert_eq!(restored.overlays, overlays);
        assert!(restored.missing_fonts.is_empty());
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert!(matches!(
            from_json("not json at all"),
            Err(MetadataError::InvalidJson(_))
        ));
    }

    #[test]
    fn non_finite_position_is_rejected() {
        let json = r#"{"version":1,"overlays":[{"page":1,"x":1e40,"y":0.0,
            "text":"x","font_family":"Helvetica","font_size":12.0,
            "width":null,"min_height":null}],"streams":[]}"#;
        assert!(matches!(
            from_json(json),
            Err(MetadataError::InvalidValue(_))
        ));
    }

    #[test]
    fn non_positive_font_size_is_rejected() {
        let json = r#"{"version":1,"overlays":[{"page":1,"x":0.0,"y":0.0,
            "text":"x","font_family":"Helvetica","font_size":0.0,
            "width":null,"min_height":null}],"streams":[]}"#;
        assert!(matches!(
            from_json(json),
            Err(MetadataError::InvalidValue(_))
        ));
    }

    #[test]
    fn future_version_is_rejected() {
        let json = format!(
            r#"{{"version": {}, "overlays": [], "streams": []}}"#,
            METADATA_VERSION + 1
        );
        assert!(matches!(
            from_json(&json),
            Err(MetadataError::UnsupportedVersion { found }) if found == METADATA_VERSION + 1
        ));
    }

    #[test]
    fn unknown_font_falls_back_to_default_and_is_reported() {
        let registry = FontRegistry::new();
        let metadata = OverlayMetadata {
            version: METADATA_VERSION,
            overlays: vec![
                OverlayRecord {
                    page: 1,
                    x: 72.0,
                    y: 720.0,
                    text: "Ghost font".to_string(),
                    font_family: "Uninstalled Family".to_string(),
                    font_size: 12.0,
                    width: None,
                    min_height: None,
                },
                OverlayRecord {
                    page: 1,
                    x: 72.0,
                    y: 700.0,
                    text: "Same ghost".to_string(),
                    font_family: "Uninstalled Family".to_string(),
                    font_size: 12.0,
                    width: None,
                    min_height: None,
                },
            ],
            streams: vec![],
        };
        let restored = resolve_overlays(&metadata, &registry);
        assert_eq!(restored.overlays[0].font, registry.default_font());
        assert_eq!(
            restored.missing_fonts,
            vec!["Uninstalled Family".to_string()],
            "each missing family is reported once"
        );
    }
}
