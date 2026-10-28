//! The settings window itself (egui / eframe, immediate mode).

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use eframe::egui;
use egui::{Context, FontData, FontDefinitions, FontFamily, RichText, Ui};
use egui_file_dialog::FileDialog;
use sloth_core::config::AppConfig;

use crate::daemon::{self, DaemonStatus};
use crate::settings::Settings;

pub const CONFIG_PATH: &str = "data/config.json";
pub const WINDOW_TITLE: &str = "sloth 設定";
pub const WINDOW_SIZE: [f32; 2] = [680.0, 480.0];

/// Width of the label column in the 全般 tab, so the inputs line up.
pub(super) const LABEL_WIDTH: f32 = 220.0;

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
pub struct OpResult {
    status_text: String,
    daemon: DaemonStatus,
}

impl OpResult {
    pub fn new(status_text: impl Into<String>, daemon: DaemonStatus) -> Self {
        Self {
            status_text: status_text.into(),
            daemon,
        }
    }
}

pub struct App {
    /// The on-disk config, used as the base every save is written onto so
    /// fields the window doesn't expose survive round trips.
    pub(super) base_cfg: AppConfig,
    pub(super) settings: Settings,
    /// Snapshot of the last saved state; `settings != saved` is the dirty flag.
    pub(super) saved: Settings,
    category: Category,

    pub(super) default_layout_dialog: FileDialog,
    pub(super) ime_off_layout_dialog: FileDialog,

    pub(super) status_text: String,
    pub(super) daemon: DaemonStatus,
    /// While an op is in flight the daemon buttons are disabled: a rapid
    /// double-click could otherwise fire the not-running auto-start fallback
    /// twice and spawn two daemons that then race to answer IPC queries.
    pub(super) op_in_flight: bool,
    pub(super) op_tx: Sender<OpResult>,
    pub(super) op_rx: Receiver<OpResult>,
    /// Kept so worker threads can wake the (otherwise idle) UI when their
    /// result lands -- without a repaint nothing would poll `op_rx`.
    pub(super) ctx: Context,
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
            Category::General => self.general(ui),
            Category::DisableKeys => self.disable_keys(ui),
            Category::Log => self.log(ui),
            Category::Ime => self.ime(ui),
            Category::Process => self.process(ui),
        }
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
