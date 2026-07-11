//! Integration smoke for the DvorakJ parser + 同時打鍵 InputMatcher.
//! Run with: cargo test -p sloth-core
//!
//! The matcher is now a timed simultaneous-press engine. These tests exercise
//! the behaviours that broke before the rewrite: in-order solo output of
//! chord-trigger keys, actual chords firing, function-key tokens, sustained
//! SandS layers, and the bypass paths (disable key / suspend / Ctrl).

use sloth_core::{
    Event, EventKind, InputMatcher, KeyCode, LayoutLoader, LayoutMode, MatchAction, Modifiers,
    OutputToken, SpecialKey,
};
use sloth_dvorakj_adapter::RmapDvorakJLayoutLoader;

fn down(code: KeyCode) -> Event {
    Event::new(EventKind::KeyDown, code, Modifiers::empty())
}
fn up(code: KeyCode) -> Event {
    Event {
        kind: EventKind::KeyUp,
        code,
        modifiers: Modifiers::empty(),
        timestamp: 0,
        held: false,
    }
}
fn emit(a: MatchAction) -> Vec<OutputToken> {
    match a {
        MatchAction::Emit(seq) | MatchAction::EmitThenPass(seq) => seq,
        other => panic!("expected Emit, got {other:?}"),
    }
}
fn load(name: &str) -> sloth_core::Layout {
    let loader = RmapDvorakJLayoutLoader::new();
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
        sloth_core::layout::canon_sort(&mut v);
        v
    };
    let combo_out = layout
        .combos
        .get(&chord)
        .expect("K+Q chord must exist")
        .clone();

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
        r.iter()
            .any(|t| matches!(t, OutputToken::Named(SpecialKey::Enter))),
        "R base output must contain a real Enter key, got {r:?}"
    );
    assert!(
        !r.iter()
            .any(|t| matches!(t, OutputToken::Text(s) if s.contains("enter"))),
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
        KeyCode::Q,
        KeyCode::W,
        KeyCode::E,
        KeyCode::R,
        KeyCode::T,
        KeyCode::Y,
        KeyCode::U,
        KeyCode::I,
        KeyCode::O,
        KeyCode::P,
    ];
    let mut m = InputMatcher::default();
    let mut got: Vec<OutputToken> = vec![];
    for &k in &row {
        // Down defers (these are all chord keys); up (within window) resolves.
        assert_eq!(m.process(&down(k), &layout), MatchAction::Block);
        if let MatchAction::Emit(seq) | MatchAction::EmitThenPass(seq) = m.process(&up(k), &layout)
        {
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
    assert_eq!(
        m.process(&down(KeyCode::Space), &layout),
        MatchAction::Block
    );
    assert!(!emit(m.process(&up(KeyCode::Space), &layout)).is_empty());

    // Hold Space, press Q -> shifted Q from the layer; multiple keys keep shifting.
    assert_eq!(
        m.process(&down(KeyCode::Space), &layout),
        MatchAction::Block
    );
    let q = emit(m.process(&down(KeyCode::Q), &layout));
    assert!(q.iter().any(|t| matches!(
        t,
        OutputToken::Key {
            code: KeyCode::Q,
            ..
        }
    )));
    let w = emit(m.process(&down(KeyCode::W), &layout));
    assert!(w.iter().any(|t| matches!(
        t,
        OutputToken::Key {
            code: KeyCode::W,
            ..
        }
    )));
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
    assert_eq!(
        m.process(&down(KeyCode::CtrlL), &layout),
        MatchAction::PassThrough
    );
    assert_eq!(
        m.process(&down(KeyCode::Q), &layout),
        MatchAction::PassThrough
    );
    assert_eq!(
        m.process(&up(KeyCode::Q), &layout),
        MatchAction::PassThrough
    );
    assert_eq!(
        m.process(&up(KeyCode::CtrlL), &layout),
        MatchAction::PassThrough
    );
}

/// Ctrl held auto-bypasses (Ctrl+letter stays a shortcut, never becomes kana).
#[test]
fn ctrl_held_bypasses() {
    let layout = load("data/layouts/圧縮版_新下駄配列.txt");
    let mut m = InputMatcher::default();
    assert_eq!(
        m.process(&down(KeyCode::CtrlL), &layout),
        MatchAction::PassThrough
    );
    // A would normally defer as a combo key; under Ctrl it passes through.
    assert_eq!(
        m.process(&down(KeyCode::A), &layout),
        MatchAction::PassThrough
    );
    assert_eq!(
        m.process(&up(KeyCode::A), &layout),
        MatchAction::PassThrough
    );
    assert_eq!(
        m.process(&up(KeyCode::CtrlL), &layout),
        MatchAction::PassThrough
    );
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
    assert_eq!(
        m.process(&down(KeyCode::ShiftL), &layout),
        MatchAction::PassThrough
    );
    assert_eq!(
        m.process(&up(KeyCode::ShiftL), &layout),
        MatchAction::PassThrough
    );
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
    assert_eq!(
        m.process(&down(KeyCode::Space), &layout),
        MatchAction::Block
    );
    let tap = emit(m.process(&up(KeyCode::Space), &layout));
    assert_eq!(
        tap,
        vec![OutputToken::Key {
            code: KeyCode::Space,
            mods: Modifiers::empty()
        }],
        "tapping Space alone must emit a Space"
    );

    // Hold Space + Q: inject synthetic LSHIFT↓ and pass the physical Q through.
    // The OS sees real Shift + real Q, preserving scan codes and extended flags.
    assert_eq!(
        m.process(&down(KeyCode::Space), &layout),
        MatchAction::Block
    );
    let q_action = m.process(&down(KeyCode::Q), &layout);
    assert_eq!(
        q_action,
        MatchAction::EmitThenPass(vec![OutputToken::ModDown(KeyCode::ShiftL)]),
        "first SandS partner injects ShiftL↓ and passes the content key through"
    );
    // Q key-up passes through (it was never eaten).
    assert_eq!(
        m.process(&up(KeyCode::Q), &layout),
        MatchAction::PassThrough
    );
    // Space release with a partner -> release the synthetic ShiftL.
    let space_up = emit(m.process(&up(KeyCode::Space), &layout));
    assert_eq!(
        space_up,
        vec![OutputToken::ModUp(KeyCode::ShiftL)],
        "Space release after partner must release synthetic ShiftL"
    );
}

/// When SandS is disabled (per-IME-state toggle off), the built-in Space role
/// is inert: Space behaves as an ordinary unmapped key (no tap/hold magic).
#[test]
fn builtin_sands_disabled_space_is_ordinary() {
    let layout = load("data/layouts/圧縮版_新下駄配列.txt");
    let mut m = InputMatcher::default();
    m.set_sands_enabled(false);

    // Space is unmapped & not a combo key in 新下駄 -> passes straight through.
    assert_eq!(
        m.process(&down(KeyCode::Space), &layout),
        MatchAction::PassThrough
    );
    assert_eq!(
        m.process(&up(KeyCode::Space), &layout),
        MatchAction::PassThrough
    );
}

/// FR-8: suspend -> passthrough; resume -> remapping restored.
#[test]
fn suspend_toggle() {
    let layout = load("data/layouts/samples/toy_simul.txt");
    let mut m = InputMatcher::default();
    assert!(m.toggle_suspended());
    assert_eq!(
        m.process(&down(KeyCode::Q), &layout),
        MatchAction::PassThrough
    );
    assert_eq!(
        m.process(&up(KeyCode::Q), &layout),
        MatchAction::PassThrough
    );
    assert!(!m.toggle_suspended());
    // Q is not a combo key in the SandS toy -> immediate base emit.
    assert!(!emit(m.process(&down(KeyCode::Q), &layout)).is_empty());
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
}

// ===== 順次打鍵 (prefix/sequential) tests =====

/// 月2-263 is detected as Sequential mode, with D and K as prefix triggers.
#[test]
fn tsuki_mode_and_triggers() {
    let layout = load("data/layouts/月2-263.txt");
    assert_eq!(layout.mode, LayoutMode::Sequential);
    assert!(layout.prefix_triggers.contains(&KeyCode::D));
    assert!(layout.prefix_triggers.contains(&KeyCode::K));
    assert!(!layout.sustained_triggers.contains(&KeyCode::D));
    assert!(!layout.sustained_triggers.contains(&KeyCode::K));
    assert!(!layout.prefix_maps.is_empty());
}

/// 月2-263: press D (trigger), release D, then press Q (content) → prefix output.
#[test]
fn sequential_prefix_basic() {
    let layout = load("data/layouts/月2-263.txt");
    let d_layer = {
        let mut v = vec![KeyCode::D];
        sloth_core::layout::canon_sort(&mut v);
        v
    };
    let prefix_map = layout
        .prefix_maps
        .get(&d_layer)
        .expect("D prefix layer must exist");
    let expected = prefix_map
        .get(&KeyCode::Q)
        .expect("D+Q prefix mapping")
        .clone();

    let mut m = InputMatcher::default();
    // D down: blocked (prefix trigger)
    assert_eq!(m.process(&down(KeyCode::D), &layout), MatchAction::Block);
    // D up: arms prefix window
    assert_eq!(m.process(&up(KeyCode::D), &layout), MatchAction::Block);
    // Q down within prefix window: emits the prefix-mapped output
    let out = emit(m.process(&down(KeyCode::Q), &layout));
    assert_eq!(
        out, expected,
        "D prefix + Q content must produce the prefix mapping"
    );
    assert_eq!(m.process(&up(KeyCode::Q), &layout), MatchAction::Block);
}

/// 月2-263: D held while content key pressed (overlapping timing) → still prefix.
#[test]
fn sequential_prefix_overlap() {
    let layout = load("data/layouts/月2-263.txt");
    let d_layer = {
        let mut v = vec![KeyCode::D];
        sloth_core::layout::canon_sort(&mut v);
        v
    };
    let prefix_map = layout
        .prefix_maps
        .get(&d_layer)
        .expect("D prefix layer must exist");
    let expected = prefix_map
        .get(&KeyCode::W)
        .expect("D+W prefix mapping")
        .clone();

    let mut m = InputMatcher::default();
    // D down
    assert_eq!(m.process(&down(KeyCode::D), &layout), MatchAction::Block);
    // W down while D still held: prefix resolves immediately
    let out = emit(m.process(&down(KeyCode::W), &layout));
    assert_eq!(
        out, expected,
        "D held + W must produce prefix mapping even with overlap"
    );
    assert_eq!(m.process(&up(KeyCode::W), &layout), MatchAction::Block);
    // D up: had partner, just block
    assert_eq!(m.process(&up(KeyCode::D), &layout), MatchAction::Block);
}

/// 新下駄: simultaneous mode is unchanged — combos still work, no prefix.
#[test]
fn simultaneous_mode_unchanged() {
    let layout = load("data/layouts/圧縮版_新下駄配列.txt");
    assert_eq!(layout.mode, LayoutMode::Simultaneous);
    assert!(layout.prefix_maps.is_empty());
    assert!(layout.prefix_triggers.is_empty());
    assert!(!layout.combos.is_empty());
}

/// Legacy SandS toy: mode is Legacy, sustained triggers still work.
#[test]
fn legacy_sands_mode_unchanged() {
    let layout = load("data/layouts/samples/toy_simul.txt");
    assert_eq!(layout.mode, LayoutMode::Legacy);
    assert!(layout.sustained_triggers.contains(&KeyCode::Space));
    assert!(layout.prefix_maps.is_empty());
}

// ===== 混合モード (Mixed) tests =====

/// ローナ: detected as Mixed mode, compound triggers parsed correctly.
#[test]
fn rona_mixed_mode_and_triggers() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    assert_eq!(layout.mode, LayoutMode::Mixed);
    // K (0x25) is a prefix trigger (registered via {カ行} | [k], ([k])
    assert!(layout.prefix_triggers.contains(&KeyCode::K));
    // All letter keys are triggers (registered via individual [x] | -XX entries)
    assert!(layout.prefix_triggers.contains(&KeyCode::S));
    assert!(layout.prefix_triggers.contains(&KeyCode::H));
    // Base grid has single-map entries
    assert!(!layout.single_map.is_empty());
    // Prefix maps exist for single-key triggers
    assert!(!layout.prefix_maps.is_empty());
    // K should have a prefix layer (from {カ行}[...] block)
    let k_layer = vec![KeyCode::K];
    assert!(
        layout.prefix_maps.contains_key(&k_layer),
        "K prefix layer must exist from カ行 block"
    );
}

/// ローナ: data structure sanity check.
#[test]
fn rona_data_structure() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");

    // Consonant keys have NO base grid entry (empty cells skipped).
    assert!(
        !layout.single_map.contains_key(&KeyCode::K),
        "K has no base grid"
    );
    assert!(
        !layout.single_map.contains_key(&KeyCode::G),
        "G has no base grid"
    );

    // Vowel keys DO have base grid entries.
    assert!(layout.single_map.contains_key(&KeyCode::A), "A = あ");
    assert!(layout.single_map.contains_key(&KeyCode::E), "E = え");

    // K is in both combo_keys and prefix_triggers (PrefixAndCombo route).
    assert!(layout.combo_keys.contains(&KeyCode::K), "K in combo_keys");
    assert!(
        layout.prefix_triggers.contains(&KeyCode::K),
        "K in prefix_triggers"
    );

    // Vowel A is in combo_keys (as content key in combo entries).
    assert!(layout.combo_keys.contains(&KeyCode::A), "A in combo_keys");
    // But A is NOT a prefix trigger (no block uses A as header).
    assert!(
        !layout.prefix_triggers.contains(&KeyCode::A),
        "A not prefix trigger"
    );

    // Combo K+A exists.
    let mut chord = vec![KeyCode::K, KeyCode::A];
    sloth_core::layout::canon_sort(&mut chord);
    assert!(layout.combos.contains_key(&chord), "combo K+A exists");

    // Prefix K→A exists.
    assert!(layout
        .prefix_maps
        .get(&vec![KeyCode::K])
        .unwrap()
        .contains_key(&KeyCode::A));

    // layer_taps for K: since K has no base grid entry and block has 1 name,
    // the fallback is raw K key.
    assert!(
        layout.layer_taps.contains_key(&KeyCode::K),
        "K has layer_taps"
    );
}

/// ローナ: all consonant triggers registered, prefix + combo data present.
#[test]
fn rona_all_triggers_registered() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");

    // All consonant triggers from -option-input with ([x] form
    let combo_triggers = [
        KeyCode::K,
        KeyCode::G,
        KeyCode::S,
        KeyCode::Z,
        KeyCode::T,
        KeyCode::D,
        KeyCode::N,
        KeyCode::H,
        KeyCode::B,
        KeyCode::P,
        KeyCode::M,
        KeyCode::Y,
        KeyCode::R,
        KeyCode::W,
    ];

    for &k in &combo_triggers {
        assert!(
            layout.combo_keys.contains(&k),
            "{:?} must be in combo_keys",
            k
        );
        assert!(
            layout.prefix_triggers.contains(&k),
            "{:?} must be in prefix_triggers",
            k
        );
        assert!(
            layout.prefix_maps.contains_key(&vec![k]),
            "{:?} must have prefix_maps entry",
            k
        );
        // Each consonant+A should produce a combo
        let mut chord = vec![k, KeyCode::A];
        sloth_core::layout::canon_sort(&mut chord);
        assert!(
            layout.combos.contains_key(&chord),
            "{:?}+A combo must exist",
            k
        );
    }

    // Prefix-only triggers (no `(` form in -option-input).
    // These keys are NOT combo-capable as triggers, but may still appear in
    // combo_keys as content keys from other blocks' combo grids (e.g. F and V
    // appear in the ダ行 grid).
    let prefix_only = [KeyCode::L];
    for &k in &prefix_only {
        assert!(
            layout.prefix_triggers.contains(&k),
            "{:?} must be in prefix_triggers",
            k
        );
    }
}

/// ローナ: L (ァ行 trigger) data structure verification.
#[test]
fn rona_l_data_structure() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");

    // L must be in prefix_triggers (ァ行 trigger, Prefix route).
    assert!(
        layout.prefix_triggers.contains(&KeyCode::L),
        "L must be in prefix_triggers"
    );

    // L should NOT be in combo_keys (ァ行 has no ( form).
    assert!(
        !layout.combo_keys.contains(&KeyCode::L),
        "L must NOT be in combo_keys"
    );

    // L prefix map must exist.
    let l_prefix = layout.prefix_maps.get(&vec![KeyCode::L]);
    assert!(
        l_prefix.is_some(),
        "L must have prefix_maps entry. All prefix keys: {:?}",
        layout.prefix_triggers
    );

    // L→A should give ァ.
    assert!(
        l_prefix.unwrap().contains_key(&KeyCode::A),
        "L prefix must map A → ァ"
    );

    // H prefix→L: check if ハ行 grid has an entry at L position.
    let h_prefix = layout.prefix_maps.get(&vec![KeyCode::H]).unwrap();
    eprintln!(
        "H prefix keys with values: {:?}",
        h_prefix.keys().collect::<Vec<_>>()
    );
    eprintln!("H prefix has L: {}", h_prefix.contains_key(&KeyCode::L));
}

/// ローナ: H (ハ行 trigger) data structure verification.
#[test]
fn rona_h_data_structure() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");

    // H must be in combo_keys and prefix_triggers (PrefixAndCombo route).
    assert!(
        layout.combo_keys.contains(&KeyCode::H),
        "H must be in combo_keys"
    );
    assert!(
        layout.prefix_triggers.contains(&KeyCode::H),
        "H must be in prefix_triggers"
    );

    // Combo H+A must exist → は
    let mut chord = vec![KeyCode::H, KeyCode::A];
    sloth_core::layout::canon_sort(&mut chord);
    assert!(
        layout.combos.contains_key(&chord),
        "combo H+A must exist, combos with H: {:?}",
        layout
            .combos
            .keys()
            .filter(|c| c.contains(&KeyCode::H))
            .collect::<Vec<_>>()
    );

    // Prefix H→A must exist
    let h_prefix = layout.prefix_maps.get(&vec![KeyCode::H]);
    assert!(h_prefix.is_some(), "H must have prefix_maps entry");
    assert!(
        h_prefix.unwrap().contains_key(&KeyCode::A),
        "H prefix must map A, keys: {:?}",
        h_prefix.unwrap().keys().collect::<Vec<_>>()
    );

    // A must be in combo_keys
    assert!(
        layout.combo_keys.contains(&KeyCode::A),
        "A must be in combo_keys"
    );
}

/// ローナ: H+A simultaneous (combo) and H→A sequential (prefix) with flush_due.
#[test]
fn rona_h_a_combo_and_prefix() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let mut chord = vec![KeyCode::H, KeyCode::A];
    sloth_core::layout::canon_sort(&mut chord);
    let expected = layout
        .combos
        .get(&chord)
        .expect("H+A combo must exist")
        .clone();

    // Simultaneous: H down, A down → combo
    let mut m = InputMatcher::default();
    assert_eq!(m.process(&down(KeyCode::H), &layout), MatchAction::Block);
    let out = m.process(&down(KeyCode::A), &layout);
    assert_eq!(
        out,
        MatchAction::Emit(expected.clone()),
        "H+A simultaneous must fire combo"
    );

    // Sequential via flush_due: H down → combo timeout → prefix → A down
    let mut m = InputMatcher::default();
    m.set_combo_window_ms(0);
    m.set_prefix_window_ms(5000);
    assert_eq!(m.process(&down(KeyCode::H), &layout), MatchAction::Block);
    assert_eq!(
        m.flush_due(&layout),
        None,
        "H must transition to prefix, not emit solo"
    );
    let out = m.process(&down(KeyCode::A), &layout);
    assert_eq!(
        out,
        MatchAction::Emit(expected.clone()),
        "H→A sequential via prefix must give same output"
    );

    // Sequential via key-up: H down, H up, A down
    let mut m = InputMatcher::default();
    m.set_prefix_window_ms(5000);
    assert_eq!(m.process(&down(KeyCode::H), &layout), MatchAction::Block);
    assert_eq!(m.process(&up(KeyCode::H), &layout), MatchAction::Block);
    let out = m.process(&down(KeyCode::A), &layout);
    assert_eq!(
        out,
        MatchAction::Emit(expected),
        "H(down,up)→A sequential must give same output"
    );
}

/// ローナ: check for spurious single-key combos (trigger == content after dedup).
#[test]
fn rona_no_single_key_combos() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    for (chord, _out) in &layout.combos {
        assert!(
            chord.len() >= 2,
            "single-key combo should not exist: {:?}",
            chord
        );
    }
}

/// ローナ: G+L combo should NOT exist (L position is empty in ガ行 grid).
#[test]
fn rona_gl_no_combo() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let mut chord = vec![KeyCode::G, KeyCode::L];
    sloth_core::layout::canon_sort(&mut chord);
    assert!(
        !layout.combos.contains_key(&chord),
        "G+L combo should not exist; ガ行 grid has empty L position"
    );
}

/// ローナ: dump combo entries involving G for diagnostics.
#[test]
fn rona_g_combos_diagnostic() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let g_combos: Vec<_> = layout
        .combos
        .keys()
        .filter(|chord| chord.contains(&KeyCode::G))
        .collect();
    // G should only combo with vowel-position keys (A, E, I, O, U, W, etc.),
    // NOT with other consonants like L.
    for chord in &g_combos {
        assert!(
            !chord.contains(&KeyCode::L),
            "unexpected G+L combo: {:?}",
            chord
        );
    }
}

/// ローナ: K release then A press (sequential) → prefix output.
#[test]
fn rona_prefix_ka_row() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let k_layer = vec![KeyCode::K];
    let prefix_map = layout
        .prefix_maps
        .get(&k_layer)
        .expect("K prefix layer must exist");
    assert!(
        !prefix_map.is_empty(),
        "K prefix layer must have content mappings"
    );

    let mut m = InputMatcher::default();
    // K down → K up → A down: sequential prefix
    assert_eq!(m.process(&down(KeyCode::K), &layout), MatchAction::Block);
    assert_eq!(m.process(&up(KeyCode::K), &layout), MatchAction::Block);
    if let Some(expected) = prefix_map.get(&KeyCode::A) {
        let out = emit(m.process(&down(KeyCode::A), &layout));
        assert_eq!(out, *expected, "K release then A must produce prefix kana");
    }
}

/// ローナ: K+A simultaneous (K first) → combo output, order irrelevant.
#[test]
fn rona_combo_ka_k_first() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let mut chord = vec![KeyCode::K, KeyCode::A];
    sloth_core::layout::canon_sort(&mut chord);
    let expected = layout
        .combos
        .get(&chord)
        .expect("K+A combo must exist")
        .clone();

    let mut m = InputMatcher::default();
    // K down, then A down while K held (within combo window) → combo fires
    assert_eq!(m.process(&down(KeyCode::K), &layout), MatchAction::Block);
    let out = emit(m.process(&down(KeyCode::A), &layout));
    assert_eq!(out, expected, "K first then A must fire combo");
}

/// ローナ: A+K simultaneous (A first) → same combo output, order irrelevant.
#[test]
fn rona_combo_ka_a_first() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let mut chord = vec![KeyCode::K, KeyCode::A];
    sloth_core::layout::canon_sort(&mut chord);
    let expected = layout
        .combos
        .get(&chord)
        .expect("K+A combo must exist")
        .clone();

    let mut m = InputMatcher::default();
    // A down first, then K down while A held → same combo must fire
    assert_eq!(m.process(&down(KeyCode::A), &layout), MatchAction::Block);
    let out = emit(m.process(&down(KeyCode::K), &layout));
    assert_eq!(out, expected, "A first then K must fire same combo");
}

// =========================================================================
// flush_due simulation tests — these test the REAL timing path that the
// daemon uses (combo_window expires → flush_due fires → prefix or solo).
// =========================================================================

/// ローナ: consonant held past combo_window → flush_due transitions to prefix
/// (does NOT emit solo), then content key resolves via prefix_maps.
/// Prefix state persists indefinitely — no timeout.
#[test]
fn rona_consonant_hold_then_content() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let prefix_out = layout
        .prefix_maps
        .get(&vec![KeyCode::K])
        .unwrap()
        .get(&KeyCode::A)
        .expect("K prefix → A must exist")
        .clone();

    let mut m = InputMatcher::default();
    m.set_combo_window_ms(0);

    // K down → pending
    assert_eq!(m.process(&down(KeyCode::K), &layout), MatchAction::Block);

    // combo_window expired (0ms) → flush_due transitions to prefix, no output
    assert_eq!(
        m.flush_due(&layout),
        None,
        "prefix_trigger must NOT emit solo on combo timeout"
    );

    // A down → resolves via prefix_maps (prefix persists indefinitely)
    let out = emit(m.process(&down(KeyCode::A), &layout));
    assert_eq!(
        out, prefix_out,
        "content key must resolve via prefix after flush_due"
    );
}

/// ローナ: consonant held past combo_window → flush_due → no content key →
/// prefix persists indefinitely (no solo timeout).
/// When a new trigger key arrives, the old prefix is flushed and replaces it.
#[test]
fn rona_consonant_hold_solo_timeout() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let solo_tap = layout
        .layer_taps
        .get(&KeyCode::K)
        .expect("K must have layer_taps")
        .clone();

    let mut m = InputMatcher::default();
    m.set_combo_window_ms(0);

    // K down → pending
    assert_eq!(m.process(&down(KeyCode::K), &layout), MatchAction::Block);

    // combo_window expired → transitions to prefix (no output)
    assert_eq!(m.flush_due(&layout), None);

    // K up → prefix trigger release will re-arm, but prefix persists
    assert_eq!(m.process(&up(KeyCode::K), &layout), MatchAction::Block);

    // Now press another trigger (G) → flushes K's prefix (emits K solo tap),
    // and defers G to new pending.
    assert_eq!(
        emit(m.process(&down(KeyCode::G), &layout)),
        solo_tap,
        "pressing new trigger must flush old prefix solo tap"
    );
}

/// ローナ: vowel (combo_key, NOT prefix_trigger) held past combo_window →
/// flush_due emits solo output immediately (not prefix transition).
#[test]
fn rona_vowel_hold_emits_immediately() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let vowel_out = layout
        .single_map
        .get(&KeyCode::A)
        .expect("A must be in single_map")
        .clone();

    let mut m = InputMatcher::default();
    m.set_combo_window_ms(0);
    m.set_hold_mode(true);

    // A down → pending
    assert_eq!(m.process(&down(KeyCode::A), &layout), MatchAction::Block);

    // combo_window expired → A is NOT a prefix_trigger → emits solo immediately
    let out = m
        .flush_due(&layout)
        .expect("vowel must emit solo on combo timeout");
    assert_eq!(out, vowel_out, "vowel solo must be single_map[A]");
}

/// ローナ: consonant auto-repeat while in prefix state must be blocked (no output).
#[test]
fn rona_consonant_autorepeat_blocked_in_prefix() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");

    let mut m = InputMatcher::default();
    m.set_combo_window_ms(0);
    m.set_prefix_window_ms(5000);

    // K down → pending
    assert_eq!(m.process(&down(KeyCode::K), &layout), MatchAction::Block);
    // flush_due → prefix transition
    assert_eq!(m.flush_due(&layout), None);

    // Auto-repeat of K (held flag)
    let repeat_event = Event {
        kind: EventKind::KeyDown,
        code: KeyCode::K,
        modifiers: Modifiers::empty(),
        timestamp: 0,
        held: true,
    };
    assert_eq!(
        m.process(&repeat_event, &layout),
        MatchAction::Block,
        "auto-repeat of prefix trigger in prefix state must be blocked"
    );
}

/// ローナ: S down, S up, then A down (classic sequential prefix path).
/// Full sequence including key-ups — must produce ONLY prefix output, no leak.
#[test]
fn rona_full_sequential_sa_no_leak() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let prefix_out = layout
        .prefix_maps
        .get(&vec![KeyCode::S])
        .unwrap()
        .get(&KeyCode::A)
        .expect("S prefix → A must exist")
        .clone();

    let mut m = InputMatcher::default();

    // S down → deferred (combo-capable key)
    assert_eq!(m.process(&down(KeyCode::S), &layout), MatchAction::Block);
    // S up → transition to prefix (single pending, prefix trigger)
    assert_eq!(m.process(&up(KeyCode::S), &layout), MatchAction::Block);
    // A down → resolve via prefix_maps
    let result = m.process(&down(KeyCode::A), &layout);
    assert!(
        matches!(result, MatchAction::Emit(_)),
        "A must resolve via prefix, got {result:?}"
    );
    assert_eq!(
        emit(result),
        prefix_out,
        "S→A sequential must produce prefix output"
    );
    // A up → consumed (key-down was blocked)
    assert_eq!(m.process(&up(KeyCode::A), &layout), MatchAction::Block);
    // Verify nothing pending (no prefix window still active)
    assert!(
        !m.has_pending(),
        "nothing should be pending after full sequence"
    );
}

/// ローナ: S held past combo_window → flush_due transitions to prefix →
/// S released → re-arms prefix (no timeout, so re-arm is harmless).
/// A down resolves the prefix normally.
#[test]
fn rona_flush_due_prefix_then_release_no_double_arm() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");

    let mut m = InputMatcher::default();
    m.set_combo_window_ms(0);

    // S down → pending
    assert_eq!(m.process(&down(KeyCode::S), &layout), MatchAction::Block);
    // flush_due → combo expired → transition to prefix
    assert_eq!(m.flush_due(&layout), None);
    // S up → re-arm (expected, no harm with infinite prefix)
    assert_eq!(m.process(&up(KeyCode::S), &layout), MatchAction::Block);
    // A down → resolve via prefix_maps
    let prefix_out = layout
        .prefix_maps
        .get(&vec![KeyCode::S])
        .unwrap()
        .get(&KeyCode::A)
        .expect("S prefix → A must exist")
        .clone();
    assert_eq!(
        emit(m.process(&down(KeyCode::A), &layout)),
        prefix_out,
        "must produce prefix output"
    );
    assert_eq!(m.process(&up(KeyCode::A), &layout), MatchAction::Block);
}

/// ローナ: worst-case timing — flush_due transitions S to prefix,
/// S released (re-arms prefix, harmless), A resolves prefix.
/// Since prefix never timeouts, re-arm is cosmetic.
#[test]
fn rona_flush_due_double_arm_then_prefix_resolve_no_leak() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");

    let mut m = InputMatcher::default();
    m.set_combo_window_ms(0);

    // S down → pending
    assert_eq!(m.process(&down(KeyCode::S), &layout), MatchAction::Block);

    // flush_due → combo expired → transition to prefix
    assert_eq!(m.flush_due(&layout), None);

    // S release → re-arms prefix (harmless, no timeout)
    assert_eq!(m.process(&up(KeyCode::S), &layout), MatchAction::Block);

    // A down → prefix resolves
    let prefix_out = layout
        .prefix_maps
        .get(&vec![KeyCode::S])
        .unwrap()
        .get(&KeyCode::A)
        .expect("S prefix → A")
        .clone();
    assert_eq!(
        emit(m.process(&down(KeyCode::A), &layout)),
        prefix_out,
        "prefix must resolve to さ"
    );

    // A up → consumed
    assert_eq!(m.process(&up(KeyCode::A), &layout), MatchAction::Block);

    // flush_due must NOT emit anything (no chord, prefix consumed)
    assert_eq!(m.flush_due(&layout), None, "no spurious output");
}

/// ローナ: G then A (sequential prefix, any delay) → が.
/// Prefix never timeouts — infinite wait.
#[test]
fn rona_ga_sequential_infinite_wait() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let prefix_out = layout
        .prefix_maps
        .get(&vec![KeyCode::G])
        .unwrap()
        .get(&KeyCode::A)
        .expect("G prefix → A")
        .clone();

    let mut m = InputMatcher::default();

    // G down → G up → prefix armed
    assert_eq!(m.process(&down(KeyCode::G), &layout), MatchAction::Block);
    assert_eq!(m.process(&up(KeyCode::G), &layout), MatchAction::Block);

    // A down → prefix resolves (works even after arbitrary delay)
    assert_eq!(
        emit(m.process(&down(KeyCode::A), &layout)),
        prefix_out,
        "G→A must produce が regardless of delay"
    );
}

/// ローナ: GGA → っが.
/// First G arms prefix. Second G flushes old prefix (っ) + re-arms.
/// Then A resolves → が.
#[test]
fn rona_gga_sequence() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let g_solo = layout
        .layer_taps
        .get(&KeyCode::G)
        .expect("G must have layer_taps")
        .clone();
    let ga_output = layout
        .prefix_maps
        .get(&vec![KeyCode::G])
        .unwrap()
        .get(&KeyCode::A)
        .expect("G prefix → A")
        .clone();

    let mut m = InputMatcher::default();

    // First G: arms prefix (no output)
    assert_eq!(m.process(&down(KeyCode::G), &layout), MatchAction::Block);
    assert_eq!(m.process(&up(KeyCode::G), &layout), MatchAction::Block);

    // Second G: prefix active → flush old prefix solo (っ) + defer G
    assert_eq!(
        emit(m.process(&down(KeyCode::G), &layout)),
        g_solo,
        "second G must emit old prefix solo tap (っ)"
    );
    // G up → arms new prefix
    assert_eq!(m.process(&up(KeyCode::G), &layout), MatchAction::Block);

    // A → が
    assert_eq!(
        emit(m.process(&down(KeyCode::A), &layout)),
        ga_output,
        "A must resolve via new prefix to が"
    );
}

/// ローナ: S+A combo resolves, A released first, S released last → no leak.
#[test]
fn rona_combo_release_order_invariant_no_leak() {
    let layout = load("data/layouts/一打鍵ローマ字入力「ローナ」.txt");
    let mut chord = vec![KeyCode::S, KeyCode::A];
    sloth_core::layout::canon_sort(&mut chord);
    let expected = layout.combos.get(&chord).expect("S+A combo").clone();

    let mut m = InputMatcher::default();

    assert_eq!(m.process(&down(KeyCode::S), &layout), MatchAction::Block);
    assert_eq!(emit(m.process(&down(KeyCode::A), &layout)), expected);

    // A up first
    assert_eq!(m.process(&up(KeyCode::A), &layout), MatchAction::Block);
    // S up last → no prefix arm
    assert_eq!(
        m.process(&up(KeyCode::S), &layout),
        MatchAction::Block,
        "S release last after combo must not arm prefix"
    );
    assert!(!m.has_pending());
}
