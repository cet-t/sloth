//! Key vocabulary for sloth-parser (self-contained, no internal deps).
//!
//! Variant names mirror the rest of rmap so downstream crates can convert with
//! a total `match`. Triggers in config files resolve via [`Key::from_name`].

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Minus,
    Equal,
    LBracket,
    RBracket,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Dot,
    Slash,
    Grave,
    ShiftL,
    ShiftR,
    CtrlL,
    CtrlR,
    AltL,
    AltR,
    MetaL,
    MetaR,
    Space,
    Enter,
    Tab,
    Backspace,
    Escape,
    CapsLock,
    Left,
    Right,
    Up,
    Down,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Muhenkan,
    Henkan,
    KanaKatakana,
    HankakuZenkaku,
    Yen,
    Caret,
    Colon,
    AtSign,
    Unknown(u32),
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Key {
    /// Resolve a trigger/key name as written in config files to a [`Key`].
    ///
    /// Accepts the dvorakj-style alphabetic names plus a symbol supplement for
    /// punctuation keys that the alphabetic table does not cover.
    pub fn from_name(name: &str) -> Option<Key> {
        let lower = name.to_lowercase();
        let k = match lower.as_str() {
            "a" => Key::A,
            "b" => Key::B,
            "c" => Key::C,
            "d" => Key::D,
            "e" => Key::E,
            "f" => Key::F,
            "g" => Key::G,
            "h" => Key::H,
            "i" => Key::I,
            "j" => Key::J,
            "k" => Key::K,
            "l" => Key::L,
            "m" => Key::M,
            "n" => Key::N,
            "o" => Key::O,
            "p" => Key::P,
            "q" => Key::Q,
            "r" => Key::R,
            "s" => Key::S,
            "t" => Key::T,
            "u" => Key::U,
            "v" => Key::V,
            "w" => Key::W,
            "x" => Key::X,
            "y" => Key::Y,
            "z" => Key::Z,
            "0" | "num0" => Key::Num0,
            "1" | "num1" => Key::Num1,
            "2" | "num2" => Key::Num2,
            "3" | "num3" => Key::Num3,
            "4" | "num4" => Key::Num4,
            "5" | "num5" => Key::Num5,
            "6" | "num6" => Key::Num6,
            "7" | "num7" => Key::Num7,
            "8" | "num8" => Key::Num8,
            "9" | "num9" => Key::Num9,
            "space" => Key::Space,
            "enter" | "return" => Key::Enter,
            "tab" => Key::Tab,
            "bs" | "backspace" => Key::Backspace,
            "esc" | "escape" => Key::Escape,
            "capslock" | "caps" => Key::CapsLock,
            "lshift" | "shift" => Key::ShiftL,
            "rshift" => Key::ShiftR,
            "lctrl" | "lcontrol" => Key::CtrlL,
            "rctrl" | "rcontrol" => Key::CtrlR,
            "lalt" => Key::AltL,
            "ralt" => Key::AltR,
            "lwin" | "lmeta" => Key::MetaL,
            "rwin" | "rmeta" => Key::MetaR,
            "muhenkan" => Key::Muhenkan,
            "henkan" => Key::Henkan,
            "kana" | "kanakatakana" => Key::KanaKatakana,
            "hankaku" | "zenkaku" | "hankakuzenkaku" => Key::HankakuZenkaku,
            "yen" => Key::Yen,
            "^" | "caret" => Key::Caret,
            ":" | "colon" => Key::Colon,
            "@" | "at" | "atmark" => Key::AtSign,
            "left" => Key::Left,
            "right" => Key::Right,
            "up" => Key::Up,
            "down" => Key::Down,
            "f1" => Key::F1,
            "f2" => Key::F2,
            "f3" => Key::F3,
            "f4" => Key::F4,
            "f5" => Key::F5,
            "f6" => Key::F6,
            "f7" => Key::F7,
            "f8" => Key::F8,
            "f9" => Key::F9,
            "f10" => Key::F10,
            "f11" => Key::F11,
            "f12" => Key::F12,
            _ => return resolve_symbol(name),
        };
        Some(k)
    }

    /// The canonical trigger name for this key as written in a config file
    /// -- the inverse of [`Key::from_name`]. Used when serializing a
    /// compiled layout back out to TOML (e.g. `sloth convert`). Variants
    /// with more than one accepted spelling (e.g. `shift`/`lshift`) emit
    /// their primary/preferred one; `Unknown` has no textual form and
    /// returns `None`.
    pub fn name(&self) -> Option<&'static str> {
        Some(match self {
            Key::A => "a",
            Key::B => "b",
            Key::C => "c",
            Key::D => "d",
            Key::E => "e",
            Key::F => "f",
            Key::G => "g",
            Key::H => "h",
            Key::I => "i",
            Key::J => "j",
            Key::K => "k",
            Key::L => "l",
            Key::M => "m",
            Key::N => "n",
            Key::O => "o",
            Key::P => "p",
            Key::Q => "q",
            Key::R => "r",
            Key::S => "s",
            Key::T => "t",
            Key::U => "u",
            Key::V => "v",
            Key::W => "w",
            Key::X => "x",
            Key::Y => "y",
            Key::Z => "z",
            Key::Num0 => "0",
            Key::Num1 => "1",
            Key::Num2 => "2",
            Key::Num3 => "3",
            Key::Num4 => "4",
            Key::Num5 => "5",
            Key::Num6 => "6",
            Key::Num7 => "7",
            Key::Num8 => "8",
            Key::Num9 => "9",
            Key::Minus => "-",
            Key::Equal => "=",
            Key::LBracket => "[",
            Key::RBracket => "]",
            Key::Backslash => "\\",
            Key::Semicolon => ";",
            Key::Quote => "'",
            Key::Comma => ",",
            Key::Dot => ".",
            Key::Slash => "/",
            Key::Grave => "`",
            Key::Space => "space",
            Key::Enter => "enter",
            Key::Tab => "tab",
            Key::Backspace => "bs",
            Key::Escape => "esc",
            Key::CapsLock => "capslock",
            Key::ShiftL => "shift",
            Key::ShiftR => "rshift",
            Key::CtrlL => "lctrl",
            Key::CtrlR => "rctrl",
            Key::AltL => "lalt",
            Key::AltR => "ralt",
            Key::MetaL => "lwin",
            Key::MetaR => "rwin",
            Key::Muhenkan => "muhenkan",
            Key::Henkan => "henkan",
            Key::KanaKatakana => "kana",
            Key::HankakuZenkaku => "hankaku",
            Key::Yen => "yen",
            Key::Caret => "^",
            Key::Colon => ":",
            Key::AtSign => "@",
            Key::Left => "left",
            Key::Right => "right",
            Key::Up => "up",
            Key::Down => "down",
            Key::F1 => "f1",
            Key::F2 => "f2",
            Key::F3 => "f3",
            Key::F4 => "f4",
            Key::F5 => "f5",
            Key::F6 => "f6",
            Key::F7 => "f7",
            Key::F8 => "f8",
            Key::F9 => "f9",
            Key::F10 => "f10",
            Key::F11 => "f11",
            Key::F12 => "f12",
            Key::Unknown(_) => return None,
        })
    }
}

/// Symbol-key supplement (punctuation not covered by the alphabetic table).
fn resolve_symbol(name: &str) -> Option<Key> {
    Some(match name {
        "-" => Key::Minus,
        "=" => Key::Equal,
        "`" => Key::Grave,
        "\\" => Key::Backslash,
        "[" => Key::LBracket,
        "]" => Key::RBracket,
        ";" => Key::Semicolon,
        "'" => Key::Quote,
        "," => Key::Comma,
        "." => Key::Dot,
        "/" => Key::Slash,
        "+" => Key::Equal,
        _ => return None,
    })
}

/// Rank used to put any chord/key set into a stable canonical order so that
/// combo lookups are reproducible regardless of press/parse order.
pub fn canon_key_order(k: Key) -> u16 {
    match k {
        Key::Space => 1,
        Key::ShiftL => 2,
        Key::ShiftR => 3,
        Key::CtrlL => 4,
        Key::CtrlR => 5,
        Key::AltL => 6,
        Key::AltR => 7,
        Key::MetaL => 8,
        Key::MetaR => 9,
        Key::Muhenkan => 10,
        Key::Henkan => 11,
        Key::KanaKatakana => 12,
        Key::HankakuZenkaku => 13,
        Key::Yen => 14,
        Key::Caret => 15,
        Key::Colon => 16,
        Key::AtSign => 17,
        Key::Unknown(_) => 200,
        _ => 100,
    }
}

/// Canonical sort: rank first, then Debug name as a total tie-break.
pub fn canon_sort(keys: &mut [Key]) {
    keys.sort_by_key(|k| (canon_key_order(*k), format!("{:?}", k)));
}

/// A canonically-ordered set of keys used as a combo (同時押し) key.
///
/// Always canonicalized on construction so lookups are order-independent.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyChord(Vec<Key>);

impl KeyChord {
    pub fn new(keys: impl Into<Vec<Key>>) -> Self {
        let mut v = keys.into();
        canon_sort(&mut v);
        Self(v)
    }

    pub fn as_slice(&self) -> &[Key] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<Key> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
