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
use slint::{Model, VecModel};
use std::path::Path;
use std::rc::Rc;

slint::slint! {
    import { Button, CheckBox, LineEdit, ComboBox, VerticalBox, HorizontalBox, ScrollView } from "std-widgets.slint";

    export struct ProfileRow {
        name: string,
        layout: string,
        sands: bool,
        gestures: bool,
        shortcuts: bool,
    }

    export component AppWindow inherits Window {
        title: "rmap 設定";
        preferred-width: 540px;
        preferred-height: 560px;

        in-out property <string> default_layout;
        in-out property <int> default_profile_index: 0;
        in-out property <bool> enable_log;
        in-out property <bool> disable_ctrl;
        in-out property <bool> disable_alt;
        in-out property <bool> disable_win;
        in-out property <bool> disable_shift;
        in-out property <[ProfileRow]> profiles;
        in property <[string]> profile_names;
        in-out property <int> current_profile_index: 0;
        in property <string> status_text;

        callback save();
        callback browse_default_layout();
        callback browse_profile_layout();
        callback profile_layout_edited(string);
        callback toggle_profile_sands();
        callback toggle_profile_gestures();
        callback toggle_profile_shortcuts();

        ScrollView {
            VerticalBox {
                Text { text: "全般"; font-weight: 700; }
                HorizontalBox {
                    Text { text: "デフォルトレイアウト:"; vertical-alignment: center; }
                    LineEdit { text <=> default_layout; }
                    Button { text: "📁"; clicked => { browse_default_layout(); } }
                }
                HorizontalBox {
                    Text { text: "デフォルトプロファイル:"; vertical-alignment: center; }
                    ComboBox {
                        model: profile_names;
                        current-index <=> default_profile_index;
                    }
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
                ComboBox {
                    model: profile_names;
                    current-index <=> current_profile_index;
                }
                if profiles.length > 0 && current_profile_index >= 0 : VerticalBox {
                    HorizontalBox {
                        Text { text: "レイアウト:"; vertical-alignment: center; }
                        LineEdit {
                            text: profiles[current_profile_index].layout;
                            edited(text) => { profile_layout_edited(text); }
                        }
                        Button { text: "📁"; clicked => { browse_profile_layout(); } }
                    }
                    HorizontalBox {
                        CheckBox {
                            text: "SandS";
                            checked: profiles[current_profile_index].sands;
                            toggled => { toggle_profile_sands(); }
                        }
                        CheckBox {
                            text: "Gestures";
                            checked: profiles[current_profile_index].gestures;
                            toggled => { toggle_profile_gestures(); }
                        }
                        CheckBox {
                            text: "Shortcuts";
                            checked: profiles[current_profile_index].shortcuts;
                            toggled => { toggle_profile_shortcuts(); }
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

/// Pick a layout file via a native file dialog, starting in `data/layouts` if it exists.
fn pick_layout_file() -> Option<String> {
    let mut dialog = rfd::FileDialog::new().add_filter("Layout", &["txt"]);
    let start_dir = Path::new("data/layouts");
    if start_dir.exists() {
        dialog = dialog.set_directory(start_dir);
    }
    dialog.pick_file().map(|p| p.to_string_lossy().replace('\\', "/"))
}

fn run_settings_window() -> Result<()> {
    let cfg = AppConfig::load(Path::new(CONFIG_PATH)).unwrap_or_else(|_| AppConfig::fallback());

    let window = AppWindow::new()?;

    window.set_default_layout(cfg.default_layout.clone().into());
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
    let profiles_model = Rc::new(VecModel::from(rows));
    window.set_profiles(slint::ModelRc::from(profiles_model.clone()));

    let names_model = Rc::new(VecModel::from(
        profile_names.iter().map(|n| n.clone().into()).collect::<Vec<slint::SharedString>>(),
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

    // 設定 -> デフォルトレイアウトの参照ファイルを選択ダイアログで指定する。
    let window_weak = window.as_weak();
    window.on_browse_default_layout(move || {
        let window = window_weak.unwrap();
        if let Some(path) = pick_layout_file() {
            window.set_default_layout(path.into());
        }
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
        if let Some(path) = pick_layout_file() {
            if let Some(mut row) = model.row_data(idx) {
                row.layout = path.into();
                model.set_row_data(idx, row);
            }
        }
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
