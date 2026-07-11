//! Canonical KeyCode enum. NOT platform raw codes.
//! Bidirectional maps live in platform hook modules.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    // Letters
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

    // Numbers
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

    // Symbols / punctuation (US QWERTY positions for reference)
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

    // Modifiers (distinguish L/R where possible)
    ShiftL,
    ShiftR,
    CtrlL,
    CtrlR,
    AltL,
    AltR,
    MetaL,
    MetaR, // Win / Cmd / Super

    // Special
    Space,
    Enter,
    Tab,
    Backspace,
    Escape,
    CapsLock,

    // Arrows
    Left,
    Right,
    Up,
    Down,

    // Function
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

    // Japan-specific (for DvorakJ layouts)
    Muhenkan,
    Henkan,
    KanaKatakana,
    HankakuZenkaku,

    // Extra symbols for full JP grids
    Yen,
    Caret,
    Colon,
    AtSign,

    // Others as needed (add from DvorakJ corpus)
    // For v1 start with common + Japan keys above.
    // Use Unknown for unmapped during dev; fail fast in prod loader.
    Unknown(u32), // raw platform hint for debugging
}

/// Physical keyboard layout, as reported by the OS input locale.
/// Drives which physical row table the DvorakJ grid is compiled against
/// and how raw VK codes map to [`KeyCode`] on Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardLayout {
    /// JIS 109-key (has dedicated @, ^, ¥, : and \ろ keys).
    Jis,
    /// US/ANSI 104-key. Fallback when the OS layout can't be determined
    /// or isn't Japanese.
    #[default]
    Us,
}

impl KeyCode {
    /// For DvorakJ name table in loader.
    pub fn from_dvorakj_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "a" => Some(Self::A),
            "b" => Some(Self::B),
            "c" => Some(Self::C),
            "d" => Some(Self::D),
            "e" => Some(Self::E),
            "f" => Some(Self::F),
            "g" => Some(Self::G),
            "h" => Some(Self::H),
            "i" => Some(Self::I),
            "j" => Some(Self::J),
            "k" => Some(Self::K),
            "l" => Some(Self::L),
            "m" => Some(Self::M),
            "n" => Some(Self::N),
            "o" => Some(Self::O),
            "p" => Some(Self::P),
            "q" => Some(Self::Q),
            "r" => Some(Self::R),
            "s" => Some(Self::S),
            "t" => Some(Self::T),
            "u" => Some(Self::U),
            "v" => Some(Self::V),
            "w" => Some(Self::W),
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            "z" => Some(Self::Z),
            "0" | "num0" => Some(Self::Num0),
            "1" | "num1" => Some(Self::Num1),
            "2" | "num2" => Some(Self::Num2),
            "3" | "num3" => Some(Self::Num3),
            "4" | "num4" => Some(Self::Num4),
            "5" | "num5" => Some(Self::Num5),
            "6" | "num6" => Some(Self::Num6),
            "7" | "num7" => Some(Self::Num7),
            "8" | "num8" => Some(Self::Num8),
            "9" | "num9" => Some(Self::Num9),
            "space" => Some(Self::Space),
            "enter" | "return" => Some(Self::Enter),
            "tab" => Some(Self::Tab),
            "bs" | "backspace" => Some(Self::Backspace),
            "esc" | "escape" => Some(Self::Escape),
            "capslock" | "caps" => Some(Self::CapsLock),
            "lshift" | "shift" => Some(Self::ShiftL), // DvorakJ often uses -shift for both, map L
            "rshift" => Some(Self::ShiftR),
            "lctrl" | "lcontrol" => Some(Self::CtrlL),
            "rctrl" | "rcontrol" => Some(Self::CtrlR),
            "lalt" => Some(Self::AltL),
            "ralt" => Some(Self::AltR),
            "lwin" | "lmeta" => Some(Self::MetaL),
            "rwin" | "rmeta" => Some(Self::MetaR),
            "muhenkan" => Some(Self::Muhenkan),
            "henkan" => Some(Self::Henkan),
            "kana" | "kanakatakana" => Some(Self::KanaKatakana),
            "hankaku" | "zenkaku" | "hankakuzenkaku" => Some(Self::HankakuZenkaku),
            "yen" => Some(Self::Yen),
            "^" | "caret" => Some(Self::Caret),
            ":" | "colon" => Some(Self::Colon),
            "@" | "at" | "atmark" => Some(Self::AtSign),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            _ => None,
        }
    }
}
