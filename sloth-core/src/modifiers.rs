//! Modifiers bitflags. Shared between Event and Output.

use bitflags2::flags;
use serde::{Deserialize, Serialize};

#[flags]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifiers {
    SHIFT = 0b0000_0000_0000_0001,
    CTRL = 0b0000_0000_0000_0010,
    ALT = 0b0000_0000_0000_0100,
    META = 0b0000_0000_0000_1000,

    // L/R distinguished where platform reports
    SHIFT_L = 0b0000_0000_0001_0000,
    SHIFT_R = 0b0000_0000_0010_0000,
    CTRL_L = 0b0000_0000_0100_0000,
    CTRL_R = 0b0000_0000_1000_0000,
    ALT_L = 0b0000_0001_0000_0000,
    ALT_R = 0b0000_0010_0000_0000,
    META_L = 0b0000_0100_0000_0000,
    META_R = 0b0000_1000_0000_0000,
}

impl Modifiers {
    pub fn from_shift_l_r(shift_l: bool, shift_r: bool) -> Self {
        let mut m = Modifiers::empty();
        if shift_l {
            m |= Modifiers::SHIFT_L | Modifiers::SHIFT;
        }
        if shift_r {
            m |= Modifiers::SHIFT_R | Modifiers::SHIFT;
        }
        m
    }
}
