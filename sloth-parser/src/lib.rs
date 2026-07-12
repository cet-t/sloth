//! sloth-parser: standalone TOML/JSON layout-config parser for rmap.
//!
//! Parses the schema prototyped under `config-idea/` into a self-contained
//! [`CompiledLayout`]. Deliberately free of any internal (workspace) crate
//! dependency so it can be reused outside this repository. Downstream crates
//! (e.g. `sloth-core`) convert [`CompiledLayout`] into their own `Layout` type.
//!
//! # Example
//! ```no_run
//! let toml_src = r#"
//! [meta]
//! name = "demo"
//! [layers.base]
//! grid = [["q"], ["a"]]
//! [combos]
//! "a,b" = "@"
//! [sequences]
//! "d,v" = "★"
//! "#;
//! let layout = sloth_parser::compile_toml(toml_src).unwrap();
//! assert_eq!(layout.name, "demo");
//! ```

mod key;
mod model;
mod parse;
mod serialize;

pub use key::{canon_key_order, canon_sort, Key, KeyChord};
pub use model::{
    CompileError, CompiledLayer, CompiledLayout, Config, Error, InputMode, KeyboardLayout, Layer,
    LayoutMode, Meta, Modifiers, OutputSeq, OutputToken, ParseError, SpecialKey,
};
pub use parse::{compile_json, compile_toml, parse_json, parse_toml};
pub use serialize::{to_toml, ToTomlResult};

#[cfg(test)]
mod tests {
    use super::*;

    const TOML_FIXTURE: &str = include_str!("../../config-idea/config.toml");
    const JSON_FIXTURE: &str = include_str!("../../config-idea/config.json");

    #[test]
    fn toml_fixture_compiles() {
        let l = compile_toml(TOML_FIXTURE).expect("toml parse + compile");
        assert_eq!(l.name, "my-layout");
        assert!(l.layers.contains_key("base"));
        assert!(l.layers.contains_key("shift"));
        assert!(l.layers.contains_key("kana"));

        // base: Q -> "q"
        assert_eq!(
            l.layers["base"].keys.get(&Key::Q),
            Some(&vec![OutputToken::Text("q".into())])
        );
        // shift inherits base then overrides Q -> "Q"
        assert_eq!(
            l.layers["shift"].keys.get(&Key::Q),
            Some(&vec![OutputToken::Text("Q".into())])
        );
        // shift inherits base then overrides "`" -> "~"
        assert_eq!(
            l.layers["shift"].keys.get(&Key::Grave),
            Some(&vec![OutputToken::Text("~".into())])
        );

        // 同時押し: a,b -> "@"
        assert_eq!(
            l.combos.get(&KeyChord::new([Key::A, Key::B])),
            Some(&vec![OutputToken::Text("@".into())])
        );
        // combo key order is irrelevant
        assert!(l.combos.contains_key(&KeyChord::new([Key::B, Key::A])));

        // 順押し: d,v -> "★"
        assert_eq!(
            l.sequences.get(&vec![Key::D, Key::V]),
            Some(&vec![OutputToken::Text("★".into())])
        );

        // states
        assert_eq!(l.states.get("ime_on"), Some(&"kana".to_string()));
        assert_eq!(l.states.get("default"), Some(&"base".to_string()));
    }

    #[test]
    fn json_fixture_compiles() {
        let l = compile_json(JSON_FIXTURE).expect("json parse + compile");
        assert_eq!(l.name, "my-layout");
        assert!(l.layers.contains_key("base"));
        assert!(l.layers.contains_key("kana"));
        assert!(l.combos.contains_key(&KeyChord::new([Key::A, Key::B])));
    }

    #[test]
    fn unknown_key_is_reported() {
        let bad = r#"
[meta]
name = "x"
[combos]
"a,zzz" = "@"
"#;
        let err = compile_toml(bad).expect_err("should fail");
        assert!(matches!(err, Error::Compile(CompileError::UnknownKey(_))));
    }

    #[test]
    fn override_named_map_still_works() {
        let src = r#"
[meta]
name = "x"
[layers.base]
grid = [["q"], ["a"]]
[layers.shift]
inherit = "base"
[layers.shift.override]
"q" = "Q"
"a" = "A"
"#;
        let l = compile_toml(src).expect("compile");
        assert_eq!(
            l.layers["shift"].keys.get(&Key::Q),
            Some(&vec![OutputToken::Text("Q".into())])
        );
        assert_eq!(
            l.layers["shift"].keys.get(&Key::A),
            Some(&vec![OutputToken::Text("A".into())])
        );
        // base value preserved where not overridden
        assert_eq!(
            l.layers["shift"].keys.get(&Key::Q),
            Some(&vec![OutputToken::Text("Q".into())])
        );
    }

    #[test]
    fn override_grid_form_works() {
        let src = r#"
[meta]
name = "x"
[layers.base]
grid = [["g"], ["q"], ["a"], ["z"]]
[layers.shift]
inherit = "base"
[layers.shift.override]
grid = [["G"], ["Q"], ["A"], ["Z"]]
"#;
        let l = compile_toml(src).expect("compile");
        // Row 1 col 0 is physical Q; grid override lands there.
        assert_eq!(
            l.layers["shift"].keys.get(&Key::Q),
            Some(&vec![OutputToken::Text("Q".into())])
        );
        assert_eq!(
            l.layers["shift"].keys.get(&Key::A),
            Some(&vec![OutputToken::Text("A".into())])
        );
    }

    #[test]
    fn missing_inherit_is_reported() {
        let bad = r#"
[meta]
name = "x"
[layers.foo]
inherit = "nope"
"#;
        let err = compile_toml(bad).expect_err("should fail");
        assert!(matches!(err, Error::Compile(CompileError::MissingLayer(_))));
    }

    #[test]
    fn shingeta_fixture_compiles() {
        let s = include_str!("../../config-idea/shingeta.toml");
        let l = compile_toml(s).expect("shingeta parse + compile");
        assert_eq!(l.name, "shingeta");
        assert!(!l.layers["base"].keys.is_empty());
        assert!(!l.combos.is_empty());

        // combo k,q -> ふぁ
        assert_eq!(
            l.combos.get(&KeyChord::new([Key::K, Key::Q])),
            Some(&vec![OutputToken::Text("ふぁ".into())])
        );
        // combo order is irrelevant
        assert!(l.combos.contains_key(&KeyChord::new([Key::Q, Key::K])));

        // {enter} token parsed into a Named special key:
        // base row1 col3 (R) -> 、 + Enter
        let seq = l.layers["base"]
            .keys
            .get(&Key::R)
            .expect("R mapped in base");
        assert_eq!(seq[0], OutputToken::Text("、".into()));
        assert_eq!(seq[1], OutputToken::Named(SpecialKey::Enter));
    }

    #[test]
    fn jis_number_row_has_caret_and_yen() {
        let src = r#"
[meta]
name = "x"
keyboard = "jis"
[layers.base]
grid = [["1","2","3","4","5","6","7","8","9","0","-","^","¥"]]
"#;
        let l = compile_toml(src).expect("compile");
        assert_eq!(
            l.layers["base"].keys.get(&Key::Caret),
            Some(&vec![OutputToken::Text("^".into())])
        );
        assert_eq!(
            l.layers["base"].keys.get(&Key::Yen),
            Some(&vec![OutputToken::Text("¥".into())])
        );
    }

    #[test]
    fn empty_output_is_ignored() {
        let src = r#"
[meta]
name = "x"
[layers.base]
grid = [[], ["q"]]
[layers.shift]
inherit = "base"
[layers.shift.override]
"q" = ""
[combos]
"a,b" = ""
[sequences]
"d,v" = ""
"#;
        let l = compile_toml(src).expect("compile");
        // empty named override keeps the inherited value (no empty seq inserted)
        assert_eq!(
            l.layers["shift"].keys.get(&Key::Q),
            Some(&vec![OutputToken::Text("q".into())])
        );
        assert!(l.combos.is_empty());
        assert!(l.sequences.is_empty());
    }

    #[test]
    fn combo_grid_form_works() {
        let src = r#"
[meta]
name = "x"
[layers.base]
grid = [["q"], ["a"]]
[combos.k]
grid = [
  ["A"],
  ["K"],
]
"#;
        let l = compile_toml(src).expect("compile");
        // [combos.k] grid: row0 col0 (Grave) -> "A"; row1 col0 (Q) -> "K"
        assert_eq!(
            l.combos.get(&KeyChord::new([Key::K, Key::Grave])),
            Some(&vec![OutputToken::Text("A".into())])
        );
        assert_eq!(
            l.combos.get(&KeyChord::new([Key::K, Key::Q])),
            Some(&vec![OutputToken::Text("K".into())])
        );
    }

    #[test]
    fn to_toml_round_trips_shingeta_fixture() {
        let src = include_str!("../../config-idea/shingeta.toml");
        let original = compile_toml(src).expect("compile shingeta");

        let result = to_toml(&original);
        // shingeta uses combos with a lone `,` (Comma) key, which can't
        // round-trip through the comma-joined spec string (see
        // `serialize::join_key_names`'s doc comment) -- those specific
        // entries are expected to warn and drop, nothing else should.
        assert!(
            result.warnings.iter().all(|w| w.contains("Comma")),
            "unexpected non-Comma warnings: {:?}",
            result.warnings
        );

        let reparsed = compile_toml(&result.toml).expect("re-parse serialized toml");
        assert_eq!(reparsed.name, original.name);
        let original_combos_sans_comma: std::collections::BTreeMap<_, _> = original
            .combos
            .iter()
            .filter(|(chord, _)| !chord.as_slice().contains(&Key::Comma))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(reparsed.combos, original_combos_sans_comma);
        assert_eq!(reparsed.layers["base"].keys, original.layers["base"].keys);
    }
}
