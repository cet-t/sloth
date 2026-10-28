use eframe::egui::{self, Ui};

use crate::{
    app::OpResult,
    daemon::{self, DaemonOp},
};

impl crate::app::App {
    fn spawn_op(&mut self, op: DaemonOp) {
        if self.op_in_flight {
            return;
        }
        self.op_in_flight = true;
        self.status_text = "処理中...".to_owned();
        let was_suspended = self.daemon.suspended;
        let tx = self.op_tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let status_text = daemon::run_op(op, was_suspended);
            let _ = tx.send(OpResult::new(status_text, daemon::query_status()));
            ctx.request_repaint();
        });
    }

    pub fn process(&mut self, ui: &mut Ui) {
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
