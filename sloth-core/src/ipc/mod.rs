//! IPC protocol for daemon <-> config (roadmap 11).
//! Windows: named pipe, same-user ACL (prototype: \\.\pipe\sloth).
//! Messages are length-prefixed JSON for simplicity.

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "windows")]
pub use windows::{send_command, send_reload_command, start_ipc_server};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum IpcCommand {
    Reload,
    Status,
    Quit,
    /// FR-8 daemon control hotkeys.
    Stop,
    Resume,
    ToggleRunning,
    /// Re-exec the daemon (same as tray 再起動).
    Restart,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum IpcResponse {
    Ok,
    Status {
        version: String,
        active_app: String,
        suspended: bool,
    },
    Error(String),
}
