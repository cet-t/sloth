//! Input Event. Timestamped, with hold flag for repeat/hold distinction.

use crate::{KeyCode, Modifiers};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    KeyDown,
    KeyUp,
    // Mouse wheel etc later for gestures
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub kind: EventKind,
    pub code: KeyCode,
    pub modifiers: Modifiers,
    pub timestamp: u64, // ms since epoch, for combo window
    /// true if this is continuation of a hold (key already in pressed set)
    pub held: bool,
}

impl Event {
    pub fn new(kind: EventKind, code: KeyCode, modifiers: Modifiers) -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            kind,
            code,
            modifiers,
            timestamp: ts,
            held: false,
        }
    }

    pub fn with_held(mut self, held: bool) -> Self {
        self.held = held;
        self
    }
}
