//! rmap-daemon: resident remapper. Owns hooks, applies layouts, IPC server, tray.

use anyhow::Result;
use rmap_core::config::AppConfig;
use rmap_core::hook::{install_and_run_windows_hook, reload_layout, set_suspend, toggle_suspend, is_suspended};
use rmap_core::ipc::start_ipc_server;
use rmap_core::log;
use std::path::Path;
use std::time::Duration;
use tray_icon::{TrayIconBuilder, menu::{Menu, MenuItem, PredefinedMenuItem, MenuEvent}};
use notify::{Watcher, RecursiveMode, RecommendedWatcher};
use notify::event::EventKind;
use windows::Win32::UI::WindowsAndMessaging::{
    PeekMessageW, TranslateMessage, DispatchMessageW, MSG, PM_REMOVE,
};

fn main() -> Result<()> {
    // Load config early so we know whether file logging is enabled (it must
    // be init'd before any log::log() call; install_and_run_windows_hook()
    // also loads AppConfig itself for the hook state, independently).
    let startup_cfg = AppConfig::load(Path::new("data/config.json")).unwrap_or_else(|_| AppConfig::fallback());
    log::init(startup_cfg.enable_log);
    log::log("daemon starting");

    println!("rmap-daemon (Windows prototype) starting real hook + tray + watcher...");
    println!("Config: data/config.json (or falls back to embedded sample).");
    println!("Tray: right-click for 再生 / 停止 / 再起動 / 設定 / 終了. Layout changes also reload automatically on file watch.");
    println!("Live remap: Space+letter (per sample grid) -> shifted; Space tap -> Space.");

    if !Path::new("data/config.json").exists() {
        println!("(note: data/config.json not found; using embedded layout inside hook)");
        log::log("data/config.json not found; using embedded layout");
    }

    // Start the low-level hook on its own thread (message pump for LL keyboard).
    let _hook_handle = install_and_run_windows_hook();
    log::log("keyboard hook installed");

    // Create a minimal tray icon + menu.
    let icon = create_simple_icon();
    let tray_menu = Menu::new();
    let resume_item = MenuItem::new("再生", true, None);   // FR-8 resume (set_suspend false)
    let stop_item = MenuItem::new("停止", true, None);     // FR-8 stop   (set_suspend true)
    let restart_item = MenuItem::new("再起動", true, None); // re-exec the daemon
    let settings_item = MenuItem::new("設定", true, None);  // open config.json in default app
    let quit_item = MenuItem::new("終了", true, None);
    tray_menu.append(&resume_item).ok();
    tray_menu.append(&stop_item).ok();
    tray_menu.append(&restart_item).ok();
    tray_menu.append(&PredefinedMenuItem::separator()).ok();
    tray_menu.append(&settings_item).ok();
    tray_menu.append(&PredefinedMenuItem::separator()).ok();
    tray_menu.append(&quit_item).ok();

    let _tray = TrayIconBuilder::new()
        .with_tooltip("rmap")
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
            rmap_core::ipc::IpcCommand::Reload => {
                println!("IPC: reload");
                log::log("IPC: reload");
                reload_layout();
                rmap_core::ipc::IpcResponse::Ok
            }
            rmap_core::ipc::IpcCommand::Status => {
                rmap_core::ipc::IpcResponse::Status {
                    version: env!("CARGO_PKG_VERSION").into(),
                    active_app: String::new(),
                    suspended: is_suspended(),
                }
            }
            rmap_core::ipc::IpcCommand::Quit => {
                println!("IPC: quit requested");
                log::log("IPC: quit requested");
                // In real: signal main loop; for prototype we just ack.
                rmap_core::ipc::IpcResponse::Ok
            }
            // FR-8: daemon control hotkeys / commands.
            rmap_core::ipc::IpcCommand::Stop => {
                println!("IPC: stop (suspend remapping)");
                log::log("IPC: stop (suspend remapping)");
                set_suspend(true);
                rmap_core::ipc::IpcResponse::Ok
            }
            rmap_core::ipc::IpcCommand::Resume => {
                println!("IPC: resume remapping");
                log::log("IPC: resume remapping");
                set_suspend(false);
                rmap_core::ipc::IpcResponse::Ok
            }
            rmap_core::ipc::IpcCommand::ToggleRunning => {
                let now = toggle_suspend();
                println!("IPC: toggle running -> {}", if now { "stopped" } else { "running" });
                log::log(format!("IPC: toggle running -> {}", if now { "stopped" } else { "running" }));
                rmap_core::ipc::IpcResponse::Ok
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
            if event.id == resume_item.id() {
                set_suspend(false);
                println!("Tray: 再生 (remap resumed)");
                log::log("tray: resume (remap resumed)");
            } else if event.id == stop_item.id() {
                set_suspend(true);
                println!("Tray: 停止 (remap paused)");
                log::log("tray: stop (remap paused)");
            } else if event.id == restart_item.id() {
                println!("Tray: 再起動 (restarting daemon)");
                log::log("tray: restart");
                restart_daemon();
            } else if event.id == settings_item.id() {
                println!("Tray: 設定 (opening config)");
                log::log("tray: open settings");
                open_settings();
            } else if event.id == quit_item.id() {
                println!("Tray: 終了 (quit)");
                log::log("tray: quit");
                std::process::exit(0);
            }
        }

        // Watcher (debounced events come as batches)
        if let Ok(Ok(evt)) = rx.try_recv() {
            if matches!(evt.kind, EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)) {
                // Only care about .txt or config.json changes
                let relevant = evt.paths.iter().any(|p| {
                    let s = p.to_string_lossy();
                    s.ends_with(".txt") || s.ends_with("config.json")
                });
                if relevant {
                    println!("Watcher: layout/config change detected -> reload");
                    log::log("watcher: layout/config change detected -> reload");
                    reload_layout();
                }
            }
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
                Err(e) => eprintln!("再起動 failed (spawn): {e}"),
            }
        }
        Err(e) => eprintln!("再起動 failed (current_exe): {e}"),
    }
}

/// 設定: launch the rmap-config settings window (Slint GUI). Falls back to
/// opening data/config.json in the OS default handler if the settings binary
/// can't be found (e.g. not yet built next to the daemon).
fn open_settings() {
    let rmap_config = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("rmap-config.exe")));

    if let Some(exe) = &rmap_config {
        if exe.exists() {
            match std::process::Command::new(exe).spawn() {
                Ok(_) => log::log(format!("settings: launched {}", exe.display())),
                Err(e) => {
                    eprintln!("設定 failed to launch {}: {e}", exe.display());
                    log::log(format!("settings: failed to launch {}: {e}", exe.display()));
                }
            }
            return;
        }
        log::log(format!("settings: {} not found, falling back to file open", exe.display()));
    } else {
        log::log("settings: could not determine current_exe, falling back to file open");
    }

    let path = Path::new("data/config.json");
    let target = if path.exists() { "data/config.json" } else { "data" };
    // `cmd /C start "" <target>` opens with the default associated program.
    if let Err(e) = std::process::Command::new("cmd")
        .args(["/C", "start", "", target])
        .spawn()
    {
        eprintln!("設定 failed to open {target}: {e}");
        log::log(format!("settings: failed to open {target}: {e}"));
    }
}

/// Create a minimal 16x16 RGBA icon (no external assets).
fn create_simple_icon() -> tray_icon::Icon {
    let mut rgba = vec![0u8; 16 * 16 * 4];
    // Dark background
    for y in 0..16 {
        for x in 0..16 {
            let i = (y * 16 + x) * 4;
            rgba[i] = 30;     // R
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
