//! Minimal config + profile loader for prototype.
//! JSON for app->profile map and global defaults. Layouts still loaded via LayoutLoader trait (DvorakJ files).

use crate::keycode::KeyCode;
use crate::profile::{AppProfileMap, Profile, ProfileId, ProfileToggles};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub profiles: HashMap<ProfileId, ProfileDef>,
    pub app_map: AppProfileMap,
    pub default_layout: String, // layout file path or id for the default profile
    /// FR-6: keys that disable remapping while held (e.g. ["ctrl","alt","win"]).
    /// Generic modifier names expand to both L/R variants. Empty = feature off.
    #[serde(default)]
    pub disable_keys: Vec<String>,
    /// When true, write diagnostic/event lines (English) to a `log` file next
    /// to the daemon executable. Default off.
    #[serde(default)]
    pub enable_log: bool,
    /// When true, remapping is active only while the IME is ON (Japanese
    /// conversion mode); while the IME is OFF (direct alphanumeric) every key
    /// passes through. Default false = always active regardless of IME state.
    #[serde(default, alias = "activate_only_when_ime_off")]
    pub activate_only_when_ime_on: bool,
    /// Optional alternate layout file used while the IME is OFF, when
    /// `activate_only_when_ime_on` is true. Empty (default) keeps the old
    /// behaviour: pass everything through while the IME is OFF.
    #[serde(default)]
    pub ime_off_layout: String,
    /// SandS direct-input key (e.g. "shift", "muhenkan" — same names as
    /// `disable_keys`). Which physical key, while held, triggers
    /// `direct_input_mode`. Empty (default) -> no key configured.
    #[serde(default)]
    pub direct_input_key: String,
    /// SandS: what holding `direct_input_key` does.
    /// - `"off"` (default): nothing — `direct_input_key` is inert.
    /// - `"raw"` ("オン（物理レイアウト）"): while held, fully bypass remapping
    ///   (raw physical-keyboard input), even while the IME is ON.
    /// - `"ime_off"` ("オン（IMEオフ）"): while held, switch the active layout
    ///   to `ime_off_layout` (even while the IME is ON). If `ime_off_layout`
    ///   is unset, falls back to `"raw"` behaviour.
    #[serde(default)]
    pub direct_input_mode: String,
    /// Simultaneous-press (chord) detection window, in milliseconds. Keys
    /// pressed within this window of each other are treated as a chord
    /// rather than separate taps. Default 40ms (matches the matcher's
    /// built-in default) when unset or 0.
    #[serde(default)]
    pub combo_window_ms: u64,
    /// When true, a 同時打鍵 key resolved as a solo tap (no partner within
    /// `combo_window_ms`) keeps repeating its output for as long as it's
    /// physically held, like a normal key ("ホールド扱い"). When false
    /// (default), it is emitted once regardless of how long it's held
    /// ("単打扱い").
    #[serde(default)]
    pub hold_mode: bool,
    /// SandS (Space and Shift): whether the layout's sustained while-held
    /// layer triggers (declared via `-option-input`, e.g. tap Space for a
    /// space / hold Space for Shift) are active while the IME is ON. Default
    /// true. When false, those keys behave as ordinary keys (no layer/tap
    /// distinction).
    #[serde(default = "default_true")]
    pub enable_sands_ime_on: bool,
    /// Same as `enable_sands_ime_on`, but while the IME is OFF. Default true.
    #[serde(default = "default_true")]
    pub enable_sands_ime_off: bool,
    /// Internal flush-timer dispatch rate, in milliseconds: how often the
    /// resident timer thread wakes to flush a pending chord (and, less often,
    /// poll the IME state). Default 5ms. Lower = finer-grained dispatch (helps
    /// avoid the "すり抜け" symptom on older CPUs at the cost of more wakeups);
    /// higher = fewer wakeups. 0 or unset falls back to the 5ms default.
    #[serde(default = "default_dispatch_rate_ms")]
    pub dispatch_rate_ms: u64,
    /// Sequential-input (prefix) detection window, in milliseconds. After a
    /// prefix trigger key's combo window expires without a simultaneous
    /// partner, the engine waits this long for a follow-up content key before
    /// falling back to the trigger's solo tap. Default 300ms.
    #[serde(default = "default_prefix_window_ms")]
    pub prefix_window_ms: u64,
    /// When true, pressing Ctrl+Space toggles the IME open/close state
    /// (instead of producing a space). Default false.
    #[serde(default)]
    pub enable_ctrl_space_ime_toggle: bool,
}

fn default_prefix_window_ms() -> u64 {
    300
}

fn default_true() -> bool {
    true
}

/// Default flush-timer dispatch rate (ms); mirrors the historical hard-coded
/// 5ms tick of the chord flush thread.
fn default_dispatch_rate_ms() -> u64 {
    5
}

/// Expand a config key name to the concrete `KeyCode`s it covers. Generic
/// modifier names ("ctrl"/"alt"/"win"/"shift"/"meta") expand to both L and R
/// variants so a user need not care which physical modifier they press;
/// everything else falls back to the DvorakJ name table. Unknown names yield
/// an empty Vec (caller decides whether to warn/ignore).
pub fn keycodes_from_config_name(name: &str) -> Vec<KeyCode> {
    match name.trim().to_lowercase().as_str() {
        "ctrl" | "control" => vec![KeyCode::CtrlL, KeyCode::CtrlR],
        "alt" | "menu" => vec![KeyCode::AltL, KeyCode::AltR],
        "win" | "meta" | "super" | "cmd" => vec![KeyCode::MetaL, KeyCode::MetaR],
        "shift" => vec![KeyCode::ShiftL, KeyCode::ShiftR],
        other => KeyCode::from_dvorakj_name(other).into_iter().collect(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileDef {
    pub layout: String, // path to DvorakJ .txt or later sloth-native
    #[serde(default)]
    pub toggles: ProfileToggles,
}

impl AppConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let s = std::fs::read_to_string(path)?;
        // Strip a UTF-8 BOM: Notepad and PowerShell's `Set-Content -Encoding
        // utf8` both write one, which serde_json otherwise rejects.
        let s = s.strip_prefix('\u{feff}').unwrap_or(&s);
        let cfg: AppConfig = serde_json::from_str(s)?;
        Ok(cfg)
    }

    /// Construct a minimal fallback config pointing at the toy SandS sample.
    /// Used when data/config.json is missing or unreadable (NFR-4 fail-fast otherwise).
    pub fn fallback() -> Self {
        use std::collections::HashMap;
        let mut profiles = HashMap::new();
        profiles.insert(
            "default".to_string(),
            ProfileDef {
                layout: "data/layouts/samples/toy_simul.txt".to_string(),
                toggles: ProfileToggles {
                    enable_sands: true,
                    enable_gestures: false,
                    enable_shortcuts: false,
                },
            },
        );
        profiles.insert(
            "colemak".to_string(),
            ProfileDef {
                layout: "data/layouts/samples/toy_simul.txt".to_string(),
                toggles: ProfileToggles {
                    enable_sands: false,
                    enable_gestures: false,
                    enable_shortcuts: false,
                },
            },
        );
        AppConfig {
            profiles,
            app_map: AppProfileMap {
                per_app: HashMap::new(),
                default_profile: "default".to_string(),
            },
            default_layout: "data/layouts/samples/toy_simul.txt".to_string(),
            disable_keys: Vec::new(),
            enable_log: false,
            activate_only_when_ime_on: false,
            ime_off_layout: String::new(),
            direct_input_key: String::new(),
            direct_input_mode: String::new(),
            combo_window_ms: 0,
            hold_mode: false,
            enable_sands_ime_on: true,
            enable_sands_ime_off: true,
            dispatch_rate_ms: default_dispatch_rate_ms(),
            prefix_window_ms: default_prefix_window_ms(),
            enable_ctrl_space_ime_toggle: false,
        }
    }

    /// FR-6: resolve `disable_keys` names to a flat KeyCode set for the matcher.
    pub fn disable_keycodes(&self) -> Vec<KeyCode> {
        self.disable_keys
            .iter()
            .flat_map(|n| keycodes_from_config_name(n))
            .collect()
    }

    pub fn default_profile(&self) -> Option<Profile> {
        let id = &self.app_map.default_profile;
        self.profiles.get(id).map(|p| Profile {
            id: id.clone(),
            layout_id: p.layout.clone(),
            toggles: p.toggles.clone(),
        })
    }

    /// Resolve the layout file path for a given app_id (from per_app map or default_profile).
    /// Returns default_layout if profile missing.
    pub fn layout_path_for_app(&self, app_id: &str) -> String {
        let prof_id = self
            .app_map
            .per_app
            .get(app_id)
            .unwrap_or(&self.app_map.default_profile);
        let raw = self
            .profiles
            .get(prof_id)
            .map(|p| p.layout.clone())
            .unwrap_or_else(|| self.default_layout.clone());
        resolve_layout_path(&raw)
    }
}

/// Resolve a layout path from config to an actual filesystem path.
/// Handles relative paths by trying: as-is → prepend "data/" → as-is from exe dir.
pub fn resolve_layout_path(raw: &str) -> String {
    use std::path::Path;
    let p = Path::new(raw);
    // Absolute path: use as-is
    if p.is_absolute() {
        return raw.to_string();
    }
    // Try as-is (relative to cwd)
    if p.exists() {
        return raw.to_string();
    }
    // Try prepending "data/" (config is in data/, layouts are in data/layouts/)
    let with_prefix = format!("data/{}", raw);
    if Path::new(&with_prefix).exists() {
        return with_prefix;
    }
    // Return raw as-is (fallback will handle missing file)
    raw.to_string()
}
