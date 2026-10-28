use eframe::egui::Ui;

impl crate::app::App {
    pub fn disable_keys(&mut self, ui: &mut Ui) {
        ui.checkbox(&mut self.settings.disable_ctrl, "Ctrl");
        ui.checkbox(&mut self.settings.disable_alt, "Alt");
        ui.checkbox(&mut self.settings.disable_win, "Win");
        ui.checkbox(&mut self.settings.disable_shift, "Shift");
    }
}
