//! macOS paired-device enumeration through Apple's IOBluetooth framework.
//!
//! RFCOMM control connections remain disabled until the adapter can guarantee
//! that a timed-out write cannot be transmitted later. IOBluetooth's synchronous
//! write can block until transmission, and its async write API documents a
//! completion callback but no cancellation guarantee for an accepted write.

use super::{
    is_supported_headphone, model_from_name, ActiveAudioDevice, AudioTransport,
    DeviceActivitySnapshot, PairedDevice,
};
use objc2::rc::autoreleasepool;
use objc2_io_bluetooth::IOBluetoothDevice;

pub fn activity_snapshot() -> Result<super::DeviceActivitySnapshot, String> {
    autoreleasepool(|_| {
        let Some(paired) = (unsafe { IOBluetoothDevice::pairedDevices() }) else {
            return Ok(DeviceActivitySnapshot::default());
        };
        let mut snapshot = DeviceActivitySnapshot::default();
        for index in 0..paired.count().min(256) {
            let item = paired.objectAtIndex(index);
            let Some(device) = item.downcast_ref::<IOBluetoothDevice>() else {
                continue;
            };
            if !unsafe { device.isConnected() } {
                continue;
            }
            let name = unsafe { device.name() }.to_string();
            if !is_supported_headphone(&name) {
                continue;
            }
            if let Some(address) = unsafe { device.addressString() } {
                snapshot.bluetooth_connected.push(ActiveAudioDevice {
                    identity: address.to_string(),
                    name: name.chars().filter(|c| !c.is_control()).take(128).collect(),
                    transport: AudioTransport::Bluetooth,
                });
            }
        }
        snapshot
            .bluetooth_connected
            .sort_by(|a, b| a.identity.cmp(&b.identity));
        snapshot
            .bluetooth_connected
            .dedup_by(|a, b| a.identity == b.identity);
        Ok(snapshot)
    })
}

pub fn enumerate() -> Result<Vec<PairedDevice>, String> {
    autoreleasepool(|_| {
        let Some(paired) = (unsafe { IOBluetoothDevice::pairedDevices() }) else {
            return Ok(vec![]);
        };
        let mut devices = Vec::new();
        for index in 0..paired.count().min(256) {
            let item = paired.objectAtIndex(index);
            let Some(device) = item.downcast_ref::<IOBluetoothDevice>() else {
                continue;
            };
            let name = unsafe { device.name() }.to_string();
            if !is_supported_headphone(&name) {
                continue;
            }
            if let Some(address) = unsafe { device.addressString() } {
                devices.push(PairedDevice {
                    address: address.to_string(),
                    model: model_from_name(&name),
                    name: name.chars().filter(|c| !c.is_control()).take(128).collect(),
                });
            }
        }
        devices.sort_by(|a, b| a.address.cmp(&b.address));
        devices.dedup_by(|a, b| a.address == b.address);
        devices.truncate(128);
        Ok(devices)
    })
}

pub fn connect(
    _device: &PairedDevice,
    _cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<Box<dyn super::Connection>, String> {
    Err("macOS RFCOMM control connections are unsupported: IOBluetooth does not document cancellation of an accepted write, so a timed-out command could still be sent later. Device settings and read-only discovery are unavailable on macOS until writes can be bounded safely.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn connect_fails_closed_before_opening_an_rfcomm_channel() {
        let device = PairedDevice {
            address: "00:00:00:00:00:00".into(),
            name: "WH-1000XM6".into(),
            model: "WH-1000XM6".into(),
        };
        let cancelled = Arc::new(AtomicBool::new(false));

        let error = match connect(&device, Arc::clone(&cancelled)) {
            Ok(_) => panic!("macOS RFCOMM connection unexpectedly succeeded"),
            Err(error) => error,
        };

        assert!(error.contains("unsupported"));
        assert!(error.contains("timed-out command could still be sent later"));
        assert!(!cancelled.load(Ordering::Acquire));
    }
}
