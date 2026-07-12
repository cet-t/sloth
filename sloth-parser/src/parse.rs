//! Parsing entry points and compilation of a [`Config`] into a
//! [`CompiledLayout`].

use std::collections::BTreeMap;

use crate::key::{Key, KeyChord};
use crate::model::{
    ComboValue, CompileError, CompiledLayer, CompiledLayout, Config, KeyboardLayout, Layer,
    OutputSeq, OutputToken, ParseError, SpecialKey,
};

/// Parse a TOML layout string into a [`Config`].
pub fn parse_toml(s: &str) -> Result<Config, ParseError> {
    toml::from_str(s).map_err(ParseError::Toml)
}

/// Parse a JSON layout string into a [`Config`].
pub fn parse_json(s: &str) -> Result<Config, ParseError> {
    serde_json::from_str(s).map_err(ParseError::Json)
}

/// Convenience: parse TOML and compile in one step.
pub fn compile_toml(s: &str) -> Result<CompiledLayout, crate::model::Error> {
    Ok(parse_toml(s)?.compile()?)
}

/// Convenience: parse JSON and compile in one step.
pub fn compile_json(s: &str) -> Result<CompiledLayout, crate::model::Error> {
    Ok(parse_json(s)?.compile()?)
}

impl Config {
    /// Compile the schema into a [`CompiledLayout`].
    pub fn compile(self) -> Result<CompiledLayout, CompileError> {
        let keyboard = match self.meta.keyboard.as_deref() {
            Some("jis") => KeyboardLayout::Jis,
            _ => KeyboardLayout::Us,
        };

        // Pass 1: grid-based layers (positions → physical keys).
        let mut layers: BTreeMap<String, CompiledLayer> = BTreeMap::new();
        for (name, layer) in &self.layers {
            layers.insert(name.clone(), compile_grid(layer, keyboard));
        }

        // Pass 2: inherit + override (needs base layers already present).
        for (name, layer) in &self.layers {
            if layer.inherit.is_none() && layer.override_.is_none() {
                continue;
            }
            let base_name = layer.inherit.as_deref().unwrap_or("base");
            let base = layers
                .get(base_name)
                .ok_or_else(|| CompileError::MissingLayer(base_name.to_string()))?;
            let mut keys = base.keys.clone();
            if let Some(ov) = &layer.override_ {
                // Named trigger → output overrides.
                for (kname, out) in &ov.map {
                    let k = Key::from_name(kname)
                        .ok_or_else(|| CompileError::UnknownKey(kname.clone()))?;
                    let seq = cell_to_seq(out);
                    if !seq.is_empty() {
                        keys.insert(k, seq);
                    }
                }
                // Positional grid overrides (same shape as a layer `grid`).
                if let Some(grid) = &ov.grid {
                    for (r, row) in grid.iter().enumerate() {
                        let phys = physical_row(r, keyboard);
                        let n = phys.len().min(row.len());
                        for i in 0..n {
                            let seq = cell_to_seq(&row[i]);
                            if !seq.is_empty() {
                                keys.insert(phys[i], seq);
                            }
                        }
                    }
                }
            }
            layers.insert(name.clone(), CompiledLayer { keys });
        }

        // 同時押し (combo): key set → output (order-independent).
        let mut combos = BTreeMap::new();
        for (spec, value) in &self.combos {
            let triggers = split_keys(spec)?;
            match value {
                ComboValue::Single(out) => {
                    let seq = cell_to_seq(out);
                    if !seq.is_empty() {
                        combos.insert(KeyChord::new(triggers), seq);
                    }
                }
                ComboValue::Grid(g) => {
                    for (r, row) in g.grid.iter().enumerate() {
                        let phys = physical_row(r, keyboard);
                        let n = phys.len().min(row.len());
                        for c in 0..n {
                            let cell = &row[c];
                            if cell.is_empty() || triggers.contains(&phys[c]) {
                                continue;
                            }
                            let mut chord = triggers.clone();
                            chord.push(phys[c]);
                            combos.insert(KeyChord::new(chord), cell_to_seq(cell));
                        }
                    }
                }
            }
        }

        // 順押し (sequence): ordered key list → output.
        let mut sequences = BTreeMap::new();
        for (spec, out) in &self.sequences {
            let keys = split_keys(spec)?;
            let seq = cell_to_seq(out);
            if keys.is_empty() || seq.is_empty() {
                continue;
            }
            sequences.insert(keys, seq);
        }

        // Derived matcher hints: the hand-written TOML/JSON schema has no
        // syntax (yet) for sustained layers, so layer_maps/layer_taps/
        // layer_triggers/sustained_triggers stay empty here -- only a
        // DvorakJ-sourced layout (via dvorakj-parser's `sloth` feature)
        // populates those. combo_keys/prefix_triggers, on the other hand,
        // are fully derivable from combos/sequences, so compute them for
        // every source format instead of leaving it to each caller.
        let combo_keys: std::collections::BTreeSet<Key> = combos
            .keys()
            .flat_map(|chord| chord.as_slice().iter().copied())
            .collect();
        let prefix_triggers: std::collections::BTreeSet<Key> = sequences
            .keys()
            .filter_map(|seq| seq.first().copied())
            .collect();

        Ok(CompiledLayout {
            name: self.meta.name,
            mode: crate::model::LayoutMode::default(),
            input_mode: crate::model::InputMode::default(),
            keyboard,
            layers: layers.into_iter().collect(),
            layer_maps: BTreeMap::new(),
            layer_taps: BTreeMap::new(),
            layer_triggers: std::collections::BTreeSet::new(),
            combos,
            combo_keys,
            sustained_triggers: std::collections::BTreeSet::new(),
            sequences,
            prefix_triggers,
            states: self.states,
        })
    }
}

fn split_keys(spec: &str) -> Result<Vec<Key>, CompileError> {
    spec.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| Key::from_name(s).ok_or_else(|| CompileError::UnknownKey(s.to_string())))
        .collect()
}

/// Parse a cell into an output sequence. Plain text becomes a single
/// `Text` token; `{enter}`, `{BS}`, `{left}`, ... become `Named` special-key
/// tokens (so layouts can emit key actions, not just literal text).
fn cell_to_seq(cell: &str) -> OutputSeq {
    if cell.is_empty() {
        return vec![];
    }
    let chars: Vec<char> = cell.chars().collect();
    let mut out: OutputSeq = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            if !text.is_empty() {
                out.push(OutputToken::Text(std::mem::take(&mut text)));
            }
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '}' {
                j += 1;
            }
            if j < chars.len() {
                let tok: String = chars[i + 1..j].iter().collect();
                match named_token(&tok) {
                    Some(sp) => out.push(OutputToken::Named(sp)),
                    None => {
                        text.push('{');
                        text.push_str(&tok);
                        text.push('}');
                    }
                }
                i = j + 1;
            } else {
                text.push(chars[i]);
                i += 1;
            }
        } else {
            text.push(chars[i]);
            i += 1;
        }
    }
    if !text.is_empty() {
        out.push(OutputToken::Text(text));
    }
    out
}

/// Resolve a `{...}` token name to a [`SpecialKey`].
fn named_token(tok: &str) -> Option<SpecialKey> {
    Some(match tok.to_ascii_lowercase().as_str() {
        "enter" | "return" => SpecialKey::Enter,
        "bs" | "backspace" => SpecialKey::Backspace,
        "left" => SpecialKey::Left,
        "right" => SpecialKey::Right,
        "up" => SpecialKey::Up,
        "down" => SpecialKey::Down,
        "tab" => SpecialKey::Tab,
        "esc" | "escape" => SpecialKey::Escape,
        _ => return None,
    })
}

fn compile_grid(layer: &Layer, keyboard: KeyboardLayout) -> CompiledLayer {
    let mut keys = BTreeMap::new();
    if let Some(grid) = &layer.grid {
        for (r, row) in grid.iter().enumerate() {
            let phys = physical_row(r, keyboard);
            let n = phys.len().min(row.len());
            for i in 0..n {
                let seq = cell_to_seq(&row[i]);
                if !seq.is_empty() {
                    keys.insert(phys[i], seq);
                }
            }
        }
    }
    CompiledLayer { keys }
}

/// Physical-key template matching the `config-idea` grid ordering
/// (backtick on the left of the number row, etc.).
fn physical_row(row: usize, keyboard: KeyboardLayout) -> &'static [Key] {
    match (keyboard, row) {
        (KeyboardLayout::Us, 0) => &[
            Key::Grave,
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
            Key::Num0,
            Key::Minus,
            Key::Equal,
        ],
        (KeyboardLayout::Us, 1) => &[
            Key::Q,
            Key::W,
            Key::E,
            Key::R,
            Key::T,
            Key::Y,
            Key::U,
            Key::I,
            Key::O,
            Key::P,
            Key::LBracket,
            Key::RBracket,
            Key::Backslash,
        ],
        (KeyboardLayout::Us, 2) => &[
            Key::A,
            Key::S,
            Key::D,
            Key::F,
            Key::G,
            Key::H,
            Key::J,
            Key::K,
            Key::L,
            Key::Semicolon,
            Key::Quote,
        ],
        (KeyboardLayout::Us, 3) => &[
            Key::Z,
            Key::X,
            Key::C,
            Key::V,
            Key::B,
            Key::N,
            Key::M,
            Key::Comma,
            Key::Dot,
            Key::Slash,
        ],
        (KeyboardLayout::Jis, 0) => &[
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
            Key::Num0,
            Key::Minus,
            Key::Caret,
            Key::Yen,
        ],
        (KeyboardLayout::Jis, 1) => &[
            Key::Q,
            Key::W,
            Key::E,
            Key::R,
            Key::T,
            Key::Y,
            Key::U,
            Key::I,
            Key::O,
            Key::P,
            Key::AtSign,
            Key::LBracket,
        ],
        (KeyboardLayout::Jis, 2) => &[
            Key::A,
            Key::S,
            Key::D,
            Key::F,
            Key::G,
            Key::H,
            Key::J,
            Key::K,
            Key::L,
            Key::Semicolon,
            Key::Colon,
            Key::RBracket,
        ],
        (KeyboardLayout::Jis, 3) => &[
            Key::Z,
            Key::X,
            Key::C,
            Key::V,
            Key::B,
            Key::N,
            Key::M,
            Key::Comma,
            Key::Dot,
            Key::Slash,
            Key::Backslash,
        ],
        (_, _) => &[],
    }
}
