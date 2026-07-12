//! Bridge: `sloth_parser::CompiledLayout` -> `sloth_core::Layout`.
//!
//! `CompiledLayout` is the single canonical "sloth format" representation
//! that *every* source format compiles into -- hand-written TOML/JSON via
//! `sloth_parser::compile_toml`/`compile_json`, or a DvorakJ `.txt` via
//! `dvorakj-parser`'s `sloth` feature (see `sloth-dvorakj-adapter`). This
//! module is the one place that turns that shared representation into the
//! `KeyCode`-keyed runtime `Layout` the matcher actually consumes, so
//! sloth-core doesn't need to know which format a given layout came from.
//!
//! TODO (not yet modeled): named layers (shift/kana) and `states`-driven
//! layer switching -- `single_map` is always taken from `states.default`
//! (or the `base` layer), other named layers are parsed but unused so far.

use std::collections::HashMap;

use sloth_parser::{
    CompiledLayer, CompiledLayout, Key as PKey, KeyboardLayout as PKeyboard, OutputSeq as PSeq,
    OutputToken as PTok,
};

use crate::layout::{canon_sort, Layout};
use crate::loader::{LayoutLoader, LoadError};
use crate::{InputMode, KeyCode, KeyboardLayout, OutputSeq, OutputToken};

/// `LayoutLoader` for the sloth TOML/JSON layout format.
#[derive(Default)]
pub struct SlothLayoutLoader;

impl SlothLayoutLoader {
    pub fn new() -> Self {
        Self
    }
}

impl LayoutLoader for SlothLayoutLoader {
    fn format_name(&self) -> &'static str {
        "sloth"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["toml", "json"]
    }

    fn load(&self, bytes: &[u8], id: &str) -> Result<Layout, LoadError> {
        let text = std::str::from_utf8(bytes).map_err(|e| LoadError::Parse(e.to_string()))?;
        let compiled = if text.trim_start().starts_with('{') {
            sloth_parser::compile_json(text)
        } else {
            sloth_parser::compile_toml(text)
        }
        .map_err(|e| LoadError::Parse(e.to_string()))?;
        Ok(to_core_layout(compiled, id))
    }
}

fn to_core_key(k: PKey) -> KeyCode {
    use KeyCode as C;
    match k {
        PKey::A => C::A,
        PKey::B => C::B,
        PKey::C => C::C,
        PKey::D => C::D,
        PKey::E => C::E,
        PKey::F => C::F,
        PKey::G => C::G,
        PKey::H => C::H,
        PKey::I => C::I,
        PKey::J => C::J,
        PKey::K => C::K,
        PKey::L => C::L,
        PKey::M => C::M,
        PKey::N => C::N,
        PKey::O => C::O,
        PKey::P => C::P,
        PKey::Q => C::Q,
        PKey::R => C::R,
        PKey::S => C::S,
        PKey::T => C::T,
        PKey::U => C::U,
        PKey::V => C::V,
        PKey::W => C::W,
        PKey::X => C::X,
        PKey::Y => C::Y,
        PKey::Z => C::Z,
        PKey::Num0 => C::Num0,
        PKey::Num1 => C::Num1,
        PKey::Num2 => C::Num2,
        PKey::Num3 => C::Num3,
        PKey::Num4 => C::Num4,
        PKey::Num5 => C::Num5,
        PKey::Num6 => C::Num6,
        PKey::Num7 => C::Num7,
        PKey::Num8 => C::Num8,
        PKey::Num9 => C::Num9,
        PKey::Minus => C::Minus,
        PKey::Equal => C::Equal,
        PKey::LBracket => C::LBracket,
        PKey::RBracket => C::RBracket,
        PKey::Backslash => C::Backslash,
        PKey::Semicolon => C::Semicolon,
        PKey::Quote => C::Quote,
        PKey::Comma => C::Comma,
        PKey::Dot => C::Dot,
        PKey::Slash => C::Slash,
        PKey::Grave => C::Grave,
        PKey::ShiftL => C::ShiftL,
        PKey::ShiftR => C::ShiftR,
        PKey::CtrlL => C::CtrlL,
        PKey::CtrlR => C::CtrlR,
        PKey::AltL => C::AltL,
        PKey::AltR => C::AltR,
        PKey::MetaL => C::MetaL,
        PKey::MetaR => C::MetaR,
        PKey::Space => C::Space,
        PKey::Enter => C::Enter,
        PKey::Tab => C::Tab,
        PKey::Backspace => C::Backspace,
        PKey::Escape => C::Escape,
        PKey::CapsLock => C::CapsLock,
        PKey::Left => C::Left,
        PKey::Right => C::Right,
        PKey::Up => C::Up,
        PKey::Down => C::Down,
        PKey::F1 => C::F1,
        PKey::F2 => C::F2,
        PKey::F3 => C::F3,
        PKey::F4 => C::F4,
        PKey::F5 => C::F5,
        PKey::F6 => C::F6,
        PKey::F7 => C::F7,
        PKey::F8 => C::F8,
        PKey::F9 => C::F9,
        PKey::F10 => C::F10,
        PKey::F11 => C::F11,
        PKey::F12 => C::F12,
        PKey::Muhenkan => C::Muhenkan,
        PKey::Henkan => C::Henkan,
        PKey::KanaKatakana => C::KanaKatakana,
        PKey::HankakuZenkaku => C::HankakuZenkaku,
        PKey::Yen => C::Yen,
        PKey::Caret => C::Caret,
        PKey::Colon => C::Colon,
        PKey::AtSign => C::AtSign,
        PKey::Unknown(n) => C::Unknown(n),
    }
}

fn to_core_seq(s: PSeq) -> OutputSeq {
    s.into_iter()
        .map(|t| match t {
            PTok::Text(s) => OutputToken::Text(s),
            PTok::Key { code, mods } => OutputToken::Key {
                code: to_core_key(code),
                mods: to_core_mods(mods),
            },
            PTok::Named(sp) => OutputToken::Named(to_core_special(sp)),
            PTok::ModDown(k) => OutputToken::ModDown(to_core_key(k)),
            PTok::ModUp(k) => OutputToken::ModUp(to_core_key(k)),
        })
        .collect()
}

fn to_core_mods(m: sloth_parser::Modifiers) -> crate::Modifiers {
    crate::Modifiers::from_bits_truncate(m.bits())
}

fn to_core_special(s: sloth_parser::SpecialKey) -> crate::SpecialKey {
    use crate::SpecialKey as C;
    match s {
        sloth_parser::SpecialKey::Backspace => C::Backspace,
        sloth_parser::SpecialKey::Enter => C::Enter,
        sloth_parser::SpecialKey::Tab => C::Tab,
        sloth_parser::SpecialKey::Escape => C::Escape,
        sloth_parser::SpecialKey::Left => C::Left,
        sloth_parser::SpecialKey::Right => C::Right,
        sloth_parser::SpecialKey::Up => C::Up,
        sloth_parser::SpecialKey::Down => C::Down,
    }
}

fn to_core_mode(m: sloth_parser::LayoutMode) -> crate::layout::LayoutMode {
    use crate::layout::LayoutMode as C;
    match m {
        sloth_parser::LayoutMode::Legacy => C::Legacy,
        sloth_parser::LayoutMode::Sequential => C::Sequential,
        sloth_parser::LayoutMode::Simultaneous => C::Simultaneous,
        sloth_parser::LayoutMode::Mixed => C::Mixed,
    }
}

fn to_core_input_mode(m: sloth_parser::InputMode) -> InputMode {
    match m {
        sloth_parser::InputMode::Direct => InputMode::Direct,
        sloth_parser::InputMode::Romaji => InputMode::Romaji,
        sloth_parser::InputMode::Kana => InputMode::Kana,
    }
}

fn to_core_key_vec(v: &[PKey]) -> Vec<KeyCode> {
    let mut out: Vec<KeyCode> = v.iter().map(|k| to_core_key(*k)).collect();
    canon_sort(&mut out);
    out
}

fn layer_to_map(ly: &CompiledLayer) -> HashMap<KeyCode, OutputSeq> {
    ly.keys
        .iter()
        .map(|(k, v)| (to_core_key(*k), to_core_seq(v.clone())))
        .collect()
}

/// Compile a `sloth_parser::CompiledLayout` -- from *any* source format --
/// into the runtime `Layout` the matcher consumes. The one canonical
/// entry point for turning "sloth format" into a usable `Layout`; both
/// [`SlothLayoutLoader`] (native TOML/JSON) and `sloth-dvorakj-adapter`
/// (DvorakJ `.txt`, converted to `CompiledLayout` via `dvorakj-parser`'s
/// `sloth` feature) go through this.
pub fn to_core_layout(l: CompiledLayout, id: &str) -> Layout {
    let default_name = l
        .states
        .get("default")
        .cloned()
        .unwrap_or_else(|| "base".to_string());
    let single_map = l
        .layers
        .get(&default_name)
        .map(layer_to_map)
        .unwrap_or_default();

    let combos: HashMap<Vec<KeyCode>, OutputSeq> = l
        .combos
        .iter()
        .map(|(chord, out)| (to_core_key_vec(chord.as_slice()), to_core_seq(out.clone())))
        .collect();

    let layer_maps: HashMap<Vec<KeyCode>, HashMap<KeyCode, OutputSeq>> = l
        .layer_maps
        .iter()
        .map(|(chord, inner)| {
            (
                to_core_key_vec(chord),
                inner
                    .iter()
                    .map(|(k, v)| (to_core_key(*k), to_core_seq(v.clone())))
                    .collect(),
            )
        })
        .collect();

    // 順押し: a completed key sequence -> output. Represent as
    // trigger=(prefix) -> content=last, matching the matcher's prefix-trie
    // model. A len==1 "sequence" has no prefix to key a trigger by, so it
    // can't distinguish itself from a plain single-key press; skip it.
    // TODO: full multi-branch prefix tries for sequences that share a
    // prefix but diverge past the first content key.
    let mut prefix_maps: HashMap<Vec<KeyCode>, HashMap<KeyCode, OutputSeq>> = HashMap::new();
    for (seq, out) in &l.sequences {
        if seq.len() < 2 {
            continue;
        }
        let trigger: Vec<KeyCode> = seq[..seq.len() - 1]
            .iter()
            .map(|k| to_core_key(*k))
            .collect();
        let content = to_core_key(seq[seq.len() - 1]);
        prefix_maps
            .entry(trigger)
            .or_default()
            .insert(content, to_core_seq(out.clone()));
    }

    Layout {
        id: id.to_string(),
        name: l.name,
        mode: to_core_mode(l.mode),
        input_mode: to_core_input_mode(l.input_mode),
        keyboard: match l.keyboard {
            PKeyboard::Us => KeyboardLayout::Us,
            PKeyboard::Jis => KeyboardLayout::Jis,
        },
        single_map,
        layer_maps,
        layer_taps: l
            .layer_taps
            .iter()
            .map(|(k, v)| (to_core_key(*k), to_core_seq(v.clone())))
            .collect(),
        layer_triggers: l.layer_triggers.iter().map(|k| to_core_key(*k)).collect(),
        combos,
        combo_keys: l.combo_keys.iter().map(|k| to_core_key(*k)).collect(),
        sustained_triggers: l
            .sustained_triggers
            .iter()
            .map(|k| to_core_key(*k))
            .collect(),
        prefix_maps,
        prefix_triggers: l.prefix_triggers.iter().map(|k| to_core_key(*k)).collect(),
        simultaneous: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KeyCode, OutputToken};

    #[test]
    fn skeleton_loads_toml_fixture() {
        let src = include_str!("../../config-idea/config.toml");
        let loader = SlothLayoutLoader::new();
        let layout = loader.load(src.as_bytes(), "config.toml").expect("load");

        assert_eq!(layout.name, "my-layout");

        // base layer: Q -> "q"
        assert_eq!(
            layout.single_map.get(&KeyCode::Q),
            Some(&vec![OutputToken::Text("q".into())])
        );

        // 同時押し: a,b -> "@"
        let combo = layout.combos.get(&vec![KeyCode::A, KeyCode::B]);
        assert!(combo.is_some(), "combo a,b should be present");
        assert_eq!(combo.unwrap(), &vec![OutputToken::Text("@".into())]);

        // 順押し: d,v -> "★" (trigger=[D], content=V)
        let prefix = layout.prefix_maps.get(&vec![KeyCode::D]);
        assert!(prefix.is_some(), "prefix trigger [D] should exist");
        assert_eq!(
            prefix.unwrap().get(&KeyCode::V),
            Some(&vec![OutputToken::Text("★".into())])
        );

        // TODO: shift/kana layers & states not yet wired (layer_maps empty).
        assert!(layout.layer_maps.is_empty());
    }
}
