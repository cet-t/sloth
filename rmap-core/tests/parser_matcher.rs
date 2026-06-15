//! Integration smoke for the DvorakJ parser + 同時打鍵 InputMatcher.
//! Run with: cargo test -p rmap-core
//!
//! The matcher is now a timed simultaneous-press engine. These tests exercise
//! the behaviours that broke before the rewrite: in-order solo output of
//! chord-trigger keys, actual chords firing, function-key tokens, sustained
//! SandS layers, and the bypass paths (disable key / suspend / Ctrl).

use rmap_core::{
    DvorakJLayoutLoader, LayoutLoader, InputMatcher, MatchAction, Event, EventKind, KeyCode,
    Modifiers, OutputToken, SpecialKey,
};

fn down(code: KeyCode) -> Event {
    Event::new(EventKind::KeyDown, code, Modifiers::empty())
}
fn up(code: KeyCode) -> Event {
    Event { kind: EventKind::KeyUp, code, modifiers: Modifiers::empty(), timestamp: 0, held: false }
}
fn emit(a: MatchAction) -> Vec<OutputToken> {
    match a {
        MatchAction::Emit(seq) | MatchAction::EmitThenPass(seq) => seq,
        other => panic!("expected Emit, got {other:?}"),
    }
}
fn load(name: &str) -> rmap_core::Layout {
    let loader = DvorakJLayoutLoader::new();
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&manifest).join("..").join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|_| panic!("missing sample {name}"));
    loader.load(&bytes, name).expect("parse must succeed")
}

/// 新下駄: a chord-trigger key tapped alone (released before the window) emits
/// its base-grid kana, in order — not the raw letter, not dropped.
#[test]
fn chord_trigger_tapped_alone_emits_base_kana() {
    let layout = load("data/layouts/圧縮版_新下駄配列.txt");
    // E (scan 0x12) is both a chord trigger and a base key (は).
    assert!(layout.combo_keys.contains(&KeyCode::E));
    assert!(layout.single_map.contains_key(&KeyCode::E));

    let mut m = InputMatcher::default();
    // Down defers (might start a chord) -> Block, nothing emitted yet.
    assert_eq!(m.process(&down(KeyCode::E), &layout), MatchAction::Block);
    // Quick release (within window) resolves to the solo/base output.
    let out = emit(m.process(&up(KeyCode::E), &layout));
    let expect = layout.single_map.get(&KeyCode::E).unwrap();
    assert_eq!(&out, expect, "E alone must emit its base kana (は)");
}

/// 新下駄: two overlapping keys that form a defined chord emit the chord output.
#[test]
fn simultaneous_chord_fires() {
    let layout = load("data/layouts/圧縮版_新下駄配列.txt");
    // K (0x25) shift + Q (0x10) content -> ふぁ per the -25 block, row1 col0.
    let chord = {
        let mut v = vec![KeyCode::K, KeyCode::Q];
        rmap_core::layout::canon_sort(&mut v);
        v
    };
    let combo_out = layout.combos.get(&chord).expect("K+Q chord must exist").clone();

    let mut m = InputMatcher::default();
    // Press K then Q within the window -> chord resolves on the 2nd key-down.
    assert_eq!(m.process(&down(KeyCode::K), &layout), MatchAction::Block);
    let out = emit(m.process(&down(KeyCode::Q), &layout));
    assert_eq!(out, combo_out, "K+Q must emit the chord output");
    // Both key-ups are consumed (their downs were eaten).
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
    assert_eq!(m.process(&up(KeyCode::K), &layout), MatchAction::Block);
}

/// Function-key tokens compile and survive into a combo output (mixed cell).
#[test]
fn function_key_tokens_compile() {
    let layout = load("data/layouts/圧縮版_新下駄配列.txt");
    // The base grid has 、{enter} at the R key; it must compile to a kana char
    // plus a Named(Enter), not the literal text "{enter}".
    let r = layout.single_map.get(&KeyCode::R).expect("R base mapping");
    assert!(
        r.iter().any(|t| matches!(t, OutputToken::Named(SpecialKey::Enter))),
        "R base output must contain a real Enter key, got {r:?}"
    );
    assert!(
        !r.iter().any(|t| matches!(t, OutputToken::Text(s) if s.contains("enter"))),
        "must not emit literal '{{enter}}' text"
    );
}

/// 新下駄: typing the whole top row as discrete taps (down+up each, released
/// within the window) reproduces every base kana in order — the exact scenario
/// that previously came out delayed / reordered / dropped.
#[test]
fn sequential_top_row_in_order() {
    let layout = load("data/layouts/圧縮版_新下駄配列.txt");
    let row = [
        KeyCode::Q, KeyCode::W, KeyCode::E, KeyCode::R, KeyCode::T,
        KeyCode::Y, KeyCode::U, KeyCode::I, KeyCode::O, KeyCode::P,
    ];
    let mut m = InputMatcher::default();
    let mut got: Vec<OutputToken> = vec![];
    for &k in &row {
        // Down defers (these are all chord keys); up (within window) resolves.
        assert_eq!(m.process(&down(k), &layout), MatchAction::Block);
        if let MatchAction::Emit(seq) | MatchAction::EmitThenPass(seq) = m.process(&up(k), &layout) {
            got.extend(seq);
        }
    }
    let mut expect: Vec<OutputToken> = vec![];
    for &k in &row {
        expect.extend(layout.single_map.get(&k).cloned().unwrap_or_default());
    }
    assert_eq!(got, expect, "top row taps must emit base kana in order");
}

/// SandS toy: Space is a sustained while-held layer; tap -> Space, hold -> shift.
#[test]
fn sustained_sands_layer_and_tap() {
    let layout = load("data/layouts/samples/toy_simul.txt");
    assert!(layout.sustained_triggers.contains(&KeyCode::Space));

    let mut m = InputMatcher::default();
    // Space down blocked; tap (up, no partner) emits the tap output.
    assert_eq!(m.process(&down(KeyCode::Space), &layout), MatchAction::Block);
    assert!(!emit(m.process(&up(KeyCode::Space), &layout)).is_empty());

    // Hold Space, press Q -> shifted Q from the layer; multiple keys keep shifting.
    assert_eq!(m.process(&down(KeyCode::Space), &layout), MatchAction::Block);
    let q = emit(m.process(&down(KeyCode::Q), &layout));
    assert!(q.iter().any(|t| matches!(t, OutputToken::Key { code: KeyCode::Q, .. })));
    let w = emit(m.process(&down(KeyCode::W), &layout));
    assert!(w.iter().any(|t| matches!(t, OutputToken::Key { code: KeyCode::W, .. })));
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
    assert_eq!(m.process(&up(KeyCode::W), &layout), MatchAction::Block);
    // Space had a partner -> no tap on release.
    assert_eq!(m.process(&up(KeyCode::Space), &layout), MatchAction::Block);
}

/// FR-6: a configured disable key held -> everything passes through.
#[test]
fn disable_key_passthrough() {
    let layout = load("data/layouts/samples/toy_simul.txt");
    let mut m = InputMatcher::default();
    m.set_disable_keys([KeyCode::CtrlL, KeyCode::CtrlR]);
    assert_eq!(m.process(&down(KeyCode::CtrlL), &layout), MatchAction::PassThrough);
    assert_eq!(m.process(&down(KeyCode::Q), &layout), MatchAction::PassThrough);
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::PassThrough);
    assert_eq!(m.process(&up(KeyCode::CtrlL), &layout), MatchAction::PassThrough);
}

/// Ctrl held auto-bypasses (Ctrl+letter stays a shortcut, never becomes kana).
#[test]
fn ctrl_held_bypasses() {
    let layout = load("data/layouts/圧縮版_新下駄配列.txt");
    let mut m = InputMatcher::default();
    assert_eq!(m.process(&down(KeyCode::CtrlL), &layout), MatchAction::PassThrough);
    // A would normally defer as a combo key; under Ctrl it passes through.
    assert_eq!(m.process(&down(KeyCode::A), &layout), MatchAction::PassThrough);
    assert_eq!(m.process(&up(KeyCode::A), &layout), MatchAction::PassThrough);
    assert_eq!(m.process(&up(KeyCode::CtrlL), &layout), MatchAction::PassThrough);
}

/// 新下駄: this layout declares no `-shift[...]` block, so LShift is neither a
/// combo key nor a sustained trigger — it must pass through untouched (the
/// matcher never reacts to it at all).
#[test]
fn shift_passes_through_when_layout_has_no_shift_block() {
    let layout = load("data/layouts/圧縮版_新下駄配列.txt");
    assert!(!layout.sustained_triggers.contains(&KeyCode::ShiftL));
    assert!(!layout.combo_keys.contains(&KeyCode::ShiftL));
    assert!(!layout.single_map.contains_key(&KeyCode::ShiftL));

    let mut m = InputMatcher::default();
    assert_eq!(m.process(&down(KeyCode::ShiftL), &layout), MatchAction::PassThrough);
    assert_eq!(m.process(&up(KeyCode::ShiftL), &layout), MatchAction::PassThrough);
}

/// Built-in SandS (Space and Shift), layout-independent: 新下駄 declares no
/// `-option-input` SandS layer, yet Space must still tap -> space and
/// hold + key -> Shift+key. This is the behaviour the user reported missing.
#[test]
fn builtin_sands_space_and_shift() {
    let layout = load("data/layouts/圧縮版_新下駄配列.txt");
    // Precondition: the layout itself declares no Space SandS layer.
    assert!(!layout.sustained_triggers.contains(&KeyCode::Space));

    let mut m = InputMatcher::default();

    // Tap Space: down is held (Block), release with no partner emits a Space.
    assert_eq!(m.process(&down(KeyCode::Space), &layout), MatchAction::Block);
    let tap = emit(m.process(&up(KeyCode::Space), &layout));
    assert_eq!(
        tap,
        vec![OutputToken::Key { code: KeyCode::Space, mods: Modifiers::empty() }],
        "tapping Space alone must emit a Space"
    );

    // Hold Space + Q -> Shift+Q (capital), regardless of any kana mapping.
    assert_eq!(m.process(&down(KeyCode::Space), &layout), MatchAction::Block);
    let q = emit(m.process(&down(KeyCode::Q), &layout));
    assert_eq!(
        q,
        vec![OutputToken::Key { code: KeyCode::Q, mods: Modifiers::SHIFT }],
        "Space held + Q must emit Shift+Q"
    );
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
    // Space had a partner -> no space emitted on its release.
    assert_eq!(m.process(&up(KeyCode::Space), &layout), MatchAction::Block);
}

/// When SandS is disabled (per-IME-state toggle off), the built-in Space role
/// is inert: Space behaves as an ordinary unmapped key (no tap/hold magic).
#[test]
fn builtin_sands_disabled_space_is_ordinary() {
    let layout = load("data/layouts/圧縮版_新下駄配列.txt");
    let mut m = InputMatcher::default();
    m.set_sands_enabled(false);

    // Space is unmapped & not a combo key in 新下駄 -> passes straight through.
    assert_eq!(m.process(&down(KeyCode::Space), &layout), MatchAction::PassThrough);
    assert_eq!(m.process(&up(KeyCode::Space), &layout), MatchAction::PassThrough);
}

/// FR-8: suspend -> passthrough; resume -> remapping restored.
#[test]
fn suspend_toggle() {
    let layout = load("data/layouts/samples/toy_simul.txt");
    let mut m = InputMatcher::default();
    assert!(m.toggle_suspended());
    assert_eq!(m.process(&down(KeyCode::Q), &layout), MatchAction::PassThrough);
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::PassThrough);
    assert!(!m.toggle_suspended());
    // Q is not a combo key in the SandS toy -> immediate base emit.
    assert!(!emit(m.process(&down(KeyCode::Q), &layout)).is_empty());
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
}
