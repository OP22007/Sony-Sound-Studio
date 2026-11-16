//! Read-only peak levels from the current Windows render endpoint.

#[cfg(windows)]
pub fn read_peaks(expected_identity: &str) -> Option<(f32, f32)> {
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioMeterInformation, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
    }
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    let _com = ComGuard;

    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let endpoint = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
        let endpoint_id = endpoint.GetId().ok()?;
        let id = endpoint_id.to_string().ok()?;
        windows::Win32::System::Com::CoTaskMemFree(Some(endpoint_id.0 as *const _));
        if endpoint_identity(&id) != expected_identity {
            return None;
        }

        let meter: IAudioMeterInformation = endpoint.Activate(CLSCTX_ALL, None).ok()?;
        let peak = meter.GetPeakValue().ok()?;
        // Core Audio's peak value is a scalar for the endpoint. It is not a channel capture API.
        let peak = sanitize_peak(peak)?;
        Some((peak, peak))
    }
}

#[cfg(windows)]
fn endpoint_identity(id: &str) -> String {
    use windows::Win32::Devices::FunctionDiscovery::{PKEY_Device_InstanceId};
    use windows::Win32::Media::Audio::{IDeviceTopology, IMMDevice};
    use windows::Win32::System::Com::{CoTaskMemFree, STGM_READ};
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
    use windows::Win32::System::Variant::VT_LPWSTR;

    unsafe {
        // Resolve the same native endpoint identity used by transport::windows::default_output_endpoint.
        let enumerator: windows::Win32::Media::Audio::IMMDeviceEnumerator =
            windows::Win32::System::Com::CoCreateInstance(
                &windows::Win32::Media::Audio::MMDeviceEnumerator,
                None,
                windows::Win32::System::Com::CLSCTX_ALL,
            ).ok()?;
        let endpoint = enumerator.GetDefaultAudioEndpoint(
            windows::Win32::Media::Audio::eRender,
            windows::Win32::Media::Audio::eMultimedia,
        ).ok()?;
        let store = endpoint.OpenPropertyStore(STGM_READ).ok()?;
        let mut value: PROPVARIANT = store.GetValue(&PKEY_Device_InstanceId).ok()?;
        let instance = if (&*value.Anonymous.Anonymous).vt == VT_LPWSTR {
            (&*value.Anonymous.Anonymous).Anonymous.pwszVal.to_string().unwrap_or_default()
        } else { String::new() };
        let _ = PropVariantClear(&mut value);
        let topology_id = endpoint.Activate::<IDeviceTopology>(windows::Win32::System::Com::CLSCTX_ALL, None).ok()
            .and_then(|topology| topology.GetConnector(0).ok())
            .and_then(|connector| connector.GetDeviceIdConnectedTo().ok())
            .and_then(|ptr| {
                let value = ptr.to_string().ok();
                CoTaskMemFree(Some(ptr.0 as *const _));
                value
            }).unwrap_or_default();
        let native_id = if topology_id.is_empty() { instance.clone() } else { topology_id.clone() };
        let has_physical_id = !native_id.is_empty() && !native_id.to_ascii_lowercase().contains("swd\\mmdevapi");
        let lower = format!("{id} {instance} {topology_id}").to_ascii_lowercase();
        let bluetooth = lower.contains("bth") || lower.contains("bluetooth");
        let identity = native_id.split("DEV_").nth(1).and_then(|v| v.get(..12))
            .filter(|v| v.chars().all(|c| c.is_ascii_hexdigit())).unwrap_or(id);
        // Transport treats known non-Bluetooth physical endpoints as Other and cannot map them
        // to a paired headset, so preserving the raw endpoint-derived identity is sufficient.
        let _ = has_physical_id;
        let _ = bluetooth;
        identity.to_string()
    }
}

#[cfg(windows)]
fn sanitize_peak(value: f32) -> Option<f32> {
    (value.is_finite() && (0.0..=1.0).contains(&value)).then_some(value)
}

#[cfg(windows)]
#[cfg(test)]
mod tests {
    use super::sanitize_peak;

    #[test]
    fn accepts_only_normalized_finite_peaks() {
        assert_eq!(sanitize_peak(0.0), Some(0.0));
        assert_eq!(sanitize_peak(0.75), Some(0.75));
        assert_eq!(sanitize_peak(1.0), Some(1.0));
        assert_eq!(sanitize_peak(f32::NAN), None);
        assert_eq!(sanitize_peak(f32::INFINITY), None);
        assert_eq!(sanitize_peak(-0.1), None);
        assert_eq!(sanitize_peak(1.1), None);
    }
}

#[cfg(not(windows))]
pub fn read_peaks(_expected_identity: &str) -> Option<(f32, f32)> { None }
