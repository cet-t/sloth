//! Dependency-free domain types for the DvorakJ parser.
//!
//! These mirror the shapes previously borrowed from `sloth-core`, but live
//! entirely inside this crate so the default (rlib) build has zero
//! dependencies. Conversion to `sloth_core::layout::Layout` is the caller's
//! responsibility (see the `sloth-dvorakj-adapter` crate).

use std::collections::{BTreeMap, BTreeSet};

/// Canonical key identifier used by the DvorakJ parser. Intentionally kept
/// variant-for-variant compatible with `sloth_core::KeyCode` so the sloth
/// adapter can convert with a total `match`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Minus,
    Equal,
    LBracket,
    RBracket,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Dot,
    Slash,
    Grave,
    ShiftL,
    ShiftR,
    CtrlL,
    CtrlR,
    AltL,
    AltR,
    MetaL,
    MetaR,
    Space,
    Enter,
    Tab,
    Backspace,
    Escape,
    CapsLock,
    Left,
    Right,
    Up,
    Down,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Muhenkan,
    Henkan,
    KanaKatakana,
    HankakuZenkaku,
    Yen,
    Caret,
    Colon,
    AtSign,
    Unknown(u32),
}

impl Key {
    /// Resolve a DvorakJ trigger/key name to a [`Key`]. Mirrors the table that
    /// previously lived in `sloth_core::KeyCode::from_dvorakj_name`.
    pub fn from_dvorakj_name(name: &str) -> Option<Key> {
        match name.to_lowercase().as_str() {
            "a" => Some(Key::A),
            "b" => Some(Key::B),
            "c" => Some(Key::C),
            "d" => Some(Key::D),
            "e" => Some(Key::E),
            "f" => Some(Key::F),
            "g" => Some(Key::G),
            "h" => Some(Key::H),
            "i" => Some(Key::I),
            "j" => Some(Key::J),
            "k" => Some(Key::K),
            "l" => Some(Key::L),
            "m" => Some(Key::M),
            "n" => Some(Key::N),
            "o" => Some(Key::O),
            "p" => Some(Key::P),
            "q" => Some(Key::Q),
            "r" => Some(Key::R),
            "s" => Some(Key::S),
            "t" => Some(Key::T),
            "u" => Some(Key::U),
            "v" => Some(Key::V),
            "w" => Some(Key::W),
            "x" => Some(Key::X),
            "y" => Some(Key::Y),
            "z" => Some(Key::Z),
            "0" | "num0" => Some(Key::Num0),
            "1" | "num1" => Some(Key::Num1),
            "2" | "num2" => Some(Key::Num2),
            "3" | "num3" => Some(Key::Num3),
            "4" | "num4" => Some(Key::Num4),
            "5" | "num5" => Some(Key::Num5),
            "6" | "num6" => Some(Key::Num6),
            "7" | "num7" => Some(Key::Num7),
            "8" | "num8" => Some(Key::Num8),
            "9" | "num9" => Some(Key::Num9),
            "space" => Some(Key::Space),
            "enter" | "return" => Some(Key::Enter),
            "tab" => Some(Key::Tab),
            "bs" | "backspace" => Some(Key::Backspace),
            "esc" | "escape" => Some(Key::Escape),
            "capslock" | "caps" => Some(Key::CapsLock),
            "lshift" | "shift" => Some(Key::ShiftL),
            "rshift" => Some(Key::ShiftR),
            "lctrl" | "lcontrol" => Some(Key::CtrlL),
            "rctrl" | "rcontrol" => Some(Key::CtrlR),
            "lalt" => Some(Key::AltL),
            "ralt" => Some(Key::AltR),
            "lwin" | "lmeta" => Some(Key::MetaL),
            "rwin" | "rmeta" => Some(Key::MetaR),
            "muhenkan" => Some(Key::Muhenkan),
            "henkan" => Some(Key::Henkan),
            "kana" | "kanakatakana" => Some(Key::KanaKatakana),
            "hankaku" | "zenkaku" | "hankakuzenkaku" => Some(Key::HankakuZenkaku),
            "yen" => Some(Key::Yen),
            "^" | "caret" => Some(Key::Caret),
            ":" | "colon" => Some(Key::Colon),
            "@" | "at" | "atmark" => Some(Key::AtSign),
            "left" => Some(Key::Left),
            "right" => Some(Key::Right),
            "up" => Some(Key::Up),
            "down" => Some(Key::Down),
            _ => None,
        }
    }
}

/// Physical keyboard layout the grid is compiled against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardLayout {
    Jis,
    #[default]
    Us,
}

impl KeyboardLayout {
    /// `.en.txt` → US, everything else → JIS. Mirrors the current loader.
    pub fn from_source_id(source_id: &str) -> Self {
        if source_id.ends_with(".en.txt") {
            KeyboardLayout::Us
        } else {
            KeyboardLayout::Jis
        }
    }
}

/// Layout routing mode, detected from the first line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutMode {
    #[default]
    Legacy,
    Sequential,
    Simultaneous,
    Mixed,
}

/// Per-layout input interpretation. The parser always emits [`InputMode::Direct`];
/// the other variants are reserved for future kana/romaji support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Direct,
    Romaji,
    Kana,
}

/// Modifier bitset. Dependency-free newtype (no `bitflags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers(u16);

impl Modifiers {
    pub const SHIFT: Self = Self(0b0000_0000_0000_0001);
    pub const CTRL: Self = Self(0b0000_0000_0000_0010);
    pub const ALT: Self = Self(0b0000_0000_0000_0100);
    pub const META: Self = Self(0b0000_0000_0000_1000);

    pub const fn empty() -> Self {
        Self(0)
    }
    pub const fn bits(self) -> u16 {
        self.0
    }
    pub const fn from_bits_truncate(bits: u16) -> Self {
        Self(bits)
    }
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

/// A single output element produced by a cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputToken {
    Key { code: Key, mods: Modifiers },
    Text(String),
    Named(SpecialKey),
    ModDown(Key),
    ModUp(Key),
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

pub type OutputSeq = Vec<OutputToken>;

/// A canonically-ordered set of keys used as a `combos` key. Public
/// construction always canonicalizes; the parser uses [`KeyChord::from_vec`]
/// internally to preserve exact legacy ordering for layer/prefix keys.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyChord(Vec<Key>);

impl KeyChord {
    /// Build a chord, applying canonical ordering (hides press/parse-order
    /// jitter from external callers).
    pub fn new(keys: impl Into<Vec<Key>>) -> Self {
        let mut v = keys.into();
        sort_keys_canonical(&mut v);
        Self(v)
    }

    /// Build a chord from an already-ordered vec, preserving its order exactly.
    /// Used internally to reproduce the legacy `key_sort` ordering for
    /// `layer_maps` / `prefix_maps` keys.
    pub(crate) fn from_vec(keys: Vec<Key>) -> Self {
        Self(keys)
    }

    pub fn as_slice(&self) -> &[Key] {
        &self.0
    }
    pub fn into_vec(self) -> Vec<Key> {
        self.0
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn contains(&self, k: &Key) -> bool {
        self.0.contains(k)
    }
}

/// Primary sort rank for a key. Mirrors `sloth_core::layout::canon_key_order`.
pub fn canonical_key_order(k: Key) -> u16 {
    match k {
        Key::Space => 1,
        Key::ShiftL => 2,
        Key::ShiftR => 3,
        Key::CtrlL => 4,
        Key::CtrlR => 5,
        Key::AltL => 6,
        Key::AltR => 7,
        Key::MetaL => 8,
        Key::MetaR => 9,
        Key::Muhenkan => 10,
        Key::Henkan => 11,
        Key::KanaKatakana => 12,
        Key::HankakuZenkaku => 13,
        Key::Yen => 14,
        Key::Caret => 15,
        Key::Colon => 16,
        Key::AtSign => 17,
        Key::Unknown(_) => 200,
        _ => 100,
    }
}

/// Canonical sort (rank + Debug tie-break). Mirrors `sloth_core::layout::canon_sort`.
pub fn sort_keys_canonical(keys: &mut [Key]) {
    keys.sort_by_key(|k| (canonical_key_order(*k), format!("{:?}", k)));
}

/// Rank-only stable sort. Reproduces the legacy `block.rs::key_sort` behavior
/// used for `layer_maps` / `prefix_maps` keys (no Debug tie-break; ties keep
/// input order via the stable sort).
pub(crate) fn sort_keys_by_rank(keys: &mut [Key]) {
    keys.sort_by_key(|k| canonical_key_order(*k));
}

/// Parsed DvorakJ layout. Dependency-free analogue of `sloth_core::layout::Layout`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedLayout {
    pub source_id: Option<String>,
    pub name: String,
    pub mode: LayoutMode,
    pub input_mode: InputMode,
    pub keyboard: KeyboardLayout,
    pub single_map: BTreeMap<Key, OutputSeq>,
    pub layer_maps: BTreeMap<KeyChord, BTreeMap<Key, OutputSeq>>,
    pub layer_taps: BTreeMap<Key, OutputSeq>,
    pub layer_triggers: BTreeSet<Key>,
    pub combos: BTreeMap<KeyChord, OutputSeq>,
    pub combo_keys: BTreeSet<Key>,
    pub sustained_triggers: BTreeSet<Key>,
    pub prefix_maps: BTreeMap<KeyChord, BTreeMap<Key, OutputSeq>>,
    pub prefix_triggers: BTreeSet<Key>,
}

/// Options controlling a parse. `strict=false` reproduces the legacy lenient
/// parser; `strict=true` surfaces unknown triggers / malformed blocks as errors.
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    pub source_id: Option<String>,
    pub keyboard: KeyboardLayout,
    pub strict: bool,
}

impl ParseOptions {
    /// Derive options from a source id (extension decides keyboard).
    pub fn from_source_id(source_id: impl Into<String>) -> Self {
        let sid = source_id.into();
        let keyboard = KeyboardLayout::from_source_id(&sid);
        Self {
            source_id: Some(sid),
            keyboard,
            strict: false,
        }
    }
}

pub type ParseResult<T> = Result<T, ParseError>;

/// A successful parse plus any non-fatal warnings collected in lenient mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseReport {
    pub layout: ParsedLayout,
    pub warnings: Vec<ParseWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    // decode 系（`encoding` feature の parse_bytes / FFI バイト入力でのみ発生）。
    // parse_str(&str) は UTF-8 済み入力を受けるため、これらは返さない。
    UnsupportedEncoding,
    InvalidUtf8,
    // parse 系（strict モードで発生）。
    UnknownTrigger {
        value: String,
        line: Option<usize>,
    },
    MalformedBlock {
        line: Option<usize>,
        message: String,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnsupportedEncoding => write!(f, "unsupported encoding"),
            ParseError::InvalidUtf8 => write!(f, "invalid UTF-8"),
            ParseError::UnknownTrigger { value, line } => {
                write!(f, "unknown trigger '{value}' (line {line:?})")
            }
            ParseError::MalformedBlock { line, message } => {
                write!(f, "malformed block (line {line:?}): {message}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseWarning {
    UnknownTrigger { value: String, line: Option<usize> },
    MissingLayer { name: String, line: Option<usize> },
    SkippedBlock { line: Option<usize>, reason: String },
    DecodeReplacement { source_id: Option<String> },
}
