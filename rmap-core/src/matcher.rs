//! InputMatcher: Single + Simultaneous + layer shifts (Sequential later per roadmap).
//! Hot path: must be fast, no alloc in common case if possible.

use crate::{Event, EventKind, KeyCode, OutputSeq, layout::Layout};
use std::collections::HashSet;
use std::time::Duration;

const DEFAULT_COMBO_WINDOW_MS: u64 = 50;

/// What the hook should do with the current physical event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchAction {
    /// Inject this output sequence and block (eat) the original event.
    Emit(OutputSeq),
    /// Block the original event, inject nothing (e.g. a layer key being held,
    /// or the key-up of a key whose key-down we already consumed).
    Block,
    /// Forward the original event unchanged to the rest of the system.
    PassThrough,
}

#[derive(Debug)]
pub struct InputMatcher {
    /// Currently physically down keys (our state, independent of OS repeat)
    pressed: HashSet<KeyCode>,
    #[allow(dead_code)]
    combo_window: Duration, // used for timing in full impl; kept for API and future windowed logic
    /// Layers that had a content partner while held (suppress tap on their up)
    layer_had_partner: HashSet<KeyCode>,
    /// Keys whose key-down we blocked/consumed, so their key-up is blocked too
    /// (prevents a stray key-up reaching the OS for a key it never saw go down).
    blocked: HashSet<KeyCode>,
    /// FR-6: while any of these keys is held, the engine fully passes through
    /// (acts as if rmap were not running). Config-level, not per-layout.
    disable_keys: HashSet<KeyCode>,
    /// FR-8: persistent stop/resume state (toggled by hotkey/IPC). While true,
    /// every event passes through (except draining in-flight blocked key-ups).
    suspended: bool,
    // For Sequential later...
}

impl Default for InputMatcher {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            combo_window: Duration::from_millis(DEFAULT_COMBO_WINDOW_MS),
            layer_had_partner: HashSet::new(),
            blocked: HashSet::new(),
            disable_keys: HashSet::new(),
            suspended: false,
        }
    }
}

impl InputMatcher {
    pub fn new(combo_window_ms: Option<u64>) -> Self {
        Self {
            combo_window: Duration::from_millis(combo_window_ms.unwrap_or(DEFAULT_COMBO_WINDOW_MS)),
            ..Self::default()
        }
    }

    /// Decide what to do with one physical event. Distinguishes three outcomes
    /// (see `MatchAction`): remap+block, block-only, or pass through.
    /// Supports DvorakJ layers (SandS etc.) via layer_maps + layer_taps.
    pub fn process(&mut self, event: &Event, layout: &Layout) -> MatchAction {
        match event.kind {
            EventKind::KeyDown => {
                if !self.pressed.contains(&event.code) {
                    self.pressed.insert(event.code);
                }

                // FR-6/FR-8 bypass: if globally suspended, or any disable key is
                // now held (the disable key itself is inserted above, so its own
                // down also passes through), forward everything unchanged. Keys
                // whose key-down was consumed *before* the bypass began still
                // drain their key-up via `blocked` in the KeyUp arm below.
                if self.bypass_active() {
                    return MatchAction::PassThrough;
                }

                // Layer key down: block it (the OS must not see the raw layer
                // key) and emit nothing now. Tap is decided on key-up. A held
                // layer key likewise stays blocked and keeps selecting its layer.
                if layout.is_layer_trigger(event.code) {
                    self.blocked.insert(event.code);
                    return MatchAction::Block;
                }

                // Any fresh content press while a layer is held marks that layer
                // as having had a partner, so the layer key will not also tap on
                // release (SandS / thumb-shift semantics) — independent of
                // whether the content key resolves to a mapping below.
                if !event.held && self.any_layer_held(layout) {
                    self.mark_layers_had_partner(layout);
                }

                // Combo rules (legacy `simultaneous`) only initiate on a fresh
                // press, never on OS auto-repeat of a held key (Key Hold vs
                // Repeat). Resolution of layer/base maps, however, runs for both
                // fresh and held events so a held remapped key keeps emitting and
                // stays blocked instead of leaking the original on auto-repeat.
                if !event.held {
                    if let Some(seq) = self.check_simultaneous(&event.code, layout) {
                        self.blocked.insert(event.code);
                        return MatchAction::Emit(seq);
                    }
                }

                let active_layers = self.current_active_layers(layout);
                if !active_layers.is_empty() {
                    if let Some(layer_map) = layout.layer_maps.get(&active_layers) {
                        if let Some(seq) = layer_map.get(&event.code).cloned() {
                            self.blocked.insert(event.code);
                            return MatchAction::Emit(seq);
                        }
                    }
                }

                if let Some(seq) = layout.single_map.get(&event.code).cloned() {
                    self.blocked.insert(event.code);
                    return MatchAction::Emit(seq);
                }

                // No mapping: let the physical key through unchanged.
                MatchAction::PassThrough
            }
            EventKind::KeyUp => {
                // FR-6/FR-8: evaluate bypass while the key is still in `pressed`
                // (so a disable key's own up is still seen as "held" -> passthrough).
                let bypass = self.bypass_active();
                let was_pressed = self.pressed.remove(&event.code);

                if bypass {
                    // In-flight key consumed before bypass began: block its up so
                    // no stray key-up reaches the OS; everything else passes.
                    if self.blocked.remove(&event.code) {
                        return MatchAction::Block;
                    }
                    return MatchAction::PassThrough;
                }

                if layout.is_layer_trigger(event.code) {
                    self.blocked.remove(&event.code);
                    let had_partner = self.layer_had_partner.remove(&event.code);
                    // Tap-alone (no partner during the hold) emits the tap output;
                    // either way the layer key-up is blocked (its down was blocked).
                    if !had_partner {
                        if let Some(tap_seq) = layout.layer_taps.get(&event.code).cloned() {
                            return MatchAction::Emit(tap_seq);
                        }
                    }
                    return MatchAction::Block;
                }

                // Content key-up: if we consumed its key-down, block the key-up
                // too; otherwise pass it through.
                let _ = was_pressed;
                if self.blocked.remove(&event.code) {
                    MatchAction::Block
                } else {
                    MatchAction::PassThrough
                }
            }
        }
    }

    fn check_simultaneous(&self, new_key: &KeyCode, layout: &Layout) -> Option<OutputSeq> {
        for rule in &layout.simultaneous {
            let all_pressed = rule.layers.iter().all(|l| self.pressed.contains(l) || l == new_key);
            if all_pressed && rule.layers.contains(new_key) {
                return Some(rule.output.clone());
            }
        }
        None
    }

    fn current_active_layers(&self, layout: &Layout) -> Vec<KeyCode> {
        let mut v: Vec<KeyCode> = self.pressed.iter().copied().filter(|k| layout.is_layer_trigger(*k)).collect();
        v.sort_by_key(|k| key_sort_for_layer(*k));
        v
    }

    fn any_layer_held(&self, layout: &Layout) -> bool {
        self.pressed.iter().any(|k| layout.is_layer_trigger(*k))
    }

    /// FR-6/FR-8: true when the engine should pass everything through —
    /// either globally suspended, or a configured disable key is held.
    fn bypass_active(&self) -> bool {
        self.suspended || self.any_disable_key_held()
    }

    fn any_disable_key_held(&self) -> bool {
        !self.disable_keys.is_empty() && self.pressed.iter().any(|k| self.disable_keys.contains(k))
    }

    /// FR-6: set the keys that, while held, fully disable remapping. Replaces
    /// any previous set. Called at config load/reload.
    pub fn set_disable_keys(&mut self, keys: impl IntoIterator<Item = KeyCode>) {
        self.disable_keys = keys.into_iter().collect();
    }

    /// FR-8: persistent stop/resume.
    pub fn set_suspended(&mut self, suspended: bool) {
        self.suspended = suspended;
    }

    pub fn toggle_suspended(&mut self) -> bool {
        self.suspended = !self.suspended;
        self.suspended
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    fn mark_layers_had_partner(&mut self, layout: &Layout) {
        for k in self.pressed.iter() {
            if layout.is_layer_trigger(*k) {
                self.layer_had_partner.insert(*k);
            }
        }
    }

    pub fn clear(&mut self) {
        self.pressed.clear();
        self.layer_had_partner.clear();
        self.blocked.clear();
    }

    /// Returns true if the key is currently in our pressed set (used by hook to decide the `held` flag for repeats).
    pub(crate) fn was_already_pressed(&self, code: &KeyCode) -> bool {
        self.pressed.contains(code)
    }
}

fn key_sort_for_layer(k: KeyCode) -> u16 {
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
        _ => 100,
    }
}
