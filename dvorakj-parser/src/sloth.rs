//! Converts a parsed DvorakJ layout into `sloth_parser::CompiledLayout`, the
//! canonical "sloth format" internal representation. This lets a downstream
//! consumer (e.g. `sloth-core`) treat a DvorakJ `.txt` file and a hand-written
//! sloth TOML/JSON file identically once loaded, without caring which source
//! format either one came from.
//!
//! Requires the `sloth` feature (off by default, see this crate's Cargo.toml).

use std::collections::BTreeMap;

use sloth_parser::{
    CompiledLayer, CompiledLayout, InputMode as SInputMode, KeyChord as SKeyChord,
    KeyboardLayout as SKeyboardLayout, LayoutMode as SLayoutMode, Modifiers as SModifiers,
    OutputSeq as SOutputSeq, OutputToken as SToken, SpecialKey as SSpecialKey,
};

use crate::model::{
    InputMode, Key, KeyChord, KeyboardLayout, LayoutMode, Modifiers, OutputSeq, OutputToken,
    ParsedLayout, SpecialKey,
};

/// Convert a [`ParsedLayout`] into a [`CompiledLayout`].
pub fn to_compiled_layout(l: ParsedLayout) -> CompiledLayout {
    let single = to_compiled_layer(l.single_map);
    let mut layers = std::collections::HashMap::new();
    layers.insert("base".to_string(), single);

    CompiledLayout {
        name: l.name,
        mode: to_sloth_mode(l.mode),
        input_mode: to_sloth_input_mode(l.input_mode),
        keyboard: to_sloth_keyboard(l.keyboard),
        layers,
        layer_maps: l
            .layer_maps
            .into_iter()
            .map(|(chord, inner)| (chord_to_vec(chord), inner_map(inner)))
            .collect(),
        layer_taps: l
            .layer_taps
            .into_iter()
            .map(|(k, v)| (to_sloth_key(k), to_sloth_seq(v)))
            .collect(),
        layer_triggers: l.layer_triggers.into_iter().map(to_sloth_key).collect(),
        combos: l
            .combos
            .into_iter()
            .map(|(chord, out)| (SKeyChord::new(chord_to_vec(chord)), to_sloth_seq(out)))
            .collect(),
        combo_keys: l.combo_keys.into_iter().map(to_sloth_key).collect(),
        sustained_triggers: l.sustained_triggers.into_iter().map(to_sloth_key).collect(),
        // dvorakj's prefix_maps (trigger-chord -> {content -> output}, a
        // one-level trie) flattens 1:1 into CompiledLayout's sequences
        // (full completed key list -> output): each (chord, content, out)
        // triple becomes one `chord_keys ++ [content]` entry.
        sequences: l
            .prefix_maps
            .into_iter()
            .flat_map(|(chord, inner)| {
                let prefix = chord_to_vec(chord);
                inner.into_iter().map(move |(content, out)| {
                    let mut full = prefix.clone();
                    full.push(to_sloth_key(content));
                    (full, to_sloth_seq(out))
                })
            })
            .collect(),
        prefix_triggers: l.prefix_triggers.into_iter().map(to_sloth_key).collect(),
        states: std::collections::HashMap::new(),
    }
}

fn to_compiled_layer(map: std::collections::BTreeMap<Key, OutputSeq>) -> CompiledLayer {
    CompiledLayer {
        keys: inner_map(map),
    }
}

fn inner_map(
    map: std::collections::BTreeMap<Key, OutputSeq>,
) -> BTreeMap<sloth_parser::Key, SOutputSeq> {
    map.into_iter()
        .map(|(k, v)| (to_sloth_key(k), to_sloth_seq(v)))
        .collect()
}

fn chord_to_vec(c: KeyChord) -> Vec<sloth_parser::Key> {
    c.into_vec().into_iter().map(to_sloth_key).collect()
}

fn to_sloth_seq(seq: OutputSeq) -> SOutputSeq {
    seq.into_iter().map(to_sloth_token).collect()
}

fn to_sloth_token(t: OutputToken) -> SToken {
    match t {
        OutputToken::Key { code, mods } => SToken::Key {
            code: to_sloth_key(code),
            mods: to_sloth_mods(mods),
        },
        OutputToken::Text(s) => SToken::Text(s),
        OutputToken::Named(sp) => SToken::Named(to_sloth_special(sp)),
        OutputToken::ModDown(k) => SToken::ModDown(to_sloth_key(k)),
        OutputToken::ModUp(k) => SToken::ModUp(to_sloth_key(k)),
    }
}

fn to_sloth_mods(m: Modifiers) -> SModifiers {
    // Low four bits (SHIFT/CTRL/ALT/META) share the same layout in both enums.
    SModifiers::from_bits_truncate(m.bits())
}

fn to_sloth_special(s: SpecialKey) -> SSpecialKey {
    match s {
        SpecialKey::Backspace => SSpecialKey::Backspace,
        SpecialKey::Enter => SSpecialKey::Enter,
        SpecialKey::Tab => SSpecialKey::Tab,
        SpecialKey::Escape => SSpecialKey::Escape,
        SpecialKey::Left => SSpecialKey::Left,
        SpecialKey::Right => SSpecialKey::Right,
        SpecialKey::Up => SSpecialKey::Up,
        SpecialKey::Down => SSpecialKey::Down,
    }
}

fn to_sloth_mode(m: LayoutMode) -> SLayoutMode {
    match m {
        LayoutMode::Legacy => SLayoutMode::Legacy,
        LayoutMode::Sequential => SLayoutMode::Sequential,
        LayoutMode::Simultaneous => SLayoutMode::Simultaneous,
        LayoutMode::Mixed => SLayoutMode::Mixed,
    }
}

fn to_sloth_input_mode(m: InputMode) -> SInputMode {
    match m {
        InputMode::Direct => SInputMode::Direct,
        InputMode::Romaji => SInputMode::Romaji,
        InputMode::Kana => SInputMode::Kana,
    }
}

fn to_sloth_keyboard(k: KeyboardLayout) -> SKeyboardLayout {
    match k {
        KeyboardLayout::Jis => SKeyboardLayout::Jis,
        KeyboardLayout::Us => SKeyboardLayout::Us,
    }
}

/// `dvorakj_parser::Key` and `sloth_parser::Key` are deliberately kept
/// variant-for-variant identical (see both enums' doc comments), so this
/// conversion is a total, exhaustive `match`.
fn to_sloth_key(k: Key) -> sloth_parser::Key {
    use sloth_parser::Key as S;
    match k {
        Key::A => S::A,
        Key::B => S::B,
        Key::C => S::C,
        Key::D => S::D,
        Key::E => S::E,
        Key::F => S::F,
        Key::G => S::G,
        Key::H => S::H,
        Key::I => S::I,
        Key::J => S::J,
        Key::K => S::K,
        Key::L => S::L,
        Key::M => S::M,
        Key::N => S::N,
        Key::O => S::O,
        Key::P => S::P,
        Key::Q => S::Q,
        Key::R => S::R,
        Key::S => S::S,
        Key::T => S::T,
        Key::U => S::U,
        Key::V => S::V,
        Key::W => S::W,
        Key::X => S::X,
        Key::Y => S::Y,
        Key::Z => S::Z,
        Key::Num0 => S::Num0,
        Key::Num1 => S::Num1,
        Key::Num2 => S::Num2,
        Key::Num3 => S::Num3,
        Key::Num4 => S::Num4,
        Key::Num5 => S::Num5,
        Key::Num6 => S::Num6,
        Key::Num7 => S::Num7,
        Key::Num8 => S::Num8,
        Key::Num9 => S::Num9,
        Key::Minus => S::Minus,
        Key::Equal => S::Equal,
        Key::LBracket => S::LBracket,
        Key::RBracket => S::RBracket,
        Key::Backslash => S::Backslash,
        Key::Semicolon => S::Semicolon,
        Key::Quote => S::Quote,
        Key::Comma => S::Comma,
        Key::Dot => S::Dot,
        Key::Slash => S::Slash,
        Key::Grave => S::Grave,
        Key::ShiftL => S::ShiftL,
        Key::ShiftR => S::ShiftR,
        Key::CtrlL => S::CtrlL,
        Key::CtrlR => S::CtrlR,
        Key::AltL => S::AltL,
        Key::AltR => S::AltR,
        Key::MetaL => S::MetaL,
        Key::MetaR => S::MetaR,
        Key::Space => S::Space,
        Key::Enter => S::Enter,
        Key::Tab => S::Tab,
        Key::Backspace => S::Backspace,
        Key::Escape => S::Escape,
        Key::CapsLock => S::CapsLock,
        Key::Left => S::Left,
        Key::Right => S::Right,
        Key::Up => S::Up,
        Key::Down => S::Down,
        Key::F1 => S::F1,
        Key::F2 => S::F2,
        Key::F3 => S::F3,
        Key::F4 => S::F4,
        Key::F5 => S::F5,
        Key::F6 => S::F6,
        Key::F7 => S::F7,
        Key::F8 => S::F8,
        Key::F9 => S::F9,
        Key::F10 => S::F10,
        Key::F11 => S::F11,
        Key::F12 => S::F12,
        Key::Muhenkan => S::Muhenkan,
        Key::Henkan => S::Henkan,
        Key::KanaKatakana => S::KanaKatakana,
        Key::HankakuZenkaku => S::HankakuZenkaku,
        Key::Yen => S::Yen,
        Key::Caret => S::Caret,
        Key::Colon => S::Colon,
        Key::AtSign => S::AtSign,
        Key::Unknown(n) => S::Unknown(n),
    }
}
