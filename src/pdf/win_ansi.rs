//! WinAnsiEncoding (Windows code page 1252), the single encoding the overlay
//! fonts declare.
//!
//! Overlay text is Rust `String` (Unicode), but a PDF shows text as bytes
//! interpreted through the font's `/Encoding`. Everything that has to agree on
//! what byte means what character — the text emitted in the content stream, the
//! `/Widths` array indexed by character code, and the `ToUnicode` CMap that
//! makes the text extractable — derives from the one table below, so the three
//! cannot drift apart.

use std::borrow::Cow;
use std::ops::RangeInclusive;

use unicode_normalization::{UnicodeNormalization, is_nfc};

/// Codes that WinAnsiEncoding shares with ASCII, mapping to the same scalar.
const ASCII_RANGE: RangeInclusive<u8> = 0x20..=0x7E;

/// Codes that WinAnsiEncoding shares with Latin-1, mapping to the same scalar.
const LATIN1_RANGE: RangeInclusive<u8> = 0xA0..=0xFF;

/// The 27 codes in 0x80-0x9F where WinAnsiEncoding diverges from Latin-1,
/// which reserves that block for C1 control characters. The five remaining
/// codes in the block (0x81, 0x8D, 0x8F, 0x90, 0x9D) are undefined.
const HIGH_CONTROL_BLOCK: &[(u8, char)] = &[
    (0x80, '\u{20AC}'),
    (0x82, '\u{201A}'),
    (0x83, '\u{0192}'),
    (0x84, '\u{201E}'),
    (0x85, '\u{2026}'),
    (0x86, '\u{2020}'),
    (0x87, '\u{2021}'),
    (0x88, '\u{02C6}'),
    (0x89, '\u{2030}'),
    (0x8A, '\u{0160}'),
    (0x8B, '\u{2039}'),
    (0x8C, '\u{0152}'),
    (0x8E, '\u{017D}'),
    (0x91, '\u{2018}'),
    (0x92, '\u{2019}'),
    (0x93, '\u{201C}'),
    (0x94, '\u{201D}'),
    (0x95, '\u{2022}'),
    (0x96, '\u{2013}'),
    (0x97, '\u{2014}'),
    (0x98, '\u{02DC}'),
    (0x99, '\u{2122}'),
    (0x9A, '\u{0161}'),
    (0x9B, '\u{203A}'),
    (0x9C, '\u{0153}'),
    (0x9E, '\u{017E}'),
    (0x9F, '\u{0178}'),
];

/// Stands in for characters WinAnsiEncoding cannot represent. Chosen because
/// every font that declares WinAnsiEncoding has a glyph for it, so the loss is
/// visible in the page rather than silently swallowed.
pub const SUBSTITUTE: u8 = b'?';

/// Text encoded for a PDF content stream, plus whatever had to be substituted.
pub struct EncodedText {
    pub bytes: Vec<u8>,
    /// Characters with no WinAnsiEncoding code, in first-seen order, each listed
    /// once. Empty when the text encoded losslessly.
    pub unencodable: Vec<char>,
}

/// The WinAnsiEncoding code for `c`, or `None` if the encoding has none.
pub fn encode_char(c: char) -> Option<u8> {
    if let Ok(code) = u8::try_from(u32::from(c))
        && (ASCII_RANGE.contains(&code) || LATIN1_RANGE.contains(&code))
    {
        return Some(code);
    }
    HIGH_CONTROL_BLOCK
        .iter()
        .find(|(_, mapped)| *mapped == c)
        .map(|(code, _)| *code)
}

/// The character WinAnsiEncoding assigns to `code`, or `None` if undefined.
pub fn decode(code: u8) -> Option<char> {
    if ASCII_RANGE.contains(&code) || LATIN1_RANGE.contains(&code) {
        return Some(char::from(code));
    }
    HIGH_CONTROL_BLOCK
        .iter()
        .find(|(mapped, _)| *mapped == code)
        .map(|(_, c)| *c)
}

/// Add `chars` to `seen`, skipping any it already holds, so a report names each
/// character once and in the order it was first met.
pub fn merge_unencodable(seen: &mut Vec<char>, chars: impl IntoIterator<Item = char>) {
    for c in chars {
        if !seen.contains(&c) {
            seen.push(c);
        }
    }
}

/// Encode overlay text for a content stream, substituting [`SUBSTITUTE`] for
/// characters the encoding cannot represent and reporting them to the caller.
///
/// Text is NFC-normalized first: input arriving as NFD (a base letter plus a
/// combining mark, e.g. some IMEs and macOS paste) has no direct WinAnsiEncoding
/// code for the combining mark alone, so without normalization "café" would
/// encode as "cafe?" instead of finding WinAnsi's precomposed 'é'.
pub fn encode(text: &str) -> EncodedText {
    let normalized: Cow<str> = if is_nfc(text) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.nfc().collect())
    };
    let mut bytes = Vec::with_capacity(normalized.len());
    let mut unencodable: Vec<char> = Vec::new();
    for c in normalized.chars() {
        match encode_char(c) {
            Some(code) => bytes.push(code),
            None => {
                bytes.push(SUBSTITUTE);
                merge_unencodable(&mut unencodable, [c]);
            }
        }
    }
    EncodedText { bytes, unencodable }
}

/// Build a ToUnicode CMap (PDF 32000-1 §9.10.3) mapping WinAnsiEncoding
/// character codes to Unicode, so readers can extract, copy and search text
/// shown in an embedded TrueType font instead of treating it as unmappable
/// glyphs.
///
/// The ASCII and Latin-1 stretches are emitted as ranges; only the 0x80-0x9F
/// block, which diverges from Latin-1, needs per-code entries.
pub fn to_unicode_cmap() -> String {
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n\
         /CMapType 2 def\n\
         1 begincodespacerange\n",
    );
    cmap.push_str(&format!(
        "<{:02X}> <{:02X}>\nendcodespacerange\n2 beginbfrange\n",
        ASCII_RANGE.start(),
        LATIN1_RANGE.end()
    ));
    for range in [&ASCII_RANGE, &LATIN1_RANGE] {
        cmap.push_str(&format!(
            "<{:02X}> <{:02X}> <{:04X}>\n",
            range.start(),
            range.end(),
            u32::from(*range.start())
        ));
    }
    cmap.push_str(&format!(
        "endbfrange\n{} beginbfchar\n",
        HIGH_CONTROL_BLOCK.len()
    ));
    for &(code, c) in HIGH_CONTROL_BLOCK {
        cmap.push_str(&format!("<{code:02X}> <{:04X}>\n", u32::from(c)));
    }
    cmap.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 27 divergent CP1252 characters, written out independently of the
    /// production table so a corrupted entry cannot pass by agreeing with itself.
    const DIVERGENT: &[(char, u8)] = &[
        ('€', 0x80),
        ('‚', 0x82),
        ('ƒ', 0x83),
        ('„', 0x84),
        ('…', 0x85),
        ('†', 0x86),
        ('‡', 0x87),
        ('ˆ', 0x88),
        ('‰', 0x89),
        ('Š', 0x8A),
        ('‹', 0x8B),
        ('Œ', 0x8C),
        ('Ž', 0x8E),
        ('\u{2018}', 0x91),
        ('\u{2019}', 0x92),
        ('\u{201C}', 0x93),
        ('\u{201D}', 0x94),
        ('•', 0x95),
        ('–', 0x96),
        ('—', 0x97),
        ('˜', 0x98),
        ('™', 0x99),
        ('š', 0x9A),
        ('›', 0x9B),
        ('œ', 0x9C),
        ('ž', 0x9E),
        ('Ÿ', 0x9F),
    ];

    /// Codes WinAnsiEncoding leaves undefined: the C0 range, DEL, and the five
    /// unassigned slots in the 0x80-0x9F block.
    const UNDEFINED_CODES: &[u8] = &[0x00, 0x09, 0x0A, 0x1F, 0x7F, 0x81, 0x8D, 0x8F, 0x90, 0x9D];

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
        let cmap = to_unicode_cmap();

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
        let cmap = to_unicode_cmap();

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

    /// The CMap must resolve every code to exactly the character `decode` names,
    /// which is what stops the extraction table drifting from the bytes written.
    #[test]
    fn tounicode_cmap_agrees_with_the_encoding_table_for_every_code() {
        let cmap = to_unicode_cmap();
        for code in 0u8..=0xFF {
            assert_eq!(
                cmap_lookup(&cmap, code).and_then(char::from_u32),
                decode(code),
                "CMap disagrees with the encoding table at {code:#04X}"
            );
        }
    }

    #[test]
    fn ascii_characters_encode_to_their_own_byte() {
        for code in 0x20u8..=0x7E {
            let c = char::from(code);
            assert_eq!(encode_char(c), Some(code), "ASCII {c:?}");
        }
    }

    #[test]
    fn latin1_characters_encode_to_their_own_byte() {
        for code in 0xA0u8..=0xFF {
            let c = char::from(code);
            assert_eq!(encode_char(c), Some(code), "Latin-1 {c:?}");
        }
    }

    #[test]
    fn divergent_cp1252_characters_encode_to_their_win_ansi_code() {
        for &(c, code) in DIVERGENT {
            assert_eq!(encode_char(c), Some(code), "divergent {c:?}");
        }
    }

    #[test]
    fn c1_control_characters_have_no_win_ansi_code() {
        for code in 0x80u32..=0x9F {
            let c = char::from_u32(code).expect("C1 controls are valid scalars");
            assert_eq!(encode_char(c), None, "C1 control U+{code:04X}");
        }
    }

    #[test]
    fn characters_outside_win_ansi_have_no_code() {
        for c in ['中', '😀', 'Ā', '\u{7F}', '\t', '\n', 'ﬁ'] {
            assert_eq!(encode_char(c), None, "unencodable {c:?}");
        }
    }

    #[test]
    fn decode_rejects_codes_win_ansi_leaves_undefined() {
        for &code in UNDEFINED_CODES {
            assert_eq!(decode(code), None, "undefined code {code:#04X}");
        }
    }

    #[test]
    fn decode_maps_divergent_codes_to_their_cp1252_character() {
        for &(c, code) in DIVERGENT {
            assert_eq!(decode(code), Some(c), "divergent code {code:#04X}");
        }
    }

    /// The anti-drift guarantee: every code the encoding defines survives a
    /// decode/encode round trip, so the byte the writer emits, the slot the
    /// `/Widths` array uses, and the CMap entry all name the same character.
    #[test]
    fn every_defined_code_round_trips_through_decode_and_encode() {
        let defined: Vec<u8> = (0u8..=0xFF).filter(|&c| decode(c).is_some()).collect();
        assert_eq!(
            defined.len(),
            0x5F + 0x60 + 27,
            "WinAnsiEncoding defines 218 codes"
        );
        for code in defined {
            let c = decode(code).expect("filtered to defined codes");
            assert_eq!(encode_char(c), Some(code), "round trip of {code:#04X}");
        }
    }

    #[test]
    fn encode_maps_mixed_text_to_win_ansi_bytes() {
        let encoded = encode("café €5");
        assert_eq!(
            encoded.bytes,
            vec![b'c', b'a', b'f', 0xE9, b' ', 0x80, b'5']
        );
        assert!(encoded.unencodable.is_empty());
    }

    #[test]
    fn encode_substitutes_question_mark_for_unencodable_characters() {
        let encoded = encode("a中b");
        assert_eq!(encoded.bytes, vec![b'a', SUBSTITUTE, b'b']);
    }

    #[test]
    fn encode_reports_each_unencodable_character_once_in_first_seen_order() {
        let encoded = encode("中😀中");
        assert_eq!(encoded.unencodable, vec!['中', '😀']);
    }

    /// "café" typed on some input methods (or pasted from macOS) arrives as
    /// NFD: 'e' followed by a combining acute accent, rather than the
    /// precomposed 'é'. WinAnsiEncoding only has a code for the precomposed
    /// form, so without normalization the accent is silently dropped to '?'.
    #[test]
    fn encode_normalizes_nfd_input_to_nfc_before_encoding() {
        let nfd = "cafe\u{0301}"; // e + combining acute accent (U+0301)
        let nfc = "café"; // precomposed é (U+00E9)
        assert_eq!(encode(nfd).bytes, encode(nfc).bytes);
        assert!(encode(nfd).unencodable.is_empty());
    }
}
