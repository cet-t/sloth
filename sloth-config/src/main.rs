#![windows_subsystem = "windows"]
//! sloth-config: settings window (egui/eframe GUI). Edits the fields that exist
//! in `data/config.json` (sloth_core::config::AppConfig) and writes them back.
//!
//! Usage:
//!   sloth-config            -> open settings window
//!   sloth-config reload     -> send IPC reload to the running daemon (CLI helper)
//!   sloth-config status     -> (stub)
//!   sloth-config quit       -> (stub)

mod app;
mod daemon;
mod settings;
mod tabs;

use std::path::Path;

use anyhow::Result;
use clap::{Parser, Subcommand};
use eframe::egui;
use sloth_core::config::AppConfig;
use sloth_core::ipc::{send_reload_command, IpcResponse};

use crate::app::{App, CONFIG_PATH, WINDOW_SIZE, WINDOW_TITLE};

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

    if focus_existing_window() {
        return Ok(());
    }

    run_settings_window()
}

fn run_settings_window() -> Result<()> {
    sloth_core::config::use_packaged_data_directory()?;
    let cfg = AppConfig::load(Path::new(CONFIG_PATH)).unwrap_or_else(|_| AppConfig::fallback());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size(WINDOW_SIZE)
            .with_resizable(false)
            .with_maximize_button(false)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            App::setup_fonts(&cc.egui_ctx);
            Ok(Box::new(App::new(cfg, cc.egui_ctx.clone())))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

/// A second launch (e.g. from the daemon's tray menu) shouldn't open a second
/// window: bring the existing one to the front instead.
#[cfg(windows)]
fn focus_existing_window() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    let title: Vec<u16> = WINDOW_TITLE
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
