//! LayoutLoader trait and default-loader registry.
//!
//! The concrete DvorakJ parser lives in the `dvorakj-parser` crate.
//! Callers register a loader at startup via [`register_default_loader`]; the
//! hook and other internal consumers obtain it through [`default_loader`].

use crate::layout::Layout;
use std::sync::OnceLock;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("Encoding (expected Shift-JIS/CP932): {0}")]
    Encoding(String),
    #[error("Parse: {0}")]
    Parse(String),
    #[error("Unknown trigger name: {0}")]
    UnknownTrigger(String),
    #[error("Schema: {0}")]
    Schema(String),
}

pub trait LayoutLoader: Send + Sync {
    fn load(&self, bytes: &[u8], id: &str) -> Result<Layout, LoadError>;
    fn format_name(&self) -> &'static str;
    /// Lowercase, dot-less file extensions this loader is the native format
    /// for (e.g. `&["toml", "json"]`). Used by [`CompositeLoader`] to prefer
    /// the right loader by file extension before falling back to trying the
    /// rest in registration order. Default: none.
    ///
    /// This matters because "try each loader, keep the first success" is
    /// unsafe on its own: a lenient parser for one format may not cleanly
    /// reject another format's input (e.g. a lenient DvorakJ `.txt` parser
    /// can extract a garbage "layout" from a TOML file's comment lines
    /// instead of erroring), so trying loaders in the wrong order can
    /// silently produce a nonsense layout instead of the intended one.
    fn extensions(&self) -> &'static [&'static str] {
        &[]
    }
}

/// Dispatches to one of several format-specific loaders. Lets a caller
/// support multiple on-disk layout formats (e.g. DvorakJ `.txt` and sloth
/// TOML/JSON) behind a single registered [`LayoutLoader`], without
/// sloth-core needing to know about any concrete format itself.
///
/// `id`'s file extension picks the candidate set: if any loader declares
/// that extension (via [`LayoutLoader::extensions`]), *only* those loaders
/// are tried (in registration order) and a parse failure is reported as-is
/// -- never papered over by handing the bytes to some other format's
/// (possibly lenient) parser, which could "succeed" with a nonsense layout
/// (see [`LayoutLoader::extensions`]'s doc comment). Only when no loader
/// claims the extension (or `id` has none) are all loaders tried in
/// registration order, first success winning.
pub struct CompositeLoader {
    loaders: Vec<Box<dyn LayoutLoader>>,
}

impl CompositeLoader {
    pub fn new(loaders: Vec<Box<dyn LayoutLoader>>) -> Self {
        Self { loaders }
    }
}

fn extension_of(id: &str) -> Option<String> {
    id.rsplit_once('.').map(|(_, ext)| ext.to_lowercase())
}

impl LayoutLoader for CompositeLoader {
    fn format_name(&self) -> &'static str {
        "auto"
    }

    fn load(&self, bytes: &[u8], id: &str) -> Result<Layout, LoadError> {
        let ext = extension_of(id);
        let matches_ext = |l: &dyn LayoutLoader| {
            ext.as_deref().is_some_and(|e| l.extensions().contains(&e))
        };
        let by_ext: Vec<&dyn LayoutLoader> = self
            .loaders
            .iter()
            .map(|l| l.as_ref())
            .filter(|l| matches_ext(*l))
            .collect();
        let candidates: Vec<&dyn LayoutLoader> = if by_ext.is_empty() {
            self.loaders.iter().map(|l| l.as_ref()).collect()
        } else {
            by_ext
        };

        let mut last_err = None;
        for loader in candidates {
            match loader.load(bytes, id) {
                Ok(layout) => return Ok(layout),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| LoadError::Parse("no loaders registered".to_string())))
    }
}

static DEFAULT_LOADER: OnceLock<Box<dyn LayoutLoader>> = OnceLock::new();

pub fn register_default_loader(loader: Box<dyn LayoutLoader>) {
    DEFAULT_LOADER.set(loader).ok();
}

pub fn default_loader() -> &'static dyn LayoutLoader {
    DEFAULT_LOADER
        .get()
        .expect("loader must be registered before use")
        .as_ref()
}
