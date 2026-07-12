//! DvorakJ layout file parser.
//!
//! Parses DvorakJ-style `.txt` layout files into a dependency-free
//! [`ParsedLayout`]. The default (rlib) build has zero external dependencies
//! and accepts already-decoded `&str`. Conversion to `sloth_core::layout::Layout`
//! lives in the separate `sloth-dvorakj-adapter` crate.
//!
//! # Features
//! - `encoding`: adds [`parse_bytes`] (Shift-JIS/UTF-8 decode via `encoding_rs`).
//! - `json`: serde-based JSON serialization of [`ParseReport`].
//! - `ffi`: C ABI layer for single-file DLL/SO use (implies `json`).

mod block;
mod cell;
mod grid;
mod keymap;
mod model;
mod parse;

pub use model::{
    canonical_key_order, sort_keys_canonical, InputMode, Key, KeyChord, KeyboardLayout, LayoutMode,
    Modifiers, OutputSeq, OutputToken, ParseError, ParseOptions, ParseReport, ParseResult,
    ParseWarning, ParsedLayout, SpecialKey,
};

#[cfg(feature = "encoding")]
pub mod decode;
#[cfg(feature = "encoding")]
pub use decode::parse_bytes;

#[cfg(feature = "json")]
pub mod json;

#[cfg(feature = "ffi")]
pub mod ffi;

#[cfg(feature = "sloth")]
pub mod sloth;

/// Parse an already-decoded DvorakJ layout string into a [`ParseReport`].
///
/// Comment blocks (`/* … */`) are stripped internally, matching the legacy
/// loader. In lenient mode (`options.strict == false`) unknown triggers and
/// undefined layers are skipped and recorded in [`ParseReport::warnings`].
pub fn parse_str(text: &str, options: ParseOptions) -> ParseResult<ParseReport> {
    let stripped = strip_comments(text);
    parse::parse_report(&stripped, &options)
}

/// A reusable parser that holds [`ParseOptions`].
#[derive(Debug, Clone)]
pub struct DvorakJParser {
    options: ParseOptions,
}

impl DvorakJParser {
    pub fn new(options: ParseOptions) -> Self {
        Self { options }
    }

    pub fn parse_str(&self, text: &str) -> ParseResult<ParseReport> {
        parse_str(text, self.options.clone())
    }
}

/// Strip C-style `/* … */` comment blocks. Single pass; nested comments are not
/// supported (DvorakJ files use a single layer).
pub(crate) fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c == '/' && it.peek() == Some(&'*') {
            it.next();
            while let Some(c2) = it.next() {
                if c2 == '*' && it.peek() == Some(&'/') {
                    it.next();
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
