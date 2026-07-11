//! Profile: layout + toggles. Per-app state in daemon.

use crate::layout::LayoutId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: ProfileId,
    pub layout_id: LayoutId,
    pub toggles: ProfileToggles,
}

pub type ProfileId = String;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileToggles {
    pub enable_sands: bool, // Space as Shift
    pub enable_gestures: bool,
    pub enable_shortcuts: bool,
    // direct/japanese mode is orthogonal global toggle, not per-profile (per plan)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppProfileMap {
    /// AppId -> ProfileId. Global default if no entry.
    pub per_app: std::collections::HashMap<String, ProfileId>,
    pub default_profile: ProfileId,
}
