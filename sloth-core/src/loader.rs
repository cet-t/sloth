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
