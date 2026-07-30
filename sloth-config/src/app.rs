//! The settings window itself (egui / eframe, immediate mode).

use crate::daemon::{self, DaemonOp, DaemonStatus};
use crate::settings::{DirectInputKey, SandsMode, Settings};
use eframe::egui;
use egui::{Context, FontData, FontDefinitions, FontFamily, RichText, Ui};
use egui_file_dialog::FileDialog;
use sloth_core::config::AppConfig;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

pub const CONFIG_PATH: &str = "data/config.json";
pub const WINDOW_TITLE: &str = "sloth 設定";
pub const WINDOW_SIZE: [f32; 2] = [680.0, 480.0];

/// Width of the label column in the 全般 tab, so the inputs line up.
const LABEL_WIDTH: f32 = 220.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    General,
    DisableKeys,
    Log,
    Ime,
    Process,
}

impl Category {
    const ALL: [Self; 5] = [
        Self::General,
        Self::DisableKeys,
        Self::Log,
        Self::Ime,
        Self::Process,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => "全般",
            Self::DisableKeys => "キー無効化",
            Self::Log => "ログ",
            Self::Ime => "IME",
            Self::Process => "プロセス",
        }
    }
}

/// Result of a background daemon-control op, marshalled back to the UI thread.
struct OpResult {
    status_text: String,
    daemon: DaemonStatus,
}

pub struct App {
    /// The on-disk config, used as the base every save is written onto so
    /// fields the window doesn't expose survive round trips.
    base_cfg: AppConfig,
    settings: Settings,
    /// Snapshot of the last saved state; `settings != saved` is the dirty flag.
    saved: Settings,
    category: Category,

    default_layout_dialog: FileDialog,
    ime_off_layout_dialog: FileDialog,

    status_text: String,
    daemon: DaemonStatus,
    /// While an op is in flight the daemon buttons are disabled: a rapid
    /// double-click could otherwise fire the not-running auto-start fallback
    /// twice and spawn two daemons that then race to answer IPC queries.
    op_in_flight: bool,
    op_tx: Sender<OpResult>,
    op_rx: Receiver<OpResult>,
    /// Kept so worker threads can wake the (otherwise idle) UI when their
    /// result lands -- without a repaint nothing would poll `op_rx`.
    ctx: Context,
}

impl App {
    pub fn new(cfg: AppConfig, ctx: Context) -> Self {
        let settings = Settings::from_config(&cfg);
        let (op_tx, op_rx) = mpsc::channel();
        let app = Self {
            base_cfg: cfg,
            saved: settings.clone(),
            settings,
            category: Category::General,
            default_layout_dialog: layout_file_dialog(),
            ime_off_layout_dialog: layout_file_dialog(),
            status_text: String::new(),
            daemon: DaemonStatus::default(),
            op_in_flight: false,
            op_tx,
            op_rx,
            ctx,
        };
        app.spawn_status_refresh();
        app
    }

    pub fn setup_fonts(ctx: &Context) {
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            "ZenMaruGothic".to_owned(),
            Arc::new(FontData::from_static(include_bytes!(
                "../fonts/ZenMaruGothic-Regular.ttf"
            ))),
        );
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "ZenMaruGothic".to_owned());
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .push("ZenMaruGothic".to_owned());
        ctx.set_fonts(fonts);
    }

    fn dirty(&self) -> bool {
        self.settings != self.saved
    }

    /// Queries daemon status off the UI thread: `send_command` retries for up
    /// to ~2s when the named pipe doesn't exist yet, which would otherwise
    /// freeze the window.
    fn spawn_status_refresh(&self) {
        let tx = self.op_tx.clone();
        let ctx = self.ctx.clone();
        let status_text = self.status_text.clone();
        std::thread::spawn(move || {
            let _ = tx.send(OpResult {
                status_text,
                daemon: daemon::query_status(),
            });
            ctx.request_repaint();
        });
    }

    fn spawn_op(&mut self, op: DaemonOp) {
        if self.op_in_flight {
            return;
        }
        self.op_in_flight = true;
        self.status_text = "処理中...".to_string();
        let was_suspended = self.daemon.suspended;
        let tx = self.op_tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let status_text = daemon::run_op(op, was_suspended);
            let _ = tx.send(OpResult {
                status_text,
                daemon: daemon::query_status(),
            });
            ctx.request_repaint();
        });
    }

    fn poll_ops(&mut self) {
        while let Ok(result) = self.op_rx.try_recv() {
            self.status_text = result.status_text;
            self.daemon = result.daemon;
            self.op_in_flight = false;
        }
    }

    fn save(&mut self) {
        let mut cfg = self.base_cfg.clone();
        self.settings.apply_to(&mut cfg);
        match write_config(&cfg) {
            Ok(()) => {
                self.base_cfg = cfg;
                self.saved = self.settings.clone();
                self.status_text = "保存しました".to_string();
            }
            Err(e) => self.status_text = format!("保存に失敗: {e}"),
        }
    }

    fn categories_ui(&mut self, ui: &mut Ui) {
        for category in Category::ALL {
            let selected = category == self.category;
            let mut text = RichText::new(category.label());
            if selected {
                text = text.strong();
            }
            // Full-width rows so the whole strip is clickable, like the old
            // Slint category list.
            let size = egui::vec2(ui.available_width(), 32.0);
            if ui
                .add(egui::Button::selectable(selected, text).min_size(size))
                .clicked()
            {
                self.category = category;
            }
        }
    }

    fn body_ui(&mut self, ui: &mut Ui) {
        match self.category {
            Category::General => self.general_ui(ui),
            Category::DisableKeys => {
                ui.checkbox(&mut self.settings.disable_ctrl, "Ctrl");
                ui.checkbox(&mut self.settings.disable_alt, "Alt");
                ui.checkbox(&mut self.settings.disable_win, "Win");
                ui.checkbox(&mut self.settings.disable_shift, "Shift");
            }
            Category::Log => {
                ui.checkbox(
                    &mut self.settings.enable_log,
                    "ログを有効にする (実行ファイルと同じフォルダの ./log に出力)",
                );
                if ui.button("ログを開く").clicked() {
                    self.status_text = open_log_file();
                }
            }
            Category::Ime => {
                ui.checkbox(
                    &mut self.settings.enable_ctrl_space_ime_toggle,
                    "Ctrl + Space で IME のオン/オフを切り替える",
                );
            }
            Category::Process => {
                ui.horizontal(|ui| {
                    let enabled = !self.op_in_flight;
                    let toggle_label = if self.daemon.suspended {
                        "再生"
                    } else {
                        "停止"
                    };
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(toggle_label).min_size([72.0, 28.0].into()),
                        )
                        .clicked()
                    {
                        self.spawn_op(DaemonOp::ToggleRunning);
                    }
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new("再起動").min_size([72.0, 28.0].into()),
                        )
                        .clicked()
                    {
                        self.spawn_op(DaemonOp::Restart);
                    }
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new("終了").min_size([72.0, 28.0].into()),
                        )
                        .clicked()
                    {
                        self.spawn_op(DaemonOp::Quit);
                    }
                });
            }
        }
    }

    fn general_ui(&mut self, ui: &mut Ui) {
        let ime_off_enabled = self.settings.activate_only_when_ime_on;

        labeled_row(ui, "IMEオン", |ui| {
            layout_picker(
                ui,
                &mut self.settings.default_layout,
                &mut self.default_layout_dialog,
                true,
                "",
            );
        });

        labeled_row(ui, "IMEオフ", |ui| {
            layout_picker(
                ui,
                &mut self.settings.ime_off_layout,
                &mut self.ime_off_layout_dialog,
                ime_off_enabled,
                "パススルー",
            );
        });

        labeled_row(ui, "同時打鍵判定のしきい値 (ms)", |ui| {
            ui.add(egui::DragValue::new(&mut self.settings.combo_window_ms).range(1..=500));
        });

        ui.checkbox(
            &mut self.settings.activate_only_when_ime_on,
            "IMEがオンの時だけ有効にする（オフ時は全パススルー、または上のレイアウトを使用）",
        );
        ui.checkbox(
            &mut self.settings.hold_mode,
            "長押し時に出力をリピートする（ホールド扱い、オフ＝単打扱い）",
        );

        labeled_row(ui, "直接入力キー", |ui| {
            egui::ComboBox::from_id_salt("direct_input_key")
                .selected_text(self.settings.direct_input_key.label())
                .show_ui(ui, |ui| {
                    for key in DirectInputKey::ALL {
                        ui.selectable_value(&mut self.settings.direct_input_key, key, key.label());
                    }
                });
        });
        ui.label(
            RichText::new("（押している間 IME オフレイアウトを使用）")
                .small()
                .weak(),
        );

        labeled_row(ui, "SandS", |ui| {
            egui::ComboBox::from_id_salt("sands_mode")
                .selected_text(self.settings.sands_mode.label())
                .show_ui(ui, |ui| {
                    for mode in SandsMode::ALL {
                        ui.selectable_value(&mut self.settings.sands_mode, mode, mode.label());
                    }
                });
        });

        ui.checkbox(
            &mut self.settings.enable_sands_ime_on,
            "SandS を IME オン時に有効にする",
        );
        ui.checkbox(
            &mut self.settings.enable_sands_ime_off,
            "SandS を IME オフ時に有効にする",
        );

        labeled_row(ui, "処理のディスパッチレート (ms)", |ui| {
            ui.add(egui::DragValue::new(&mut self.settings.dispatch_rate_ms).range(1..=100));
        });
    }

    fn save_bar_ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("状態");
                    let color = match self.daemon.text {
                        "稼働中" => egui::Color32::from_rgb(0x2e, 0xcc, 0x71),
                        "停止中" => egui::Color32::from_rgb(0xe7, 0x4c, 0x3c),
                        _ => ui.visuals().text_color(),
                    };
                    ui.colored_label(color, self.daemon.text);
                });
                ui.horizontal(|ui| {
                    ui.label("レイアウト");
                    ui.label(current_layout_text(&self.base_cfg));
                });
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.dirty(),
                        egui::Button::new("保存").min_size([72.0, 28.0].into()),
                    )
                    .clicked()
                {
                    self.save();
                }
                ui.label(&self.status_text);
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.poll_ops();

        egui::Panel::left("categories")
            .exact_size(160.0)
            .resizable(false)
            .show(ui, |ui| self.categories_ui(ui));

        egui::Panel::bottom("save_bar").show(ui, |ui| {
            ui.add_space(4.0);
            self.save_bar_ui(ui);
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                self.body_ui(ui);
                ui.allocate_space(egui::vec2(ui.available_width(), 0.0));
            });
        });
    }
}

fn labeled_row(ui: &mut Ui, label: &str, contents: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        // Fixed-width, left-aligned label column so every row's input starts
        // at the same x.
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_WIDTH, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| ui.label(label),
        );
        contents(ui);
    });
}

/// Text field + 📁 button pair, with an in-app file dialog. `placeholder` is
/// shown (greyed) when the field is empty.
fn layout_picker(
    ui: &mut Ui,
    value: &mut String,
    dialog: &mut FileDialog,
    enabled: bool,
    placeholder: &str,
) {
    let button_width = 52.0;
    let spacing = ui.spacing().item_spacing.x;
    let text_width = (ui.available_width() - button_width - spacing).max(60.0);
    ui.add_enabled(
        enabled,
        egui::TextEdit::singleline(value)
            .hint_text(placeholder)
            .desired_width(text_width),
    );
    if ui.add_enabled(enabled, egui::Button::new("参照")).clicked() {
        dialog.pick_file();
    }
    dialog.update(ui.ctx());
    if let Some(path) = dialog.take_picked() {
        *value = relativize_to_cwd(&path);
    }
}

fn layout_file_dialog() -> FileDialog {
    let mut dialog = FileDialog::new()
        .title("レイアウトを選択")
        .add_file_filter_extensions("Layout", vec!["txt", "toml", "json"])
        .default_file_filter("Layout");
    let start_dir = Path::new("data/layouts");
    if start_dir.exists() {
        if let Ok(absolute) = start_dir.canonicalize() {
            dialog = dialog.initial_directory(absolute);
        }
    }
    dialog
}

/// Rewrites an absolute path picked via the file dialog to be relative to the
/// process's current directory (the same base `CONFIG_PATH` and `data/layouts`
/// are resolved against), so `layout` fields saved in `data/config.json` stay
/// portable across machines/checkout locations instead of hard-coding this
/// machine's absolute path. Falls back to the absolute path (forward-slashed)
/// when there's no common root to relativize against, e.g. picking a file on a
/// different drive on Windows.
fn relativize_to_cwd(path: &Path) -> String {
    let cwd = std::env::current_dir().ok();
    let relative = cwd.as_deref().and_then(|cwd| relative_path(path, cwd));
    relative
        .unwrap_or_else(|| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

/// Component-wise `target` relative to `base`, or `None` if they share no
/// common root (e.g. different drive letters on Windows) and therefore can't
/// be expressed as a relative path at all.
fn relative_path(target: &Path, base: &Path) -> Option<PathBuf> {
    let target_comps: Vec<_> = target.components().collect();
    let base_comps: Vec<_> = base.components().collect();
    let common = target_comps
        .iter()
        .zip(base_comps.iter())
        .take_while(|(t, b)| t == b)
        .count();
    if common == 0 {
        return None;
    }
    let mut result = PathBuf::new();
    for _ in common..base_comps.len() {
        result.push("..");
    }
    for c in &target_comps[common..] {
        result.push(c.as_os_str());
    }
    Some(result)
}

fn current_layout_text(cfg: &AppConfig) -> String {
    cfg.profiles
        .get(&cfg.app_map.default_profile)
        .map(|p| p.layout.clone())
        .unwrap_or_else(|| cfg.default_layout.clone())
}

fn write_config(cfg: &AppConfig) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(cfg)?;
    if let Some(parent) = Path::new(CONFIG_PATH).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(CONFIG_PATH, json)?;
    Ok(())
}

fn open_log_file() -> String {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return "ログファイルの場所を特定できません".to_string();
    };
    let log_path = dir.join("log");
    if !log_path.exists() {
        return "ログファイルが見つかりません".to_string();
    }
    if open_with_default_app(&log_path) {
        String::new()
    } else {
        "ログファイルを開けませんでした".to_string()
    }
}

#[cfg(windows)]
fn open_with_default_app(path: &Path) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let path_wide: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let verb: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(path_wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    result.0 as isize > 32
}

#[cfg(not(windows))]
fn open_with_default_app(_path: &Path) -> bool {
    false
}
