//! Serializes a [`CompiledLayout`] back out to the sloth TOML schema (the
//! inverse of [`crate::compile_toml`]). Used by conversion tools (e.g.
//! `sloth convert`) that turn another source format (DvorakJ `.txt`) into a
//! hand-editable sloth `.toml` file.
//!
//! Lossy in one direction: `layer_maps`/`layer_taps`/`layer_triggers`/
//! `sustained_triggers`/`mode`/`input_mode` have no syntax in the TOML
//! schema yet (see `config-idea/schema.md`), so a source layout that uses
//! sustained (while-held, SandS-style) layers loses that behavior when
//! written out -- [`to_toml`] reports this via `warnings` rather than
//! silently dropping it.

use std::collections::BTreeMap;

use crate::{CompiledLayout, Key, OutputSeq, OutputToken, SpecialKey};

/// Result of serializing a [`CompiledLayout`] to TOML text.
pub struct ToTomlResult {
    pub toml: String,
    /// Human-readable notes about anything the TOML schema can't represent
    /// and therefore had to be dropped or approximated.
    pub warnings: Vec<String>,
}

/// Serialize a compiled layout to sloth TOML. Named layers, combos, and
/// sequences round-trip losslessly; see the module docs for what doesn't.
pub fn to_toml(l: &CompiledLayout) -> ToTomlResult {
    let mut warnings = Vec::new();
    let mut out = String::new();

    out.push_str("[meta]\n");
    out.push_str(&format!("name = {}\n", toml_string(&l.name)));
    let keyboard = match l.keyboard {
        crate::KeyboardLayout::Us => "us",
        crate::KeyboardLayout::Jis => "jis",
    };
    out.push_str(&format!("keyboard = {}\n", toml_string(keyboard)));

    // Layers are written as `.override` maps (not bare `[layers.<name>]`
    // grids) since a compiled layer's keys aren't necessarily confined to
    // the 4-row physical grid template (e.g. Space, Enter).
    //
    // "base" is written first, as a plain override map: with no `inherit`
    // set it defaults to inheriting "base" itself, which `Config::compile`
    // resolves as "my own (absent-here) grid plus these overrides" rather
    // than a cycle -- i.e. exactly this map.
    //
    // Every other layer is written as `inherit = "base"` plus only the keys
    // that *differ* from base. Without the diff, re-parsing would give the
    // layer base's keys *plus* its own -- writing only the differing keys
    // makes the round trip exact whenever the layer covers base (the normal
    // case, since compiled layers are produced by inheriting base in the
    // first place). A compiled layer that is *missing* a key base has can't
    // be represented (the schema has no "remove this inherited key" syntax
    // yet), so that leaks back in on re-parse; warn instead of hiding it.
    let base = l.layers.get("base");
    let mut layer_names: Vec<&String> = l.layers.keys().collect();
    layer_names.sort();
    for name in layer_names {
        let layer = &l.layers[name];
        if layer.keys.is_empty() {
            continue;
        }
        out.push('\n');
        if name == "base" || base.is_none() {
            // No base to diff against: write the full map.
            if name != "base" {
                // `inherit` defaults to "base" when an override is present;
                // this layout has no base layer at all, so make the layer
                // inherit *itself* instead -- self-inherit resolves to "own
                // (absent-here) grid plus these overrides", i.e. exactly
                // this map, and re-parsing doesn't fail on a missing "base".
                out.push_str(&format!("[layers.{}]\n", toml_key_ident(name)));
                out.push_str(&format!("inherit = {}\n", toml_string(name)));
            }
            out.push_str(&format!("[layers.{}.override]\n", toml_key_ident(name)));
            write_key_map(&mut out, &layer.keys, &mut warnings);
            continue;
        }
        let base = base.unwrap();
        let diff: BTreeMap<Key, OutputSeq> = layer
            .keys
            .iter()
            .filter(|(k, v)| base.keys.get(k) != Some(v))
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        let missing: Vec<&Key> = base
            .keys
            .keys()
            .filter(|k| !layer.keys.contains_key(k))
            .collect();
        if !missing.is_empty() {
            warnings.push(format!(
                "layer '{name}': {} key(s) present in base but not in this \
                 layer ({missing:?}) can't be masked by the TOML schema and \
                 will reappear in it on re-parse",
                missing.len()
            ));
        }
        out.push_str(&format!("[layers.{}]\n", toml_key_ident(name)));
        out.push_str("inherit = \"base\"\n");
        out.push_str(&format!("[layers.{}.override]\n", toml_key_ident(name)));
        write_key_map(&mut out, &diff, &mut warnings);
    }

    if !l.combos.is_empty() {
        out.push_str("\n[combos]\n");
        let mut entries: Vec<(String, &OutputSeq)> = l
            .combos
            .iter()
            .filter_map(|(chord, seq)| {
                join_key_names(chord.as_slice(), &mut warnings).map(|spec| (spec, seq))
            })
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (key_spec, seq) in entries {
            write_entry(&mut out, &key_spec, seq, &mut warnings);
        }
    }

    if !l.sequences.is_empty() {
        out.push_str("\n[sequences]\n");
        let mut entries: Vec<(String, &OutputSeq)> = l
            .sequences
            .iter()
            .filter_map(|(keys, seq)| join_key_names(keys, &mut warnings).map(|spec| (spec, seq)))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (key_spec, seq) in entries {
            write_entry(&mut out, &key_spec, seq, &mut warnings);
        }
    }

    if !l.states.is_empty() {
        out.push_str("\n[states]\n");
        let mut entries: Vec<(&String, &String)> = l.states.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        for (state, layer) in entries {
            out.push_str(&format!(
                "{} = {}\n",
                toml_key_ident(state),
                toml_string(layer)
            ));
        }
    }

    if !l.layer_maps.is_empty() || !l.layer_taps.is_empty() || !l.layer_triggers.is_empty() {
        warnings.push(format!(
            "{} sustained (while-held) layer trigger(s) have no TOML schema \
             equivalent yet and were dropped: this layout's SandS-style \
             behavior will not survive the round trip.",
            l.layer_triggers.len()
        ));
    }

    ToTomlResult {
        toml: out,
        warnings,
    }
}

fn write_key_map(out: &mut String, keys: &BTreeMap<Key, OutputSeq>, warnings: &mut Vec<String>) {
    let mut entries: Vec<(String, &OutputSeq)> = keys
        .iter()
        .filter_map(|(k, seq)| k.name().map(|n| (n.to_string(), seq)))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (key_name, seq) in entries {
        write_entry(out, &key_name, seq, warnings);
    }
}

fn write_entry(out: &mut String, key_spec: &str, seq: &OutputSeq, warnings: &mut Vec<String>) {
    let value = seq_to_cell(seq, warnings, key_spec);
    out.push_str(&format!(
        "{} = {}\n",
        toml_key_ident(key_spec),
        toml_string(&value)
    ));
}

/// Join a chord/sequence's keys into the comma-separated spec string the
/// TOML schema's `split_keys` expects (see `parse::split_keys`). Returns
/// `None` -- dropping the *whole* entry, with a warning, rather than
/// silently under-representing it -- when that isn't possible:
///
/// - any key has no textual `name()` (e.g. `Unknown`), or
/// - any key's name itself contains `,` (only `Key::Comma`, whose name is
///   literally `","`): joined with the same character used as the
///   inter-key separator, a lone comma key is indistinguishable from an
///   empty split segment, so `split_keys` can't reconstruct it -- e.g.
///   `[Comma, D]` joins to `",,d"`, which re-splits to just `["d"]`. This
///   is a real ambiguity in the schema itself, not something a smarter
///   join can route around.
fn join_key_names(keys: &[Key], warnings: &mut Vec<String>) -> Option<String> {
    let mut names = Vec::with_capacity(keys.len());
    for k in keys {
        match k.name() {
            Some(n) if !n.contains(',') => names.push(n),
            _ => {
                warnings.push(format!(
                    "combo/sequence {keys:?} includes {k:?}, which can't be \
                     represented in a comma-joined key spec; entry dropped"
                ));
                return None;
            }
        }
    }
    Some(names.join(","))
}

/// Inverse of `parse::cell_to_seq`: `Text` passes through as-is, `Named`
/// becomes its `{...}` token; `Key`/`ModDown`/`ModUp` tokens (raw key
/// presses, used by some DvorakJ outputs) have no TOML syntax, so they're
/// dropped with a warning rather than silently corrupting the output.
fn seq_to_cell(seq: &OutputSeq, warnings: &mut Vec<String>, key_spec: &str) -> String {
    let mut s = String::new();
    let mut dropped = 0;
    for tok in seq {
        match tok {
            OutputToken::Text(t) => s.push_str(t),
            OutputToken::Named(sp) => s.push_str(named_token_str(*sp)),
            OutputToken::Key { .. } | OutputToken::ModDown(_) | OutputToken::ModUp(_) => {
                dropped += 1;
            }
        }
    }
    if dropped > 0 {
        warnings.push(format!(
            "{key_spec}: {dropped} raw key-press output token(s) have no TOML syntax yet and were dropped"
        ));
    }
    s
}

fn named_token_str(sp: SpecialKey) -> &'static str {
    match sp {
        SpecialKey::Enter => "{enter}",
        SpecialKey::Backspace => "{bs}",
        SpecialKey::Left => "{left}",
        SpecialKey::Right => "{right}",
        SpecialKey::Up => "{up}",
        SpecialKey::Down => "{down}",
        SpecialKey::Tab => "{tab}",
        SpecialKey::Escape => "{esc}",
    }
}

/// TOML string literal, basic-string escaped (quotes/backslashes/control
/// chars). Good enough for the key names and output text this module emits;
/// not a general-purpose TOML writer.
fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A bare TOML key if `s` is a valid bare key (ASCII alnum/`_`/`-`,
/// non-empty), otherwise a quoted key -- covers both plain names (`q`,
/// `shift`) and symbol/multi-key specs (`` ` ``, `a,b`) that need quoting.
fn toml_key_ident(s: &str) -> String {
    let bare = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        s.to_string()
    } else {
        toml_string(s)
    }
}
