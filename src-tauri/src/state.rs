//! Persisted application state (%APPDATA%\...\state.json).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

fn default_bass_fc() -> f32 {
    350.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignedProfile {
    pub source: String,
    pub form: String,
    pub name: String,
    pub has_ir: bool,
    /// Low-shelf boost applied after the convolver (0 = off).
    #[serde(default)]
    pub bass_gain_db: f32,
    /// Low-shelf corner frequency in Hz.
    #[serde(default = "default_bass_fc")]
    pub bass_fc: f32,
}

fn default_headroom() -> f32 {
    -3.0
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub master_enabled: bool,
    /// Negative dB applied as an EAPO Preamp line on every device section,
    /// protecting against intersample peaks / hot masters.
    #[serde(default = "default_headroom")]
    pub headroom_db: f32,
    /// Automatically subtract each device's positive bass-shelf gain from its
    /// pre-gain, so the shelf can never introduce clipping.
    #[serde(default = "default_true")]
    pub compensate_shelf: bool,
    /// endpoint GUID -> assigned profile
    pub assignments: HashMap<String, AssignedProfile>,
    /// Cached display names for devices we have assignments on, so the UI can
    /// label profiles even when the device is currently unplugged.
    #[serde(default)]
    pub device_names: HashMap<String, String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            master_enabled: true,
            headroom_db: default_headroom(),
            compensate_shelf: true,
            assignments: HashMap::new(),
            device_names: HashMap::new(),
        }
    }
}

pub fn state_file(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("state.json")
}

pub fn load(app_data_dir: &Path) -> AppState {
    fs::read_to_string(state_file(app_data_dir))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(app_data_dir: &Path, state: &AppState) -> Result<()> {
    fs::create_dir_all(app_data_dir).context("create app data dir")?;
    let raw = serde_json::to_string_pretty(state)?;
    fs::write(state_file(app_data_dir), raw).context("write state.json")?;
    Ok(())
}
