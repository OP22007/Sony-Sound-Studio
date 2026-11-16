//! Platform transports connect only to vendor services observed in the supplied APKs.
//! No undocumented device command is transmitted by these adapters.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
#[cfg(any(target_os = "macos", test))]
mod worker;
#[cfg(any(target_os = "macos", test))]
mod macos_state;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairedDevice {
    pub address: String,
    pub name: String,
    pub model: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceActivitySnapshot {
    pub active_output: Option<ActiveAudioDevice>,
    pub bluetooth_connected: Vec<ActiveAudioDevice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveAudioDevice {
    pub identity: String,
    pub name: String,
    pub transport: AudioTransport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioTransport {
    Bluetooth,
    Other,
    Unknown,
}

pub fn activity_snapshot() -> Result<DeviceActivitySnapshot, String> {
    #[cfg(windows)]
    return windows::activity_snapshot();
    #[cfg(target_os = "linux")]
    return linux::activity_snapshot();
    #[cfg(target_os = "macos")]
    return macos::activity_snapshot();
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    Ok(DeviceActivitySnapshot::default())
}

/// Match only a unique, currently-connected device by normalized name; native Bluetooth address identities win.
pub fn match_active_device(
    active: &ActiveAudioDevice,
    paired: &[PairedDevice],
    connected: &[ActiveAudioDevice],
) -> Option<PairedDevice> {
    if active.transport == AudioTransport::Other {
        return None;
    }
    let is_connected = |device: &PairedDevice| {
        connected.iter().any(|item| {
            item.transport == AudioTransport::Bluetooth
                && normalize_identity(&item.identity) == normalize_identity(&device.address)
        })
    };
    let exact: Vec<_> = paired
        .iter()
        .filter(|d| {
            normalize_identity(&d.address) == normalize_identity(&active.identity)
                && is_connected(d)
        })
        .collect();
    if exact.len() == 1 {
        return Some(exact[0].clone());
    }
    let key = normalize_endpoint_name(&active.name);
    if key.is_empty() {
        return None;
    }
    let matches: Vec<_> = paired
        .iter()
        .filter(|d| is_connected(d) && normalize_endpoint_name(&d.name) == key)
        .collect();
    (matches.len() == 1).then(|| matches[0].clone())
}

fn normalize_identity(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(crate) fn normalize_endpoint_name(value: &str) -> String {
    let mut name = value.trim();
    for prefix in ["Headphones (", "Headset (", "Speakers ("] {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest.strip_suffix(')').unwrap_or(rest).trim();
            break;
        }
    }
    for suffix in [" Hands-Free AG Audio", " Stereo", " Hands-Free"] {
        if let Some(rest) = name.strip_suffix(suffix) {
            name = rest.trim();
        }
    }
    normalize_identity(name)
}

pub trait Connection: Send {
    fn service_uuid(&self) -> &'static str;
    /// Returns zero when peer closes; a timeout is represented by WouldBlock.
    fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> std::io::Result<usize>;
    /// Used by independently implemented discovery and capability-gated settings.
    fn write_all(&mut self, bytes: &[u8], timeout: Duration) -> std::io::Result<()>;
}

pub fn is_sony_headphone(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    name.contains("WH-")
        || name.contains("WF-")
        || name.contains("LINKBUDS")
        || name.contains("WI-")
        || name.contains("1000X THE COLLEXION")
}

pub fn is_supported_headphone(name: &str) -> bool {
    is_sony_headphone(name) || crate::oneplus_models::is_oneplus_name(name)
}

pub fn service_uuids(device: &PairedDevice) -> &'static [&'static str] {
    if let Some(model) = crate::oneplus_models::by_name(&device.model)
        .or_else(|| crate::oneplus_models::by_name(&device.name))
    {
        return match model.service_uuid.as_str() {
            crate::oneplus::CURRENT_SERVICE_UUID => &[crate::oneplus::CURRENT_SERVICE_UUID],
            crate::oneplus::SERVICE_UUID => &[crate::oneplus::SERVICE_UUID],
            _ => &[],
        };
    }
    if crate::oneplus_models::is_oneplus_name(&device.model)
        || crate::oneplus_models::is_oneplus_name(&device.name)
    {
        return &[
            crate::oneplus::CURRENT_SERVICE_UUID,
            crate::oneplus::SERVICE_UUID,
        ];
    }
    &[
        "96cc203e-5068-46ad-b32d-e316f5e069ba",
        "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
    ]
}

pub fn model_from_name(name: &str) -> String {
    if let Some(model) = crate::oneplus_models::by_name(name) {
        return model.name.clone();
    }
    if crate::oneplus_models::is_oneplus_name(name) {
        return "OnePlus headphones (model unverified)".into();
    }
    let upper = name.to_ascii_uppercase();
    for model in ["WH-1000XM6", "WH-1000XM5", "WH-CH720N"] {
        if upper.contains(model) {
            return model.into();
        }
    }
    "Sony headphones (model unverified)".into()
}

pub fn status() -> &'static str {
    if cfg!(windows) {
        "Windows Sony and experimental OnePlus control sessions are available. The bundled HeyMelody catalog covers 32 OnePlus product IDs; controls require a recognized product ID, model-specific mappings and live readback. OnePlus hardware validation is pending."
    } else if cfg!(target_os = "linux") {
        "Linux BlueZ paired-device enumeration and RFCOMM transport require bluetoothctl and sdptool; not hardware-tested."
    } else if cfg!(target_os = "macos") {
        "macOS experimental Sony and OnePlus RFCOMM control sessions use live service discovery and confirmed device readback. Hardware validation is pending. Active audio-output identity and automatic listening remain unavailable."
    } else {
        "Native Bluetooth transport is unavailable on this operating system."
    }
}

pub fn enumerate() -> Result<Vec<PairedDevice>, String> {
    #[cfg(windows)]
    return windows::enumerate();
    #[cfg(target_os = "linux")]
    return linux::enumerate();
    #[cfg(target_os = "macos")]
    return macos::enumerate();
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    Err(status().into())
}

pub fn connect(
    device: &PairedDevice,
    cancelled: Arc<AtomicBool>,
) -> Result<Box<dyn Connection>, String> {
    #[cfg(windows)]
    return windows::connect(device, cancelled);
    #[cfg(target_os = "linux")]
    return linux::connect(device, cancelled);
    #[cfg(target_os = "macos")]
    return macos::connect(device, cancelled);
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = (device, cancelled);
        Err(status().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_matching_requires_bluetooth_and_unique_identity_or_name() {
        let a = PairedDevice {
            address: "AA:BB:CC:DD:EE:FF".into(),
            name: "WH-1000XM6".into(),
            model: "WH-1000XM6".into(),
        };
        let active = ActiveAudioDevice {
            identity: "aabbccddeeff".into(),
            name: "unknown".into(),
            transport: AudioTransport::Bluetooth,
        };
        let connected = [active.clone()];
        assert_eq!(
            match_active_device(&active, std::slice::from_ref(&a), &connected),
            Some(a.clone())
        );
        let by_name = ActiveAudioDevice {
            identity: "endpoint-guid".into(),
            name: "wh 1000xm6".into(),
            transport: AudioTransport::Bluetooth,
        };
        assert_eq!(
            match_active_device(&by_name, std::slice::from_ref(&a), &connected),
            Some(a.clone())
        );
        let other = ActiveAudioDevice {
            transport: AudioTransport::Other,
            ..by_name.clone()
        };
        assert_eq!(
            match_active_device(&other, std::slice::from_ref(&a), &connected),
            None
        );
        let duplicate = PairedDevice {
            address: "11:22:33:44:55:66".into(),
            ..a.clone()
        };
        assert_eq!(
            match_active_device(&by_name, &[a.clone(), duplicate], &connected),
            Some(a.clone())
        );
        assert_eq!(
            match_active_device(&by_name, std::slice::from_ref(&a), &[]),
            None
        );
        for endpoint_name in [
            "Headphones (WH-1000XM6)",
            "Headset (WH-1000XM6 Hands-Free AG Audio)",
        ] {
            let endpoint = ActiveAudioDevice {
                name: endpoint_name.into(),
                ..by_name.clone()
            };
            assert_eq!(
                match_active_device(&endpoint, std::slice::from_ref(&a), &connected),
                Some(a.clone())
            );
        }
        let opaque_endpoint = ActiveAudioDevice {
            transport: AudioTransport::Unknown,
            ..by_name.clone()
        };
        assert_eq!(
            match_active_device(&opaque_endpoint, std::slice::from_ref(&a), &connected),
            Some(a.clone())
        );
        let similar = ActiveAudioDevice {
            name: "Headphones (WH-1000XM6 LE)".into(),
            ..by_name.clone()
        };
        assert_eq!(
            match_active_device(&similar, std::slice::from_ref(&a), &connected),
            None
        );
    }

    #[test]
    fn model_label_does_not_create_capabilities() {
        assert_eq!(model_from_name("LE_WH-1000XM6"), "WH-1000XM6");
        assert!(is_sony_headphone("WH-CH720N"));
        assert!(!is_sony_headphone("Unrelated speaker"));
        assert!(model_from_name("WH-UNKNOWN").contains("unverified"));
    }

    #[test]
    fn oneplus_model_routes_only_to_its_vendor_service() {
        let name = "oneplus nord buds 3 pro";
        assert!(is_supported_headphone(name));
        assert!(!is_sony_headphone(name));
        let device = PairedDevice {
            address: "fixture".into(),
            name: name.into(),
            model: model_from_name(name),
        };
        assert_eq!(device.model, "OnePlus Nord Buds 3 Pro");
        assert_eq!(
            service_uuids(&device),
            [crate::oneplus::CURRENT_SERVICE_UUID]
        );
        assert!(is_supported_headphone("OnePlus Buds Pro 3"));
        assert!(is_supported_headphone("OnePlus Nord Buds 3"));
        for model in crate::oneplus_models::models() {
            for name in std::iter::once(&model.name).chain(&model.aliases) {
                assert!(is_supported_headphone(name));
                let device = PairedDevice {
                    address: "fixture".into(),
                    name: name.clone(),
                    model: model_from_name(name),
                };
                assert_eq!(service_uuids(&device), [model.service_uuid.as_str()]);
                assert_eq!(device.model, model.name);
            }
        }
        let unknown = PairedDevice {
            address: "fixture".into(),
            name: "OnePlus Buds Future".into(),
            model: model_from_name("OnePlus Buds Future"),
        };
        assert_eq!(
            service_uuids(&unknown),
            [
                crate::oneplus::CURRENT_SERVICE_UUID,
                crate::oneplus::SERVICE_UUID
            ]
        );
        assert!(!is_supported_headphone("OnePlus 15"));
    }

    #[test]
    #[ignore = "Requires the host Bluetooth stack; explicitly run for native discovery validation"]
    fn native_enumeration_smoke() {
        let devices = enumerate().expect("native Bluetooth enumeration failed");
        println!("Native paired Sony devices: {}", devices.len());
        for device in &devices {
            println!("Model label: {}", device.model);
            assert!(!device.address.is_empty());
        }
        assert!(devices.len() <= 128);
    }

    #[test]
    #[ignore = "Requires a powered-on paired XM6; explicitly run for a read-only native socket test"]
    fn native_connection_smoke() {
        let devices = enumerate().expect("native Bluetooth enumeration failed");
        let device = devices
            .iter()
            .find(|device| device.model == "WH-1000XM6")
            .expect("pair and power on an XM6 first");
        let cancelled = Arc::new(AtomicBool::new(false));
        let connection =
            connect(device, cancelled).expect("native RFCOMM service connection failed");
        println!("Real WH-1000XM6 RFCOMM service socket connected. No payload sent.");
        drop(connection);
        println!("Socket closed cleanly.");
    }

    #[test]
    #[ignore = "Requires a powered-on paired XM6; sends only verified read-only protocol-info and transport ACKs"]
    fn native_protocol_info_smoke() {
        let devices = enumerate().expect("native Bluetooth enumeration failed");
        let device = devices
            .iter()
            .find(|device| device.model == "WH-1000XM6")
            .expect("pair and power on an XM6 first");
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut connection = connect(device, Arc::clone(&cancelled))
            .expect("native RFCOMM service connection failed");
        let info = crate::discovery::query_protocol_info(&mut *connection, &cancelled)
            .expect("read-only protocol-info query failed");
        println!(
            "Real XM6 protocol-info response: format={}, rawVersion={}, transportAckObserved={}",
            info.format, info.version, info.ack_observed
        );
        drop(connection);
        println!("Read-only protocol session closed cleanly. No settings changed.");
    }

    #[test]
    #[ignore = "Requires a powered-on paired XM6; verified protocol/model/firmware read-only GETs only"]
    fn native_device_info_smoke() {
        let devices = enumerate().expect("native Bluetooth enumeration failed");
        let device = devices
            .iter()
            .find(|device| device.model == "WH-1000XM6")
            .expect("pair and power on an XM6 first");
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut connection = connect(device, Arc::clone(&cancelled))
            .expect("native RFCOMM service connection failed");
        let (basics, _) = crate::discovery::read_basics(&mut *connection, &cancelled);
        assert!(basics.protocol.is_ok(), "{:?}", basics.protocol);
        assert!(basics.model.is_some(), "{:?}", basics.warnings);
        assert!(basics.firmware.is_some(), "{:?}", basics.warnings);
        assert!(basics.battery.is_some(), "{:?}", basics.warnings);
        let battery = basics.battery.unwrap();
        println!(
            "Real device readback: model={}, firmware={}; no settings writes",
            basics.model.unwrap(),
            basics.firmware.unwrap()
        );
        println!(
            "Real battery readback: {}%, charging={}",
            battery.level, battery.charging
        );
        drop(connection);
        println!("Read-only device-info session closed cleanly.");
    }
}
