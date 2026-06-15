//! rmap-config: settings window (Slint GUI). Edits the fields that exist in
//! `data/config.json` (rmap_core::config::AppConfig) and writes them back.
//!
//! Usage:
//!   rmap-config            -> open settings window
//!   rmap-config reload     -> send IPC reload to the running daemon (CLI helper)
//!   rmap-config status     -> (stub)
//!   rmap-config quit       -> (stub)

use anyhow::Result;
use clap::{Parser, Subcommand};
use rmap_core::config::AppConfig;
use rmap_core::ipc::{send_command, send_reload_command, IpcCommand, IpcResponse};
use slint::{Model, VecModel};
use std::path::Path;
use std::rc::Rc;

slint::include_modules!();

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Send IPC reload to the running daemon
    Reload,
    /// (stub) show daemon status
    Status,
    /// (stub) quit the daemon
    Quit,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Reload) => {
            match send_reload_command() {
                Ok(IpcResponse::Ok) => println!("reload sent"),
                Ok(r) => println!("response: {:?}", r),
                Err(e) => eprintln!("IPC error: {e} (is daemon running?)"),
            }
            return Ok(());
        }
        Some(Commands::Status) => {
            println!("status: (IPC status not fully wired in prototype; daemon tray shows state)");
            return Ok(());
        }
        Some(Commands::Quit) => {
            println!("quit: (send IpcCommand::Quit via pipe in full impl)");
            return Ok(());
        }
        None => {}
    }

    // Avoid piling up windows when 設定 is pressed repeatedly from the tray:
    // if a settings window is already open, just bring it to front.
    if focus_existing_window() {
        return Ok(());
    }

    run_settings_window()
}

/// Find an already-open settings window by its title and bring it to the
/// foreground. Returns true if such a window was found (and this process
/// should exit without creating a new one).
#[cfg(windows)]
fn focus_existing_window() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    let title: Vec<u16> = "rmap 設定"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let hwnd = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr()));
        if hwnd.0 == 0 {
            return false;
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
        true
    }
}

#[cfg(not(windows))]
fn focus_existing_window() -> bool {
    false
}

const CONFIG_PATH: &str = "data/config.json";

/// A bare HWND wrapper so the native file dialog can be parented (owned) by our
/// settings window. Without an owner the dialog opens *behind* our topmost
/// window and can't be interacted with.
#[cfg(windows)]
struct HwndParent(std::num::NonZeroIsize);

#[cfg(windows)]
impl raw_window_handle::HasWindowHandle for HwndParent {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let handle = raw_window_handle::Win32WindowHandle::new(self.0);
        // SAFETY: the HWND outlives the short-lived dialog call below.
        Ok(unsafe {
            raw_window_handle::WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::Win32(
                handle,
            ))
        })
    }
}

#[cfg(windows)]
impl raw_window_handle::HasDisplayHandle for HwndParent {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let handle = raw_window_handle::WindowsDisplayHandle::new();
        Ok(unsafe {
            raw_window_handle::DisplayHandle::borrow_raw(raw_window_handle::RawDisplayHandle::Windows(
                handle,
            ))
        })
    }
}

/// Find our settings window's HWND so the file dialog can be parented to it.
#[cfg(windows)]
fn settings_window_parent() -> Option<HwndParent> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    let title: Vec<u16> = "rmap 設定"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) };
    std::num::NonZeroIsize::new(hwnd.0).map(HwndParent)
}

/// Pick a layout file via the native file dialog asynchronously, starting in
/// `data/layouts` if it exists. Async (driven by `slint::spawn_local`) so the
/// dialog never blocks the UI event loop — a synchronous dialog froze the
/// window (blank, no repaint) and fought the event loop. On Windows the dialog
/// is parented to the settings window so it isn't trapped behind our topmost
/// window.
async fn pick_layout_file_async() -> Option<String> {
    let mut dialog = rfd::AsyncFileDialog::new().add_filter("Layout", &["txt"]);
    let start_dir = Path::new("data/layouts");
    if start_dir.exists() {
        dialog = dialog.set_directory(start_dir);
    }
    #[cfg(windows)]
    if let Some(parent) = settings_window_parent() {
        dialog = dialog.set_parent(&parent);
    }
    dialog
        .pick_file()
        .await
        .map(|f| f.path().to_string_lossy().replace('\\', "/"))
}

fn run_settings_window() -> Result<()> {
    let cfg = AppConfig::load(Path::new(CONFIG_PATH)).unwrap_or_else(|_| AppConfig::fallback());

    let window = AppWindow::new()?;

    window.set_default_layout(cfg.default_layout.clone().into());
    window.set_enable_log(cfg.enable_log);
    window.set_activate_only_when_ime_on(cfg.activate_only_when_ime_on);
    window.set_ime_off_layout(cfg.ime_off_layout.clone().into());
    let direct_input_key_index = match cfg.direct_input_key.trim().to_lowercase().as_str() {
        "shift" | "lshift" => 1,
        "muhenkan" => 2,
        "henkan" => 3,
        "capslock" | "caps" => 4,
        _ => 0,
    };
    window.set_direct_input_key_index(direct_input_key_index);
    let sands_mode_index = match cfg.direct_input_mode.trim().to_lowercase().as_str() {
        "raw" => 1,
        "ime_off" => 2,
        _ => 0,
    };
    window.set_sands_mode_index(sands_mode_index);
    window.set_combo_window_ms(if cfg.combo_window_ms > 0 { cfg.combo_window_ms as i32 } else { 40 });
    window.set_hold_mode(cfg.hold_mode);
    window.set_enable_sands_ime_on(cfg.enable_sands_ime_on);
    window.set_enable_sands_ime_off(cfg.enable_sands_ime_off);

    let lower_disable: Vec<String> = cfg
        .disable_keys
        .iter()
        .map(|s| s.trim().to_lowercase())
        .collect();
    window.set_disable_ctrl(lower_disable.iter().any(|s| s == "ctrl" || s == "control"));
    window.set_disable_alt(lower_disable.iter().any(|s| s == "alt" || s == "menu"));
    window.set_disable_win(
        lower_disable
            .iter()
            .any(|s| matches!(s.as_str(), "win" | "meta" | "super" | "cmd")),
    );
    window.set_disable_shift(lower_disable.iter().any(|s| s == "shift"));

    // Keep custom (non ctrl/alt/win/shift) entries so we round-trip them unchanged.
    let custom_disable_keys: Vec<String> = cfg
        .disable_keys
        .iter()
        .filter(|s| {
            !matches!(
                s.trim().to_lowercase().as_str(),
                "ctrl" | "control" | "alt" | "menu" | "win" | "meta" | "super" | "cmd" | "shift"
            )
        })
        .cloned()
        .collect();

    let mut profile_names: Vec<String> = cfg.profiles.keys().cloned().collect();
    profile_names.sort();
    let rows: Vec<ProfileRow> = profile_names
        .iter()
        .map(|name| {
            let p = &cfg.profiles[name];
            ProfileRow {
                name: name.clone().into(),
                layout: p.layout.clone().into(),
                sands: p.toggles.enable_sands,
                gestures: p.toggles.enable_gestures,
                shortcuts: p.toggles.enable_shortcuts,
            }
        })
        .collect();
    let profiles_model = Rc::new(VecModel::from(rows));
    window.set_profiles(slint::ModelRc::from(profiles_model.clone()));

    let names_model = Rc::new(VecModel::from(
        profile_names
            .iter()
            .map(|n| n.clone().into())
            .collect::<Vec<slint::SharedString>>(),
    ));
    window.set_profile_names(slint::ModelRc::from(names_model));
    window.set_current_profile_index(if profile_names.is_empty() { -1 } else { 0 });

    let default_profile_index = profile_names
        .iter()
        .position(|n| n == &cfg.app_map.default_profile)
        .map(|i| i as i32)
        .unwrap_or(0);
    window.set_default_profile_index(default_profile_index);

    window.set_status_text("".into());
    refresh_profile_layout_text(&window, &cfg);
    refresh_daemon_status(&window);

    // 設定 -> デフォルトレイアウトの参照ファイルを選択ダイアログで指定する。
    let window_weak = window.as_weak();
    window.on_browse_default_layout(move || {
        let window_weak = window_weak.clone();
        let _ = slint::spawn_local(async move {
            if let Some(path) = pick_layout_file_async().await {
                if let Some(window) = window_weak.upgrade() {
                    window.set_default_layout(path.into());
                }
            }
        });
    });

    // IMEオフ時のレイアウトファイルを選択ダイアログで指定する。
    let window_weak = window.as_weak();
    window.on_browse_ime_off_layout(move || {
        let window_weak = window_weak.clone();
        let _ = slint::spawn_local(async move {
            if let Some(path) = pick_layout_file_async().await {
                if let Some(window) = window_weak.upgrade() {
                    window.set_ime_off_layout(path.into());
                }
            }
        });
    });

    // 選択中プロファイルのレイアウトファイルを選択ダイアログで指定する。
    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_browse_profile_layout(move || {
        let window = window_weak.unwrap();
        let idx = window.get_current_profile_index();
        if idx < 0 {
            return;
        }
        let idx = idx as usize;
        let model = model.clone();
        let _ = slint::spawn_local(async move {
            if let Some(path) = pick_layout_file_async().await {
                if let Some(mut row) = model.row_data(idx) {
                    row.layout = path.into();
                    model.set_row_data(idx, row);
                }
            }
        });
    });

    // 選択中プロファイルのレイアウトを直接入力で編集する。
    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_profile_layout_edited(move |text| {
        let window = window_weak.unwrap();
        let idx = window.get_current_profile_index();
        if idx < 0 {
            return;
        }
        let idx = idx as usize;
        if let Some(mut row) = model.row_data(idx) {
            row.layout = text;
            model.set_row_data(idx, row);
        }
    });

    // 選択中プロファイルのトグル（SandS / Gestures / Shortcuts）。
    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_toggle_profile_sands(move || {
        let window = window_weak.unwrap();
        let idx = window.get_current_profile_index();
        if idx < 0 {
            return;
        }
        let idx = idx as usize;
        if let Some(mut row) = model.row_data(idx) {
            row.sands = !row.sands;
            model.set_row_data(idx, row);
        }
    });

    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_toggle_profile_gestures(move || {
        let window = window_weak.unwrap();
        let idx = window.get_current_profile_index();
        if idx < 0 {
            return;
        }
        let idx = idx as usize;
        if let Some(mut row) = model.row_data(idx) {
            row.gestures = !row.gestures;
            model.set_row_data(idx, row);
        }
    });

    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_toggle_profile_shortcuts(move || {
        let window = window_weak.unwrap();
        let idx = window.get_current_profile_index();
        if idx < 0 {
            return;
        }
        let idx = idx as usize;
        if let Some(mut row) = model.row_data(idx) {
            row.shortcuts = !row.shortcuts;
            model.set_row_data(idx, row);
        }
    });

    // デーモン操作（再生/停止トグル／再起動／終了）。デーモンが起動していない
    // 場合は status_text にエラーを表示するだけ（NFR-4 fail-fast）。
    let window_weak = window.as_weak();
    window.on_toggle_running(move || {
        let window = window_weak.unwrap();
        report_ipc_result(&window, send_command(&IpcCommand::ToggleRunning), "切り替えました");
        refresh_daemon_status(&window);
    });

    let window_weak = window.as_weak();
    window.on_quit(move || {
        let window = window_weak.unwrap();
        report_ipc_result(&window, send_command(&IpcCommand::Quit), "終了しました");
        refresh_daemon_status(&window);
    });

    let window_weak = window.as_weak();
    window.on_restart(move || {
        let window = window_weak.unwrap();
        report_ipc_result(
            &window,
            send_command(&IpcCommand::Restart),
            "再起動しました",
        );
        refresh_daemon_status(&window);
    });

    let mut base_cfg = cfg;
    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_save(move || {
        let window = window_weak.unwrap();

        base_cfg.default_layout = window.get_default_layout().to_string();
        let idx = window.get_default_profile_index();
        if let Some(name) = window.get_profile_names().iter().nth(idx.max(0) as usize) {
            base_cfg.app_map.default_profile = name.to_string();
        }
        base_cfg.enable_log = window.get_enable_log();
        base_cfg.activate_only_when_ime_on = window.get_activate_only_when_ime_on();
        base_cfg.ime_off_layout = window.get_ime_off_layout().to_string();
        base_cfg.direct_input_key = match window.get_direct_input_key_index() {
            1 => "shift",
            2 => "muhenkan",
            3 => "henkan",
            4 => "capslock",
            _ => "",
        }.to_string();
        base_cfg.direct_input_mode = match window.get_sands_mode_index() {
            1 => "raw",
            2 => "ime_off",
            _ => "off",
        }.to_string();
        base_cfg.combo_window_ms = window.get_combo_window_ms().max(1) as u64;
        base_cfg.hold_mode = window.get_hold_mode();
        base_cfg.enable_sands_ime_on = window.get_enable_sands_ime_on();
        base_cfg.enable_sands_ime_off = window.get_enable_sands_ime_off();

        let mut disable_keys = custom_disable_keys.clone();
        if window.get_disable_ctrl() {
            disable_keys.push("ctrl".to_string());
        }
        if window.get_disable_alt() {
            disable_keys.push("alt".to_string());
        }
        if window.get_disable_win() {
            disable_keys.push("win".to_string());
        }
        if window.get_disable_shift() {
            disable_keys.push("shift".to_string());
        }
        base_cfg.disable_keys = disable_keys;

        for row in model.iter() {
            if let Some(p) = base_cfg.profiles.get_mut(row.name.as_str()) {
                p.layout = row.layout.to_string();
                p.toggles.enable_sands = row.sands;
                p.toggles.enable_gestures = row.gestures;
                p.toggles.enable_shortcuts = row.shortcuts;
            }
        }

        match save_config(&base_cfg) {
            Ok(()) => window.set_status_text("保存しました".into()),
            Err(e) => window.set_status_text(format!("保存に失敗: {e}").into()),
        }
        refresh_profile_layout_text(&window, &base_cfg);
    });

    window.show()?;
    spawn_always_on_top();
    slint::run_event_loop()?;
    window.hide()?;
    Ok(())
}

/// Pin the settings window above all others (Win32: SetWindowPos with
/// HWND_TOPMOST). Runs on a background thread because the native window
/// isn't registered with the OS until the event loop starts pumping
/// messages, so we can't find it by title synchronously after `show()`.
/// No-op on non-Windows targets.
#[cfg(windows)]
fn spawn_always_on_top() {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    std::thread::spawn(|| {
        let title: Vec<u16> = "rmap 設定"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            for _ in 0..40 {
                let hwnd = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr()));
                if hwnd.0 != 0 {
                    let _ = SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    });
}

#[cfg(not(windows))]
fn spawn_always_on_top() {}

/// Reflect the result of an IPC daemon-control command in the status line.
fn report_ipc_result(window: &AppWindow, result: anyhow::Result<IpcResponse>, ok_text: &str) {
    match result {
        Ok(IpcResponse::Ok) => window.set_status_text(ok_text.into()),
        Ok(r) => window.set_status_text(format!("{r:?}").into()),
        Err(e) => window.set_status_text(format!("デーモンに接続できません: {e}").into()),
    }
}

/// Query the daemon's running/suspended state via IPC and update the
/// left-pane status panel. Shows "未起動" if the daemon isn't reachable.
fn refresh_daemon_status(window: &AppWindow) {
    let (text, suspended) = match send_command(&IpcCommand::Status) {
        Ok(IpcResponse::Status { suspended, .. }) => {
            (if suspended { "停止中" } else { "稼働中" }, suspended)
        }
        // Daemon unreachable: show "再生" as the actionable toggle label.
        _ => ("未起動", true),
    };
    window.set_running_status_text(text.into());
    window.set_daemon_suspended(suspended);
}

/// Update the left-pane "current profile / layout" display from `cfg`'s
/// default profile.
fn refresh_profile_layout_text(window: &AppWindow, cfg: &AppConfig) {
    let profile = cfg.app_map.default_profile.clone();
    let layout = cfg
        .profiles
        .get(&profile)
        .map(|p| p.layout.clone())
        .unwrap_or_else(|| cfg.default_layout.clone());
    window.set_current_profile_text(profile.into());
    window.set_current_layout_text(layout.into());
}

fn save_config(cfg: &AppConfig) -> Result<()> {
    let json = serde_json::to_string_pretty(cfg)?;
    if let Some(parent) = Path::new(CONFIG_PATH).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(CONFIG_PATH, json)?;
    Ok(())
}
