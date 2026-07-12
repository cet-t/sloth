//! Adapter bridging the dependency-free `dvorakj-parser` to `sloth-core`.
//!
//! DvorakJ `.txt` bytes are decoded/parsed by `dvorakj-parser`, converted
//! into `sloth_parser::CompiledLayout` (the same canonical "sloth format"
//! representation hand-written TOML/JSON layouts compile into -- see
//! `dvorakj-parser`'s `sloth` feature), and then compiled into
//! `sloth_core::Layout` via `sloth_core::sloth_parser::to_core_layout`, the
//! one shared bridge every source format goes through. This crate is now
//! just the DvorakJ-specific half of that pipeline (decode + parse); the
//! `CompiledLayout -> Layout` step is shared with the native sloth loader.

use sloth_core::layout::Layout;
use sloth_core::loader::{LayoutLoader, LoadError};

/// `LayoutLoader` implementation that decodes DvorakJ `.txt` bytes with the
/// parser's `encoding` feature and converts the result to `sloth_core::Layout`
/// by way of `sloth_parser::CompiledLayout`.
#[derive(Default)]
pub struct RmapDvorakJLayoutLoader;

impl RmapDvorakJLayoutLoader {
    pub fn new() -> Self {
        Self
    }
}

impl LayoutLoader for RmapDvorakJLayoutLoader {
    fn format_name(&self) -> &'static str {
        "dvorakj"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["txt"]
    }

    fn load(&self, bytes: &[u8], id: &str) -> Result<Layout, LoadError> {
        let options = dvorakj_parser::ParseOptions::from_source_id(id);
        let report = dvorakj_parser::parse_bytes(bytes, id, options)
            .map_err(|e| LoadError::Parse(e.to_string()))?;
        let compiled = dvorakj_parser::sloth::to_compiled_layout(report.layout);
        Ok(sloth_core::sloth_parser::to_core_layout(compiled, id))
    }
}
