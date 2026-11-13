//! This computer's own Bluetooth address(es). A headphone lists every device it is paired with,
//! including the computer running this app; knowing which entry that is keeps the app from ever
//! disconnecting itself when it frees a connection slot for another device.

/// Windows reports the 48-bit address least significant byte first.
pub fn format_address(least_significant_first: [u8; 6]) -> String {
    let b = least_significant_first;
    format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", b[5], b[4], b[3], b[2], b[1], b[0])
}

#[cfg(windows)]
pub fn addresses() -> Vec<String> {
    use windows_sys::Win32::Devices::Bluetooth::{
        BluetoothFindFirstRadio, BluetoothFindNextRadio, BluetoothFindRadioClose, BluetoothGetRadioInfo,
        BLUETOOTH_FIND_RADIO_PARAMS, BLUETOOTH_RADIO_INFO,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    let mut found = Vec::new();
    unsafe {
        let params = BLUETOOTH_FIND_RADIO_PARAMS { dwSize: std::mem::size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32 };
        let mut radio: HANDLE = std::ptr::null_mut();
        let find = BluetoothFindFirstRadio(&params, &mut radio);
        if find.is_null() {
            return found;
        }
        loop {
            let mut info: BLUETOOTH_RADIO_INFO = std::mem::zeroed();
            info.dwSize = std::mem::size_of::<BLUETOOTH_RADIO_INFO>() as u32;
            if BluetoothGetRadioInfo(radio, &mut info) == 0 {
                found.push(format_address(info.address.Anonymous.rgBytes));
            }
            CloseHandle(radio);
            if BluetoothFindNextRadio(find, &mut radio) == 0 {
                break;
            }
        }
        BluetoothFindRadioClose(find);
    }
    found
}

/// Other platforms do not expose it here, so "this computer" stays unknown and the swap rules are
/// stricter (see `multipoint::plan_connect`).
#[cfg(not(windows))]
pub fn addresses() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_is_printed_most_significant_byte_first() {
        assert_eq!(format_address([0x22, 0x11, 0x00, 0xCC, 0xBB, 0xAA]), "AA:BB:CC:00:11:22");
    }
}
