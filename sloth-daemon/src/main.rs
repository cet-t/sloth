#![windows_subsystem = "windows"]
//! sloth-daemon: resident remapper. Owns hooks, applies layouts, IPC server, tray.

use anyhow::Result;
use notify::event::EventKind;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use sloth_core::config::AppConfig;
use sloth_core::hook::{
    install_and_run_windows_hook, is_suspended, reload_layout, set_suspend, toggle_suspend,
};
use sloth_core::ipc::start_ipc_server;
use sloth_core::log;
use std::path::Path;
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

fn main() -> Result<()> {
    sloth_core::loader::register_default_loader(Box::new(
        sloth_dvorakj_adapter::RmapDvorakJLayoutLoader::new(),
    ));

    // Load config early so we know whether file logging is enabled (it must
    // be init'd before any log::log() call; install_and_run_windows_hook()
    // also loads AppConfig itself for the hook state, independently).
    let startup_cfg =
        AppConfig::load(Path::new("data/config.json")).unwrap_or_else(|_| AppConfig::fallback());
    log::init(startup_cfg.enable_log);
    log::log("daemon starting");

    println!("sloth-daemon (Windows prototype) starting real hook + tray + watcher...");
    println!("Config: data/config.json (or falls back to embedded sample).");
    println!("Tray: right-click for 再生 / 停止 / 再起動 / 設定 / 終了. Layout changes also reload automatically on file watch.");
    println!("Live remap: Space+letter (per sample grid) -> shifted; Space tap -> Space.");

    if !Path::new("data/config.json").exists() {
        sloth_core::notify!("note: data/config.json not found; using embedded layout");
    }

    // Start the low-level hook on its own thread (message pump for LL keyboard).
    let _hook_handle = install_and_run_windows_hook();
    log::log("keyboard hook installed");

    // Create a minimal tray icon + menu.
    let icon = create_simple_icon();
    let tray_menu = Menu::new();
    // FR-8: single toggle item, label reflects current state (再生 when
    // suspended -> click resumes; 停止 when running -> click suspends).
    let mut suspended = is_suspended();
    let toggle_item = MenuItem::new(if suspended { "再生" } else { "停止" }, true, None);
    let restart_item = MenuItem::new("再起動", true, None); // re-exec the daemon
    let settings_item = MenuItem::new("設定", true, None); // open config.json in default app
    let quit_item = MenuItem::new("終了", true, None);
    tray_menu.append(&toggle_item).ok();
    tray_menu.append(&restart_item).ok();
    tray_menu.append(&PredefinedMenuItem::separator()).ok();
    tray_menu.append(&settings_item).ok();
    tray_menu.append(&PredefinedMenuItem::separator()).ok();
    tray_menu.append(&quit_item).ok();

    let _tray = TrayIconBuilder::new()
        .with_tooltip("sloth")
        .with_icon(icon)
        .with_menu(Box::new(tray_menu))
        .build()
        .expect("failed to create tray icon");

    let menu_channel = MenuEvent::receiver();

    // Debounced file watcher for layout hot-reload (NFR-4 safe boundary: reload clears pressed).
    // Watch the samples dir + config for simplicity in prototype.
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(tx)?;
    watcher.watch(Path::new("data"), RecursiveMode::Recursive)?;
    // notify 6 uses a Config; default debounce is fine for prototype.

    // IPC server (named pipe). On Reload command we call the same reload_layout used by tray.
    start_ipc_server(|cmd| {
        match cmd {
            sloth_core::ipc::IpcCommand::Reload => {
                sloth_core::notify!("IPC: reload");
                reload_layout();
                sloth_core::ipc::IpcResponse::Ok
            }
            sloth_core::ipc::IpcCommand::Status => sloth_core::ipc::IpcResponse::Status {
                version: env!("CARGO_PKG_VERSION").into(),
                active_app: String::new(),
                suspended: is_suspended(),
            },
            sloth_core::ipc::IpcCommand::Quit => {
                sloth_core::notify!("IPC: quit requested");
                // Exit on its own thread after a short delay so the IPC
                // response below reaches the client first (mirrors restart).
                std::thread::spawn(|| {
                    std::thread::sleep(Duration::from_millis(200));
                    std::process::exit(0);
                });
                sloth_core::ipc::IpcResponse::Ok
            }
            // FR-8: daemon control hotkeys / commands.
            sloth_core::ipc::IpcCommand::Stop => {
                sloth_core::notify!("IPC: stop (suspend remapping)");
                set_suspend(true);
                sloth_core::ipc::IpcResponse::Ok
            }
            sloth_core::ipc::IpcCommand::Resume => {
                sloth_core::notify!("IPC: resume remapping");
                set_suspend(false);
                sloth_core::ipc::IpcResponse::Ok
            }
            sloth_core::ipc::IpcCommand::ToggleRunning => {
                let now = toggle_suspend();
                sloth_core::notify!(
                    "IPC: toggle running -> {}",
                    if now { "stopped" } else { "running" }
                );
                sloth_core::ipc::IpcResponse::Ok
            }
            sloth_core::ipc::IpcCommand::Restart => {
                sloth_core::notify!("IPC: restart requested");
                // Restart on its own thread: restart_daemon() spawns a fresh
                // copy and exits this process, which we don't want to do from
                // inside the IPC server's response handler.
                std::thread::spawn(restart_daemon);
                sloth_core::ipc::IpcResponse::Ok
            }
        }
    });

    // Main loop: pump Win32 messages (required so the tray icon's right-click
    // menu appears and responds — tray-icon relies on the message queue of the
    // thread that created the icon), then poll the menu/watcher channels.
    loop {
        pump_win32_messages();

        // Menu
        if let Ok(event) = menu_channel.try_recv() {
            if event.id == toggle_item.id() {
                let now_suspended = toggle_suspend();
                suspended = now_suspended;
                toggle_item.set_text(if suspended { "再生" } else { "停止" });
                if suspended {
                    sloth_core::notify!("Tray: stop (remap paused)");
                } else {
                    sloth_core::notify!("Tray: resume (remap resumed)");
                }
            } else if event.id == restart_item.id() {
                sloth_core::notify!("Tray: restart (restarting daemon)");
                restart_daemon();
            } else if event.id == settings_item.id() {
                sloth_core::notify!("Tray: settings (opening config)");
                open_settings();
            } else if event.id == quit_item.id() {
                sloth_core::notify!("Tray: quit");
                std::process::exit(0);
            }
        }

        // Watcher (debounced events come as batches)
        if let Ok(Ok(evt)) = rx.try_recv() {
            if matches!(
                evt.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
            ) {
                // Only care about .txt or config.json changes
                let relevant = evt.paths.iter().any(|p| {
                    let s = p.to_string_lossy();
                    s.ends_with(".txt") || s.ends_with("config.json")
                });
                if relevant {
                    sloth_core::notify!("Watcher: layout/config change detected -> reload");
                    reload_layout();
                }
            }
        }

        // Pick up suspend/resume state changes made via IPC (settings app) so
        // the tray label stays in sync even when not triggered from this menu.
        let now_suspended = is_suspended();
        if now_suspended != suspended {
            suspended = now_suspended;
            toggle_item.set_text(if suspended { "再生" } else { "停止" });
        }

        // Short idle so the first right-click is dispatched promptly (the popup
        // menu, once open, runs its own modal loop). Keeps CPU near zero.
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Drain all pending Win32 messages without blocking. The tray icon creates a
/// hidden window on this thread; its right-click menu and click notifications
/// are delivered as window messages, so they only work if we keep the queue
/// pumped. `PM_REMOVE` dequeues; we Translate/Dispatch so menu commands fire.
fn pump_win32_messages() {
    let mut msg = MSG::default();
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// 再起動: spawn a fresh copy of this executable (same args / cwd) and exit
/// the current process. Effectively reinstalls the hook with reloaded config.
fn restart_daemon() {
    match std::env::current_exe() {
        Ok(exe) => {
            let args: Vec<String> = std::env::args().skip(1).collect();
            match std::process::Command::new(&exe).args(&args).spawn() {
                Ok(_) => {
                    // Give the new instance a moment to install its hook before we
                    // drop ours, so input is never left fully unhooked.
                    std::thread::sleep(Duration::from_millis(300));
                    std::process::exit(0);
                }
                Err(e) => sloth_core::notify_err!("restart failed (spawn): {e}"),
            }
        }
        Err(e) => sloth_core::notify_err!("restart failed (current_exe): {e}"),
    }
}

/// 設定: launch the sloth-config settings window (Slint GUI). Falls back to
/// opening data/config.json in the OS default handler if the settings binary
/// can't be found (e.g. not yet built next to the daemon).
fn open_settings() {
    let sloth_config = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("sloth-config.exe")));

    if let Some(exe) = &sloth_config {
        if exe.exists() {
            match std::process::Command::new(exe).spawn() {
                Ok(_) => log::log(format!("settings: launched {}", exe.display())),
                Err(e) => {
                    sloth_core::notify_err!("settings: failed to launch {}: {e}", exe.display())
                }
            }
            return;
        }
        log::log(format!(
            "settings: {} not found, falling back to file open",
            exe.display()
        ));
    } else {
        log::log("settings: could not determine current_exe, falling back to file open");
    }

    let path = Path::new("data/config.json");
    let target = if path.exists() {
        "data/config.json"
    } else {
        "data"
    };
    // `cmd /C start "" <target>` opens with the default associated program.
    if let Err(e) = std::process::Command::new("cmd")
        .args(["/C", "start", "", target])
        .spawn()
    {
        sloth_core::notify_err!("settings: failed to open {target}: {e}");
    }
}

/// Create a minimal 16x16 RGBA icon (no external assets).
fn create_simple_icon() -> tray_icon::Icon {
    let mut rgba = vec![0u8; 16 * 16 * 4];
    // Dark background
    for y in 0..16 {
        for x in 0..16 {
            let i = (y * 16 + x) * 4;
            rgba[i] = 30; // R
            rgba[i + 1] = 30; // G
            rgba[i + 2] = 40; // B
            rgba[i + 3] = 255; // A
        }
    }
    // Small "R" like dot in center
    for dy in 5..11 {
        for dx in 5..11 {
            let i = ((dy * 16) + dx) * 4;
            rgba[i] = 200;
            rgba[i + 1] = 200;
            rgba[i + 2] = 220;
        }
    }
    tray_icon::Icon::from_rgba(rgba, 16, 16).expect("icon rgba")
}
