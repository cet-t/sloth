//! Byte-level decoding (`encoding` feature).
//!
//! Provides [`parse_bytes`], which decodes a raw `.txt` byte buffer
//! (Shift-JIS / UTF-8 / BOM) and parses it. Decode rules mirror the legacy
//! `DvorakJLayoutLoader::load`:
//!
//! | condition                        | keyboard | decode                        |
//! |----------------------------------|----------|-------------------------------|
//! | `source_id.ends_with(".en.txt")` | Us       | UTF-8 lossy                   |
//! | `source_id.ends_with(".jp.txt")` | Jis      | Shift-JIS                     |
//! | otherwise                        | Jis      | BOM strip → UTF-8 → SJIS       |

use crate::model::{KeyboardLayout, ParseOptions, ParseReport, ParseResult};

/// Decode `bytes` per `source_id`, then parse.
///
/// The keyboard in the returned layout is derived from `source_id` (overriding
/// `options.keyboard`), matching the legacy loader semantics.
pub fn parse_bytes(
    bytes: &[u8],
    source_id: &str,
    options: ParseOptions,
) -> ParseResult<ParseReport> {
    let (keyboard, text) = decode(bytes, source_id);
    let opts = ParseOptions {
        source_id: Some(source_id.to_string()),
        keyboard,
        strict: options.strict,
    };
    crate::parse_str(&text, opts)
}

/// Decode raw bytes to `(keyboard, text)` using the extension/BOM rules above.
pub fn decode(bytes: &[u8], source_id: &str) -> (KeyboardLayout, String) {
    if source_id.ends_with(".en.txt") {
        (
            KeyboardLayout::Us,
            String::from_utf8_lossy(bytes).into_owned(),
        )
    } else if source_id.ends_with(".jp.txt") {
        (
            KeyboardLayout::Jis,
            encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
        )
    } else {
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        let text = match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
        };
        (KeyboardLayout::Jis, text)
    }
}
