//! Layout: compiled mapping + metadata.

use crate::{InputMode, KeyCode, KeyboardLayout, OutputSeq};
use std::collections::HashMap;

pub type LayoutId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutMode {
    #[default]
    Legacy,
    Sequential,
    Simultaneous,
    Mixed,
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub id: LayoutId,
    pub name: String,
    pub mode: LayoutMode,
    pub input_mode: InputMode,
    /// Physical keyboard layout this layout's grid was compiled against,
    /// determined by the loader from the file's format (`.jp.txt` = JIS,
    /// `.en.txt` = US). Drives VK<->KeyCode translation in the hook.
    pub keyboard: KeyboardLayout,
    /// Base (no layers): physical -> output
    pub single_map: HashMap<KeyCode, OutputSeq>,
    /// Layered shifts: sorted active layers vec -> (content key -> output).
    /// Consumed by the *sustained* (while-held) layer path for SandS-style
    /// `-option-input` triggers (Space/Muhenkan/Henkan/Shift).
    pub layer_maps: HashMap<Vec<KeyCode>, HashMap<KeyCode, OutputSeq>>,
    /// Tap output when a layer key is released alone (within window, no partner).
    /// Used as the solo fallback for trigger keys with no base-grid mapping.
    pub layer_taps: HashMap<KeyCode, OutputSeq>,
    /// Keys that act as layer/chord triggers (declared `-option-input` or bare
    /// scan-code blocks). A trigger may also have its own base/solo output.
    pub layer_triggers: std::collections::HashSet<KeyCode>,
    /// 同時打鍵 (simultaneous-press) chords: a canonically-sorted set of keys
    /// that, when pressed together within the combo window, emit this output.
    /// Built from bare scan-code (`-XX[...]`) blocks — the 新下駄-style corpus.
    pub combos: HashMap<Vec<KeyCode>, OutputSeq>,
    /// Union of every key that participates in any `combos` entry. A key in
    /// this set defers its solo output (it might start a chord); a key not in
    /// it emits immediately on key-down (no combo-window latency).
    pub combo_keys: std::collections::HashSet<KeyCode>,
    /// Triggers that behave as *sustained* while-held layers (SandS): declared
    /// via `-option-input`. These stay active for every content key until
    /// released, as opposed to one-shot 同時打鍵 chords.
    pub sustained_triggers: std::collections::HashSet<KeyCode>,
    /// 順次打鍵 (prefix/sequential) layers: trigger key(s) pressed then released,
    /// followed by a content key within the prefix window.
    pub prefix_maps: HashMap<Vec<KeyCode>, HashMap<KeyCode, OutputSeq>>,
    /// Keys that act as prefix (sequential) triggers.
    pub prefix_triggers: std::collections::HashSet<KeyCode>,
    /// Legacy simultaneous rules (kept for compatibility)
    pub simultaneous: Vec<ComboRule>,
}

#[derive(Debug, Clone)]
pub struct ComboRule {
    pub layers: Vec<KeyCode>, // e.g. [Space] for SandS, or [Muhenkan, Henkan] etc.
    pub output: OutputSeq,
}

// LayerTap concept folded into Layout.layer_taps. This struct kept only for possible future external use.
#[derive(Debug, Clone)]
pub struct LayerTap {
    pub layer_key: KeyCode,
    pub tap_output: OutputSeq,
}

impl Layout {
    pub fn is_layer_trigger(&self, k: KeyCode) -> bool {
        self.layer_triggers.contains(&k)
    }
}

/// Canonical ordering rank for a key, used to put any chord / layer key set
/// into a stable order so `combos`/`layer_maps` lookups are reproducible.
/// Build-time (loader) and run-time (matcher) MUST use this same function.
pub fn canon_key_order(k: KeyCode) -> u16 {
    match k {
        KeyCode::Space => 1,
        KeyCode::ShiftL => 2,
        KeyCode::ShiftR => 3,
        KeyCode::CtrlL => 4,
        KeyCode::CtrlR => 5,
        KeyCode::AltL => 6,
        KeyCode::AltR => 7,
        KeyCode::MetaL => 8,
        KeyCode::MetaR => 9,
        KeyCode::Muhenkan => 10,
        KeyCode::Henkan => 11,
        KeyCode::KanaKatakana => 12,
        KeyCode::HankakuZenkaku => 13,
        KeyCode::Yen => 14,
        KeyCode::Caret => 15,
        KeyCode::Colon => 16,
        KeyCode::AtSign => 17,
        KeyCode::Unknown(_) => 200,
        _ => 100,
    }
}

/// Sort a key set into canonical order in place. Ties within a primary rank
/// (e.g. letters all rank 100) are broken by the variant's Debug name, which
/// is distinct per `KeyCode` variant (incl. `Unknown(n)` by n), giving a fully
/// reproducible total order independent of press/parse order.
pub fn canon_sort(v: &mut [KeyCode]) {
    v.sort_by_key(|k| (canon_key_order(*k), format!("{:?}", k)));
}
