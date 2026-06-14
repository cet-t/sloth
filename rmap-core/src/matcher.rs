//! InputMatcher: 同時打鍵 (simultaneous-press) engine + sustained while-held
//! layers (SandS) + single/base mapping.
//!
//! ## Why a timed engine
//! In a 新下駄-style layout the centre keys are *both* a character key and a
//! chord shift. A pure hold-layer model emits a trigger key's own kana only on
//! key-**up**, so fast sequential typing comes out delayed / out of order /
//! dropped. Instead we defer a combo-participating key's output by a short
//! window: if a partner key overlaps within the window we emit the chord;
//! otherwise the key resolves to its solo output (flushed by `flush_due` from a
//! timer, or on the key's own release) — preserving order with zero loss.
//!
//! Keys that participate in *no* chord (digits, symbols, space, modifiers …)
//! are emitted immediately on key-down, so normal typing has no added latency.
//!
//! `-option-input`-declared triggers (Space / Muhenkan / Henkan / Shift) keep
//! the classic *sustained* while-held layer behaviour (SandS), so that path is
//! preserved for those layouts.

use crate::{Event, EventKind, KeyCode, OutputSeq, layout::{Layout, canon_sort}};
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// Default 同時打鍵 window: keys overlapping within this span may form a chord.
/// Short enough that ordinary sequential typing (release-before-next) is not
/// mistaken for a chord, long enough for an intentional co-press.
const DEFAULT_COMBO_WINDOW_MS: u64 = 40;

/// What the hook should do with the current physical event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchAction {
    /// Inject this output sequence and block (eat) the original event.
    Emit(OutputSeq),
    /// Inject this output sequence, then let the original event pass through
    /// unchanged (used to flush a pending chord while an unmapped key still
    /// reaches the OS as itself).
    EmitThenPass(OutputSeq),
    /// Block the original event, inject nothing (a deferred/blocked key).
    Block,
    /// Forward the original event unchanged to the rest of the system.
    PassThrough,
}

#[derive(Debug)]
pub struct InputMatcher {
    /// Currently physically down keys (our state, independent of OS repeat).
    pressed: HashSet<KeyCode>,
    /// 同時打鍵 chord accumulator: combo-participating keys pressed (in order)
    /// that haven't resolved yet.
    pending: Vec<KeyCode>,
    /// When the current `pending` chord started (for the combo window).
    pending_since: Option<Instant>,
    combo_window: Duration,
    /// Sustained (while-held) layers that had a content partner during the
    /// hold (suppresses their tap on release).
    layer_had_partner: HashSet<KeyCode>,
    /// Keys whose key-down we consumed, so their key-up is blocked too.
    blocked: HashSet<KeyCode>,
    /// FR-6: while any of these is held, the engine fully passes through.
    disable_keys: HashSet<KeyCode>,
    /// FR-8: persistent stop/resume.
    suspended: bool,
    /// External (non-user) bypass, set by the hook layer — e.g. IME is ON and
    /// the layout is configured to be active only while the IME is OFF. While
    /// true everything passes through, exactly like `suspended`.
    external_bypass: bool,
    /// When true, a solo key resolved via `flush_due` (timed out alone, past
    /// `combo_window`) keeps repeating its resolved output for as long as the
    /// key is physically held (OS auto-repeat), like a normal held key
    /// ("ホールド扱い"). When false (default), it is emitted once and further
    /// auto-repeat events are swallowed ("単打扱い").
    hold_mode: bool,
    /// Keys currently repeating their resolved output (see `hold_mode`).
    repeating: std::collections::HashMap<KeyCode, OutputSeq>,
}

impl Default for InputMatcher {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            pending: Vec::new(),
            pending_since: None,
            combo_window: Duration::from_millis(DEFAULT_COMBO_WINDOW_MS),
            layer_had_partner: HashSet::new(),
            blocked: HashSet::new(),
            disable_keys: HashSet::new(),
            suspended: false,
            external_bypass: false,
            hold_mode: false,
            repeating: std::collections::HashMap::new(),
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

    /// Decide what to do with one physical event.
    pub fn process(&mut self, event: &Event, layout: &Layout) -> MatchAction {
        match event.kind {
            EventKind::KeyDown => self.on_key_down(event, layout),
            EventKind::KeyUp => self.on_key_up(event, layout),
        }
    }

    fn on_key_down(&mut self, event: &Event, layout: &Layout) -> MatchAction {
        let k = event.code;
        let was_pressed = self.pressed.contains(&k);
        self.pressed.insert(k);

        // Global bypass (suspended / disable key / Ctrl|Alt|Win held): act as if
        // rmap were off. Drop any half-formed chord so it can't inject mid-shortcut.
        if self.bypass_active() {
            self.clear_pending();
            return MatchAction::PassThrough;
        }

        // OS auto-repeat of an already-held key: never re-enter the engine.
        // Re-block consumed keys; let others repeat through.
        if was_pressed || event.held {
            // Hold mode: a solo key resolved by flush_due keeps repeating its
            // resolved output for as long as it's physically held.
            if let Some(seq) = self.repeating.get(&k).cloned() {
                return MatchAction::Emit(seq);
            }
            if self.blocked.contains(&k) || layout.sustained_triggers.contains(&k) {
                return MatchAction::Block;
            }
            return MatchAction::PassThrough;
        }

        // --- Sustained (while-held) layer path (SandS) ---
        if layout.sustained_triggers.contains(&k) {
            // A new sustained trigger starts a hold layer; flush any chord first.
            let flushed = self.take_pending_output(layout);
            self.blocked.insert(k);
            return match flushed {
                Some(seq) => MatchAction::Emit(seq), // eats this trigger's down
                None => MatchAction::Block,
            };
        }
        if self.any_sustained_held(layout) {
            // Content key while a sustained layer is held: resolve via that layer.
            self.mark_sustained_partners(layout);
            let layers = self.active_sustained_layers(layout);
            if let Some(map) = layout.layer_maps.get(&layers) {
                if let Some(seq) = map.get(&k).cloned() {
                    self.blocked.insert(k);
                    return MatchAction::Emit(seq);
                }
            }
            // No layer mapping: fall back to base, else pass through.
            if let Some(seq) = layout.single_map.get(&k).cloned() {
                self.blocked.insert(k);
                return MatchAction::Emit(seq);
            }
            return MatchAction::PassThrough;
        }

        // --- 同時打鍵 chord path ---
        let is_combo_key = layout.combo_keys.contains(&k);

        if self.pending.is_empty() {
            if is_combo_key {
                // Defer: this key may start a chord. Resolved by partner / timer / up.
                self.pending.push(k);
                self.pending_since = Some(Instant::now());
                self.blocked.insert(k);
                return MatchAction::Block;
            }
            // Not a combo participant: emit its solo immediately (no latency).
            return self.emit_solo_now(k, layout);
        }

        // A chord is already pending.
        let within_window = self
            .pending_since
            .map(|t| t.elapsed() <= self.combo_window)
            .unwrap_or(false);

        if within_window && is_combo_key {
            self.pending.push(k);
            self.blocked.insert(k);
            // Exact chord and no longer chord can extend it -> resolve now.
            let mut sorted = self.pending.clone();
            canon_sort(&mut sorted);
            if let Some(out) = layout.combos.get(&sorted) {
                if !self.could_extend(&sorted, layout) {
                    let out = out.clone();
                    self.clear_pending();
                    return MatchAction::Emit(out);
                }
            }
            return MatchAction::Block;
        }

        // Window passed, or this key can't extend a chord: finalize the old
        // pending chord, then handle this key fresh.
        let flushed = self.resolve_chord(&self.pending.clone(), layout);
        self.clear_pending();

        if is_combo_key {
            // Start a new chord with this key; emit the flushed output, eat this down.
            self.pending.push(k);
            self.pending_since = Some(Instant::now());
            self.blocked.insert(k);
            return MatchAction::Emit(flushed);
        }
        // This key is not a combo participant.
        if let Some(mut seq) = layout.single_map.get(&k).cloned() {
            self.blocked.insert(k);
            let mut out = flushed;
            out.append(&mut seq);
            return MatchAction::Emit(out);
        }
        // Unmapped: flush the chord, let this key pass through unchanged.
        if flushed.is_empty() {
            MatchAction::PassThrough
        } else {
            MatchAction::EmitThenPass(flushed)
        }
    }

    fn on_key_up(&mut self, event: &Event, layout: &Layout) -> MatchAction {
        let k = event.code;
        // Evaluate bypass while the key is still "pressed" so e.g. a disable
        // key's own up still counts as held.
        let bypass = self.bypass_active();
        self.pressed.remove(&k);

        if bypass {
            if self.blocked.remove(&k) {
                return MatchAction::Block; // drain an in-flight consumed key-up
            }
            return MatchAction::PassThrough;
        }

        // Sustained trigger release: tap-alone emits its tap output.
        if layout.sustained_triggers.contains(&k) {
            self.blocked.remove(&k);
            let had_partner = self.layer_had_partner.remove(&k);
            if !had_partner {
                let tap = layout
                    .layer_taps
                    .get(&k)
                    .cloned()
                    .or_else(|| layout.single_map.get(&k).cloned());
                if let Some(seq) = tap {
                    return MatchAction::Emit(seq);
                }
            }
            return MatchAction::Block;
        }

        // Releasing a key that's part of the pending chord finalizes the chord.
        if self.pending.contains(&k) {
            let out = self.resolve_chord(&self.pending.clone(), layout);
            self.clear_pending();
            self.blocked.remove(&k);
            return MatchAction::Emit(out);
        }

        // Content key-up: block it iff its key-down was consumed.
        let was_blocked = self.blocked.remove(&k);
        self.repeating.remove(&k);
        if was_blocked {
            MatchAction::Block
        } else {
            MatchAction::PassThrough
        }
    }

    /// Called from the hook's timer thread: if the pending chord's window has
    /// elapsed, resolve it to its solo/combo output. Returns the output to
    /// inject (the keys stay `blocked` until their key-ups arrive).
    pub fn flush_due(&mut self, layout: &Layout) -> Option<OutputSeq> {
        if self.bypass_active() {
            return None;
        }
        let due = self
            .pending_since
            .map(|t| t.elapsed() >= self.combo_window)
            .unwrap_or(false);
        if !due || self.pending.is_empty() {
            return None;
        }

        // Hold mode: a single timed-out key keeps repeating its resolved
        // output for as long as it's physically held (OS auto-repeat).
        if self.hold_mode && self.pending.len() == 1 {
            let k = self.pending[0];
            let out = self.resolve_chord(&self.pending.clone(), layout);
            self.clear_pending();
            if out.is_empty() {
                return None;
            }
            self.repeating.insert(k, out.clone());
            return Some(out);
        }

        let out = self.resolve_chord(&self.pending.clone(), layout);
        self.clear_pending();
        if out.is_empty() { None } else { Some(out) }
    }

    /// Whether a timer wakeup is needed (a chord is pending).
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    // --- chord resolution ---

    /// Resolve a set of co-pressed keys into output: an exact combo if one
    /// exists, a solo for a single key, otherwise a greedy decomposition into
    /// the largest available sub-combos plus per-key solos (press order).
    fn resolve_chord(&self, pending: &[KeyCode], layout: &Layout) -> OutputSeq {
        let mut sorted = pending.to_vec();
        canon_sort(&mut sorted);
        if let Some(out) = layout.combos.get(&sorted) {
            return out.clone();
        }
        if pending.len() == 1 {
            return self.solo_seq(pending[0], layout);
        }
        let mut remaining: Vec<KeyCode> = pending.to_vec();
        let mut out: OutputSeq = vec![];
        while !remaining.is_empty() {
            let best: Option<(Vec<KeyCode>, OutputSeq)> = layout
                .combos
                .iter()
                .filter(|(keys, _)| {
                    keys.len() >= 2
                        && keys.len() <= remaining.len()
                        && keys.iter().all(|kk| remaining.contains(kk))
                })
                .max_by_key(|(keys, _)| keys.len())
                .map(|(keys, seq)| (keys.clone(), seq.clone()));
            match best {
                Some((keys, mut seq)) => {
                    out.append(&mut seq);
                    remaining.retain(|kk| !keys.contains(kk));
                }
                None => {
                    let kk = remaining.remove(0);
                    out.extend(self.solo_seq(kk, layout));
                }
            }
        }
        out
    }

    /// True if some combo strictly contains this key set (so it could still be
    /// completed by another key — don't early-resolve yet).
    fn could_extend(&self, sorted_pending: &[KeyCode], layout: &Layout) -> bool {
        layout.combos.keys().any(|c| {
            c.len() > sorted_pending.len() && sorted_pending.iter().all(|k| c.contains(k))
        })
    }

    /// Solo output for a key: base mapping, else declared tap, else the key
    /// itself (so a trigger with no base still produces something sane).
    fn solo_seq(&self, k: KeyCode, layout: &Layout) -> OutputSeq {
        layout
            .single_map
            .get(&k)
            .cloned()
            .or_else(|| layout.layer_taps.get(&k).cloned())
            .unwrap_or_else(|| vec![crate::OutputToken::Key { code: k, mods: crate::Modifiers::empty() }])
    }

    /// Emit a non-combo key's mapped output immediately, or pass it through.
    fn emit_solo_now(&mut self, k: KeyCode, layout: &Layout) -> MatchAction {
        if let Some(seq) = layout
            .single_map
            .get(&k)
            .cloned()
            .or_else(|| layout.layer_taps.get(&k).cloned())
        {
            self.blocked.insert(k);
            if self.hold_mode {
                self.repeating.insert(k, seq.clone());
            }
            MatchAction::Emit(seq)
        } else {
            MatchAction::PassThrough
        }
    }

    /// Resolve and clear the current pending chord, returning its output.
    fn take_pending_output(&mut self, layout: &Layout) -> Option<OutputSeq> {
        if self.pending.is_empty() {
            return None;
        }
        let out = self.resolve_chord(&self.pending.clone(), layout);
        self.clear_pending();
        if out.is_empty() { None } else { Some(out) }
    }

    fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_since = None;
    }

    // --- sustained layer helpers ---

    fn any_sustained_held(&self, layout: &Layout) -> bool {
        self.pressed.iter().any(|k| layout.sustained_triggers.contains(k))
    }

    fn active_sustained_layers(&self, layout: &Layout) -> Vec<KeyCode> {
        let mut v: Vec<KeyCode> = self
            .pressed
            .iter()
            .copied()
            .filter(|k| layout.sustained_triggers.contains(k))
            .collect();
        canon_sort(&mut v);
        v
    }

    fn mark_sustained_partners(&mut self, layout: &Layout) {
        for k in self.pressed.iter() {
            if layout.sustained_triggers.contains(k) {
                self.layer_had_partner.insert(*k);
            }
        }
    }

    // --- bypass (FR-6/FR-8) ---

    fn bypass_active(&self) -> bool {
        self.suspended
            || self.external_bypass
            || self.any_disable_key_held()
            || self.shortcut_modifier_held()
    }

    fn any_disable_key_held(&self) -> bool {
        !self.disable_keys.is_empty() && self.pressed.iter().any(|k| self.disable_keys.contains(k))
    }

    /// Ctrl/Alt/Win held -> pass everything through so OS shortcuts (Ctrl+A,
    /// Alt+Tab, Win+E …) work and are never turned into kana.
    fn shortcut_modifier_held(&self) -> bool {
        self.pressed.iter().any(|k| {
            matches!(
                k,
                KeyCode::CtrlL
                    | KeyCode::CtrlR
                    | KeyCode::AltL
                    | KeyCode::AltR
                    | KeyCode::MetaL
                    | KeyCode::MetaR
            )
        })
    }

    pub fn set_disable_keys(&mut self, keys: impl IntoIterator<Item = KeyCode>) {
        self.disable_keys = keys.into_iter().collect();
    }

    /// Configurable simultaneous-press (chord) detection window.
    pub fn set_combo_window_ms(&mut self, ms: u64) {
        self.combo_window = Duration::from_millis(ms);
    }

    /// Whether a solo key resolved by `flush_due` repeats its output while
    /// held ("ホールド扱い") vs emitting once ("単打扱い", default).
    pub fn set_hold_mode(&mut self, hold_mode: bool) {
        self.hold_mode = hold_mode;
    }

    pub fn set_suspended(&mut self, suspended: bool) {
        self.suspended = suspended;
    }

    /// Hook-layer bypass (e.g. IME-on gating). Independent of FR-8 suspend.
    pub fn set_external_bypass(&mut self, bypass: bool) {
        self.external_bypass = bypass;
    }

    pub fn toggle_suspended(&mut self) -> bool {
        self.suspended = !self.suspended;
        self.suspended
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    pub fn clear(&mut self) {
        self.pressed.clear();
        self.layer_had_partner.clear();
        self.blocked.clear();
        self.repeating.clear();
        self.clear_pending();
    }

    /// True if the key is currently in our pressed set (hook uses this for the
    /// `held` repeat flag).
    pub(crate) fn was_already_pressed(&self, code: &KeyCode) -> bool {
        self.pressed.contains(code)
    }
}
