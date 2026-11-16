//! Read output peak metadata only. This never opens an audio capture stream.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct OutputLevels {
    pub left: f32,
    pub right: f32,
}

fn stereo_peaks(channels: &[f32]) -> Option<OutputLevels> {
    // Other layouts need an explicit channel mask before they can be labelled L/R.
    if channels.is_empty()
        || channels.len() > 2
        || channels
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return None;
    }
    Some(OutputLevels {
        left: channels[0],
        right: *channels.get(1).unwrap_or(&channels[0]),
    })
}

#[cfg(not(windows))]
pub fn read(_expected_name: &str) -> Option<OutputLevels> {
    None
}

#[cfg(windows)]
pub fn read(expected_name: &str) -> Option<OutputLevels> {
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
        STGM_READ,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return None;
        }
        struct ComGuard;
        impl Drop for ComGuard {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _guard = ComGuard;
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let endpoint = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .ok()?;
        let store = endpoint.OpenPropertyStore(STGM_READ).ok()?;
        let mut name = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        let raw = &*name.Anonymous.Anonymous;
        let friendly = if raw.vt == VT_LPWSTR {
            raw.Anonymous.pwszVal.to_string().ok()
        } else {
            None
        };
        let _ = PropVariantClear(&mut name);
        if crate::transport::normalize_endpoint_name(&friendly?)
            != crate::transport::normalize_endpoint_name(expected_name)
        {
            return None;
        }
        let meter: IAudioMeterInformation = endpoint.Activate(CLSCTX_ALL, None).ok()?;
        let count = meter.GetMeteringChannelCount().ok()?;
        if count == 0 || count > 2 {
            return None;
        }
        let mut channels = [0.0f32; 2];
        meter
            .GetChannelsPeakValues(&mut channels[..count as usize])
            .ok()?;
        stereo_peaks(&channels[..count as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_bounded_stereo_and_mono_without_guessing_multichannel_layouts() {
        assert_eq!(
            stereo_peaks(&[0.25, 0.75]),
            Some(OutputLevels {
                left: 0.25,
                right: 0.75
            })
        );
        assert_eq!(
            stereo_peaks(&[0.4]),
            Some(OutputLevels {
                left: 0.4,
                right: 0.4
            })
        );
        assert_eq!(
            stereo_peaks(&[0.0, 0.0]),
            Some(OutputLevels {
                left: 0.0,
                right: 0.0
            })
        );
        for values in [
            &[][..],
            &[0.1, 0.2, 0.3],
            &[f32::NAN],
            &[f32::INFINITY],
            &[-0.1],
            &[1.01],
        ] {
            assert!(stereo_peaks(values).is_none());
        }
    }
}
