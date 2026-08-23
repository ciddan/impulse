mod autoeq;
mod devices;
mod eapo;
mod state;

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;

use state::{AppState, AssignedProfile};

struct Shared {
    state: Mutex<AppState>,
}

struct TrayHandles {
    toggle: CheckMenuItem<tauri::Wry>,
}

/// Whether the Mica backdrop rendered (false = frontend paints solid).
struct BackdropState(bool);

#[derive(Serialize)]
struct DeviceView {
    #[serde(flatten)]
    device: devices::AudioDevice,
    apo_enabled: bool,
    assignment: Option<AssignedProfile>,
}

#[derive(Serialize)]
struct AppStatus {
    eapo: eapo::EapoStatus,
    devices: Vec<DeviceView>,
    /// Assignments for devices that are not currently connected.
    offline_assignments: Vec<OfflineAssignment>,
    master_enabled: bool,
    headroom_db: f32,
    compensate_shelf: bool,
    autostart: bool,
    mica: bool,
    /// System accent colors as "#RRGGBB": (base, light variant, dark variant).
    accent: Option<String>,
    accent_light: Option<String>,
    accent_dark: Option<String>,
}

/// Read the Windows accent color and its light/dark variants.
fn system_accent() -> (Option<String>, Option<String>, Option<String>) {
    use windows::UI::ViewManagement::{UIColorType, UISettings};
    let Ok(ui) = UISettings::new() else {
        return (None, None, None);
    };
    let get = |t: UIColorType| {
        ui.GetColorValue(t)
            .ok()
            .map(|c| format!("#{:02X}{:02X}{:02X}", c.R, c.G, c.B))
    };
    (
        get(UIColorType::Accent),
        get(UIColorType::AccentLight2),
        get(UIColorType::AccentDark1),
    )
}

#[derive(Serialize)]
struct OfflineAssignment {
    device_guid: String,
    device_name: String,
    assignment: AssignedProfile,
}

type CmdResult<T> = Result<T, String>;

fn err_str(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(err_str)
}

/// Rebuild the managed EAPO config from current state + live device rates.
fn regenerate(app: &AppHandle) -> Result<(), String> {
    let eapo_status = eapo::detect();
    let Some(config_dir) = eapo_status.config_dir.as_deref() else {
        return Ok(()); // Nothing to write without an EAPO install.
    };

    let shared = app.state::<Shared>();
    let st = shared.state.lock().unwrap().clone();
    let devices = devices::list_render_devices().unwrap_or_default();

    let mut sections = Vec::new();
    for (guid, profile) in &st.assignments {
        let live = devices.iter().find(|d| d.guid.eq_ignore_ascii_case(guid));
        let key = eapo::sanitize_key(&profile.source, &profile.name);
        let profile_dir = eapo::profiles_dir(config_dir).join(&key);

        let rate = live.map(|d| d.sample_rate);
        let ir_file = match rate {
            Some(44100) => Some("minphase_44100.wav"),
            Some(48000) => Some("minphase_48000.wav"),
            _ => None,
        };

        // Convolution or nothing: without an IR matching the device's mix
        // rate, the device gets no processing (the UI explains why).
        let Some(ir_file) = ir_file else {
            continue;
        };
        if !profile_dir.join(ir_file).is_file() {
            continue;
        }

        let mut lines = Vec::new();
        // Pre-gain: user headroom, plus automatic compensation for the bass
        // shelf's positive gain so the shelf can never introduce clipping.
        let mut preamp = st.headroom_db;
        if st.compensate_shelf {
            preamp -= profile.bass_gain_db.max(0.0);
        }
        if preamp < 0.0 {
            lines.push(format!("Preamp: {:.1} dB", preamp));
        }
        lines.push(format!("Convolution: profiles\\{}\\{}", key, ir_file));
        // Bass shelf goes after the convolver — EAPO applies filters in
        // config order.
        if profile.bass_gain_db.abs() >= 0.05 {
            lines.push(format!(
                "Filter 1: ON LS Fc {:.0} Hz Gain {:.1} dB",
                profile.bass_fc, profile.bass_gain_db
            ));
        }

        let device_label = live
            .map(|d| d.name.clone())
            .or_else(|| st.device_names.get(guid).cloned())
            .unwrap_or_else(|| guid.clone());
        // Two alternatives (EAPO ORs on ';'): the endpoint GUID, plus the
        // device name reduced to bare words ("Speakers (Schiit USB Multibit)"
        // -> "speakers schiit usb multibit") since EAPO's device string has
        // no punctuation and patterns match per-word as substrings.
        let bare_guid = guid.trim_matches(|c| c == '{' || c == '}').to_lowercase();
        let name_words: String = device_label
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let device_pattern = if name_words.is_empty() {
            bare_guid
        } else {
            format!("{}; {}", bare_guid, name_words)
        };
        sections.push(eapo::DeviceSection {
            device_pattern,
            comment: format!("{} -> {} ({})", device_label, profile.name, profile.source),
            lines,
        });
    }

    eapo::write_managed_config(config_dir, st.master_enabled, &sections).map_err(err_str)?;
    Ok(())
}

fn build_status(app: &AppHandle) -> Result<AppStatus, String> {
    let eapo_status = eapo::detect();
    let shared = app.state::<Shared>();
    let st = shared.state.lock().unwrap().clone();
    let devs = devices::list_render_devices().map_err(err_str)?;

    let device_views: Vec<DeviceView> = devs
        .iter()
        .map(|d| DeviceView {
            apo_enabled: eapo_status.installed && eapo::device_enabled(&d.guid),
            assignment: st
                .assignments
                .iter()
                .find(|(g, _)| g.eq_ignore_ascii_case(&d.guid))
                .map(|(_, p)| p.clone()),
            device: d.clone(),
        })
        .collect();

    let offline_assignments = st
        .assignments
        .iter()
        .filter(|(g, _)| !devs.iter().any(|d| d.guid.eq_ignore_ascii_case(g)))
        .map(|(g, p)| OfflineAssignment {
            device_guid: g.clone(),
            device_name: st.device_names.get(g).cloned().unwrap_or_else(|| g.clone()),
            assignment: p.clone(),
        })
        .collect();

    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    let (accent, accent_light, accent_dark) = system_accent();

    Ok(AppStatus {
        eapo: eapo_status,
        devices: device_views,
        offline_assignments,
        master_enabled: st.master_enabled,
        headroom_db: st.headroom_db,
        compensate_shelf: st.compensate_shelf,
        autostart,
        mica: app
            .try_state::<BackdropState>()
            .map(|s| s.0)
            .unwrap_or(false),
        accent,
        accent_light,
        accent_dark,
    })
}

fn sync_tray(app: &AppHandle, enabled: bool) {
    if let Some(handles) = app.try_state::<TrayHandles>() {
        let _ = handles.toggle.set_checked(enabled);
    }
}

#[tauri::command]
fn get_status(app: AppHandle) -> CmdResult<AppStatus> {
    // Opportunistically resync the managed config (device rates change, etc.).
    let _ = regenerate(&app);
    build_status(&app)
}

#[tauri::command]
async fn get_index(app: AppHandle, force: bool) -> CmdResult<autoeq::IndexCache> {
    let dir = app_data_dir(&app)?;
    autoeq::get_index(&dir, force).await.map_err(err_str)
}

#[tauri::command]
async fn apply_profile(
    app: AppHandle,
    device_guid: String,
    device_name: String,
    source: String,
    form: String,
    name: String,
    has_ir: bool,
) -> CmdResult<AppStatus> {
    let eapo_status = eapo::detect();
    let config_dir = eapo_status
        .config_dir
        .clone()
        .ok_or("Equalizer APO is not installed")?;

    // Download outside any lock.
    let files = autoeq::download_profile(&source, &form, &name)
        .await
        .map_err(err_str)?;

    let key = eapo::sanitize_key(&source, &name);
    let mut to_write: Vec<(String, Vec<u8>)> = files.irs.iter().cloned().collect();
    to_write.extend(files.extras.iter().cloned());
    eapo::install_profile_files(&config_dir, &key, &to_write).map_err(err_str)?;
    eapo::ensure_include(&config_dir).map_err(err_str)?;

    {
        let shared = app.state::<Shared>();
        let mut st = shared.state.lock().unwrap();
        // Keep the device's bass shelf when swapping profiles.
        let (bass_gain_db, bass_fc) = st
            .assignments
            .get(&device_guid)
            .map(|p| (p.bass_gain_db, p.bass_fc))
            .unwrap_or((0.0, 350.0));
        st.assignments.insert(
            device_guid.clone(),
            AssignedProfile {
                source,
                form,
                name,
                // Kept for state-file compatibility; download_profile fails
                // without IRs, so an assignment always has them.
                has_ir: has_ir || !files.irs.is_empty(),
                bass_gain_db,
                bass_fc,
            },
        );
        st.device_names.insert(device_guid, device_name);
        state::save(&app_data_dir(&app)?, &st).map_err(err_str)?;
    }

    regenerate(&app)?;
    build_status(&app)
}

/// Marker source for user-supplied impulse responses.
const CUSTOM_SOURCE: &str = "custom";

/// Pick a WAV impulse response and assign it to a device. Returns None when
/// the user cancels the picker.
#[tauri::command]
async fn apply_custom_ir(
    app: AppHandle,
    device_guid: String,
    device_name: String,
) -> CmdResult<Option<AppStatus>> {
    use tauri_plugin_dialog::DialogExt;

    let eapo_status = eapo::detect();
    let config_dir = eapo_status
        .config_dir
        .ok_or("Equalizer APO is not installed")?;
    let device_rate = devices::list_render_devices()
        .map_err(err_str)?
        .into_iter()
        .find(|d| d.guid.eq_ignore_ascii_case(&device_guid))
        .map(|d| d.sample_rate)
        .ok_or("Output device not found")?;

    let dialog = app
        .dialog()
        .file()
        .add_filter("Impulse response", &["wav"])
        .set_title("Choose a convolver impulse response");
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file())
        .await
        .map_err(err_str)?;
    let Some(file) = picked else {
        return Ok(None);
    };
    let path = file.into_path().map_err(err_str)?;

    let ir_rate = hound::WavReader::open(&path)
        .map_err(|e| format!("Could not read WAV: {}", e))?
        .spec()
        .sample_rate;
    if ir_rate != 44100 && ir_rate != 48000 {
        return Err(format!(
            "This IR is {} Hz — Equalizer APO's convolver needs 44100 or 48000 Hz",
            ir_rate
        ));
    }
    if ir_rate != device_rate {
        return Err(format!(
            "This IR is {} Hz but \"{}\" mixes at {} Hz. Set the device to {} Hz in Windows \
             sound settings, or use an IR at the device's rate.",
            ir_rate, device_name, device_rate, ir_rate
        ));
    }

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("impulse")
        .to_string();
    let bytes = fs::read(&path).map_err(err_str)?;
    let key = eapo::sanitize_key(CUSTOM_SOURCE, &stem);
    let ir_name = format!("minphase_{}.wav", ir_rate);
    eapo::install_profile_files(&config_dir, &key, &[(ir_name, bytes)]).map_err(err_str)?;
    eapo::ensure_include(&config_dir).map_err(err_str)?;

    {
        let shared = app.state::<Shared>();
        let mut st = shared.state.lock().unwrap();
        let (bass_gain_db, bass_fc) = st
            .assignments
            .get(&device_guid)
            .map(|p| (p.bass_gain_db, p.bass_fc))
            .unwrap_or((0.0, 350.0));
        st.assignments.insert(
            device_guid.clone(),
            AssignedProfile {
                source: CUSTOM_SOURCE.into(),
                form: CUSTOM_SOURCE.into(),
                name: stem,
                has_ir: true,
                bass_gain_db,
                bass_fc,
            },
        );
        st.device_names.insert(device_guid, device_name);
        state::save(&app_data_dir(&app)?, &st).map_err(err_str)?;
    }

    regenerate(&app)?;
    build_status(&app).map(Some)
}

#[tauri::command]
fn clear_profile(app: AppHandle, device_guid: String) -> CmdResult<AppStatus> {
    {
        let shared = app.state::<Shared>();
        let mut st = shared.state.lock().unwrap();
        st.assignments
            .retain(|g, _| !g.eq_ignore_ascii_case(&device_guid));
        st.device_names
            .retain(|g, _| !g.eq_ignore_ascii_case(&device_guid));
        state::save(&app_data_dir(&app)?, &st).map_err(err_str)?;
    }
    regenerate(&app)?;
    build_status(&app)
}

#[tauri::command]
fn set_master(app: AppHandle, enabled: bool) -> CmdResult<AppStatus> {
    {
        let shared = app.state::<Shared>();
        let mut st = shared.state.lock().unwrap();
        st.master_enabled = enabled;
        state::save(&app_data_dir(&app)?, &st).map_err(err_str)?;
    }
    regenerate(&app)?;
    sync_tray(&app, enabled);
    build_status(&app)
}

#[derive(Serialize, Default)]
struct CurveData {
    /// The realized correction curve, measured from the actual IR wav.
    /// Empty when no IR matches the device's mix rate (no processing).
    eq: Vec<(f32, f32)>,
    /// "convolver" when eq was measured from the IR, else "none".
    eq_source: String,
    /// The IR's built-in preamp (its peak gain, dB) — the chart's eq curve is
    /// normalized by this so shapes are comparable against target.
    ir_preamp_db: f32,
    /// Measured raw response (dBr, AutoEq-normalized).
    raw: Vec<(f32, f32)>,
    /// Smoothed measured response.
    smoothed: Vec<(f32, f32)>,
    /// AutoEq's post-EQ response, raw / smoothed variants.
    equalized_raw: Vec<(f32, f32)>,
    equalized_smoothed: Vec<(f32, f32)>,
    /// The target the correction aims for.
    target: Vec<(f32, f32)>,
}

/// Parse AutoEq's per-profile CSV by header name (columns vary by vintage).
fn parse_curves_csv(content: &str) -> CurveData {
    let mut lines = content.lines();
    let Some(header) = lines.next() else {
        return CurveData::default();
    };
    let cols: Vec<&str> = header.split(',').map(|c| c.trim()).collect();
    let idx = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
    let (Some(fi), Some(ri)) = (idx("frequency"), idx("raw")) else {
        return CurveData::default();
    };
    let ti = idx("target");
    let ei = idx("equalization");
    let si = idx("smoothed");
    let eri = idx("equalized_raw");
    let esi = idx("equalized_smoothed");
    let mut out = CurveData::default();
    let cell = |cells: &[&str], i: Option<usize>| -> Option<f32> {
        i.and_then(|i| cells.get(i).and_then(|v| v.trim().parse::<f32>().ok()))
    };
    for line in lines {
        let cells: Vec<&str> = line.split(',').collect();
        let Some(f) = cells.get(fi).and_then(|v| v.trim().parse::<f32>().ok()) else {
            continue;
        };
        if let Some(v) = cell(&cells, Some(ri)) {
            out.raw.push((f, v));
        }
        if let Some(v) = cell(&cells, ti) {
            out.target.push((f, v));
        }
        if let Some(v) = cell(&cells, ei) {
            out.eq.push((f, v));
        }
        if let Some(v) = cell(&cells, si) {
            out.smoothed.push((f, v));
        }
        if let Some(v) = cell(&cells, eri) {
            out.equalized_raw.push((f, v));
        }
        if let Some(v) = cell(&cells, esi) {
            out.equalized_smoothed.push((f, v));
        }
    }
    out
}

/// Linear interpolation in log-frequency space over ascending (hz, dB) points.
fn interp_log(pts: &[(f32, f32)], f: f32) -> Option<f32> {
    if pts.len() < 2 || f < pts[0].0 || f > pts[pts.len() - 1].0 {
        return None;
    }
    let i = pts.partition_point(|p| p.0 <= f).min(pts.len() - 1);
    let (f0, g0) = pts[i - 1];
    let (f1, g1) = pts[i];
    if f1 <= f0 {
        return Some(g0);
    }
    let t = (f.ln() - f0.ln()) / (f1.ln() - f0.ln());
    Some(g0 + t * (g1 - g0))
}

/// Measure an impulse response's magnitude at log-spaced frequencies by
/// direct DFT. Returns (points, peak_gain_db).
fn compute_ir_response(path: &std::path::Path, fs: u32) -> anyhow::Result<(Vec<(f32, f32)>, f32)> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let taps: Vec<f64> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .step_by(channels)
            .map(|s| s.map(|v| v as f64))
            .collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f64;
            reader
                .samples::<i32>()
                .step_by(channels)
                .map(|s| s.map(|v| v as f64 / scale))
                .collect::<Result<_, _>>()?
        }
    };
    if taps.is_empty() {
        anyhow::bail!("IR has no samples");
    }

    const N: usize = 240;
    let fmin = 20.0f64;
    let fmax = (fs as f64 * 0.47).min(20000.0);
    let mut points = Vec::with_capacity(N + 1);
    let mut peak = f32::MIN;
    for i in 0..=N {
        let f = fmin * (fmax / fmin).powf(i as f64 / N as f64);
        let w = 2.0 * std::f64::consts::PI * f / fs as f64;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        // Recurrence for cos/sin(w*n) avoids N*taps trig calls.
        let (cw, sw) = (w.cos(), w.sin());
        let (mut c, mut s) = (1.0f64, 0.0f64);
        for &h in &taps {
            re += h * c;
            im -= h * s;
            let c2 = c * cw - s * sw;
            s = c * sw + s * cw;
            c = c2;
        }
        let db = 20.0 * (re * re + im * im).sqrt().max(1e-12).log10();
        let db = db as f32;
        peak = peak.max(db);
        points.push((f as f32, db));
    }
    Ok((points, peak))
}

/// Curves for the assigned profile: the realized correction (measured from
/// the actual convolver IR when in convolution mode), plus raw/smoothed
/// measurement and target from AutoEq's CSV (downloaded on demand).
#[tauri::command]
async fn get_curve(app: AppHandle, device_guid: String) -> CmdResult<CurveData> {
    let eapo_status = eapo::detect();
    let config_dir = eapo_status
        .config_dir
        .ok_or("Equalizer APO is not installed")?;
    let (key, source, form, name) = {
        let shared = app.state::<Shared>();
        let st = shared.state.lock().unwrap();
        let profile = st
            .assignments
            .iter()
            .find(|(g, _)| g.eq_ignore_ascii_case(&device_guid))
            .map(|(_, p)| p)
            .ok_or("No profile assigned to this device")?;
        (
            eapo::sanitize_key(&profile.source, &profile.name),
            profile.source.clone(),
            profile.form.clone(),
            profile.name.clone(),
        )
    };
    let profile_dir = eapo::profiles_dir(&config_dir).join(&key);

    let csv_path = profile_dir.join("curves.csv");
    if !csv_path.is_file() && source != CUSTOM_SOURCE {
        // Profile predates CSV support (or download failed at apply time).
        if let Ok(bytes) = autoeq::download_curves_csv(&source, &form, &name).await {
            let _ = fs::write(&csv_path, bytes);
        }
    }

    let mut data = fs::read_to_string(&csv_path)
        .map(|c| parse_curves_csv(&c))
        .unwrap_or_default();

    // Realized correction: measured from the actual convolver IR. The CSV's
    // idealized equalization column is only used to level-align the measured
    // curve, never displayed itself.
    let csv_eq = std::mem::take(&mut data.eq);
    data.eq_source = "none".into();
    let rate = devices::list_render_devices()
        .ok()
        .and_then(|ds| {
            ds.into_iter()
                .find(|d| d.guid.eq_ignore_ascii_case(&device_guid))
        })
        .map(|d| d.sample_rate);
    let ir_name = match rate {
        Some(44100) => Some(("minphase_44100.wav", 44100u32)),
        Some(48000) => Some(("minphase_48000.wav", 48000u32)),
        _ => None,
    };
    if let Some((name, fs_rate)) = ir_name {
        let p = profile_dir.join(name);
        if p.is_file() {
            let measured =
                tauri::async_runtime::spawn_blocking(move || compute_ir_response(&p, fs_rate))
                    .await
                    .map_err(err_str)?;
            match measured {
                Ok((points, peak)) => {
                    // The IR is peak-normalized (unity max gain), which shifts
                    // the whole curve down by its maximum boost. Level-align
                    // to the ideal equalization curve for shape comparison;
                    // fall back to peak-normalization when no CSV data.
                    let mut diffs: Vec<f32> = points
                        .iter()
                        .filter_map(|&(f, g)| interp_log(&csv_eq, f).map(|ideal| g - ideal))
                        .collect();
                    let offset = if diffs.is_empty() {
                        peak
                    } else {
                        diffs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        diffs[diffs.len() / 2]
                    };
                    data.eq = points.into_iter().map(|(f, g)| (f, g - offset)).collect();
                    // The IR's overall insertion gain relative to the ideal
                    // curve — effectively its built-in anti-clipping preamp.
                    data.ir_preamp_db = offset;
                    data.eq_source = "convolver".into();
                }
                Err(e) => eprintln!("IR analysis failed: {}", e),
            }
        }
    }
    // eq may legitimately be empty (no IR at this device's mix rate); the
    // chart still shows the measurement curves from the CSV.
    if data.eq.is_empty() && data.raw.is_empty() {
        return Err("No curve data found for this profile".into());
    }
    Ok(data)
}

#[tauri::command]
fn set_bass_shelf(
    app: AppHandle,
    device_guid: String,
    gain_db: f32,
    fc: f32,
) -> CmdResult<AppStatus> {
    {
        let shared = app.state::<Shared>();
        let mut st = shared.state.lock().unwrap();
        let profile = st
            .assignments
            .iter_mut()
            .find(|(g, _)| g.eq_ignore_ascii_case(&device_guid))
            .map(|(_, p)| p)
            .ok_or("No profile assigned to this device")?;
        profile.bass_gain_db = gain_db.clamp(-12.0, 12.0);
        profile.bass_fc = fc.clamp(50.0, 350.0);
        state::save(&app_data_dir(&app)?, &st).map_err(err_str)?;
    }
    regenerate(&app)?;
    build_status(&app)
}

#[tauri::command]
fn set_headroom(app: AppHandle, db: f32) -> CmdResult<AppStatus> {
    let clamped = db.clamp(-24.0, 0.0);
    {
        let shared = app.state::<Shared>();
        let mut st = shared.state.lock().unwrap();
        st.headroom_db = clamped;
        state::save(&app_data_dir(&app)?, &st).map_err(err_str)?;
    }
    regenerate(&app)?;
    build_status(&app)
}

#[tauri::command]
fn set_compensate_shelf(app: AppHandle, enabled: bool) -> CmdResult<AppStatus> {
    {
        let shared = app.state::<Shared>();
        let mut st = shared.state.lock().unwrap();
        st.compensate_shelf = enabled;
        state::save(&app_data_dir(&app)?, &st).map_err(err_str)?;
    }
    regenerate(&app)?;
    build_status(&app)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<bool> {
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable().map_err(err_str)?;
    } else {
        launcher.disable().map_err(err_str)?;
    }
    Ok(launcher.is_enabled().unwrap_or(enabled))
}

#[tauri::command]
fn setup_eapo_include(app: AppHandle) -> CmdResult<AppStatus> {
    let eapo_status = eapo::detect();
    let config_dir = eapo_status
        .config_dir
        .ok_or("Equalizer APO is not installed")?;
    eapo::ensure_include(&config_dir).map_err(err_str)?;
    regenerate(&app)?;
    build_status(&app)
}

/// Whether Mica can actually render: needs Windows 11 (build 22000+) and the
/// user's "Transparency effects" setting enabled.
fn mica_available() -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let build_ok = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion")
        .and_then(|k| k.get_value::<String, _>("CurrentBuildNumber"))
        .ok()
        .and_then(|b| b.parse::<u32>().ok())
        .map(|b| b >= 22000)
        .unwrap_or(false);
    if !build_ok {
        return false;
    }
    // Missing value means transparency is enabled (the default).
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        .and_then(|k| k.get_value::<u32, _>("EnableTransparency"))
        .map(|v| v != 0)
        .unwrap_or(true)
}

/// Apply Mica when available; report whether it took effect. The frontend
/// paints an opaque background when it didn't.
fn apply_backdrop(app: &AppHandle) -> bool {
    use tauri::window::{Effect, EffectsBuilder};
    let Some(window) = app.get_webview_window("main") else {
        return false;
    };
    if !mica_available() {
        return false;
    }
    window
        .set_effects(EffectsBuilder::new().effect(Effect::Mica).build())
        .is_ok()
}

#[tauri::command]
fn open_configurator() -> CmdResult<()> {
    let status = eapo::detect();
    let install = PathBuf::from(status.install_dir.ok_or("Equalizer APO is not installed")?);
    // EAPO 1.4+ ships "DeviceSelector.exe"; older versions "Configurator.exe".
    let exe = ["DeviceSelector.exe", "Configurator.exe"]
        .iter()
        .map(|n| install.join(n))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("Device Selector not found in {}", install.display()))?;
    std::process::Command::new(exe).spawn().map_err(err_str)?;
    Ok(())
}

async fn self_heal(app: AppHandle) {
    let eapo_status = eapo::detect();
    let Some(config_dir) = eapo_status.config_dir else {
        return;
    };
    let assignments: Vec<AssignedProfile> = {
        let shared = app.state::<Shared>();
        let st = shared.state.lock().unwrap();
        st.assignments.values().cloned().collect()
    };
    if assignments.is_empty() {
        // Nothing to restore; don't touch the user's EAPO config.
        let _ = regenerate(&app);
        return;
    }
    for profile in assignments {
        let key = eapo::sanitize_key(&profile.source, &profile.name);
        let dir = eapo::profiles_dir(&config_dir).join(&key);
        if dir.join("minphase_44100.wav").is_file() || dir.join("minphase_48000.wav").is_file() {
            continue;
        }
        if profile.source == CUSTOM_SOURCE {
            // User-supplied IRs cannot be re-downloaded.
            eprintln!(
                "self-heal: custom IR files for '{}' are missing and cannot be restored",
                profile.name
            );
            continue;
        }
        match autoeq::download_profile(&profile.source, &profile.form, &profile.name).await {
            Ok(files) => {
                let mut to_write: Vec<(String, Vec<u8>)> = files.irs;
                to_write.extend(files.extras);
                if let Err(e) = eapo::install_profile_files(&config_dir, &key, &to_write) {
                    eprintln!("self-heal: reinstall of {} failed: {}", key, e);
                }
            }
            Err(e) => eprintln!("self-heal: redownload of {} failed: {}", key, e),
        }
    }
    if let Err(e) = eapo::ensure_include(&config_dir) {
        eprintln!("self-heal: ensure_include failed: {}", e);
    }
    let _ = regenerate(&app);
    let _ = app.emit("status-changed", ());
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn toggle_master_from_tray(app: &AppHandle) {
    let enabled = {
        let shared = app.state::<Shared>();
        let st = shared.state.lock().unwrap();
        st.master_enabled
    };
    let _ = set_master(app.clone(), !enabled);
    let _ = app.emit("status-changed", ());
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            let mica = apply_backdrop(app.handle());
            app.manage(BackdropState(mica));

            let data_dir = app.path().app_data_dir()?;
            let initial = state::load(&data_dir);
            let master_enabled = initial.master_enabled;
            app.manage(Shared {
                state: Mutex::new(initial),
            });

            // Tray
            let open_item = MenuItem::with_id(app, "open", "Open Impulse", true, None::<&str>)?;
            let toggle_item = CheckMenuItem::with_id(
                app,
                "toggle",
                "EQ enabled",
                true,
                master_enabled,
                None::<&str>,
            )?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&open_item, &toggle_item, &separator, &quit_item])?;

            let mut tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .tooltip("Impulse")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => show_main_window(app),
                    "toggle" => toggle_master_from_tray(app),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            app.manage(TrayHandles {
                toggle: toggle_item,
            });

            // Start hidden when launched with --minimized (autostart).
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            // Self-heal on startup: if the EAPO config was wiped (updates,
            // manual resets), restore profile files, the Include line, and
            // the managed config from persisted state.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                self_heal(handle).await;
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            // Close button minimizes to tray instead of quitting.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_index,
            apply_profile,
            apply_custom_ir,
            clear_profile,
            set_master,
            set_headroom,
            set_bass_shelf,
            set_compensate_shelf,
            get_curve,
            set_autostart,
            setup_eapo_include,
            open_configurator,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
