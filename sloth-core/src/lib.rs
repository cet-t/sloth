//! sloth-core: shared types, layout loading, matching logic for the key remapper.
//! Platform-neutral. Hooks live in platform crates/binaries.

pub mod config;
pub mod event;
pub mod hook;
pub mod ipc;
pub mod keycode;
pub mod layout;
pub mod loader;
pub mod log;
pub mod matcher;
pub mod modifiers;
pub mod output;
pub mod profile;

// Re-exports for convenience
pub use event::{Event, EventKind};
pub use keycode::{KeyCode, KeyboardLayout};
pub use layout::{Layout, LayoutMode};
pub use loader::LayoutLoader;
pub use matcher::{InputMatcher, MatchAction};
pub use modifiers::Modifiers;
pub use output::{InputMode, OutputSeq, OutputToken, SpecialKey};
pub use profile::Profile;
