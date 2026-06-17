//! Output model: compiled at load time to keep hot path fast (NFR-1).

use crate::{KeyCode, Modifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputToken {
    /// Synthesized key + optional mods (full down+up tap)
    Key { code: KeyCode, mods: Modifiers },
    /// Raw Unicode text (for IME feed or direct chars not producible by keystrokes)
    Text(String),
    /// Named special: {BS}, {Enter}, arrows, etc.
    Named(SpecialKey),
    /// Press a modifier key (key-down only, no release). Used by SandS to hold
    /// Shift across multiple content keys so that e.g. Space+Arrow produces
    /// continuous selection.
    ModDown(KeyCode),
    /// Release a modifier key (key-up only). Paired with a prior `ModDown`.
    ModUp(KeyCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialKey {
    Backspace,
    Enter,
    Tab,
    Escape,
    Left,
    Right,
    Up,
    Down,
    // Add more as DvorakJ corpus requires: Home, End, PgUp, etc.
}

pub type OutputSeq = Vec<OutputToken>;

/// input_mode per layout (plan.md Output Model)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Direct,  // ASCII/Dvorak/Colemak etc. 1:1 key+mod
    Romaji,  // kana strings -> romaji keystrokes via bundled encoder
    Kana,    // kana strings -> JIS-kana key positions
}
