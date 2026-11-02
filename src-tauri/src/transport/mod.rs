//! Platform transports connect only to services observed in the supplied Sony APK.
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

#[derive(Clone, Debug)]
pub struct PairedDevice {
    pub address: String,
    pub name: String,
    pub model: String,
}

pub trait Connection: Send {
    fn service_uuid(&self) -> &'static str;
    /// Returns zero when peer closes; a timeout is represented by WouldBlock.
    fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> std::io::Result<usize>;
    /// Used only by independently verified read-only discovery and transport acknowledgements.
    fn write_all(&mut self, bytes: &[u8], timeout: Duration) -> std::io::Result<()>;
}

pub fn is_sony_headphone(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    name.contains("WH-")
        || name.contains("WF-")
        || name.contains("LINKBUDS")
        || name.contains("WI-")
}

pub fn model_from_name(name: &str) -> String {
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
        "Windows Sony control sessions are available. Controls follow capability discovery and device readback. WH-1000XM6 firmware 3.1.5 is the current hardware test target; other models still require validation."
    } else if cfg!(target_os = "linux") {
        "Linux BlueZ paired-device enumeration and RFCOMM transport require bluetoothctl and sdptool; not hardware-tested."
    } else if cfg!(target_os = "macos") {
        "macOS supports paired-device discovery. Headphone control connections are currently disabled; device settings are unavailable on this preview."
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
    fn model_label_does_not_create_capabilities() {
        assert_eq!(model_from_name("LE_WH-1000XM6"), "WH-1000XM6");
        assert!(is_sony_headphone("WH-CH720N"));
        assert!(!is_sony_headphone("Unrelated speaker"));
        assert!(model_from_name("WH-UNKNOWN").contains("unverified"));
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
