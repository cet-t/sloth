use std::{
    ops::Index,
    path::{Path, PathBuf},
};

use eframe::egui::{self, RichText, Ui};
use egui_file_dialog::FileDialog;

use crate::{
    app::LABEL_WIDTH,
    settings::{DirectInputKey, SandsMode},
};

impl crate::app::App {
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

    fn relativize_to_cwd(path: &Path) -> String {
        let cwd = std::env::current_dir().ok();
        let relative = cwd
            .as_deref()
            .and_then(|cwd| Self::relative_path(path, cwd));
        relative
            .unwrap_or_else(|| path.to_path_buf())
            .to_string_lossy()
            .replace('\\', "/")
    }

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
        target_comps
            .index(common..)
            .iter()
            .for_each(|c| result.push(c.as_os_str()));
        for c in &target_comps[common..] {
            result.push(c.as_os_str());
        }
        Some(result)
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
            *value = Self::relativize_to_cwd(&path);
        }
    }

    pub fn general(&mut self, ui: &mut egui::Ui) {
        let ime_off_enabled = self.settings.activate_only_when_ime_on;

        Self::labeled_row(ui, "IMEオン", |ui| {
            Self::layout_picker(
                ui,
                &mut self.settings.default_layout,
                &mut self.default_layout_dialog,
                true,
                "",
            );
        });

        Self::labeled_row(ui, "IMEオフ", |ui| {
            Self::layout_picker(
                ui,
                &mut self.settings.ime_off_layout,
                &mut self.ime_off_layout_dialog,
                ime_off_enabled,
                "パススルー",
            );
        });

        Self::labeled_row(ui, "同時打鍵判定のしきい値 (ms)", |ui| {
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

        Self::labeled_row(ui, "直接入力キー", |ui| {
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

        Self::labeled_row(ui, "SandS", |ui| {
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

        Self::labeled_row(ui, "処理のディスパッチレート (ms)", |ui| {
            ui.add(egui::DragValue::new(&mut self.settings.dispatch_rate_ms).range(1..=100));
        });
    }
}
