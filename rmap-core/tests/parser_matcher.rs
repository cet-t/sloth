//! Integration smoke for DvorakJ parser + InputMatcher layers/SandS/tap.
//! Run with: cargo test -p rmap-core

use rmap_core::{DvorakJLayoutLoader, LayoutLoader, InputMatcher, MatchAction, Event, EventKind, KeyCode, Modifiers, OutputToken};

fn down(code: KeyCode) -> Event {
    Event::new(EventKind::KeyDown, code, Modifiers::empty())
}
fn up(code: KeyCode) -> Event {
    Event { kind: EventKind::KeyUp, code, modifiers: Modifiers::empty(), timestamp: 0, held: false }
}
/// Extract the emitted OutputSeq, asserting the action was `Emit`.
fn emit(a: MatchAction) -> Vec<OutputToken> {
    match a {
        MatchAction::Emit(seq) => seq,
        other => panic!("expected Emit, got {other:?}"),
    }
}

#[test]
fn sands_layer_and_tap_from_sample() {
    let loader = DvorakJLayoutLoader::new();
    // Resolve relative to crate root so `cargo test -p rmap-core` finds it regardless of cwd.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&manifest).join("../data/layouts/samples/toy_simul.txt");
    let bytes = std::fs::read(&path).expect("sample layout must exist for this test");
    let layout = loader.load(&bytes, "test-sands").expect("parse must succeed");

    assert!(layout.layer_triggers.contains(&KeyCode::Space), "Space must be registered as layer trigger");
    assert!(!layout.layer_taps.is_empty(), "layer tap for Space should be present (default or marker)");

    let mut m = InputMatcher::default();

    // 1. Layer key down is blocked (never leaks to the OS), no output yet.
    assert_eq!(m.process(&down(KeyCode::Space), &layout), MatchAction::Block);
    // Clean tap: Space up with no partner -> tap output (Space key).
    let out = emit(m.process(&up(KeyCode::Space), &layout));
    assert!(!out.is_empty());

    // 2. Combo: hold Space, press Q -> shifted from layer map.
    assert_eq!(m.process(&down(KeyCode::Space), &layout), MatchAction::Block);
    let out2 = emit(m.process(&down(KeyCode::Q), &layout));
    // In the toy sample the shifted Q cell is 'Q' (upper) -> Key Q + SHIFT.
    assert!(out2.iter().any(|t| matches!(t, OutputToken::Key { code: KeyCode::Q, .. })));

    // Q up was consumed (its down was remapped) -> blocked, not passed through.
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
    // Space up after a partner: blocked, and must NOT tap.
    assert_eq!(m.process(&up(KeyCode::Space), &layout), MatchAction::Block);
}

#[test]
fn combo_layers_omitted_number_row_and_tap() {
    let loader = DvorakJLayoutLoader::new();
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&manifest).join("../data/layouts/samples/toy_combo.txt");
    let bytes = std::fs::read(&path).expect("toy_combo sample must exist");
    let layout = loader.load(&bytes, "test-combo").expect("parse must succeed");

    // Both declared layer keys registered.
    assert!(layout.layer_triggers.contains(&KeyCode::Muhenkan));
    assert!(layout.layer_triggers.contains(&KeyCode::Henkan));
    // Single layer + combo layer both present.
    assert!(layout.layer_maps.contains_key(&vec![KeyCode::Muhenkan]));
    assert!(layout.layer_maps.contains_key(&vec![KeyCode::Muhenkan, KeyCode::Henkan]));

    let mut m = InputMatcher::default();

    // Single layer with the number row omitted bottom-aligns: the first grid
    // row maps to the physical Q row, so Q -> 'A' (uppercase => SHIFT).
    assert_eq!(m.process(&down(KeyCode::Muhenkan), &layout), MatchAction::Block);
    let out = emit(m.process(&down(KeyCode::Q), &layout));
    assert!(out.iter().any(|t| matches!(t, OutputToken::Key { code: KeyCode::A, .. })));

    // Held repeat of Q must still remap (M1: no leak of the original key).
    let held_q = down(KeyCode::Q).with_held(true);
    let out_held = emit(m.process(&held_q, &layout));
    assert!(out_held.iter().any(|t| matches!(t, OutputToken::Key { code: KeyCode::A, .. })));

    // Q up consumed -> blocked. Muhenkan had a partner -> blocked, no tap.
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
    assert_eq!(m.process(&up(KeyCode::Muhenkan), &layout), MatchAction::Block);
}

#[test]
fn unmapped_key_passes_through() {
    let loader = DvorakJLayoutLoader::new();
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&manifest).join("../data/layouts/samples/toy_combo.txt");
    let bytes = std::fs::read(&path).unwrap();
    let layout = loader.load(&bytes, "test-base").unwrap();

    let mut m = InputMatcher::default();
    // F1 has no mapping in this layout -> both down and up pass through.
    assert_eq!(m.process(&down(KeyCode::F1), &layout), MatchAction::PassThrough);
    assert_eq!(m.process(&up(KeyCode::F1), &layout), MatchAction::PassThrough);

    // A base-mapped key emits and then its up is blocked (down was consumed).
    let base = emit(m.process(&down(KeyCode::Q), &layout));
    assert!(!base.is_empty());
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
}

/// FR-6: while a configured disable key is held, every key passes through
/// unchanged (rmap acts as if not running); remapping resumes after release.
#[test]
fn disable_key_held_passes_everything_through() {
    let loader = DvorakJLayoutLoader::new();
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&manifest).join("../data/layouts/samples/toy_combo.txt");
    let bytes = std::fs::read(&path).unwrap();
    let layout = loader.load(&bytes, "test-disable").unwrap();

    let mut m = InputMatcher::default();
    m.set_disable_keys([KeyCode::CtrlL, KeyCode::CtrlR]);

    // Disable key itself passes through (OS still sees Ctrl for its own shortcuts).
    assert_eq!(m.process(&down(KeyCode::CtrlL), &layout), MatchAction::PassThrough);
    // A normally-remapped key is NOT remapped while Ctrl is held.
    assert_eq!(m.process(&down(KeyCode::Q), &layout), MatchAction::PassThrough);
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::PassThrough);
    // Release the disable key (still seen as held at evaluation time -> passthrough).
    assert_eq!(m.process(&up(KeyCode::CtrlL), &layout), MatchAction::PassThrough);

    // Remapping is active again once the disable key is up.
    let base = emit(m.process(&down(KeyCode::Q), &layout));
    assert!(!base.is_empty());
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
}

/// FR-6: a key whose key-down was consumed *before* the disable key went down
/// still has its key-up blocked (symmetric), so no stray key-up leaks.
#[test]
fn disable_key_drains_inflight_blocked_keyup() {
    let loader = DvorakJLayoutLoader::new();
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&manifest).join("../data/layouts/samples/toy_combo.txt");
    let bytes = std::fs::read(&path).unwrap();
    let layout = loader.load(&bytes, "test-disable2").unwrap();

    let mut m = InputMatcher::default();
    m.set_disable_keys([KeyCode::AltL, KeyCode::AltR]);

    // Q consumed (remapped) before any disable key.
    let _ = emit(m.process(&down(KeyCode::Q), &layout));
    // Now Alt goes down -> bypass begins; Alt passes through.
    assert_eq!(m.process(&down(KeyCode::AltL), &layout), MatchAction::PassThrough);
    // Q's key-up must still be blocked (its down was consumed) — symmetric blocking.
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
    assert_eq!(m.process(&up(KeyCode::AltL), &layout), MatchAction::PassThrough);
}

/// FR-8: persistent stop/resume. While suspended, everything passes through;
/// resuming restores remapping. toggle_suspended flips and reports the state.
#[test]
fn suspend_toggle_passes_through_and_restores() {
    let loader = DvorakJLayoutLoader::new();
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&manifest).join("../data/layouts/samples/toy_combo.txt");
    let bytes = std::fs::read(&path).unwrap();
    let layout = loader.load(&bytes, "test-suspend").unwrap();

    let mut m = InputMatcher::default();
    assert!(!m.is_suspended());

    // Stop: everything passes through.
    assert!(m.toggle_suspended(), "toggle should report suspended=true");
    assert!(m.is_suspended());
    assert_eq!(m.process(&down(KeyCode::Q), &layout), MatchAction::PassThrough);
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::PassThrough);

    // Resume: remapping restored.
    assert!(!m.toggle_suspended(), "toggle should report suspended=false");
    let base = emit(m.process(&down(KeyCode::Q), &layout));
    assert!(!base.is_empty());
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
}
