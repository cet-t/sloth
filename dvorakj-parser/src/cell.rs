//! Cell compilation: text/token → OutputToken sequence.

use crate::model::{Key, KeyboardLayout, Modifiers, OutputSeq, OutputToken, SpecialKey};

pub(crate) fn compile_cell(cell: &str, keyboard: KeyboardLayout) -> OutputSeq {
    let c = cell.trim();
    if c.is_empty() || c == "@@@" {
        return vec![];
    }
    let mut seq = vec![];
    let mut chars = c.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut inner = String::new();
            let mut closed = false;
            for nc in chars.by_ref() {
                if nc == '}' {
                    closed = true;
                    break;
                }
                inner.push(nc);
            }
            if closed {
                seq.push(brace_token(&inner, keyboard));
            } else {
                seq.push(key_or_text('{', keyboard));
                for ic in inner.chars() {
                    seq.push(key_or_text(ic, keyboard));
                }
            }
        } else {
            seq.push(key_or_text(ch, keyboard));
        }
    }
    seq
}

fn brace_token(inner: &str, keyboard: KeyboardLayout) -> OutputToken {
    let s = inner.to_lowercase();
    match s.as_str() {
        "bs" | "backspace" => OutputToken::Named(SpecialKey::Backspace),
        "enter" | "return" => OutputToken::Named(SpecialKey::Enter),
        "tab" => OutputToken::Named(SpecialKey::Tab),
        "esc" | "escape" => OutputToken::Named(SpecialKey::Escape),
        "left" => OutputToken::Named(SpecialKey::Left),
        "right" => OutputToken::Named(SpecialKey::Right),
        "up" => OutputToken::Named(SpecialKey::Up),
        "down" => OutputToken::Named(SpecialKey::Down),
        "space" => OutputToken::Key {
            code: Key::Space,
            mods: Modifiers::empty(),
        },
        "pipe" | "bar" => OutputToken::Text("|".to_string()),
        _ if s.len() == 1 => key_or_text(s.chars().next().unwrap(), keyboard),
        _ => OutputToken::Text(format!("{{{}}}", inner)),
    }
}

pub(crate) fn key_or_text(ch: char, keyboard: KeyboardLayout) -> OutputToken {
    if keyboard == KeyboardLayout::Us && !ch.is_ascii_alphanumeric() {
        return OutputToken::Text(ch.to_string());
    }
    let code = ascii_to_key(ch);
    if matches!(code, Key::Unknown(_)) {
        OutputToken::Text(ch.to_string())
    } else {
        let mods = if ch.is_ascii_uppercase() {
            Modifiers::SHIFT
        } else {
            Modifiers::empty()
        };
        OutputToken::Key { code, mods }
    }
}

pub(crate) fn ascii_to_key(c: char) -> Key {
    match c.to_ascii_lowercase() {
        'a' => Key::A,
        'b' => Key::B,
        'c' => Key::C,
        'd' => Key::D,
        'e' => Key::E,
        'f' => Key::F,
        'g' => Key::G,
        'h' => Key::H,
        'i' => Key::I,
        'j' => Key::J,
        'k' => Key::K,
        'l' => Key::L,
        'm' => Key::M,
        'n' => Key::N,
        'o' => Key::O,
        'p' => Key::P,
        'q' => Key::Q,
        'r' => Key::R,
        's' => Key::S,
        't' => Key::T,
        'u' => Key::U,
        'v' => Key::V,
        'w' => Key::W,
        'x' => Key::X,
        'y' => Key::Y,
        'z' => Key::Z,
        '0' => Key::Num0,
        '1' => Key::Num1,
        '2' => Key::Num2,
        '3' => Key::Num3,
        '4' => Key::Num4,
        '5' => Key::Num5,
        '6' => Key::Num6,
        '7' => Key::Num7,
        '8' => Key::Num8,
        '9' => Key::Num9,
        '-' => Key::Minus,
        '=' => Key::Equal,
        '[' => Key::LBracket,
        ']' => Key::RBracket,
        '\\' => Key::Backslash,
        ';' => Key::Semicolon,
        '\'' => Key::Quote,
        ',' => Key::Comma,
        '.' => Key::Dot,
        '/' => Key::Slash,
        '`' => Key::Grave,
        ' ' => Key::Space,
        '\n' => Key::Enter,
        '\t' => Key::Tab,
        _ => Key::Unknown(c as u32),
    }
}
