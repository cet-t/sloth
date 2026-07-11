//! Zero-dependency smoke tests for the standalone parser (default features).
//! Exercises the `parse_str` path with no `sloth-core` or `encoding_rs`.

use dvorakj_parser::{parse_str, LayoutMode, ParseError, ParseOptions};

const TOY: &str = "\
テストレイアウト
/* comment is stripped */
[
q|w|e
]
";

#[test]
fn parses_name_mode_and_base_grid() {
    let report = parse_str(TOY, ParseOptions::from_source_id("toy.jp.txt")).expect("parse ok");
    assert_eq!(report.layout.name, "テストレイアウト");
    assert_eq!(report.layout.mode, LayoutMode::Legacy);
    assert!(
        !report.layout.single_map.is_empty(),
        "base grid must populate single_map"
    );
    assert!(report.warnings.is_empty(), "clean input has no warnings");
    assert_eq!(report.layout.source_id.as_deref(), Some("toy.jp.txt"));
}

#[test]
fn comments_are_stripped() {
    // The comment body must not leak into the layout name.
    let report = parse_str(TOY, ParseOptions::default()).expect("parse ok");
    assert!(!report.layout.name.contains("comment"));
}

#[test]
fn lenient_skips_unknown_trigger_with_warning() {
    let text = "名前\n-option-input\n[\n{zz} | -ff\n]\n";
    let report = parse_str(text, ParseOptions::from_source_id("x.jp.txt")).expect("lenient ok");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, dvorakj_parser::ParseWarning::UnknownTrigger { .. })),
        "unknown trigger must be recorded as a warning: {:?}",
        report.warnings
    );
}

#[test]
fn strict_errors_on_unknown_trigger() {
    let text = "名前\n-option-input\n[\n{zz} | -ff\n]\n";
    let opts = ParseOptions {
        strict: true,
        ..ParseOptions::from_source_id("x.jp.txt")
    };
    let err = parse_str(text, opts).unwrap_err();
    assert!(matches!(err, ParseError::UnknownTrigger { .. }));
}
