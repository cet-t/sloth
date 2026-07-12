//! Exercises `dvorakj_parser::sloth::to_compiled_layout` (requires the
//! `sloth` feature): a DvorakJ layout should convert into a
//! `sloth_parser::CompiledLayout` carrying the same base/combo/layer data.

use dvorakj_parser::sloth::to_compiled_layout;
use dvorakj_parser::{parse_str, ParseOptions};

const TOY: &str = "\
テストレイアウト
[
q|w|e
]
";

#[test]
fn converts_base_grid_and_name() {
    let report = parse_str(TOY, ParseOptions::from_source_id("toy.jp.txt")).expect("parse ok");
    let compiled = to_compiled_layout(report.layout);
    assert_eq!(compiled.name, "テストレイアウト");
    let base = compiled.layers.get("base").expect("base layer present");
    assert!(
        !base.keys.is_empty(),
        "single_map entries should survive conversion into the base layer"
    );
}

/// End-to-end `sloth convert -dj`: a real DvorakJ layout file, decoded and
/// parsed by dvorakj-parser, converted to CompiledLayout, serialized to
/// TOML by sloth-parser, and re-parsed as TOML -- the whole pipeline
/// `sloth.exe convert -dj` will run.
#[test]
fn real_dvorakj_layout_converts_to_valid_toml() {
    let bytes = include_bytes!("../../data/layouts/圧縮版_新下駄配列.txt");
    let options = dvorakj_parser::ParseOptions::from_source_id("圧縮版_新下駄配列.jp.txt");
    let report = dvorakj_parser::parse_bytes(bytes, "圧縮版_新下駄配列.jp.txt", options)
        .expect("decode+parse real dvorakj layout");
    let compiled = to_compiled_layout(report.layout);
    assert!(!compiled.layers["base"].keys.is_empty() || !compiled.combos.is_empty());

    let result = sloth_parser::to_toml(&compiled);
    let reparsed = sloth_parser::compile_toml(&result.toml)
        .expect("serialized TOML should be valid sloth TOML");
    assert_eq!(reparsed.name, compiled.name);
}
