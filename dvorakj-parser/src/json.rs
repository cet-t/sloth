//! JSON serialization of [`ParseReport`] (`json` feature).
//!
//! `BTreeMap<KeyChord, _>` cannot be a JSON object key, so maps are emitted as
//! lists of entries. Keys and tokens use stable string tags.

use serde::Serialize;

use crate::model::{
    Key, OutputSeq, OutputToken, ParseError, ParseReport, ParseWarning, ParsedLayout,
};

pub const SCHEMA_VERSION: u32 = 1;

fn key_name(k: Key) -> String {
    format!("{:?}", k)
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum JsonToken {
    Key { code: String, mods: u16 },
    Text { text: String },
    Named { name: String },
    ModDown { code: String },
    ModUp { code: String },
}

fn token(t: &OutputToken) -> JsonToken {
    match t {
        OutputToken::Key { code, mods } => JsonToken::Key {
            code: key_name(*code),
            mods: mods.bits(),
        },
        OutputToken::Text(s) => JsonToken::Text { text: s.clone() },
        OutputToken::Named(n) => JsonToken::Named {
            name: format!("{:?}", n),
        },
        OutputToken::ModDown(k) => JsonToken::ModDown { code: key_name(*k) },
        OutputToken::ModUp(k) => JsonToken::ModUp { code: key_name(*k) },
    }
}

fn tokens(seq: &OutputSeq) -> Vec<JsonToken> {
    seq.iter().map(token).collect()
}

#[derive(Serialize)]
struct JsonEntry {
    key: String,
    output: Vec<JsonToken>,
}

#[derive(Serialize)]
struct JsonChordEntry {
    keys: Vec<String>,
    output: Vec<JsonToken>,
}

#[derive(Serialize)]
struct JsonLayerEntry {
    keys: Vec<String>,
    map: Vec<JsonEntry>,
}

#[derive(Serialize)]
struct JsonLayout {
    source_id: Option<String>,
    name: String,
    mode: String,
    input_mode: String,
    keyboard: String,
    single_map: Vec<JsonEntry>,
    layer_maps: Vec<JsonLayerEntry>,
    layer_taps: Vec<JsonEntry>,
    layer_triggers: Vec<String>,
    combos: Vec<JsonChordEntry>,
    combo_keys: Vec<String>,
    sustained_triggers: Vec<String>,
    prefix_maps: Vec<JsonLayerEntry>,
    prefix_triggers: Vec<String>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum JsonWarning {
    UnknownTrigger { value: String, line: Option<usize> },
    MissingLayer { name: String, line: Option<usize> },
    SkippedBlock { line: Option<usize>, reason: String },
    DecodeReplacement { source_id: Option<String> },
}

#[derive(Serialize)]
struct JsonReport {
    ok: bool,
    schema_version: u32,
    layout: JsonLayout,
    warnings: Vec<JsonWarning>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum JsonError {
    UnsupportedEncoding,
    InvalidUtf8,
    UnknownTrigger {
        value: String,
        line: Option<usize>,
    },
    MalformedBlock {
        line: Option<usize>,
        message: String,
    },
}

#[derive(Serialize)]
struct JsonErrorReport {
    ok: bool,
    schema_version: u32,
    error: JsonError,
}

fn entries(map: &std::collections::BTreeMap<Key, OutputSeq>) -> Vec<JsonEntry> {
    map.iter()
        .map(|(k, v)| JsonEntry {
            key: key_name(*k),
            output: tokens(v),
        })
        .collect()
}

fn key_list(set: &std::collections::BTreeSet<Key>) -> Vec<String> {
    set.iter().map(|k| key_name(*k)).collect()
}

fn layer_entries(
    map: &std::collections::BTreeMap<
        crate::model::KeyChord,
        std::collections::BTreeMap<Key, OutputSeq>,
    >,
) -> Vec<JsonLayerEntry> {
    map.iter()
        .map(|(chord, inner)| JsonLayerEntry {
            keys: chord.as_slice().iter().map(|k| key_name(*k)).collect(),
            map: entries(inner),
        })
        .collect()
}

fn layout_dto(l: &ParsedLayout) -> JsonLayout {
    JsonLayout {
        source_id: l.source_id.clone(),
        name: l.name.clone(),
        mode: format!("{:?}", l.mode),
        input_mode: format!("{:?}", l.input_mode),
        keyboard: format!("{:?}", l.keyboard),
        single_map: entries(&l.single_map),
        layer_maps: layer_entries(&l.layer_maps),
        layer_taps: entries(&l.layer_taps),
        layer_triggers: key_list(&l.layer_triggers),
        combos: l
            .combos
            .iter()
            .map(|(chord, out)| JsonChordEntry {
                keys: chord.as_slice().iter().map(|k| key_name(*k)).collect(),
                output: tokens(out),
            })
            .collect(),
        combo_keys: key_list(&l.combo_keys),
        sustained_triggers: key_list(&l.sustained_triggers),
        prefix_maps: layer_entries(&l.prefix_maps),
        prefix_triggers: key_list(&l.prefix_triggers),
    }
}

fn warning_dto(w: &ParseWarning) -> JsonWarning {
    match w {
        ParseWarning::UnknownTrigger { value, line } => JsonWarning::UnknownTrigger {
            value: value.clone(),
            line: *line,
        },
        ParseWarning::MissingLayer { name, line } => JsonWarning::MissingLayer {
            name: name.clone(),
            line: *line,
        },
        ParseWarning::SkippedBlock { line, reason } => JsonWarning::SkippedBlock {
            line: *line,
            reason: reason.clone(),
        },
        ParseWarning::DecodeReplacement { source_id } => JsonWarning::DecodeReplacement {
            source_id: source_id.clone(),
        },
    }
}

/// Serialize a successful [`ParseReport`] to the schema-versioned JSON string.
pub fn report_to_json(report: &ParseReport) -> serde_json::Result<String> {
    let dto = JsonReport {
        ok: true,
        schema_version: SCHEMA_VERSION,
        layout: layout_dto(&report.layout),
        warnings: report.warnings.iter().map(warning_dto).collect(),
    };
    serde_json::to_string(&dto)
}

/// Serialize a [`ParseError`] to the schema-versioned error JSON string.
pub fn error_to_json(err: &ParseError) -> serde_json::Result<String> {
    let error = match err {
        ParseError::UnsupportedEncoding => JsonError::UnsupportedEncoding,
        ParseError::InvalidUtf8 => JsonError::InvalidUtf8,
        ParseError::UnknownTrigger { value, line } => JsonError::UnknownTrigger {
            value: value.clone(),
            line: *line,
        },
        ParseError::MalformedBlock { line, message } => JsonError::MalformedBlock {
            line: *line,
            message: message.clone(),
        },
    };
    let dto = JsonErrorReport {
        ok: false,
        schema_version: SCHEMA_VERSION,
        error,
    };
    serde_json::to_string(&dto)
}
