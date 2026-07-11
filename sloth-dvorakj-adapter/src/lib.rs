//! Adapter bridging the dependency-free `dvorakj-parser` to `sloth-core`.
//!
//! `dvorakj_parser::Key` and `sloth_core::KeyCode` are deliberately distinct
//! types; all conversion is confined here so the parser crate stays free of
//! any `sloth-core` reference. The variant names are kept identical between the
//! two enums so the conversions are exhaustive, total `match`es.

use std::collections::{HashMap, HashSet};

use dvorakj_parser::{
    Key, KeyChord, KeyboardLayout, LayoutMode, Modifiers, OutputSeq, OutputToken, ParsedLayout,
    SpecialKey,
};
use sloth_core::layout::Layout;
use sloth_core::loader::{LayoutLoader, LoadError};

/// `LayoutLoader` implementation that decodes DvorakJ `.txt` bytes with the
/// parser's `encoding` feature and converts the result to `sloth_core::Layout`.
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

    fn load(&self, bytes: &[u8], id: &str) -> Result<Layout, LoadError> {
        let options = dvorakj_parser::ParseOptions::from_source_id(id);
        let report = dvorakj_parser::parse_bytes(bytes, id, options)
            .map_err(|e| LoadError::Parse(e.to_string()))?;
        Ok(to_core_layout(report.layout, id))
    }
}

fn to_core_key(k: Key) -> sloth_core::KeyCode {
    use sloth_core::KeyCode as C;
    match k {
        Key::A => C::A,
        Key::B => C::B,
        Key::C => C::C,
        Key::D => C::D,
        Key::E => C::E,
        Key::F => C::F,
        Key::G => C::G,
        Key::H => C::H,
        Key::I => C::I,
        Key::J => C::J,
        Key::K => C::K,
        Key::L => C::L,
        Key::M => C::M,
        Key::N => C::N,
        Key::O => C::O,
        Key::P => C::P,
        Key::Q => C::Q,
        Key::R => C::R,
        Key::S => C::S,
        Key::T => C::T,
        Key::U => C::U,
        Key::V => C::V,
        Key::W => C::W,
        Key::X => C::X,
        Key::Y => C::Y,
        Key::Z => C::Z,
        Key::Num0 => C::Num0,
        Key::Num1 => C::Num1,
        Key::Num2 => C::Num2,
        Key::Num3 => C::Num3,
        Key::Num4 => C::Num4,
        Key::Num5 => C::Num5,
        Key::Num6 => C::Num6,
        Key::Num7 => C::Num7,
        Key::Num8 => C::Num8,
        Key::Num9 => C::Num9,
        Key::Minus => C::Minus,
        Key::Equal => C::Equal,
        Key::LBracket => C::LBracket,
        Key::RBracket => C::RBracket,
        Key::Backslash => C::Backslash,
        Key::Semicolon => C::Semicolon,
        Key::Quote => C::Quote,
        Key::Comma => C::Comma,
        Key::Dot => C::Dot,
        Key::Slash => C::Slash,
        Key::Grave => C::Grave,
        Key::ShiftL => C::ShiftL,
        Key::ShiftR => C::ShiftR,
        Key::CtrlL => C::CtrlL,
        Key::CtrlR => C::CtrlR,
        Key::AltL => C::AltL,
        Key::AltR => C::AltR,
        Key::MetaL => C::MetaL,
        Key::MetaR => C::MetaR,
        Key::Space => C::Space,
        Key::Enter => C::Enter,
        Key::Tab => C::Tab,
        Key::Backspace => C::Backspace,
        Key::Escape => C::Escape,
        Key::CapsLock => C::CapsLock,
        Key::Left => C::Left,
        Key::Right => C::Right,
        Key::Up => C::Up,
        Key::Down => C::Down,
        Key::F1 => C::F1,
        Key::F2 => C::F2,
        Key::F3 => C::F3,
        Key::F4 => C::F4,
        Key::F5 => C::F5,
        Key::F6 => C::F6,
        Key::F7 => C::F7,
        Key::F8 => C::F8,
        Key::F9 => C::F9,
        Key::F10 => C::F10,
        Key::F11 => C::F11,
        Key::F12 => C::F12,
        Key::Muhenkan => C::Muhenkan,
        Key::Henkan => C::Henkan,
        Key::KanaKatakana => C::KanaKatakana,
        Key::HankakuZenkaku => C::HankakuZenkaku,
        Key::Yen => C::Yen,
        Key::Caret => C::Caret,
        Key::Colon => C::Colon,
        Key::AtSign => C::AtSign,
        Key::Unknown(n) => C::Unknown(n),
    }
}

fn to_core_mods(m: Modifiers) -> sloth_core::Modifiers {
    // Low four bits (SHIFT/CTRL/ALT/META) share the same layout in both enums.
    sloth_core::Modifiers::from_bits_truncate(m.bits())
}

fn to_core_special(s: SpecialKey) -> sloth_core::SpecialKey {
    use sloth_core::SpecialKey as C;
    match s {
        SpecialKey::Backspace => C::Backspace,
        SpecialKey::Enter => C::Enter,
        SpecialKey::Tab => C::Tab,
        SpecialKey::Escape => C::Escape,
        SpecialKey::Left => C::Left,
        SpecialKey::Right => C::Right,
        SpecialKey::Up => C::Up,
        SpecialKey::Down => C::Down,
    }
}

fn to_core_output(t: OutputToken) -> sloth_core::OutputToken {
    use sloth_core::OutputToken as C;
    match t {
        OutputToken::Key { code, mods } => C::Key {
            code: to_core_key(code),
            mods: to_core_mods(mods),
        },
        OutputToken::Text(s) => C::Text(s),
        OutputToken::Named(sp) => C::Named(to_core_special(sp)),
        OutputToken::ModDown(k) => C::ModDown(to_core_key(k)),
        OutputToken::ModUp(k) => C::ModUp(to_core_key(k)),
    }
}

fn to_core_seq(seq: OutputSeq) -> sloth_core::OutputSeq {
    seq.into_iter().map(to_core_output).collect()
}

fn to_core_mode(m: LayoutMode) -> sloth_core::layout::LayoutMode {
    use sloth_core::layout::LayoutMode as C;
    match m {
        LayoutMode::Legacy => C::Legacy,
        LayoutMode::Sequential => C::Sequential,
        LayoutMode::Simultaneous => C::Simultaneous,
        LayoutMode::Mixed => C::Mixed,
    }
}

fn to_core_keyboard(k: KeyboardLayout) -> sloth_core::KeyboardLayout {
    use sloth_core::KeyboardLayout as C;
    match k {
        KeyboardLayout::Jis => C::Jis,
        KeyboardLayout::Us => C::Us,
    }
}

fn chord_to_vec(c: KeyChord) -> Vec<sloth_core::KeyCode> {
    c.into_vec().into_iter().map(to_core_key).collect()
}

fn inner_map(
    map: std::collections::BTreeMap<Key, OutputSeq>,
) -> HashMap<sloth_core::KeyCode, sloth_core::OutputSeq> {
    map.into_iter()
        .map(|(k, v)| (to_core_key(k), to_core_seq(v)))
        .collect()
}

/// Convert a [`ParsedLayout`] to a `sloth_core::Layout`, supplying the final id.
pub fn to_core_layout(l: ParsedLayout, id: &str) -> Layout {
    Layout {
        id: id.to_string(),
        name: l.name,
        mode: to_core_mode(l.mode),
        input_mode: match l.input_mode {
            dvorakj_parser::InputMode::Direct => sloth_core::InputMode::Direct,
            dvorakj_parser::InputMode::Romaji => sloth_core::InputMode::Romaji,
            dvorakj_parser::InputMode::Kana => sloth_core::InputMode::Kana,
        },
        keyboard: to_core_keyboard(l.keyboard),
        single_map: inner_map(l.single_map),
        layer_maps: l
            .layer_maps
            .into_iter()
            .map(|(chord, inner)| (chord_to_vec(chord), inner_map(inner)))
            .collect(),
        layer_taps: l
            .layer_taps
            .into_iter()
            .map(|(k, v)| (to_core_key(k), to_core_seq(v)))
            .collect(),
        layer_triggers: l
            .layer_triggers
            .into_iter()
            .map(to_core_key)
            .collect::<HashSet<_>>(),
        combos: l
            .combos
            .into_iter()
            .map(|(chord, out)| (chord_to_vec(chord), to_core_seq(out)))
            .collect(),
        combo_keys: l
            .combo_keys
            .into_iter()
            .map(to_core_key)
            .collect::<HashSet<_>>(),
        sustained_triggers: l
            .sustained_triggers
            .into_iter()
            .map(to_core_key)
            .collect::<HashSet<_>>(),
        prefix_maps: l
            .prefix_maps
            .into_iter()
            .map(|(chord, inner)| (chord_to_vec(chord), inner_map(inner)))
            .collect(),
        prefix_triggers: l
            .prefix_triggers
            .into_iter()
            .map(to_core_key)
            .collect::<HashSet<_>>(),
        simultaneous: vec![],
    }
}
