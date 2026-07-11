//! Modifiers bitflags. Shared between Event and Output.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct Modifiers: u16 {
        const SHIFT = 0b0000_0000_0000_0001;
        const CTRL  = 0b0000_0000_0000_0010;
        const ALT   = 0b0000_0000_0000_0100;
        const META  = 0b0000_0000_0000_1000;

        // L/R distinguished where platform reports
        const SHIFT_L = 0b0000_0000_0001_0000;
        const SHIFT_R = 0b0000_0000_0010_0000;
        const CTRL_L  = 0b0000_0000_0100_0000;
        const CTRL_R  = 0b0000_0000_1000_0000;
        const ALT_L   = 0b0000_0001_0000_0000;
        const ALT_R   = 0b0000_0010_0000_0000;
        const META_L  = 0b0000_0100_0000_0000;
        const META_R  = 0b0000_1000_0000_0000;
    }
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
