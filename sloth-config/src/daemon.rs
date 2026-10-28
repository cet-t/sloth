//! Daemon control: the IPC round trips the settings window's プロセス tab
//! drives, plus the "daemon isn't running at all" auto-start fallback.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use sloth_core::ipc::{send_command, IpcCommand, IpcResponse};

/// Human-readable daemon state plus the `suspended` flag the UI uses to pick
/// between the 再生 / 停止 labels.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DaemonStatus {
    pub text: &'static str,
    pub suspended: bool,
}

impl Default for DaemonStatus {
    fn default() -> Self {
        Self {
            text: "確認中...",
            suspended: true,
        }
    }
}

/// The (possibly slow -- up to ~2s if the daemon isn't running, see
/// `send_command`'s retry loop) IPC round trip. Always run off the UI thread.
pub fn query_status() -> DaemonStatus {
    match send_command(&IpcCommand::Status) {
        Ok(IpcResponse::Status { suspended, .. }) => DaemonStatus {
            text: if suspended { "停止中" } else { "稼働中" },
            suspended,
        },
        _ => DaemonStatus {
            text: "未起動",
            suspended: true,
        },
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DaemonOp {
    ToggleRunning,
    Restart,
    Quit,
}

/// Runs one daemon-control op and returns the status line to show. Blocking;
/// callers run it on a worker thread.
///
/// `was_suspended` is the UI's last known state: when the daemon isn't running
/// at all, IPC fails outright and "再生"/"再起動" both just mean "start it up".
pub fn run_op(op: DaemonOp, was_suspended: bool) -> String {
    match op {
        DaemonOp::ToggleRunning => match send_command(&IpcCommand::ToggleRunning) {
            Err(_) if was_suspended => start_daemon_message(),
            Ok(IpcResponse::Ok) => "切り替えました".to_string(),
            Ok(r) => format!("{r:?}"),
            Err(e) => format!("デーモンに接続できません: {e}"),
        },
        DaemonOp::Restart => {
            let message = match send_command(&IpcCommand::Restart) {
                Err(_) if was_suspended => start_daemon_message(),
                Ok(IpcResponse::Ok) => "再起動しました".to_string(),
                Ok(r) => format!("{r:?}"),
                Err(e) => format!("デーモンに接続できません: {e}"),
            };
            // restart_daemon() exits the old process a short moment after
            // replying, and the new instance needs a beat to install its own
            // IPC server -- querying status immediately would race that.
            std::thread::sleep(Duration::from_millis(400));
            message
        }
        DaemonOp::Quit => {
            let result = send_command(&IpcCommand::Quit);
            // The daemon replies "Ok" immediately but only exits ~200ms later
            // (see sloth-daemon's Quit handler) so it can flush this very
            // response first. Querying status right away would still see it as
            // running and leave the toggle button showing 停止, and the next
            // click would then skip the not-running auto-start path.
            if matches!(result, Ok(IpcResponse::Ok)) {
                std::thread::sleep(Duration::from_millis(400));
            }
            match result {
                Ok(IpcResponse::Ok) => "終了しました".to_string(),
                Ok(r) => format!("{r:?}"),
                Err(e) => format!("デーモンに接続できません: {e}"),
            }
        }
    }
}

fn start_daemon_message() -> String {
    match spawn_daemon() {
        Ok(()) => "デーモンを起動しました".to_string(),
        Err(e) => format!("デーモンの起動に失敗: {e}"),
    }
}

/// Launch the daemon (`sloth.exe`). Mirrors the daemon's own
/// `sloth-config.exe` lookup (sibling exe first, `cargo run` fallback for a
/// dev checkout with no built daemon exe yet) so the pair is symmetric.
#[cfg(windows)]
fn spawn_daemon() -> Result<()> {
    use std::os::windows::process::CommandExt;

    let sibling = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("sloth.exe")));
    if let Some(exe) = &sibling {
        if exe.exists() {
            std::process::Command::new(exe).spawn()?;
            return Ok(());
        }
    }

    // Dev-checkout fallback: only if a workspace Cargo.toml is actually there
    // (never true in a packaged dist/, where the sibling exe above is
    // guaranteed to exist -- see build-release.ps1).
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf());
    let Some(workspace_root) = workspace_root else {
        anyhow::bail!("could not determine workspace root");
    };
    if !workspace_root.join("Cargo.toml").exists() {
        anyhow::bail!("sloth.exe not found next to sloth-config.exe");
    }

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("cargo")
        .args(["run", "--quiet", "-p", "sloth-daemon"])
        .current_dir(&workspace_root)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(not(windows))]
fn spawn_daemon() -> Result<()> {
    anyhow::bail!("not supported on this platform")
}
