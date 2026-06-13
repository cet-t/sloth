//! IPC protocol for daemon <-> config (roadmap 11).
//! Windows: named pipe, same-user ACL (prototype: \\.\pipe\rmap).
//! Messages are length-prefixed JSON for simplicity.

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "windows")]
pub use windows::{start_ipc_server, send_reload_command};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum IpcCommand {
    Reload,
    Status,
    Quit,
    /// FR-8 daemon control hotkeys.
    Stop,
    Resume,
    ToggleRunning,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum IpcResponse {
    Ok,
    Status { version: String, active_app: String, suspended: bool },
    Error(String),
}