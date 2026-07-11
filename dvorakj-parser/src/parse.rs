//! Main DvorakJ layout parser: orchestrates block, grid, cell, and keymap
//! modules to build a [`ParsedLayout`] from pre-processed text lines.

use std::collections::{BTreeMap, BTreeSet};

use crate::block::{
    extract_block, extract_block_from_last_bracket, is_self_marker, normalize_layer_name,
    parse_block_layer_names, split_tap_row,
};
use crate::cell::compile_cell;
use crate::grid::parse_grid;
use crate::keymap::key_from_scancode;
use crate::model::{
    sort_keys_by_rank, sort_keys_canonical, InputMode, Key, KeyChord, KeyboardLayout, LayoutMode,
    Modifiers, OutputSeq, OutputToken, ParseError, ParseOptions, ParseReport, ParseResult,
    ParseWarning, ParsedLayout,
};

/// Grid type produced by [`parse_grid`]: physical key → output sequence.
type Grid = BTreeMap<Key, OutputSeq>;

fn detect_mode(first_line: &str) -> LayoutMode {
    let has_sequential = first_line.contains('順');
    let has_simultaneous = first_line.contains("同時");
    match (has_sequential, has_simultaneous) {
        (true, true) => LayoutMode::Mixed,
        (false, true) => LayoutMode::Simultaneous,
        (true, false) => LayoutMode::Sequential,
        (false, false) => LayoutMode::Legacy,
    }
}

/// Detect `[name],[name][` bracket-named layer blocks: the part before the
/// last `[` contains at least one `]`, meaning there are bracket-enclosed names.
fn is_bracket_named_block(line: &str) -> bool {
    if let Some(last_open) = line.rfind('[') {
        last_open > 0 && line[..last_open].contains(']')
    } else {
        false
    }
}

/// Parse bracket-delimited layer names from a header like `[d],[k]`.
fn parse_bracket_names(header: &str) -> Vec<String> {
    let mut names = vec![];
    let mut rest = header;
    while let Some(open) = rest.find('[') {
        if let Some(close) = rest[open + 1..].find(']') {
            let name = rest[open + 1..open + 1 + close].trim();
            if !name.is_empty() {
                names.push(name.to_string());
            }
            rest = &rest[open + 1 + close + 1..];
        } else {
            break;
        }
    }
    names
}

fn resolve_trigger(trig: &str) -> Option<Key> {
    if let Some(k) = Key::from_dvorakj_name(trig) {
        return Some(k);
    }
    if let Ok(code) = u32::from_str_radix(trig, 16) {
        if let Some(k) = key_from_scancode(code) {
            return Some(k);
        }
    }
    None
}

/// Resolved trigger from `-option-input`.
struct TriggerSpec {
    key: Key,
    /// The `(` form was present — this name supports simultaneous (combo) routing.
    has_combo: bool,
}

/// Parse the right-hand side of `-option-input` trigger declarations.
///
/// Handles formats: `-10` (scancode), `[k]` (name ref), `[k], ([k]` (compound),
/// `[k][y]` (multi-key sequence), `([q]` (paren-only ref).
/// Returns `None` for multi-key-only entries (no single-key alternative).
fn resolve_trigger_spec(spec: &str, layer_triggers: &BTreeMap<String, Key>) -> Option<TriggerSpec> {
    let has_combo = spec.contains('(');
    for part in spec.split(',') {
        let part = part.trim().trim_start_matches('(');

        if part.starts_with('[') {
            if part.matches('[').count() > 1 {
                continue;
            }
            if let Some(name) = part.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                let name = name.trim();
                if let Some(&k) = layer_triggers.get(name) {
                    return Some(TriggerSpec { key: k, has_combo });
                }
            }
            continue;
        }

        let trig = part.trim_start_matches('-');
        if !trig.is_empty() {
            if let Some(k) = resolve_trigger(trig) {
                return Some(TriggerSpec { key: k, has_combo });
            }
        }
    }
    None
}

/// Parse pre-processed (comment-stripped) text into a [`ParseReport`].
pub(crate) fn parse_report(text: &str, options: &ParseOptions) -> ParseResult<ParseReport> {
    let keyboard = options.keyboard;
    let lines: Vec<&str> = text.lines().collect();
    let mut warnings: Vec<ParseWarning> = vec![];
    let mut layout = ParsedLayout {
        source_id: options.source_id.clone(),
        name: options.source_id.clone().unwrap_or_default(),
        mode: LayoutMode::default(),
        input_mode: InputMode::Direct,
        keyboard,
        single_map: BTreeMap::new(),
        layer_maps: BTreeMap::new(),
        layer_taps: BTreeMap::new(),
        layer_triggers: BTreeSet::new(),
        combos: BTreeMap::new(),
        combo_keys: BTreeSet::new(),
        sustained_triggers: BTreeSet::new(),
        prefix_maps: BTreeMap::new(),
        prefix_triggers: BTreeSet::new(),
    };
    let mut layer_triggers: BTreeMap<String, Key> = BTreeMap::new();
    let mut sustained_names: BTreeSet<String> = BTreeSet::new();
    let mut combo_capable_names: BTreeSet<String> = BTreeSet::new();
    layer_triggers.insert("shift".to_string(), Key::ShiftL);
    sustained_names.insert("shift".to_string());
    let mut base_row_count = 0usize;
    let mut i = 0usize;

    // Parse first non-empty line as layout name and detect mode.
    while i < lines.len() {
        let t = lines[i].trim();
        if !t.is_empty() {
            layout.name = t.to_string();
            layout.mode = detect_mode(t);
            i += 1;
            break;
        }
        i += 1;
    }

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() {
            i += 1;
            continue;
        }

        if line.starts_with("-option-input") {
            if let Some((body, end)) = extract_block(&lines, i) {
                for bl in body {
                    if let Some((lname, trig_raw)) = bl.split_once('|') {
                        let lname = normalize_layer_name(lname);
                        let trig_raw = trig_raw.trim();
                        let Some(spec) = resolve_trigger_spec(trig_raw, &layer_triggers) else {
                            if options.strict {
                                return Err(ParseError::UnknownTrigger {
                                    value: trig_raw.to_string(),
                                    line: Some(i),
                                });
                            }
                            warnings.push(ParseWarning::UnknownTrigger {
                                value: trig_raw.to_string(),
                                line: Some(i),
                            });
                            continue;
                        };
                        layer_triggers.insert(lname.clone(), spec.key);
                        if spec.has_combo {
                            combo_capable_names.insert(lname.clone());
                        }
                        match layout.mode {
                            LayoutMode::Legacy | LayoutMode::Simultaneous => {
                                sustained_names.insert(lname);
                            }
                            LayoutMode::Sequential | LayoutMode::Mixed => {}
                        }
                    }
                }
                i = end + 1;
                continue;
            }
        }

        // Bracket-named layer blocks: `[d],[k][...]` etc.
        if line.starts_with('[') && is_bracket_named_block(line) {
            if let Some((body, end)) = extract_block_from_last_bracket(&lines, i) {
                let last_open = line.rfind('[').unwrap();
                let header = &line[..last_open];
                let names = parse_bracket_names(header);
                if !names.is_empty() {
                    let Some(layer_ks) =
                        resolve_layer_keys(&names, &mut layer_triggers, options, i, &mut warnings)?
                    else {
                        i = end + 1;
                        continue;
                    };

                    let (grid_body, tap_cell) = split_tap_row(&body);
                    let total_rows = base_row_count.max(grid_body.len());
                    let offset = total_rows.saturating_sub(grid_body.len());
                    let grid = parse_grid(grid_body, offset, keyboard);

                    apply_layer_taps(&names, &layer_ks, &tap_cell, keyboard, &mut layout);

                    let is_sustained = names.iter().all(|n| sustained_names.contains(n));
                    let is_combo_cap = names.iter().any(|n| combo_capable_names.contains(n));
                    let route = determine_route(layout.mode, is_sustained, false, is_combo_cap);

                    apply_route(&route, &layer_ks, grid, &mut layout);
                    for k in &layer_ks {
                        layout.layer_triggers.insert(*k);
                    }
                    i = end + 1;
                    continue;
                }
            }
        }

        // Plain base grid: `[...]`
        if line.starts_with('[') {
            if let Some((body, end)) = extract_block(&lines, i) {
                base_row_count = body.len();
                let grid = parse_grid(&body, 0, keyboard);
                layout.single_map = grid;
                i = end + 1;
                continue;
            }
        }

        // Detect `(` paren-wrapped blocks (simultaneous in Mixed mode).
        let is_paren = line.starts_with('(');
        let effective_line = if is_paren {
            line.trim_start_matches('(')
        } else {
            line
        };

        if effective_line.starts_with('{')
            || (effective_line.starts_with('-') && !effective_line.starts_with("-option-input"))
        {
            if let Some((body, end)) = extract_block(&lines, i) {
                let names = parse_block_layer_names(effective_line);
                if names.is_empty() {
                    i += 1;
                    continue;
                }
                let Some(layer_ks) =
                    resolve_layer_keys(&names, &mut layer_triggers, options, i, &mut warnings)?
                else {
                    i = end + 1;
                    continue;
                };

                let (grid_body, tap_cell) = split_tap_row(&body);
                let total_rows = base_row_count.max(grid_body.len());
                let offset = total_rows.saturating_sub(grid_body.len());
                let grid = parse_grid(grid_body, offset, keyboard);

                apply_layer_taps(&names, &layer_ks, &tap_cell, keyboard, &mut layout);

                let is_sustained = names.iter().all(|n| sustained_names.contains(n));
                let is_combo_cap = names.iter().any(|n| combo_capable_names.contains(n));
                let route = determine_route(layout.mode, is_sustained, is_paren, is_combo_cap);

                apply_route(&route, &layer_ks, grid, &mut layout);
                i = end + 1;
                continue;
            }
        }

        i += 1;
    }

    Ok(ParseReport { layout, warnings })
}

/// Resolve a block's layer names to keys, registering scan-code names as we go.
/// Returns `None` (block should be skipped) if any name is unresolvable in
/// lenient mode; errors in strict mode. The returned vec is rank-sorted to
/// match the legacy `key_sort` ordering.
fn resolve_layer_keys(
    names: &[String],
    layer_triggers: &mut BTreeMap<String, Key>,
    options: &ParseOptions,
    line: usize,
    warnings: &mut Vec<ParseWarning>,
) -> ParseResult<Option<Vec<Key>>> {
    let mut layer_ks: Vec<Key> = Vec::with_capacity(names.len());
    for n in names {
        if let Some(k) = layer_triggers.get(n) {
            layer_ks.push(*k);
        } else if let Some(k) = u32::from_str_radix(n, 16).ok().and_then(key_from_scancode) {
            layer_triggers.insert(n.clone(), k);
            layer_ks.push(k);
        } else {
            if options.strict {
                return Err(ParseError::MalformedBlock {
                    line: Some(line),
                    message: format!("undefined layer name '{n}'"),
                });
            }
            warnings.push(ParseWarning::MissingLayer {
                name: n.clone(),
                line: Some(line),
            });
            return Ok(None);
        }
    }
    sort_keys_by_rank(&mut layer_ks);
    Ok(Some(layer_ks))
}

/// Assign `layer_taps` for a block's trigger keys (self-marker, explicit tap
/// cell, single-name base fallback, or none). Mirrors the legacy logic.
fn apply_layer_taps(
    names: &[String],
    layer_ks: &[Key],
    tap_cell: &Option<String>,
    keyboard: KeyboardLayout,
    layout: &mut ParsedLayout,
) {
    for (n, &k) in names.iter().zip(layer_ks.iter()) {
        let tap_seq = match tap_cell {
            Some(cell) if is_self_marker(cell, names) => vec![OutputToken::Key {
                code: k,
                mods: Modifiers::empty(),
            }],
            Some(cell) => compile_cell(cell, keyboard),
            None if names.len() == 1 => layout.single_map.get(&k).cloned().unwrap_or_else(|| {
                vec![OutputToken::Key {
                    code: k,
                    mods: Modifiers::empty(),
                }]
            }),
            None => vec![],
        };
        if !tap_seq.is_empty() {
            layout.layer_taps.entry(k).or_insert(tap_seq);
        }
        let _ = n;
    }
}

enum BlockRoute {
    Sustained,
    Combo,
    Prefix,
    PrefixAndCombo,
}

fn apply_route(route: &BlockRoute, layer_ks: &[Key], grid: Grid, layout: &mut ParsedLayout) {
    match route {
        BlockRoute::Sustained => {
            layout
                .layer_maps
                .insert(KeyChord::from_vec(layer_ks.to_vec()), grid);
            for &k in layer_ks {
                layout.sustained_triggers.insert(k);
            }
        }
        BlockRoute::Combo => {
            for (&content, out) in &grid {
                if layer_ks.contains(&content) {
                    continue;
                }
                let mut chord = layer_ks.to_vec();
                chord.push(content);
                sort_keys_canonical(&mut chord);
                layout
                    .combos
                    .entry(KeyChord::from_vec(chord))
                    .or_insert_with(|| out.clone());
                layout.combo_keys.insert(content);
            }
            for &k in layer_ks {
                layout.combo_keys.insert(k);
            }
            layout
                .layer_maps
                .insert(KeyChord::from_vec(layer_ks.to_vec()), grid);
        }
        BlockRoute::Prefix => {
            for &k in layer_ks {
                layout
                    .prefix_maps
                    .entry(KeyChord::from_vec(vec![k]))
                    .or_insert_with(|| grid.clone());
                layout.prefix_triggers.insert(k);
            }
        }
        BlockRoute::PrefixAndCombo => {
            for (&content, out) in &grid {
                if layer_ks.contains(&content) {
                    continue;
                }
                let mut chord = layer_ks.to_vec();
                chord.push(content);
                sort_keys_canonical(&mut chord);
                layout
                    .combos
                    .entry(KeyChord::from_vec(chord))
                    .or_insert_with(|| out.clone());
                layout.combo_keys.insert(content);
            }
            for &k in layer_ks {
                layout.combo_keys.insert(k);
                layout
                    .prefix_maps
                    .entry(KeyChord::from_vec(vec![k]))
                    .or_insert_with(|| grid.clone());
                layout.prefix_triggers.insert(k);
            }
        }
    }
    for &k in layer_ks {
        layout.layer_triggers.insert(k);
    }
}

fn determine_route(
    mode: LayoutMode,
    is_sustained: bool,
    is_paren: bool,
    is_combo_capable: bool,
) -> BlockRoute {
    if is_sustained {
        return BlockRoute::Sustained;
    }
    match mode {
        LayoutMode::Legacy | LayoutMode::Simultaneous => BlockRoute::Combo,
        LayoutMode::Sequential => BlockRoute::Prefix,
        LayoutMode::Mixed => {
            if is_paren {
                BlockRoute::Combo
            } else if is_combo_capable {
                BlockRoute::PrefixAndCombo
            } else {
                BlockRoute::Prefix
            }
        }
    }
}
