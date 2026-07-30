//! The editable form state, decoupled from `AppConfig`: loaded from a config
//! on startup and written back on 保存. `PartialEq` is what drives the dirty
//! flag -- the form is dirty exactly when it differs from the last saved
//! snapshot.

use sloth_core::config::AppConfig;

/// `disable_keys` entries the UI has dedicated checkboxes for. Anything else in
/// the config's list is preserved verbatim (see [`Settings::custom_disable_keys`]).
const KNOWN_DISABLE_KEYS: [&str; 9] = [
    "ctrl", "control", "alt", "menu", "win", "meta", "super", "cmd", "shift",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DirectInputKey {
    None,
    Shift,
    Muhenkan,
    Henkan,
    CapsLock,
}

impl DirectInputKey {
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Shift,
        Self::Muhenkan,
        Self::Henkan,
        Self::CapsLock,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "なし",
            Self::Shift => "Shift",
            Self::Muhenkan => "無変換",
            Self::Henkan => "変換",
            Self::CapsLock => "CapsLock",
        }
    }

    fn from_config(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "shift" | "lshift" => Self::Shift,
            "muhenkan" => Self::Muhenkan,
            "henkan" => Self::Henkan,
            "capslock" | "caps" => Self::CapsLock,
            _ => Self::None,
        }
    }

    fn to_config(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Shift => "shift",
            Self::Muhenkan => "muhenkan",
            Self::Henkan => "henkan",
            Self::CapsLock => "capslock",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SandsMode {
    Off,
    Raw,
    ImeOff,
}

impl SandsMode {
    pub const ALL: [Self; 3] = [Self::Off, Self::Raw, Self::ImeOff];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "オフ",
            Self::Raw => "オン（物理レイアウト）",
            Self::ImeOff => "オン（IMEオフ）",
        }
    }

    fn from_config(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "raw" => Self::Raw,
            "ime_off" => Self::ImeOff,
            _ => Self::Off,
        }
    }

    fn to_config(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Raw => "raw",
            Self::ImeOff => "ime_off",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Settings {
    pub default_layout: String,
    pub ime_off_layout: String,
    pub combo_window_ms: u64,
    pub activate_only_when_ime_on: bool,
    pub hold_mode: bool,
    pub direct_input_key: DirectInputKey,
    pub sands_mode: SandsMode,
    pub enable_sands_ime_on: bool,
    pub enable_sands_ime_off: bool,
    pub dispatch_rate_ms: u64,
    pub disable_ctrl: bool,
    pub disable_alt: bool,
    pub disable_win: bool,
    pub disable_shift: bool,
    pub enable_log: bool,
    pub enable_ctrl_space_ime_toggle: bool,
    /// `disable_keys` entries with no checkbox of their own, kept so saving
    /// doesn't drop hand-written config.
    custom_disable_keys: Vec<String>,
}

impl Settings {
    pub fn from_config(cfg: &AppConfig) -> Self {
        // The default profile's layout is what the engine actually loads;
        // `default_layout` is only the fallback when that profile is missing.
        let effective_layout = cfg
            .profiles
            .get(&cfg.app_map.default_profile)
            .map(|p| p.layout.clone())
            .unwrap_or_else(|| cfg.default_layout.clone());

        let lowered: Vec<String> = cfg
            .disable_keys
            .iter()
            .map(|s| s.trim().to_lowercase())
            .collect();
        let has = |names: &[&str]| lowered.iter().any(|s| names.contains(&s.as_str()));

        Self {
            default_layout: effective_layout,
            ime_off_layout: cfg.ime_off_layout.clone(),
            combo_window_ms: if cfg.combo_window_ms > 0 {
                cfg.combo_window_ms
            } else {
                40
            },
            activate_only_when_ime_on: cfg.activate_only_when_ime_on,
            hold_mode: cfg.hold_mode,
            direct_input_key: DirectInputKey::from_config(&cfg.direct_input_key),
            sands_mode: SandsMode::from_config(&cfg.direct_input_mode),
            enable_sands_ime_on: cfg.enable_sands_ime_on,
            enable_sands_ime_off: cfg.enable_sands_ime_off,
            dispatch_rate_ms: if cfg.dispatch_rate_ms > 0 {
                cfg.dispatch_rate_ms
            } else {
                5
            },
            disable_ctrl: has(&["ctrl", "control"]),
            disable_alt: has(&["alt", "menu"]),
            disable_win: has(&["win", "meta", "super", "cmd"]),
            disable_shift: has(&["shift"]),
            enable_log: cfg.enable_log,
            enable_ctrl_space_ime_toggle: cfg.enable_ctrl_space_ime_toggle,
            custom_disable_keys: cfg
                .disable_keys
                .iter()
                .filter(|s| !KNOWN_DISABLE_KEYS.contains(&s.trim().to_lowercase().as_str()))
                .cloned()
                .collect(),
        }
    }

    /// Writes the form back onto `cfg`, leaving every field the window doesn't
    /// expose (profiles, app map, prefix window, ...) untouched.
    pub fn apply_to(&self, cfg: &mut AppConfig) {
        cfg.default_layout = self.default_layout.clone();
        cfg.ime_off_layout = self.ime_off_layout.clone();
        cfg.combo_window_ms = self.combo_window_ms.max(1);
        cfg.activate_only_when_ime_on = self.activate_only_when_ime_on;
        cfg.hold_mode = self.hold_mode;
        cfg.direct_input_key = self.direct_input_key.to_config().to_string();
        cfg.direct_input_mode = self.sands_mode.to_config().to_string();
        cfg.enable_sands_ime_on = self.enable_sands_ime_on;
        cfg.enable_sands_ime_off = self.enable_sands_ime_off;
        cfg.dispatch_rate_ms = self.dispatch_rate_ms.max(1);
        cfg.enable_log = self.enable_log;
        cfg.enable_ctrl_space_ime_toggle = self.enable_ctrl_space_ime_toggle;

        let mut disable_keys = self.custom_disable_keys.clone();
        if self.disable_ctrl {
            disable_keys.push("ctrl".to_string());
        }
        if self.disable_alt {
            disable_keys.push("alt".to_string());
        }
        if self.disable_win {
            disable_keys.push("win".to_string());
        }
        if self.disable_shift {
            disable_keys.push("shift".to_string());
        }
        cfg.disable_keys = disable_keys;

        // The engine reads the default profile's layout, so keep it in sync
        // with the layout the window edits.
        let default_profile = cfg.app_map.default_profile.clone();
        if let Some(p) = cfg.profiles.get_mut(&default_profile) {
            p.layout = self.default_layout.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> AppConfig {
        let mut cfg = AppConfig::fallback();
        cfg.disable_keys = vec!["ctrl".into(), "f13".into()];
        cfg.direct_input_key = "muhenkan".into();
        cfg.direct_input_mode = "ime_off".into();
        cfg.combo_window_ms = 60;
        cfg.dispatch_rate_ms = 7;
        cfg
    }

    /// Loading a config into the form and saving it straight back must not
    /// change anything -- that round trip runs on every 保存.
    #[test]
    fn round_trip_is_lossless() {
        let cfg = sample_config();
        let mut applied = cfg.clone();
        Settings::from_config(&cfg).apply_to(&mut applied);

        assert_eq!(applied.direct_input_key, cfg.direct_input_key);
        assert_eq!(applied.direct_input_mode, cfg.direct_input_mode);
        assert_eq!(applied.combo_window_ms, cfg.combo_window_ms);
        assert_eq!(applied.dispatch_rate_ms, cfg.dispatch_rate_ms);
        // Order isn't meaningful, but nothing may be lost or duplicated.
        let mut before = cfg.disable_keys.clone();
        let mut after = applied.disable_keys.clone();
        before.sort();
        after.sort();
        assert_eq!(after, before);
    }

    #[test]
    fn unknown_disable_keys_survive_saving() {
        let cfg = sample_config();
        let mut settings = Settings::from_config(&cfg);
        settings.disable_ctrl = false;
        let mut applied = cfg.clone();
        settings.apply_to(&mut applied);
        assert_eq!(applied.disable_keys, vec!["f13".to_string()]);
    }

    #[test]
    fn zero_values_fall_back_to_defaults() {
        let mut cfg = AppConfig::fallback();
        cfg.combo_window_ms = 0;
        cfg.dispatch_rate_ms = 0;
        let settings = Settings::from_config(&cfg);
        assert_eq!(settings.combo_window_ms, 40);
        assert_eq!(settings.dispatch_rate_ms, 5);
    }
}
