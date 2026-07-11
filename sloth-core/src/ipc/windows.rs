//! Windows named pipe IPC.
//! Length-prefixed (u32 LE) JSON messages over `\\.\pipe\sloth`, one
//! request/response per connection. The server runs on its own thread and
//! accepts connections sequentially (the daemon only expects occasional
//! control messages from sloth-config, so this is sufficient).

use super::{IpcCommand, IpcResponse};
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FlushFileBuffers, ReadFile, WriteFile, FILE_FLAGS_AND_ATTRIBUTES,
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_MODE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
    PIPE_TYPE_MESSAGE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

const PIPE_NAME: &str = r"\\.\pipe\sloth";
const BUF_SIZE: u32 = 4096;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Start the IPC server on a background thread. `on_cmd` is called once per
/// accepted connection with the decoded command and must return a response.
pub fn start_ipc_server<F>(mut on_cmd: F)
where
    F: FnMut(IpcCommand) -> IpcResponse + Send + 'static,
{
    std::thread::spawn(move || loop {
        match create_server_pipe() {
            Ok(pipe) => {
                if !wait_for_client(pipe) {
                    unsafe {
                        let _ = CloseHandle(pipe);
                    }
                    continue;
                }
                if let Some(cmd) = read_message(pipe) {
                    let resp = on_cmd(cmd);
                    let _ = write_message(pipe, &resp);
                    // Block until the client has read the response; otherwise
                    // DisconnectNamedPipe below can discard unread data.
                    unsafe {
                        let _ = FlushFileBuffers(pipe);
                    }
                }
                unsafe {
                    let _ = DisconnectNamedPipe(pipe);
                    let _ = CloseHandle(pipe);
                }
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    });
}

fn create_server_pipe() -> anyhow::Result<HANDLE> {
    let name = to_wide(PIPE_NAME);
    unsafe {
        let handle = CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            BUF_SIZE,
            BUF_SIZE,
            0,
            None,
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(anyhow::anyhow!(
                "CreateNamedPipeW failed: {:?}",
                windows::core::Error::from_win32()
            ));
        }
        Ok(handle)
    }
}

/// Block until a client connects (or treat "already connected" as success).
fn wait_for_client(pipe: HANDLE) -> bool {
    unsafe {
        match ConnectNamedPipe(pipe, None) {
            Ok(()) => true,
            Err(e) => e.code() == windows::core::HRESULT::from_win32(ERROR_PIPE_CONNECTED.0),
        }
    }
}

fn read_message(pipe: HANDLE) -> Option<IpcCommand> {
    let mut len_buf = [0u8; 4];
    unsafe {
        ReadFile(pipe, Some(&mut len_buf), None, None).ok()?;
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > 1_000_000 {
        return None;
    }
    let mut buf = vec![0u8; len];
    unsafe {
        ReadFile(pipe, Some(&mut buf), None, None).ok()?;
    }
    serde_json::from_slice(&buf).ok()
}

fn write_message(pipe: HANDLE, resp: &IpcResponse) -> anyhow::Result<()> {
    let body = serde_json::to_vec(resp)?;
    let len = (body.len() as u32).to_le_bytes();
    unsafe {
        WriteFile(pipe, Some(&len), None, None)?;
        WriteFile(pipe, Some(&body), None, None)?;
    }
    Ok(())
}

/// Send a command to the running daemon and wait for its response.
pub fn send_command(cmd: &IpcCommand) -> anyhow::Result<IpcResponse> {
    let name = to_wide(PIPE_NAME);
    unsafe {
        // The server creates its named pipe instance on a background thread,
        // so a client that races daemon startup (or this test's setup) may
        // briefly see ERROR_FILE_NOT_FOUND/ERROR_PIPE_BUSY before the pipe
        // exists or while a previous instance is being recycled. Retry for
        // up to ~2s, which is generous for both cases.
        let mut pipe = INVALID_HANDLE_VALUE;
        for attempt in 0..40 {
            match CreateFileW(
                PCWSTR(name.as_ptr()),
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            ) {
                Ok(h) => {
                    pipe = h;
                    break;
                }
                Err(e)
                    if attempt < 39
                        && (e.code()
                            == windows::core::HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0)
                            || e.code()
                                == windows::core::HRESULT::from_win32(ERROR_PIPE_BUSY.0)) =>
                {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(anyhow::anyhow!("CreateFileW: {e}")),
            }
        }

        let body = serde_json::to_vec(cmd)?;
        let len = (body.len() as u32).to_le_bytes();
        let write_result = (|| -> anyhow::Result<IpcResponse> {
            WriteFile(pipe, Some(&len), None, None)
                .map_err(|e| anyhow::anyhow!("write len: {e}"))?;
            WriteFile(pipe, Some(&body), None, None)
                .map_err(|e| anyhow::anyhow!("write body: {e}"))?;

            let mut resp_len_buf = [0u8; 4];
            ReadFile(pipe, Some(&mut resp_len_buf), None, None)
                .map_err(|e| anyhow::anyhow!("read resp len: {e}"))?;
            let resp_len = u32::from_le_bytes(resp_len_buf) as usize;
            let mut resp_buf = vec![0u8; resp_len];
            ReadFile(pipe, Some(&mut resp_buf), None, None)
                .map_err(|e| anyhow::anyhow!("read resp body: {e}"))?;
            Ok(serde_json::from_slice(&resp_buf)?)
        })();

        let _ = CloseHandle(pipe);
        write_result
    }
}

pub fn send_reload_command() -> anyhow::Result<IpcResponse> {
    send_command(&IpcCommand::Reload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// Round-trip every IpcCommand variant through a real named pipe server.
    #[test]
    fn server_roundtrips_all_commands() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        start_ipc_server(move |cmd| {
            tx.send(cmd.clone()).ok();
            match cmd {
                IpcCommand::Status => IpcResponse::Status {
                    version: "test".into(),
                    active_app: String::new(),
                    suspended: true,
                },
                _ => IpcResponse::Ok,
            }
        });
        // Give the server thread a moment to start listening.
        std::thread::sleep(Duration::from_millis(100));

        for cmd in [
            IpcCommand::Reload,
            IpcCommand::Stop,
            IpcCommand::Resume,
            IpcCommand::ToggleRunning,
            IpcCommand::Restart,
            IpcCommand::Quit,
        ] {
            let resp = send_command(&cmd).expect("send_command");
            assert!(matches!(resp, IpcResponse::Ok));
            let got = rx
                .recv_timeout(Duration::from_secs(2))
                .expect("server received cmd");
            assert!(matches!(got, ref g if format!("{g:?}") == format!("{cmd:?}")));
        }

        let resp = send_command(&IpcCommand::Status).expect("send_command status");
        match resp {
            IpcResponse::Status { suspended, .. } => assert!(suspended),
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
