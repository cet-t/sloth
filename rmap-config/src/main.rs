//! rmap-config: settings window (Slint GUI). Edits the fields that exist in
//! `data/config.json` (rmap_core::config::AppConfig) and writes them back.
//!
//! Usage:
//!   rmap-config            -> open settings window
//!   rmap-config reload     -> send IPC reload to the running daemon (CLI helper)
//!   rmap-config status     -> (stub)
//!   rmap-config quit       -> (stub)

use anyhow::Result;
use rmap_core::config::AppConfig;
use rmap_core::ipc::{send_reload_command, IpcResponse};
use slint::Model;
use std::path::Path;

slint::slint! {
    import { Button, CheckBox, LineEdit, VerticalBox, HorizontalBox, ScrollView } from "std-widgets.slint";

    export struct ProfileRow {
        name: string,
        layout: string,
        sands: bool,
        gestures: bool,
        shortcuts: bool,
    }

    export component AppWindow inherits Window {
        title: "rmap 設定";
        preferred-width: 520px;
        preferred-height: 560px;

        in-out property <string> default_layout;
        in-out property <string> default_profile;
        in-out property <bool> enable_log;
        in-out property <bool> disable_ctrl;
        in-out property <bool> disable_alt;
        in-out property <bool> disable_win;
        in-out property <bool> disable_shift;
        in-out property <[ProfileRow]> profiles;
        in property <string> status_text;

        callback save();

        ScrollView {
            VerticalBox {
                Text { text: "全般"; font-weight: 700; }
                HorizontalBox {
                    Text { text: "デフォルトレイアウト:"; vertical-alignment: center; }
                    LineEdit { text <=> default_layout; }
                }
                HorizontalBox {
                    Text { text: "デフォルトプロファイル:"; vertical-alignment: center; }
                    LineEdit { text <=> default_profile; }
                }

                Text { text: "一時無効化キー（押している間は全パススルー）"; font-weight: 700; }
                HorizontalBox {
                    CheckBox { text: "Ctrl"; checked <=> disable_ctrl; }
                    CheckBox { text: "Alt"; checked <=> disable_alt; }
                    CheckBox { text: "Win"; checked <=> disable_win; }
                    CheckBox { text: "Shift"; checked <=> disable_shift; }
                }

                Text { text: "ログ"; font-weight: 700; }
                CheckBox { text: "ログを有効にする (実行ファイルと同じフォルダの ./log に出力)"; checked <=> enable_log; }

                Text { text: "プロファイル"; font-weight: 700; }
                for row[i] in profiles : VerticalBox {
                    Text { text: row.name; font-weight: 600; }
                    HorizontalBox {
                        Text { text: "レイアウト:"; vertical-alignment: center; }
                        LineEdit {
                            text: row.layout;
                            edited(text) => { profiles[i].layout = text; }
                        }
                    }
                    HorizontalBox {
                        CheckBox {
                            text: "SandS";
                            checked: row.sands;
                            toggled => { profiles[i].sands = !row.sands; }
                        }
                        CheckBox {
                            text: "Gestures";
                            checked: row.gestures;
                            toggled => { profiles[i].gestures = !row.gestures; }
                        }
                        CheckBox {
                            text: "Shortcuts";
                            checked: row.shortcuts;
                            toggled => { profiles[i].shortcuts = !row.shortcuts; }
                        }
                    }
                }

                HorizontalBox {
                    Button { text: "保存"; clicked => { save(); } }
                    Text { text: status_text; vertical-alignment: center; }
                }
            }
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");
    match cmd {
        "reload" => {
            match send_reload_command() {
                Ok(IpcResponse::Ok) => println!("reload sent"),
                Ok(r) => println!("response: {:?}", r),
                Err(e) => eprintln!("IPC error: {e} (is daemon running?)"),
            }
            return Ok(());
        }
        "status" => {
            println!("status: (IPC status not fully wired in prototype; daemon tray shows state)");
            return Ok(());
        }
        "quit" => {
            println!("quit: (send IpcCommand::Quit via pipe in full impl)");
            return Ok(());
        }
        _ => {}
    }

    run_settings_window()
}

const CONFIG_PATH: &str = "data/config.json";

fn run_settings_window() -> Result<()> {
    let cfg = AppConfig::load(Path::new(CONFIG_PATH)).unwrap_or_else(|_| AppConfig::fallback());

    let window = AppWindow::new()?;

    window.set_default_layout(cfg.default_layout.clone().into());
    window.set_default_profile(cfg.app_map.default_profile.clone().into());
    window.set_enable_log(cfg.enable_log);

    let lower_disable: Vec<String> = cfg
        .disable_keys
        .iter()
        .map(|s| s.trim().to_lowercase())
        .collect();
    window.set_disable_ctrl(lower_disable.iter().any(|s| s == "ctrl" || s == "control"));
    window.set_disable_alt(lower_disable.iter().any(|s| s == "alt" || s == "menu"));
    window.set_disable_win(lower_disable.iter().any(|s| matches!(s.as_str(), "win" | "meta" | "super" | "cmd")));
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
    let profiles_model = std::rc::Rc::new(slint::VecModel::from(rows));
    window.set_profiles(slint::ModelRc::from(profiles_model));

    window.set_status_text("".into());

    let mut base_cfg = cfg;
    let window_weak = window.as_weak();
    window.on_save(move || {
        let window = window_weak.unwrap();

        base_cfg.default_layout = window.get_default_layout().to_string();
        base_cfg.app_map.default_profile = window.get_default_profile().to_string();
        base_cfg.enable_log = window.get_enable_log();

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

        for row in window.get_profiles().iter() {
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
    });

    window.run()?;
    Ok(())
}

fn save_config(cfg: &AppConfig) -> Result<()> {
    let json = serde_json::to_string_pretty(cfg)?;
    if let Some(parent) = Path::new(CONFIG_PATH).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(CONFIG_PATH, json)?;
    Ok(())
}
