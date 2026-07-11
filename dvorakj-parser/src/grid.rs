//! Grid parsing: `|`-delimited rows → physical-key output map.

use crate::cell::compile_cell;
use crate::model::{Key, KeyboardLayout, OutputSeq};
use std::collections::BTreeMap;

pub(crate) fn parse_grid(
    body: &[String],
    row_offset: usize,
    keyboard: KeyboardLayout,
) -> BTreeMap<Key, OutputSeq> {
    let mut out = BTreeMap::new();
    for (r, line) in body.iter().enumerate() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let phys = physical_row(r + row_offset, keyboard);
        if phys.is_empty() {
            continue;
        }
        let n = std::cmp::min(cells.len(), phys.len());
        for i in 0..n {
            let cell = cells[i];
            if cell.is_empty() || cell == "@@@" {
                continue;
            }
            let seq = compile_cell(cell, keyboard);
            if !seq.is_empty() {
                out.insert(phys[i], seq);
            }
        }
    }
    out
}

fn physical_row(row: usize, keyboard: KeyboardLayout) -> &'static [Key] {
    match (keyboard, row) {
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

        (KeyboardLayout::Us, 0) => &[
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
            Key::Grave,
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

        (_, _) => &[],
    }
}
