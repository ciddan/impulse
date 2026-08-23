//! Equalizer APO backend: detection, per-device enablement status, and
//! generation of the managed config file that EAPO hot-reloads (~1s).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

const MANAGED_DIR: &str = "Impulse";
const MANAGED_FILE: &str = "Impulse.txt";
const INCLUDE_LINE: &str = "Include: Impulse\\Impulse.txt";
/// Pre-rename artifacts (app was briefly called WinAutoConv) to migrate away.
const LEGACY_INCLUDE_LINE: &str = "Include: WinAutoConv\\WinAutoConv.txt";
const LEGACY_DIR: &str = "WinAutoConv";

#[derive(Debug, Clone, Serialize)]
pub struct EapoStatus {
    pub installed: bool,
    pub config_dir: Option<String>,
    pub install_dir: Option<String>,
    /// True once our Include line is present in config.txt
    pub include_installed: bool,
}

pub fn detect() -> EapoStatus {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = match hklm
        .open_subkey("SOFTWARE\\EqualizerAPO")
        .or_else(|_| hklm.open_subkey("SOFTWARE\\WOW6432Node\\EqualizerAPO"))
    {
        Ok(k) => k,
        Err(_) => {
            return EapoStatus {
                installed: false,
                config_dir: None,
                install_dir: None,
                include_installed: false,
            }
        }
    };

    let install_dir: Option<String> = key.get_value::<String, _>("InstallPath").ok();
    let config_dir: Option<String> = key.get_value::<String, _>("ConfigPath").ok().or_else(|| {
        install_dir
            .as_ref()
            .map(|p| format!("{}\\config", p.trim_end_matches('\\')))
    });

    let config_dir = config_dir.filter(|p| Path::new(p).is_dir());
    let include_installed = config_dir
        .as_deref()
        .map(|dir| {
            fs::read_to_string(Path::new(dir).join("config.txt"))
                .map(|c| {
                    c.lines()
                        .any(|l| l.trim().eq_ignore_ascii_case(INCLUDE_LINE))
                })
                .unwrap_or(false)
        })
        .unwrap_or(false);

    EapoStatus {
        installed: config_dir.is_some(),
        config_dir,
        install_dir,
        include_installed,
    }
}

/// CLSIDs of EqualizerAPO's registered audio processing objects, resolved
/// from the system APO registry (falls back to the known stable GUIDs).
fn eapo_apo_clsids() -> Vec<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut ids = Vec::new();
    if let Ok(key) = hklm.open_subkey("SOFTWARE\\Classes\\AudioEngine\\AudioProcessingObjects") {
        for name in key.enum_keys().flatten() {
            if let Ok(sub) = key.open_subkey(&name) {
                let friendly: String = sub.get_value("FriendlyName").unwrap_or_default();
                if friendly.eq_ignore_ascii_case("EqualizerAPO") {
                    ids.push(name.to_lowercase());
                }
            }
        }
    }
    if ids.is_empty() {
        ids.push("{eacd2258-fcac-4ff4-b36d-419e924a6d79}".to_string());
        ids.push("{ec1cc9ce-faed-4822-828a-82a81a6f018f}".to_string());
    }
    ids
}

/// Is EAPO's APO *actually* active on this endpoint? The authoritative record
/// is the endpoint's FxProperties key referencing an EAPO CLSID — EAPO's own
/// "Child APOs" bookkeeping survives Windows updates that wipe FxProperties,
/// so it can claim devices that are no longer processed at all.
pub fn device_enabled(endpoint_guid: &str) -> bool {
    use winreg::types::FromRegValue;

    let clsids = eapo_apo_clsids();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    // NB: endpoint registry lives under SOFTWARE, not SYSTEM\CurrentControlSet
    // (verified against EqualizerAPO's DeviceAPOInfo.cpp and a live system).
    let path = format!(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render\\{}\\FxProperties",
        endpoint_guid.to_lowercase()
    );
    let Ok(key) = hklm.open_subkey(path) else {
        return false;
    };
    for (_, value) in key.enum_values().flatten() {
        if let Ok(s) = String::from_reg_value(&value) {
            let s = s.to_lowercase();
            if clsids.iter().any(|c| s.contains(c)) {
                return true;
            }
        }
    }
    false
}

pub fn managed_dir(config_dir: &str) -> PathBuf {
    Path::new(config_dir).join(MANAGED_DIR)
}

pub fn profiles_dir(config_dir: &str) -> PathBuf {
    managed_dir(config_dir).join("profiles")
}

/// Make sure config.txt pulls in our managed file. Idempotent; also migrates
/// away any pre-rename include line and directory.
pub fn ensure_include(config_dir: &str) -> Result<()> {
    let config_txt = Path::new(config_dir).join("config.txt");
    let mut current = fs::read_to_string(&config_txt).unwrap_or_default();

    // Legacy migration: drop the old include line and remove the old dir.
    if current
        .lines()
        .any(|l| l.trim().eq_ignore_ascii_case(LEGACY_INCLUDE_LINE))
    {
        current = current
            .lines()
            .filter(|l| !l.trim().eq_ignore_ascii_case(LEGACY_INCLUDE_LINE))
            .collect::<Vec<_>>()
            .join("\n");
        if !current.is_empty() {
            current.push('\n');
        }
        fs::write(&config_txt, &current).ok();
    }
    let legacy_dir = Path::new(config_dir).join(LEGACY_DIR);
    if legacy_dir.is_dir() {
        fs::remove_dir_all(&legacy_dir).ok();
    }

    if current
        .lines()
        .any(|l| l.trim().eq_ignore_ascii_case(INCLUDE_LINE))
    {
        return Ok(());
    }
    let mut updated = current;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(INCLUDE_LINE);
    updated.push('\n');
    fs::write(&config_txt, updated).with_context(|| {
        format!(
            "Could not write {}. Equalizer APO's config directory is normally writable \
             without elevation; check permissions.",
            config_txt.display()
        )
    })?;
    Ok(())
}

/// One device's rendered section of the managed config.
pub struct DeviceSection {
    /// EAPO Device: pattern — we use the endpoint GUID, which is unique.
    pub device_pattern: String,
    /// Human-readable comment (device + profile names).
    pub comment: String,
    /// Filter lines, e.g. "Convolution: profiles\\X\\minphase_48000.wav"
    pub lines: Vec<String>,
}

/// Regenerate the managed file. With `enabled == false` an empty (comment-only)
/// file is written, which EAPO hot-reloads to bypass all our filters.
pub fn write_managed_config(
    config_dir: &str,
    enabled: bool,
    sections: &[DeviceSection],
) -> Result<()> {
    let dir = managed_dir(config_dir);
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;

    let mut out = String::new();
    // ASCII only: Equalizer APO reads config files in the ANSI codepage.
    out.push_str("# Generated by Impulse. Do not edit - changes will be overwritten.\n");
    if !enabled {
        out.push_str("# EQ is currently disabled from Impulse.\n");
    } else {
        for s in sections {
            out.push('\n');
            out.push_str(&format!("# {}\n", s.comment));
            out.push_str(&format!("Device: {}\n", s.device_pattern));
            for line in &s.lines {
                out.push_str(line);
                out.push('\n');
            }
        }
        // Close the last Device scope so nothing leaks onto other devices.
        if !sections.is_empty() {
            out.push_str("\nDevice: 00000000-none-ffff-ffff-000000000000\n");
        }
    }

    let file = dir.join(MANAGED_FILE);
    fs::write(&file, out).with_context(|| format!("write {}", file.display()))?;
    Ok(())
}

/// Copy a profile's files into the EAPO config dir so audiodg can read them.
/// Returns the directory the files were placed in.
pub fn install_profile_files(
    config_dir: &str,
    profile_key: &str,
    files: &[(String, Vec<u8>)],
) -> Result<PathBuf> {
    if files.is_empty() {
        bail!("no profile files to install");
    }
    let dir = profiles_dir(config_dir).join(profile_key);
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    for (name, bytes) in files {
        let path = dir.join(name);
        fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
    }
    Ok(dir)
}

/// Filesystem-safe key for a profile.
pub fn sanitize_key(source: &str, name: &str) -> String {
    let raw = format!("{}__{}", source, name);
    raw.chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}
