//! MDR V2 table 2 "pairing device management": the headphone's list of paired devices, which of
//! them are connected, and requests to connect or disconnect one.
//!
//! Decoded from Sony Sound Connect 13.3.0 (`oo0.*` peripheral messages, `po0.b`, `PeripheralInquiredType`,
//! `ConnectivityActionType`). Table 2 messages use frame data type `0x0e`. Nothing here has been
//! exercised on a headphone yet: every change is confirmed only by a matching readback of the list.

use serde::Serialize;

/// Frame data type of table 2 messages (`DataType.DATA_MDR_NO2`).
pub const TABLE2: u8 = 0x0e;
const ADDRESS_LENGTH: usize = 17;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedDevice {
    /// Bluetooth address as the headphone sends it, for example `AA:BB:CC:00:11:22`.
    pub address: String,
    pub name: String,
    /// The headphone reports a connected status above zero for devices that are connected now.
    pub connected: bool,
    pub status: u8,
    /// The device the headphone currently plays audio from.
    pub playback: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_of_device: Option<u32>,
    /// This computer, matched by its own Bluetooth address. Never disconnected automatically.
    pub this_computer: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    pub devices: Vec<PairedDevice>,
    /// How many devices the headphone lets stay connected at once (fourth byte of its capability reply).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<u8>,
}

fn same_address(left: &str, right: &str) -> bool {
    let normal = |text: &str| text.chars().filter(|c| c.is_ascii_hexdigit()).map(|c| c.to_ascii_uppercase()).collect::<String>();
    normal(left) == normal(right)
}

fn is_computer(device: &PairedDevice) -> bool {
    device.class_of_device.is_some_and(|class| (class >> 8) & 0x1f == 1)
}

/// What connecting a device takes, given the headphone's connection limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectPlan {
    AlreadyConnected,
    Connect,
    /// The headphone is at its limit: disconnect this device first, then connect.
    SwapOut { address: String, name: String },
}

/// Decide how to connect `target`. At the connection limit the device to drop is a connected one that
/// is not the device playing audio, not this computer and not the target. When this computer cannot be
/// identified, other computers are left alone rather than risk dropping the app's own link.
pub fn plan_connect(list: &DeviceList, target: &str) -> Result<ConnectPlan, String> {
    let device = list.find(target).ok_or("That device is not in the headphone's paired list. Refresh the list.")?;
    if device.connected {
        return Ok(ConnectPlan::AlreadyConnected);
    }
    let Some(limit) = list.max_connections.filter(|limit| *limit > 0) else {
        return Ok(ConnectPlan::Connect);
    };
    if list.connected_count() < usize::from(limit) {
        return Ok(ConnectPlan::Connect);
    }
    let knows_self = list.devices.iter().any(|candidate| candidate.this_computer);
    let victim = list.devices.iter().find(|candidate| {
        candidate.connected
            && !candidate.playback
            && !candidate.this_computer
            && !same_address(&candidate.address, target)
            && (knows_self || !is_computer(candidate))
    });
    match victim {
        Some(victim) => Ok(ConnectPlan::SwapOut { address: victim.address.clone(), name: victim.name.clone() }),
        None => Err(format!(
            "The headphone allows {limit} connected {} at once, and none can be dropped automatically: each is playing audio or is this computer. Disconnect one yourself first.",
            if limit == 1 { "device" } else { "devices" }
        )),
    }
}

impl DeviceList {
    /// Record which entry is this computer and the headphone's connection limit.
    pub fn annotate(&mut self, own_addresses: &[String], max_connections: Option<u8>) {
        for device in &mut self.devices {
            device.this_computer = own_addresses.iter().any(|own| same_address(own, &device.address));
        }
        self.max_connections = max_connections;
    }
    pub fn connected_count(&self) -> usize {
        self.devices.iter().filter(|device| device.connected).count()
    }
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"devices\":[]}".into())
    }
    pub fn find(&self, address: &str) -> Option<&PairedDevice> {
        self.devices.iter().find(|device| device.address.eq_ignore_ascii_case(address))
    }
}

/// Table 2 functions `0x32` and `0x33` carry the Bluetooth class of device (inquiry type 2); `0x30` is
/// the older layout without it (inquiry type 0).
pub fn inquiry_for_functions(functions: &[u8]) -> Option<u8> {
    if functions.iter().any(|function| *function == 0x32 || *function == 0x33) {
        Some(2)
    } else if functions.contains(&0x30) {
        Some(0)
    } else {
        None
    }
}

pub fn capability_request(inquiry: u8) -> Vec<u8> {
    vec![0x30, inquiry]
}

pub fn list_request(inquiry: u8) -> Vec<u8> {
    vec![0x36, inquiry]
}

/// `SET_EXTENDED_PARAM`: `[3c, type, action, address]` with action 0 = disconnect, 1 = connect.
/// Unpairing (action 2) is deliberately not offered.
pub fn action_request(inquiry: u8, connect: bool, address: &str) -> Vec<u8> {
    let mut payload = vec![0x3c, inquiry, u8::from(connect)];
    payload.extend_from_slice(address.as_bytes());
    payload
}

pub fn valid_address(address: &str) -> bool {
    let bytes = address.as_bytes();
    bytes.len() == ADDRESS_LENGTH
        && bytes.iter().enumerate().all(|(index, byte)| {
            if index % 3 == 2 {
                *byte == b':' || *byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn clean_name(bytes: &[u8]) -> String {
    let text: String = String::from_utf8_lossy(bytes).chars().filter(|c| !c.is_control()).collect();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        "Unnamed device".into()
    } else {
        trimmed.chars().take(80).collect()
    }
}

/// `[reply, type, count, record*, playbackSlot]` where a type 0 record is
/// `address(17), status, nameLen, name` and a type 2 record is `address(17), status, classOfDevice(3),
/// nameLen, name`. `reply` is `0x37` for a GET answer and `0x39` for a notification.
pub fn parse_list(payload: &[u8], reply: u8, inquiry: u8) -> Option<DeviceList> {
    if payload.len() < 4 || payload[0] != reply || payload[1] != inquiry || !matches!(inquiry, 0 | 2) {
        return None;
    }
    let count = usize::from(payload[2]);
    let mut at = 3;
    let mut devices = Vec::new();
    for _ in 0..count {
        let address = payload.get(at..at + ADDRESS_LENGTH)?;
        let address = std::str::from_utf8(address).ok().filter(|text| valid_address(text))?;
        let status = *payload.get(at + ADDRESS_LENGTH)?;
        let (class_of_device, name_length_at) = if inquiry == 2 {
            let class = payload.get(at + 18..at + 21)?;
            (Some(u32::from(class[0]) << 16 | u32::from(class[1]) << 8 | u32::from(class[2])), at + 21)
        } else {
            (None, at + 18)
        };
        let name_length = usize::from(*payload.get(name_length_at)?);
        if !(1..=128).contains(&name_length) {
            return None;
        }
        let name = payload.get(name_length_at + 1..name_length_at + 1 + name_length)?;
        devices.push(PairedDevice {
            address: address.to_string(),
            name: clean_name(name),
            connected: status > 0,
            status,
            playback: false,
            class_of_device,
            this_computer: false,
        });
        at = name_length_at + 1 + name_length;
    }
    if at + 1 != payload.len() {
        return None;
    }
    // The last byte is a connection *slot number*, not a position in the list: the Sony app marks the
    // device whose connected status (`oo0.a.d()`) equals it as the one with playback right
    // (`aVar.f() == kVarM.d()` in `n1`). A connected device's status is its slot (1, 2, ...).
    let playback_slot = payload[at];
    for device in &mut devices {
        device.playback = device.status > 0 && device.status == playback_slot;
    }
    Some(DeviceList { devices, max_connections: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(address: &str, status: u8, class: Option<[u8; 3]>, name: &str) -> Vec<u8> {
        let mut bytes = address.as_bytes().to_vec();
        bytes.push(status);
        if let Some(class) = class {
            bytes.extend(class);
        }
        bytes.push(name.len() as u8);
        bytes.extend(name.as_bytes());
        bytes
    }

    fn list(reply: u8, inquiry: u8, records: &[Vec<u8>], playback: u8) -> Vec<u8> {
        let mut payload = vec![reply, inquiry, records.len() as u8];
        for record in records {
            payload.extend(record);
        }
        payload.push(playback);
        payload
    }

    #[test]
    fn a_type_2_list_names_every_device_and_marks_connected_and_playing() {
        let class = Some([0x5a, 0x02, 0x0c]);
        let payload = list(0x37, 2, &[
            record("AA:BB:CC:00:11:22", 1, class, "Pixel 9"),
            record("AA:BB:CC:00:11:33", 0, class, "Work laptop"),
            record("AA:BB:CC:00:11:44", 2, class, "iPad"),
        ], 2);
        let parsed = parse_list(&payload, 0x37, 2).unwrap();
        let names: Vec<_> = parsed.devices.iter().map(|d| (d.name.as_str(), d.connected, d.playback)).collect();
        assert_eq!(names, [("Pixel 9", true, false), ("Work laptop", false, false), ("iPad", true, true)]);
        assert_eq!(parsed.devices[0].class_of_device, Some(0x5a020c));
        assert_eq!(parsed.find("aa:bb:cc:00:11:33").unwrap().name, "Work laptop", "address match ignores case");
    }

    #[test]
    fn the_playing_device_is_the_one_in_the_named_connection_slot_not_the_one_at_that_position() {
        // Slot 1 holds the laptop, slot 2 the phone, and the headphone says slot 1 plays. The phone is
        // listed first, so reading the byte as a list position would wrongly pick the second entry.
        let payload = list(0x37, 0, &[
            record("AA:BB:CC:00:11:22", 2, None, "Pixel 9"),
            record("AA:BB:CC:00:11:33", 1, None, "Work laptop"),
            record("AA:BB:CC:00:11:44", 0, None, "iPad"),
        ], 1);
        let parsed = parse_list(&payload, 0x37, 0).unwrap();
        let playing: Vec<_> = parsed.devices.iter().map(|device| (device.name.as_str(), device.playback)).collect();
        assert_eq!(playing, [("Pixel 9", false), ("Work laptop", true), ("iPad", false)]);
        // A slot nobody holds, or 0 (no device), marks nothing as playing.
        for slot in [0u8, 3] {
            let none = parse_list(&list(0x37, 0, &[record("AA:BB:CC:00:11:22", 2, None, "Pixel 9"), record("AA:BB:CC:00:11:33", 1, None, "Work laptop")], slot), 0x37, 0).unwrap();
            assert!(none.devices.iter().all(|device| !device.playback), "slot {slot}");
        }
    }

    #[test]
    fn a_type_0_list_has_no_class_of_device() {
        let payload = list(0x39, 0, &[record("AA:BB:CC:00:11:22", 1, None, "Phone")], 1);
        let parsed = parse_list(&payload, 0x39, 0).unwrap();
        assert_eq!((parsed.devices[0].class_of_device, parsed.devices[0].playback), (None, true));
    }

    #[test]
    fn replies_that_do_not_fit_their_own_declarations_are_rejected() {
        let good = list(0x37, 2, &[record("AA:BB:CC:00:11:22", 1, Some([0, 0, 0]), "Phone")], 0);
        assert!(parse_list(&good, 0x37, 2).is_some());
        assert!(parse_list(&good, 0x39, 2).is_none(), "wrong reply command");
        assert!(parse_list(&good, 0x37, 0).is_none(), "wrong layout for the inquiry type");
        let mut extra = good.clone();
        extra.push(0);
        assert!(parse_list(&extra, 0x37, 2).is_none(), "trailing bytes");
        assert!(parse_list(&good[..good.len() - 1], 0x37, 2).is_none(), "missing playback byte");
        let mut bad_address = good.clone();
        bad_address[4] = b'Z';
        assert!(parse_list(&bad_address, 0x37, 2).is_none(), "address that is not a Bluetooth address");
        let empty_name = list(0x37, 2, &[record("AA:BB:CC:00:11:22", 1, Some([0, 0, 0]), "")], 0);
        assert!(parse_list(&empty_name, 0x37, 2).is_none(), "the name length must be 1 to 128");
        assert_eq!(parse_list(&list(0x37, 2, &[], 0), 0x37, 2).unwrap().devices.len(), 0, "no paired devices is valid");
    }

    fn device(address: &str, name: &str, connected: bool, playback: bool, this_computer: bool, class: Option<u32>) -> PairedDevice {
        PairedDevice { address: address.into(), name: name.into(), connected, status: u8::from(connected), playback, class_of_device: class, this_computer }
    }
    fn three(limit: Option<u8>) -> DeviceList {
        DeviceList {
            devices: vec![
                device("AA:BB:CC:00:11:22", "Pixel 9", true, false, false, Some(0x5a020c)),
                device("AA:BB:CC:00:11:33", "Work laptop", true, true, true, Some(0x02010c)),
                device("AA:BB:CC:00:11:44", "iPad", false, false, false, Some(0x5a020c)),
            ],
            max_connections: limit,
        }
    }

    #[test]
    fn at_the_limit_the_inactive_other_device_is_dropped_to_make_room() {
        let plan = plan_connect(&three(Some(2)), "AA:BB:CC:00:11:44").unwrap();
        assert_eq!(plan, ConnectPlan::SwapOut { address: "AA:BB:CC:00:11:22".into(), name: "Pixel 9".into() });
    }

    #[test]
    fn below_the_limit_or_with_an_unknown_limit_nothing_is_dropped() {
        assert_eq!(plan_connect(&three(Some(3)), "AA:BB:CC:00:11:44").unwrap(), ConnectPlan::Connect);
        assert_eq!(plan_connect(&three(None), "AA:BB:CC:00:11:44").unwrap(), ConnectPlan::Connect);
        assert_eq!(plan_connect(&three(Some(2)), "aa:bb:cc:00:11:22").unwrap(), ConnectPlan::AlreadyConnected);
    }

    #[test]
    fn the_playing_device_and_this_computer_are_never_chosen() {
        let mut list = three(Some(2));
        list.devices[0].playback = true; // the phone plays audio
        list.devices[1].playback = false; // the laptop (this computer) does not
        let error = plan_connect(&list, "AA:BB:CC:00:11:44").unwrap_err();
        assert!(error.contains("none can be dropped automatically"), "{error}");
    }

    #[test]
    fn without_knowing_which_entry_is_this_computer_other_computers_are_left_alone() {
        let mut list = three(Some(2));
        for device in &mut list.devices {
            device.this_computer = false;
            device.playback = false;
        }
        list.devices[0].playback = true; // the phone plays, so the only other connected device is a computer
        assert!(plan_connect(&list, "AA:BB:CC:00:11:44").is_err(), "a computer is not dropped when 'this computer' is unknown");
        list.devices[0].playback = false;
        assert!(matches!(plan_connect(&list, "AA:BB:CC:00:11:44"), Ok(ConnectPlan::SwapOut { name, .. }) if name == "Pixel 9"));
    }

    #[test]
    fn annotating_marks_this_computer_by_address_in_any_format() {
        let mut list = three(None);
        list.annotate(&["aa-bb-cc-00-11-33".to_string()], Some(2));
        assert_eq!(list.devices.iter().filter(|d| d.this_computer).count(), 1);
        assert!(list.devices[1].this_computer);
        assert_eq!((list.max_connections, list.connected_count()), (Some(2), 2));
    }

    #[test]
    fn requests_use_the_documented_commands_and_never_unpair() {
        assert_eq!(list_request(2), [0x36, 2]);
        assert_eq!(capability_request(0), [0x30, 0]);
        let connect = action_request(2, true, "AA:BB:CC:00:11:22");
        assert_eq!(&connect[..3], [0x3c, 2, 1]);
        assert_eq!(&connect[3..], b"AA:BB:CC:00:11:22");
        assert_eq!(action_request(2, false, "AA:BB:CC:00:11:22")[2], 0);
        assert_eq!(inquiry_for_functions(&[0x30]), Some(0));
        assert_eq!(inquiry_for_functions(&[0x30, 0x33]), Some(2), "the class-of-device layout wins");
        assert_eq!(inquiry_for_functions(&[0x41]), None);
        assert!(valid_address("AA:BB:CC:00:11:22") && valid_address("aa-bb-cc-00-11-22"));
        assert!(!valid_address("AA:BB:CC:00:11") && !valid_address("AA:BB:CC:00:11:2G") && !valid_address(""));
    }
}
