//! Schema types (serde) and compiled output for the sloth layout config.

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

use crate::key::{Key, KeyChord};

/// Modifier bitset. Low four bits (SHIFT/CTRL/ALT/META) mirror the layout used
/// by `sloth-core` so downstream conversion is a plain bit copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers(u16);

impl Modifiers {
    pub const SHIFT: Self = Self(0b0001);
    pub const CTRL: Self = Self(0b0010);
    pub const ALT: Self = Self(0b0100);
    pub const META: Self = Self(0b1000);

    pub const fn empty() -> Self {
        Self(0)
    }
    pub const fn bits(self) -> u16 {
        self.0
    }
    pub const fn from_bits_truncate(bits: u16) -> Self {
        Self(bits & 0b1111)
    }
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecialKey {
    Backspace,
    Enter,
    Tab,
    Escape,
    Left,
    Right,
    Up,
    Down,
}

/// A single output element. This is the canonical, parser-agnostic token set:
/// both `sloth-parser` (TOML/JSON) and `dvorakj-parser` emit these so that
/// `sloth-core` has exactly one representation to consume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputToken {
    /// Literal text (the common case for TOML/JSON cell output).
    Text(String),
    /// A key press, optionally with modifiers.
    Key { code: Key, mods: Modifiers },
    /// A named special key (arrow/editor keys, etc.).
    Named(SpecialKey),
    /// Press-and-hold a modifier (layer-style).
    ModDown(Key),
    /// Release a held modifier.
    ModUp(Key),
}

pub type OutputSeq = Vec<OutputToken>;

/// Physical keyboard layout the grid is compiled against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardLayout {
    Jis,
    #[default]
    Us,
}

// ---------------------------------------------------------------------------
// Schema (deserialized directly from TOML/JSON)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub meta: Meta,
    #[serde(default)]
    pub layers: HashMap<String, Layer>,
    /// 同時押し (combo)。個別エントリ `"a,b" = "@"` または、トリガ鍵ごとの
    /// 位置指定グリッド `[combos.k] grid = [...]` のいずれも可（混在可能）。
    #[serde(default)]
    pub combos: HashMap<String, ComboValue>,
    #[serde(default)]
    pub sequences: HashMap<String, String>,
    #[serde(default)]
    pub states: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Meta {
    pub name: String,
    #[serde(default)]
    pub keyboard: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

/// A single combo entry value: either a plain `"a,b" = "@"` string, or a
/// positional grid under `[combos.k]` (mirroring the DvorakJ `-25[...]` block).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ComboValue {
    /// `"a,b" = "@"` — one chord (trigger keys) → output.
    Single(String),
    /// `[combos.k] grid = [...]` — the trigger key's positional output grid.
    Grid(ComboGrid),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComboGrid {
    pub grid: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OverrideSpec {
    /// Positional grid (rows × cols), the same shape as a layer `grid`.
    /// Lets `override` be written exactly like `[layers.base]`.
    #[serde(default)]
    pub grid: Option<Vec<Vec<String>>>,
    /// Named trigger → output overrides (flattened from the table body).
    #[serde(flatten, default)]
    pub map: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Layer {
    /// Positional grid (rows × cols) of output strings.
    #[serde(default)]
    pub grid: Option<Vec<Vec<String>>>,
    /// Name of a layer whose effective map is inherited before `override`.
    #[serde(default)]
    pub inherit: Option<String>,
    /// Overrides applied on top of `inherit`/`grid`. May be a positional
    /// `grid` (written like `[layers.base]`) and/or named trigger → output
    /// entries.
    #[serde(rename = "override", default)]
    pub override_: Option<OverrideSpec>,
}

// ---------------------------------------------------------------------------
// Compiled output
// ---------------------------------------------------------------------------

/// Effective trigger-key → output map for one named layer.
#[derive(Debug, Clone, Default)]
pub struct CompiledLayer {
    pub keys: BTreeMap<Key, OutputSeq>,
}

/// Fully compiled layout, ready for downstream conversion.
#[derive(Debug, Clone)]
pub struct CompiledLayout {
    pub name: String,
    pub keyboard: KeyboardLayout,
    pub layers: HashMap<String, CompiledLayer>,
    /// 同時押し: a canonically-ordered key set → output.
    pub combos: BTreeMap<KeyChord, OutputSeq>,
    /// 順押し: an ordered key list → output.
    pub sequences: BTreeMap<Vec<Key>, OutputSeq>,
    /// State name → layer name selection (IME/modifier switching).
    pub states: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ParseError {
    Toml(toml::de::Error),
    Json(serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// `inherit` referenced a layer that does not exist.
    MissingLayer(String),
    /// A trigger name (override/combo/sequence) could not be resolved.
    UnknownKey(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Toml(e) => write!(f, "toml: {e}"),
            ParseError::Json(e) => write!(f, "json: {e}"),
        }
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::MissingLayer(n) => write!(f, "missing layer '{n}'"),
            CompileError::UnknownKey(n) => write!(f, "unknown key '{n}'"),
        }
    }
}

impl std::error::Error for ParseError {}
impl std::error::Error for CompileError {}

/// Combined error for "parse + compile" convenience entry points.
#[derive(Debug)]
pub enum Error {
    Parse(ParseError),
    Compile(CompileError),
}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Error::Parse(e)
    }
}

impl From<CompileError> for Error {
    fn from(e: CompileError) -> Self {
        Error::Compile(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(e) => write!(f, "parse: {e}"),
            Error::Compile(e) => write!(f, "compile: {e}"),
        }
    }
}

impl std::error::Error for Error {}
