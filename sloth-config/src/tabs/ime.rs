use eframe::egui::Ui;

impl crate::app::App {
    pub fn ime(&mut self, ui: &mut Ui) {
        ui.checkbox(
            &mut self.settings.enable_ctrl_space_ime_toggle,
            "Ctrl + Space で IME のオン/オフを切り替える",
        );
    }
}
