//! Platform hook abstraction.
//! For v1 Windows-first: the real impl is in windows.rs (cfg).

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "windows")]
pub use windows::{
    install_and_run_windows_hook, is_suspended, reload_layout, set_suspend, toggle_suspend,
};
