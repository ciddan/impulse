//! Enumeration of active audio render endpoints via WASAPI / MMDevice.

use anyhow::{Context, Result};
use serde::Serialize;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};

#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    /// Full MMDevice ID, e.g. "{0.0.0.00000000}.{guid}"
    pub id: String,
    /// The endpoint GUID portion, e.g. "{9bd35e4c-...}" — used for EAPO Device: matching
    pub guid: String,
    pub name: String,
    pub sample_rate: u32,
    pub is_default: bool,
}

fn ensure_com() {
    // Idempotent per thread; S_FALSE / RPC_E_CHANGED_MODE are fine to ignore here.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn device_id(device: &IMMDevice) -> Result<String> {
    unsafe {
        let pw = device.GetId().context("GetId failed")?;
        let s = pw.to_string().context("device id to_string")?;
        CoTaskMemFree(Some(pw.as_ptr() as *const _));
        Ok(s)
    }
}

fn friendly_name(device: &IMMDevice) -> Result<String> {
    unsafe {
        let store = device
            .OpenPropertyStore(STGM_READ)
            .context("OpenPropertyStore failed")?;
        let value = store
            .GetValue(&PKEY_Device_FriendlyName)
            .context("GetValue(FriendlyName) failed")?;
        Ok(value.to_string())
    }
}

fn mix_sample_rate(device: &IMMDevice) -> Result<u32> {
    unsafe {
        let client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .context("Activate(IAudioClient) failed")?;
        let fmt = client.GetMixFormat().context("GetMixFormat failed")?;
        let rate = (*fmt).nSamplesPerSec;
        CoTaskMemFree(Some(fmt as *const _));
        Ok(rate)
    }
}

/// Extract the endpoint GUID (last {...} group) from a full MMDevice ID.
pub fn guid_from_id(id: &str) -> String {
    match id.rfind('{') {
        Some(pos) => id[pos..].to_string(),
        None => id.to_string(),
    }
}

pub fn list_render_devices() -> Result<Vec<AudioDevice>> {
    ensure_com();
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .context("create MMDeviceEnumerator")?;

        let default_id = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .ok()
            .and_then(|d| device_id(&d).ok())
            .unwrap_or_default();

        let collection = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .context("EnumAudioEndpoints failed")?;
        let count = collection.GetCount().context("GetCount failed")?;

        let mut devices = Vec::with_capacity(count as usize);
        for i in 0..count {
            let device = match collection.Item(i) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let id = match device_id(&device) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let name = friendly_name(&device).unwrap_or_else(|_| id.clone());
            let sample_rate = mix_sample_rate(&device).unwrap_or(48000);
            devices.push(AudioDevice {
                guid: guid_from_id(&id),
                is_default: id == default_id,
                id,
                name,
                sample_rate,
            });
        }
        // Default device first, then alphabetical.
        devices.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.name.cmp(&b.name)));
        Ok(devices)
    }
}
