use std::path::Path;

use eframe::egui::Ui;

impl crate::app::App {
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
        if Self::open_with_default_app(&log_path) {
            String::new()
        } else {
            "ログファイルを開けませんでした".to_string()
        }
    }

    fn open_with_default_app(path: &Path) -> bool {
        #[cfg(windows)]
        {
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
        {
            false
        }
    }

    pub fn log(&mut self, ui: &mut Ui) {
        ui.checkbox(
            &mut self.settings.enable_log,
            "ログを有効にする (実行ファイルと同じフォルダの ./log に出力)",
        );
        if ui.button("ログを開く").clicked() {
            self.status_text = Self::open_log_file();
        }
    }
}
