//! sloth-daemon: resident remapper. Owns hooks, applies layouts, IPC server, tray.
//!
//! Deliberately *not* `#![windows_subsystem = "windows"]`: that would make
//! CLI usage (`-h`, `convert`) unreliable, since a GUI-subsystem process
//! never gets a console from the OS and has to fight different shells'
//! differing child-process/handle-inheritance behavior to get one (tried
//! `AttachConsole`+`SetStdHandle`; it was flaky across shells and made
//! things worse in some). Console subsystem instead: CLI usage gets a
//! console the normal, reliable way for free, and [`hide_console_window`]
//! detaches the resident daemon's own auto-allocated one immediately on
//! entering that path, before the tray/hook ever runs -- so there's no
//! lingering console window for the actual background-daemon use case.

use anyhow::Result;
use clap::{Parser, Subcommand};
use notify::event::EventKind;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use sloth_core::config::AppConfig;
use sloth_core::hook::{
    install_and_run_windows_hook, is_suspended, reload_layout, set_suspend, toggle_suspend,
};
use sloth_core::ipc::start_ipc_server;
use sloth_core::log;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Console::FreeConsole;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

/// Detach from the console the OS auto-allocated for this (console
/// subsystem) process. Called once, right at the top of the resident-daemon
/// path -- before the tray/hook/IPC server ever start -- so the background
/// daemon never shows a lingering console window, while CLI invocations
/// (`-h`, `convert`) never reach this call and keep their console.
fn hide_console_window() {
    unsafe {
        let _ = FreeConsole();
    }
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert a layout file from another source format into sloth TOML.
    Convert {
        /// Convert a DvorakJ .txt layout file. (The only source format
        /// today; a future one would get its own flag alongside this,
        /// e.g. `--other <PATH>`, without changing this one's shape.)
        #[arg(long = "dj", visible_alias = "dvorakj", value_name = "PATH")]
        dj: Option<PathBuf>,
        /// Output path (default: the input path with a .toml extension).
        #[arg(short = 'o', long = "out", value_name = "OUT")]
        out: Option<PathBuf>,
    },
}

/// Windows-named-object single-instance guard. Without this, nothing stops
/// two `sloth.exe` processes from running at once (e.g. the settings
/// window's "auto-start if not running" fallback firing twice from a rapid
/// double-click before the first instance's IPC server is up, or a leftover
/// process from manual testing) -- each installing its own keyboard hook
/// and its own IPC pipe instance (PIPE_UNLIMITED_INSTANCES lets that
/// succeed silently), so a client's status query can end up answered by
/// whichever instance happens to be listening, with no guarantee it's the
/// one the user thinks is "the" daemon. The handle is intentionally never
/// closed -- it must live for the whole process lifetime, and the OS
/// reclaims it on exit regardless.
///
/// Returns `true` if this process won the race and should proceed as the
/// one true daemon; `false` if another instance already holds it.
fn acquire_single_instance_lock() -> bool {
    let name: Vec<u16> = "Global\\sloth-daemon-singleton"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        match CreateMutexW(None, true, windows::core::PCWSTR(name.as_ptr())) {
            Ok(handle) => {
                let already_running = std::io::Error::last_os_error().raw_os_error()
                    == Some(ERROR_ALREADY_EXISTS.0 as i32);
                if already_running {
                    let _ = CloseHandle(handle);
                    false
                } else {
                    // Deliberately not closing `handle` (see doc comment
                    // above); `HANDLE` is a plain Copy wrapper with no Drop,
                    // so simply not calling CloseHandle is enough to leak it
                    // for the process lifetime.
                    true
                }
            }
            Err(_) => {
                // If we can't even create the mutex, don't block startup
                // over it -- fail open rather than bricking the daemon.
                true
            }
        }
    }
}

fn main() -> Result<()> {
    if let Some(Commands::Convert { dj, out }) = Cli::parse().command {
        return run_convert(dj, out);
    }

    // Losing the single-instance race is checked *before* detaching from
    // the console, so the message is actually visible when the second
    // instance was launched from a shell (after FreeConsole it would go
    // nowhere).
    if !acquire_single_instance_lock() {
        eprintln!("sloth-daemon: another instance is already running, exiting");
        return Ok(());
    }

    // Entering the resident daemon path for real. Hide the console this
    // (console-subsystem) process was auto-allocated with -- CLI
    // invocations above never reach this line, so their console stays.
    hide_console_window();

    // Both DvorakJ `.txt` and sloth TOML/JSON are loadable: each source
    // format compiles into the same `sloth_parser::CompiledLayout`
    // internally (see sloth-core::sloth_parser::to_core_layout), so nothing
    // downstream needs to know or care which one a given layout file is.
    sloth_core::loader::register_default_loader(Box::new(
        sloth_core::loader::CompositeLoader::new(vec![
            Box::new(sloth_dvorakj_adapter::RmapDvorakJLayoutLoader::new()),
            Box::new(sloth_core::sloth_parser::SlothLayoutLoader::new()),
        ]),
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
    // Watch the samples dir + config for simplicity in prototype. `data/` is
    // resolved relative to CWD, same as AppConfig::load above -- if the
    // daemon exe was launched from a folder without a sibling `data/` (e.g.
    // double-clicked straight out of target/release instead of dist/ or the
    // repo root), skip watching instead of letting `?` kill the whole
    // process (and, with it, the tray icon that was just created).
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(tx)?;
    if Path::new("data").is_dir() {
        if let Err(e) = watcher.watch(Path::new("data"), RecursiveMode::Recursive) {
            sloth_core::notify_err!("watcher: failed to watch data/: {e}");
        }
    } else {
        sloth_core::notify!("watcher: data/ not found next to the exe; hot-reload disabled");
    }
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
                // Only care about layout (.txt / .toml / .json) or
                // config.json changes -- .json is shared between
                // config.json and a sloth-format layout, both should
                // trigger a reload either way.
                let relevant = evt.paths.iter().any(|p| {
                    let s = p.to_string_lossy();
                    s.ends_with(".txt") || s.ends_with(".toml") || s.ends_with(".json")
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

/// 設定: launch the sloth-config settings window (Slint GUI).
///
/// Resolution order:
/// 1. `sloth-config.exe` next to the running daemon binary (the normal case
///    for a packaged/release build where `build-release.ps1` places both
///    binaries side by side).
/// 2. Debug builds only: `cargo run -p sloth-config`, so `cargo run -p
///    sloth-daemon` alone is a self-sufficient dev workflow -- no separate
///    `cargo build -p sloth-config` step required.
/// 3. Opening data/config.json in the OS default handler, as a last resort.
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
            "settings: {} not found",
            exe.display()
        ));
    } else {
        log::log("settings: could not determine current_exe");
    }

    if try_open_settings_via_cargo() {
        return;
    }

    log::log("settings: falling back to file open");
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

/// Workspace-checkout fallback: launch the settings GUI via `cargo run -p
/// sloth-config` from the workspace root, when `sloth-config.exe` wasn't
/// found next to the running daemon exe. This covers running the daemon
/// straight out of a dev checkout (either `cargo run` or a manually built
/// exe under target/{debug,release}) without a prior separate `cargo build
/// -p sloth-config` step -- regardless of whether *this* binary itself was
/// built in debug or release. `CARGO_MANIFEST_DIR` is baked in at compile
/// time as this crate's own directory (`<workspace>/sloth-daemon`), so its
/// parent is the workspace root regardless of the daemon's CWD.
///
/// Returns `false` (without spawning) if there's no workspace `Cargo.toml`
/// there (e.g. a packaged/installed build with no source checkout) or
/// `cargo` isn't on PATH, so the caller falls through to the file-open
/// fallback instead of spawning a doomed-to-fail `cargo` process.
fn try_open_settings_via_cargo() -> bool {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf());
    let Some(workspace_root) = workspace_root else {
        return false;
    };
    if !workspace_root.join("Cargo.toml").exists() {
        return false;
    }

    // CREATE_NO_WINDOW: the resident daemon detached from its console via
    // FreeConsole (see `hide_console_window`), but `cargo` is a console
    // subsystem exe -- without this flag, spawning it pops a visible
    // terminal window. Explicit `Stdio::null()` on all three streams matters
    // here too: with no console of its own to inherit handles from, a GUI
    // parent process's stdio handles are invalid, and leaving them
    // unspecified can make the child fail silently instead of running.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let log_path = workspace_root.join("cargo_fallback.log");
    let spawn_result = std::process::Command::new("cargo")
        .args(["run", "--quiet", "-p", "sloth-config"])
        .current_dir(&workspace_root)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(
            std::fs::File::create(&log_path)
                .map(std::process::Stdio::from)
                .unwrap_or_else(|_| std::process::Stdio::null()),
        )
        .spawn();
    match spawn_result {
        Ok(_) => {
            log::log("settings: sloth-config.exe not found, launched via `cargo run -p sloth-config`");
            true
        }
        Err(e) => {
            log::log(format!(
                "settings: `cargo run -p sloth-config` failed to spawn: {e}"
            ));
            false
        }
    }
}

/// `sloth convert --dj <PATH> [-o <OUT>]`: convert a layout file from
/// another source format into sloth TOML. `dj`/`out` come straight from
/// clap's `Commands::Convert` (see the `Cli` definition above); adding a
/// second source format later just means a new `Option<PathBuf>` field
/// there and a matching branch here, no parsing logic to touch. Runs
/// standalone: no hook, tray, or single-instance lock.
fn run_convert(dj: Option<PathBuf>, out: Option<PathBuf>) -> Result<()> {
    let Some(input) = dj else {
        anyhow::bail!("specify a source format and path, e.g. --dj <PATH>");
    };

    let bytes = std::fs::read(&input)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", input.display()))?;
    let id = input.to_string_lossy().to_string();
    let options = dvorakj_parser::ParseOptions::from_source_id(&id);
    let report = dvorakj_parser::parse_bytes(&bytes, &id, options)
        .map_err(|e| anyhow::anyhow!("parsing {}: {e}", input.display()))?;
    let compiled = dvorakj_parser::sloth::to_compiled_layout(report.layout);
    let result = sloth_parser::to_toml(&compiled);

    let out = out.unwrap_or_else(|| input.with_extension("toml"));
    std::fs::write(&out, &result.toml)
        .map_err(|e| anyhow::anyhow!("writing {}: {e}", out.display()))?;

    println!("converted {} -> {}", input.display(), out.display());
    for w in &result.warnings {
        eprintln!("warning: {w}");
    }
    Ok(())
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
