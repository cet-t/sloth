//! rmap-daemon: resident remapper. Owns hooks, applies layouts, IPC server, tray.

use anyhow::Result;
use rmap_core::hook::{install_and_run_windows_hook, reload_layout, set_suspend, toggle_suspend, is_suspended};
use rmap_core::ipc::start_ipc_server;
use std::path::Path;
use std::time::Duration;
use tray_icon::{TrayIconBuilder, menu::{Menu, MenuItem, MenuEvent}};
use notify::{Watcher, RecursiveMode, RecommendedWatcher};
use notify::event::EventKind;

fn main() -> Result<()> {
    println!("rmap-daemon (Windows prototype) starting real hook + tray + watcher...");
    println!("Config: data/config.json (or falls back to embedded sample).");
    println!("Tray: right-click for Reload / Quit. Layout changes take effect immediately on reload or file watch.");
    println!("Live remap: Space+letter (per sample grid) -> shifted; Space tap -> Space.");

    if !Path::new("data/config.json").exists() {
        println!("(note: data/config.json not found; using embedded layout inside hook)");
    }

    // Start the low-level hook on its own thread (message pump for LL keyboard).
    let _hook_handle = install_and_run_windows_hook();

    // Create a minimal tray icon + menu.
    let icon = create_simple_icon();
    let tray_menu = Menu::new();
    let reload_item = MenuItem::new("Reload layout", true, None);
    let toggle_item = MenuItem::new("Pause / Resume remap", true, None); // FR-8
    let quit_item = MenuItem::new("Quit", true, None);
    tray_menu.append(&reload_item).ok();
    tray_menu.append(&toggle_item).ok();
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
                // In real: signal main loop; for prototype we just ack.
                rmap_core::ipc::IpcResponse::Ok
            }
            // FR-8: daemon control hotkeys / commands.
            rmap_core::ipc::IpcCommand::Stop => {
                println!("IPC: stop (suspend remapping)");
                set_suspend(true);
                rmap_core::ipc::IpcResponse::Ok
            }
            rmap_core::ipc::IpcCommand::Resume => {
                println!("IPC: resume remapping");
                set_suspend(false);
                rmap_core::ipc::IpcResponse::Ok
            }
            rmap_core::ipc::IpcCommand::ToggleRunning => {
                let now = toggle_suspend();
                println!("IPC: toggle running -> {}", if now { "stopped" } else { "running" });
                rmap_core::ipc::IpcResponse::Ok
            }
        }
    });

    // Main loop: tray menu + watcher + keep process alive.
    loop {
        // Menu
        if let Ok(event) = menu_channel.try_recv() {
            if event.id == reload_item.id() {
                println!("Tray: manual reload");
                reload_layout();
            } else if event.id == toggle_item.id() {
                let stopped = toggle_suspend();
                println!("Tray: remap {}", if stopped { "paused" } else { "resumed" });
            } else if event.id == quit_item.id() {
                println!("Tray: quit");
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
                    reload_layout();
                }
            }
        }

        std::thread::sleep(Duration::from_millis(80));
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
