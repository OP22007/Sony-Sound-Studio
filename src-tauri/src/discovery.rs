//! Read-only protocol-info exchange, independently implemented from static REA evidence.
//! V1/V2 request [00,00], strict response layouts, initial TX0 and inverted empty ACKs
//! are recorded in the private protocol request / ACK evidence exports.

use crate::device_controls::{
    validate_change, ConfirmedControl, ControlDescriptor, ControlKind, ControlOption, ControlValue,
    DeviceControlChange, DeviceControlConfirmation,
};
use crate::multipoint as mp;
use crate::protocol::{encode, Decoder, Frame};
use crate::sony_listening::{self, UpmixMode};
use crate::system_settings::{self as sys, GsCapability, GsKind, GsValue};
use crate::transport::Connection;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Eq, PartialEq)]
pub struct ProtocolInfo {
    pub format: &'static str,
    pub version: u32,
    pub ack_observed: bool,
    /// Byte 7 of the V2 reply (`EnableDisable`, 0 = enable): the headphone supports message table 2.
    pub table2: bool,
}

fn parse_protocol_info(payload: &[u8]) -> Option<ProtocolInfo> {
    if payload.len() < 2 || payload[..2] != [0x01, 0x00] {
        return None;
    }
    let (format, version, table2) = match payload.len() {
        4 => ("v1", u16::from_be_bytes([payload[2], payload[3]]) as u32, false),
        8 if payload[6] <= 1 && payload[7] <= 1 => (
            "v2",
            u32::from_be_bytes([payload[2], payload[3], payload[4], payload[5]]),
            payload[7] == 0,
        ),
        _ => return None,
    };
    Some(ProtocolInfo {
        format,
        version,
        ack_observed: false,
        table2,
    })
}

pub fn acknowledge(connection: &mut dyn Connection, frame: &Frame) -> Result<(), String> {
    if ![0x0c, 0x0e, 0x0d, 0x2c, 0x2d].contains(&frame.data_type) {
        return Ok(());
    }
    if frame.sequence > 1 {
        return Err(
            "Device sent an unrecognized sequence value; refusing to invent an ACK.".into(),
        );
    }
    let ack = Frame {
        data_type: 1,
        sequence: 1 - frame.sequence,
        payload: vec![],
    };
    let wire = encode(&ack, 0).map_err(|error| error.to_string())?;
    connection
        .write_all(&wire, Duration::from_secs(1))
        .map_err(|_| "Transport ACK write failed.".to_string())
}

#[cfg(test)]
pub fn query_protocol_info(
    connection: &mut dyn Connection,
    cancelled: &AtomicBool,
) -> Result<ProtocolInfo, String> {
    ReadSession::new().protocol_info(connection, cancelled)
}

pub struct BasicInfo {
    pub protocol: Result<ProtocolInfo, String>,
    pub model: Option<String>,
    pub firmware: Option<String>,
    pub battery: Option<BatteryInfo>,
    pub controls: ControlInfo,
    pub warnings: Vec<String>,
}

#[derive(Default)]
pub struct ControlInfo {
    pub support_reply_confirmed: bool,
    pub capabilities: Vec<String>,
    pub noise: Option<NoiseInfo>,
    pub noise_parameter_observations: Vec<Vec<u8>>,
    pub equalizer: Option<EqualizerInfo>,
    pub eq_layout: Option<EqLayout>,
    pub read_only_values: BTreeMap<String, ControlValue>,
    pub device_controls: Vec<ConfirmedControl>,
}

#[derive(Clone, Debug)]
pub struct NoiseInfo {
    pub inquiry: u8,
    pub value_change_status: u8,
    pub nc_asm_mode: u8,
    pub ambient_mode: u8,
    pub mode: &'static str,
    pub ambient_level: u8,
    pub focus_on_voice: bool,
    pub secondary_enabled: bool,
    pub adaptive_sensitivity: u8,
    pub raw_payload: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct EqualizerInfo {
    pub preset_code: u8,
    pub bands: Vec<u8>,
}

/// One preset a headphone offers in its equalizer capability reply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EqPresetEntry {
    pub id: u8,
    pub label: String,
}

/// The equalizer a headphone describes itself: band count, level steps per band, presets and a
/// label per band. Used for every Sony model except the XM6 3.1.5 path validated on hardware.
/// A band's raw step runs from 0 to `level_steps - 1`; the centre (0 dB) is `(level_steps - 1) / 2`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EqLayout {
    pub inquiry: u8,
    pub band_count: u8,
    pub level_steps: u8,
    pub presets: Vec<EqPresetEntry>,
    pub labels: Vec<String>,
    pub clear_bass_index: Option<usize>,
    pub customizable: bool,
}

impl EqLayout {
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "inquiry": self.inquiry,
            "bandCount": self.band_count,
            "levelSteps": self.level_steps,
            "customizable": self.customizable,
            "labels": self.labels,
            "clearBassIndex": self.clear_bass_index,
            "presets": self.presets.iter().map(|preset| serde_json::json!({ "id": preset.id, "label": preset.label })).collect::<Vec<_>>(),
        })
    }
}

/// `EqPresetId.CUSTOM`: the preset that holds hand-edited band values.
const EQ_CUSTOM_PRESET: u8 = 0xa0;
/// Equalizer functions in the order they are preferred (type 4 is the XM6 layout, type 0 the common
/// customizable one, type 2 presets only) with the inquiry type each uses.
const EQUALIZER_FUNCTIONS: [(u8, u8); 3] = [(0x57, 4), (0x50, 0), (0x52, 2)];

type EqMatcher = Box<dyn Fn(&EqualizerInfo) -> bool>;

enum GenericEqRequest {
    Bands(Vec<u8>),
    Preset(u8),
}

fn eq_preset_name(id: u8, device_name: &str) -> String {
    let fixed = match id {
        0 => "Off",
        1 => "Rock",
        2 => "Pop",
        3 => "Jazz",
        4 => "Dance",
        5 => "EDM",
        6 => "R&B / Hip-Hop",
        7 => "Acoustic",
        16 => "Bright",
        17 => "Excited",
        18 => "Mellow",
        19 => "Relaxed",
        20 => "Vocal",
        21 => "Treble",
        22 => "Bass",
        23 => "Speech",
        32 => "Gaming",
        33 => "FPS 1",
        34 => "FPS 2",
        35 => "FPS 3",
        48 => "Heavy",
        49 => "Clear",
        50 => "Hard",
        51 => "Soft",
        EQ_CUSTOM_PRESET => "Custom",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.into();
    }
    if (0xa1..=0xa5).contains(&id) {
        return format!("User setting {}", id - 0xa0);
    }
    let name: String = device_name.chars().filter(|c| !c.is_control()).take(40).collect();
    if name.trim().is_empty() {
        format!("Preset {id}")
    } else {
        name.trim().to_string()
    }
}

/// `[0x51, type, bandCount, levelSteps, presetCount, (id, nameLen, name)*]`, decoded from the Sony
/// app's capability parser and its `EqCapability(bandCount, levelSteps, presets)` constructor.
fn parse_eq_capability(payload: &[u8], inquiry: u8) -> Result<(u8, u8, Vec<EqPresetEntry>), String> {
    if payload.len() < 5 || payload[0] != 0x51 || payload[1] != inquiry {
        return Err("The equalizer capability reply has an unexpected shape.".into());
    }
    let (band_count, level_steps, declared) = (payload[2], payload[3], usize::from(payload[4]));
    let mut presets = Vec::new();
    let mut at = 5;
    while at < payload.len() {
        let Some(&name_length) = payload.get(at + 1) else {
            return Err("The equalizer capability reply is truncated.".into());
        };
        let end = at + 2 + usize::from(name_length);
        if end > payload.len() {
            return Err("The equalizer capability reply is truncated.".into());
        }
        let name = String::from_utf8_lossy(&payload[at + 2..end]);
        presets.push(EqPresetEntry { id: payload[at], label: eq_preset_name(payload[at], &name) });
        at = end;
    }
    if presets.len() != declared {
        return Err("The equalizer capability reply lists a different number of presets than it declares.".into());
    }
    let bands_ok = (1..=16).contains(&band_count) || (inquiry == 2 && band_count == 0);
    if !bands_ok || !(2..=64).contains(&level_steps) {
        return Err(format!(
            "The headphone reports {band_count} equalizer bands and {level_steps} level steps, which is outside what this app can control."
        ));
    }
    Ok((band_count, level_steps, presets))
}

fn eq_frequency_label(kind: u8, value: u16) -> String {
    match kind {
        1 if value < 1000 => format!("{value} Hz"),
        1 => {
            let tenths = value / 100;
            if tenths.is_multiple_of(10) {
                format!("{} kHz", tenths / 10)
            } else {
                format!("{}.{} kHz", tenths / 10, tenths % 10)
            }
        }
        2 => format!("{value} kHz"),
        _ => String::new(),
    }
}

/// `[0x5b, type, count, (kind, hi, lo)*]`: kind 1 = Hz, 2 = kHz, 16 = specific information
/// (value 1 = Clear Bass). Returns one label per band and the index of Clear Bass, if any.
fn parse_eq_band_info(payload: &[u8], inquiry: u8, band_count: u8) -> Option<(Vec<String>, Option<usize>)> {
    if payload.len() != 3 + 3 * usize::from(band_count)
        || payload[0] != 0x5b
        || payload[1] != inquiry
        || payload[2] != band_count
    {
        return None;
    }
    let mut labels = Vec::new();
    let mut clear_bass = None;
    for (index, entry) in payload[3..].chunks(3).enumerate() {
        let value = u16::from(entry[1]) << 8 | u16::from(entry[2]);
        let label = if entry[0] == 16 && value == 1 {
            clear_bass = Some(index);
            "Clear Bass".to_string()
        } else {
            eq_frequency_label(entry[0], value)
        };
        labels.push(if label.is_empty() { format!("Band {}", index + 1) } else { label });
    }
    Some((labels, clear_bass))
}

/// `[0x57, type, presetId, count, step*]`. Every step must lie inside the reported range.
fn parse_eq_params(payload: &[u8], inquiry: u8, band_count: u8, level_steps: u8) -> Result<EqualizerInfo, String> {
    let bad = || "The equalizer values reply does not match the layout the headphone reported.".to_string();
    if payload.len() < 4 || payload[0] != 0x57 || payload[1] != inquiry {
        return Err(bad());
    }
    let count = usize::from(payload[3]);
    let preset_only = inquiry == 2 && count == 0;
    if payload.len() != 4 + count || (!preset_only && count != usize::from(band_count)) {
        return Err(bad());
    }
    if payload[4..].iter().any(|step| *step >= level_steps) {
        return Err(bad());
    }
    Ok(EqualizerInfo { preset_code: payload[2], bands: payload[4..].to_vec() })
}

#[derive(Clone, Debug)]
pub struct PowerOffInfo {
    pub active: u8,
    pub selected_time: u8,
    pub options: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BgmInfo {
    enabled: bool,
    room_size: u8,
}

#[derive(Clone, Debug)]
pub enum SettingChange {
    NoiseMode(String),
    Equalizer(Vec<u8>),
    EqualizerPreset(u8),
    /// Connect or disconnect one of the headphone's paired devices (multipoint).
    MultipointDevice { address: String, connect: bool },
    MultipointRefresh,
    DeviceControl(DeviceControlChange),
    OnePlusEqProfile(crate::oneplus_eq::EqProfileChange),
    ProbeNoiseSchema,
}

#[derive(Clone, Debug)]
pub enum SettingConfirmation {
    Noise {
        before: NoiseInfo,
        noise: NoiseInfo,
        requested_mode: String,
        confirmed: bool,
        observations: Vec<Vec<u8>>,
    },
    Equalizer(EqualizerInfo),
    DeviceControl(DeviceControlConfirmation),
    OnePlusEqProfiles(Vec<crate::oneplus_eq::EqProfile>),
    NoiseSchemaProbe {
        capability: Vec<u8>,
        status: Vec<u8>,
        parameter: Vec<u8>,
        parameter19: Vec<u8>,
    },
}

impl MultipointFailure {
    /// A message for the user. A reset link is explained as what it usually is, not as a failure.
    fn explain(self, doing: &str, target: &str, dropped: Option<&str>) -> String {
        match self {
            Self::LinkReset { request_sent: true } => format!(
                "The headphone briefly reset its control link while {doing}. It does that when its connections change, so the change most likely went through. Sound Connect is reconnecting, and the list updates once it has."
            ),
            Self::LinkReset { request_sent: false } => format!(
                "The headphone's control link dropped before the request for {target} was sent, so nothing was changed for it{}. Sound Connect is reconnecting; try again in a few seconds.",
                dropped.map(|name| format!(" ({name} was already disconnected to make room)")).unwrap_or_default()
            ),
            Self::Other(error) => match dropped {
                Some(name) => format!("{name} was disconnected to make room, but {target} did not connect. {error}"),
                None => error,
            },
        }
    }
}

/// Errors that mean the control link itself went away (as opposed to the headphone refusing a request).
fn is_link_loss(error: &str) -> bool {
    error.contains("closed its control connection") || error.contains("control stream failed") || error.contains("could not be sent")
}

/// Why a multipoint change did not finish. A reset of the control link is common while the headphone
/// changes its connections, and the change itself usually still happens.
enum MultipointFailure {
    /// The control link dropped. `request_sent` says whether the connect or disconnect request had
    /// already been delivered when it did.
    LinkReset { request_sent: bool },
    Other(String),
}

impl From<String> for MultipointFailure {
    fn from(error: String) -> Self {
        if is_link_loss(&error) {
            Self::LinkReset { request_sent: true }
        } else {
            Self::Other(error)
        }
    }
}

/// The exact text used when the headphone closes the control link while the app is reading it.
pub const CLOSED_DURING_DISCOVERY: &str = "The device closed its control connection during read-only discovery.";

/// Did the headphone drop the control link while its settings were being read? A fresh connection
/// attempt often succeeds (the headphone may still have been tearing down a previous session).
pub fn closed_during_discovery(info: &BasicInfo) -> bool {
    info.protocol.as_ref().is_err_and(|error| error == CLOSED_DURING_DISCOVERY)
        || info.warnings.iter().any(|warning| warning.contains(CLOSED_DURING_DISCOVERY))
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn read_basics(
    connection: &mut dyn Connection,
    cancelled: &AtomicBool,
) -> (BasicInfo, ReadSession) {
    read_basics_with(connection, cancelled, true)
}

/// `extras` enables the reads added for the generic equalizer and multipoint device list. They are
/// skipped on the last connection attempt so a headphone that cannot answer them still connects.
pub fn read_basics_with(
    connection: &mut dyn Connection,
    cancelled: &AtomicBool,
    extras: bool,
) -> (BasicInfo, ReadSession) {
    let mut session = ReadSession::new();
    let protocol = session.protocol_info(connection, cancelled);
    if let Ok(info) = &protocol {
        session.table2_supported = info.table2;
    }
    let mut result = BasicInfo {
        protocol,
        model: None,
        firmware: None,
        battery: None,
        controls: ControlInfo::default(),
        warnings: vec![],
    };
    // Device-info request/response schemas below are established for V2 only.
    if result
        .protocol
        .as_ref()
        .is_ok_and(|info| info.format == "v2")
    {
        for inquiry in [1u8, 2] {
            match session.device_info(connection, cancelled, inquiry) {
                Ok(value) if inquiry == 1 => result.model = Some(value),
                Ok(value) => result.firmware = Some(value),
                Err(error) => result.warnings.push(error),
            }
        }
        match session.battery_info(connection, cancelled) {
            Ok(battery) => result.battery = Some(battery),
            Err(error) => result.warnings.push(error),
        }

        // Feature discovery is driven by the device's support-function reply.
        // The model label alone never unlocks settings. The one EQ layout here
        // is specifically the capture-verified XM6 3.1.5 10-band format.
        match session.supported_functions(connection, cancelled) {
            Ok(functions) => {
                result.controls.support_reply_confirmed = true;
                if !functions.is_empty() {
                    result.warnings.push(format!(
                        "The headphone advertised MDR V2 function IDs: {}.",
                        functions
                            .iter()
                            .map(|function| format!("0x{function:02x}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                // Exact FunctionType mappings decoded from the same APK. These
                // entries mark advertised support only; they do not create an
                // editable descriptor until a typed GET/SET/readback is ready.
                if functions.contains(&sys::WEAR_FUNCTION) {
                    result.controls.capabilities.push("wear".into());
                    match session.wear_read(connection, cancelled) {
                        Ok((true, on)) => result.controls.device_controls.push(ConfirmedControl {
                            descriptor: sys::wear_descriptor(),
                            value: ControlValue::Boolean(on),
                        }),
                        Ok((false, on)) => {
                            result.controls.read_only_values.insert("wear".into(), ControlValue::Boolean(on));
                            result.warnings.push("The headphone reports wear detection as currently unavailable, so it cannot be changed right now.".into());
                        }
                        Err(error) => result.warnings.push(format!("Wear detection could not be read: {error}")),
                    }
                }
                for slot in 1..=4u8 {
                    if !functions.contains(&(0xd0 + slot)) {
                        continue;
                    }
                    match session.general_setting_read(connection, cancelled, slot) {
                        Ok((control, true)) => {
                            let key = control.descriptor.key.clone();
                            if !result.controls.capabilities.contains(&key) {
                                result.controls.capabilities.push(key);
                            }
                            result.controls.device_controls.push(control);
                        }
                        Ok((control, false)) => {
                            let key = control.descriptor.key.clone();
                            if !result.controls.capabilities.contains(&key) {
                                result.controls.capabilities.push(key.clone());
                            }
                            result.controls.read_only_values.insert(key, control.value);
                            result.warnings.push(format!("The headphone reports {} as currently unavailable, so it cannot be changed right now.", control.descriptor.label));
                        }
                        Err(error) => result.warnings.push(format!("General setting {slot} could not be read: {error}")),
                    }
                }
                if [0x41, 0x42, 0x44].iter().any(|id| functions.contains(id)) {
                    result.controls.capabilities.push("guidance".into());
                }
                if [0xe1, 0xe3].iter().any(|id| functions.contains(id)) {
                    result.controls.capabilities.push("soundQuality".into());
                }
                if functions.contains(&0x48) {
                    result.controls.capabilities.push("notificationTone".into());
                }
                // AUTO_POWER_OFF is function 0x24 (inquiry 0x04); the wearing-aware
                // AUTO_POWER_OFF_WITH_WEARING_DETECTION is function 0x25 (inquiry 0x05).
                // Both use [28, inquiry, active, selectedTime] for SET (the 0x05 builder is
                // yn0.w0 in the shipped APK) with matching GET readback.
                let power_off_inquiry = if functions.contains(&0x24) {
                    Some(0x04)
                } else if functions.contains(&0x25) {
                    Some(0x05)
                } else {
                    None
                };
                if let Some(inquiry) = power_off_inquiry {
                    match session.power_off_setting(connection, cancelled, inquiry) {
                        Ok(info) => {
                            result.controls.capabilities.push("autoPowerOff".into());
                            let options = power_off_options(&info.options);
                            result.controls.device_controls.push(ConfirmedControl {
                                descriptor: ControlDescriptor {
                                    key: "autoPowerOff".into(),
                                    label: "Automatic power off".into(),
                                    description:
                                        "Choose the power-off behavior reported by the headphone."
                                            .into(),
                                    kind: ControlKind::Select,
                                    options: Some(options.clone()),
                                    min: None,
                                    max: None,
                                    step: None,
                                },
                                value: ControlValue::Integer(i64::from(info.active)),
                            });
                            let timer_options = options
                                .into_iter()
                                .filter(|option| matches!(&option.value, ControlValue::Integer(value) if (0..=4).contains(value)))
                                .collect::<Vec<_>>();
                            if timer_options.len() >= 2
                                && timer_options.iter().any(|option| {
                                    option.value
                                        == ControlValue::Integer(i64::from(info.selected_time))
                                })
                            {
                                result.controls.capabilities.push("autoPowerOffTime".into());
                                result.controls.device_controls.push(ConfirmedControl {
                                    descriptor: ControlDescriptor {
                                        key: "autoPowerOffTime".into(),
                                        label: "Power-off timer".into(),
                                        description: "Choose the timer value while preserving the headphone's automatic power-off mode.".into(),
                                        kind: ControlKind::Select,
                                        options: Some(timer_options),
                                        min: None,
                                        max: None,
                                        step: None,
                                    },
                                    value: ControlValue::Integer(i64::from(info.selected_time)),
                                });
                            }
                        }
                        Err(error) => result.warnings.push(error),
                    }
                }
                if functions.contains(&0xe2) {
                    match session.upscaling_setting(connection, cancelled) {
                        Ok(value) => {
                            result.controls.capabilities.push("dsee".into());
                            result.controls.device_controls.push(ConfirmedControl {
                                descriptor: ControlDescriptor {
                                    key: "dsee".into(),
                                    label: "DSEE upscaling".into(),
                                    description: "Choose whether the headphone applies DSEE upscaling automatically.".into(),
                                    kind: ControlKind::Select,
                                    options: Some(vec![
                                        ControlOption { value: ControlValue::Text("off".into()), label: "Off".into(), enabled: None },
                                        ControlOption { value: ControlValue::Text("auto".into()), label: "Auto".into(), enabled: None },
                                    ]),
                                    min: None,
                                    max: None,
                                    step: None,
                                },
                                value: ControlValue::Text(value.into()),
                            });
                        }
                        Err(error) => result.warnings.push(error),
                    }
                }
                let bgm_e4 = functions.contains(&0xe4);
                let bgm_eb = functions.contains(&0xeb);
                let bgm_ambiguous = bgm_e4 && bgm_eb;
                let bgm_inquiry = match (bgm_e4, bgm_eb) {
                    (true, false) => Some(0x03),
                    (false, true) => Some(0x09),
                    _ => None,
                };
                let cinema_supported = functions.contains(&0xe5);
                let series_supported = functions.contains(&0xec);
                if bgm_e4 || bgm_eb || cinema_supported || series_supported {
                    // Presence marks that some listening-mode path is advertised;
                    // the typed descriptor below remains gated on a complete mode map.
                    result.controls.capabilities.push("listeningMode".into());
                }
                session.configure_listening_modes(bgm_inquiry, cinema_supported, series_supported);
                let bgm = if !bgm_ambiguous {
                    if let Some(inquiry) = bgm_inquiry {
                        match session.bgm_setting(connection, cancelled, inquiry) {
                            Ok(value) => Some(value),
                            Err(error) => {
                                result.warnings.push(error);
                                None
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(info) = bgm {
                    result.controls.capabilities.push("bgmRoomSize".into());
                    result.controls.device_controls.push(ConfirmedControl {
                        descriptor: ControlDescriptor {
                            key: "bgmRoomSize".into(),
                            label: "Background Music distance".into(),
                            description:
                                "Choose the room ambience used by Background Music Effect.".into(),
                            kind: ControlKind::Select,
                            options: Some(vec![
                                ControlOption {
                                    value: ControlValue::Integer(0),
                                    label: "My room".into(),
                                    enabled: None,
                                },
                                ControlOption {
                                    value: ControlValue::Integer(1),
                                    label: "Living room".into(),
                                    enabled: None,
                                },
                                ControlOption {
                                    value: ControlValue::Integer(2),
                                    label: "Cafe".into(),
                                    enabled: None,
                                },
                            ]),
                            min: None,
                            max: None,
                            step: None,
                        },
                        value: ControlValue::Integer(i64::from(info.room_size)),
                    });
                }
                if series_supported && !bgm_ambiguous {
                    match session.discover_series_control(connection, cancelled, bgm) {
                        Ok(control) => result.controls.device_controls.push(control),
                        Err(error) => result.warnings.push(error),
                    }
                }
                if !series_supported
                    && !bgm_ambiguous
                    && (bgm_inquiry.is_some() || cinema_supported)
                {
                    let cinema = if cinema_supported {
                        match session.cinema_setting(connection, cancelled) {
                            Ok(value) => Some(value == "cinema"),
                            Err(error) => {
                                result.warnings.push(error);
                                None
                            }
                        }
                    } else {
                        None
                    };
                    let bgm_read_succeeded = bgm_inquiry.is_none() || bgm.is_some();
                    let cinema_read_succeeded = !cinema_supported || cinema.is_some();
                    if bgm_read_succeeded && cinema_read_succeeded {
                        let mode = if bgm.is_some_and(|value| value.enabled) {
                            "backgroundMusic"
                        } else if cinema == Some(true) {
                            "cinema"
                        } else {
                            "standard"
                        };
                        let mut options = vec![ControlOption {
                            value: ControlValue::Text("standard".into()),
                            label: "Standard".into(),
                            enabled: None,
                        }];
                        if bgm_inquiry.is_some() {
                            options.push(ControlOption {
                                value: ControlValue::Text("backgroundMusic".into()),
                                label: "Background Music".into(),
                                enabled: None,
                            });
                        }
                        if cinema_supported {
                            options.push(ControlOption {
                                value: ControlValue::Text("cinema".into()),
                                label: "Cinema".into(),
                                enabled: None,
                            });
                        }
                        result.controls.device_controls.push(ConfirmedControl {
                            descriptor: ControlDescriptor {
                                key: "listeningMode".into(),
                                label: "Listening mode".into(),
                                description: "Choose a supported listening mode.".into(),
                                kind: ControlKind::Select,
                                options: Some(options),
                                min: None,
                                max: None,
                                step: None,
                            },
                            value: ControlValue::Text(mode.into()),
                        });
                    }
                }
                if functions.contains(&0xfc) {
                    match session.speak_to_chat_setting(connection, cancelled) {
                        Ok((enabled, _)) => {
                            result.controls.capabilities.push("speakToChat".into());
                            result.controls.device_controls.push(ConfirmedControl {
                                descriptor: ControlDescriptor {
                                    key: "speakToChat".into(),
                                    label: "Speak-to-Chat".into(),
                                    description: "Enable or disable Speak-to-Chat. Its live effect status is reported separately by the headphone.".into(),
                                    kind: ControlKind::Toggle,
                                    options: None,
                                    min: None,
                                    max: None,
                                    step: None,
                                },
                                value: ControlValue::Boolean(enabled),
                            });
                        }
                        Err(error) => result.warnings.push(error),
                    }
                }
                if functions.contains(&0x70)
                    || functions.contains(&0x66)
                    || functions.contains(&0x69)
                {
                    let use_xm6_dual_mode = result.model.as_deref() == Some("WH-1000XM6")
                        && result.firmware.as_deref() == Some("3.1.5")
                        && functions.contains(&0x69);
                    let noise = session.noise_settings_for(
                        connection,
                        cancelled,
                        if use_xm6_dual_mode { 0x19 } else { 0x17 },
                    );
                    match noise {
                        Ok(noise) => {
                            if noise.inquiry == 0x19 {
                                match session.noise_capability(connection, cancelled, 0x19) {
                                    Ok(payload) => result.warnings.push(format!(
                                        "Read-only ambient-level capability bytes: [{}].",
                                        to_hex(&payload)
                                    )),
                                    Err(error) => result.warnings.push(format!(
                                        "Ambient-level range remains unavailable: {error}"
                                    )),
                                }
                            }
                            if noise.inquiry == 0x19 {
                                result
                                    .controls
                                    .noise_parameter_observations
                                    .push(noise.raw_payload.clone());
                            }
                            result.controls.capabilities.push("noise".into());
                            if noise.inquiry == 0x19 {
                                let focus = ConfirmedControl {
                                    descriptor: ControlDescriptor {
                                        key: "focusOnVoice".into(),
                                        label: "Focus on voice".into(),
                                        description: "Prioritize voices in ambient sound mode."
                                            .into(),
                                        kind: ControlKind::Toggle,
                                        options: None,
                                        min: None,
                                        max: None,
                                        step: None,
                                    },
                                    value: ControlValue::Boolean(noise.focus_on_voice),
                                };
                                if focus.validate().is_ok() {
                                    result.controls.capabilities.push("focusOnVoice".into());
                                    result.controls.device_controls.push(focus);
                                }
                            }
                            result.controls.noise = Some(noise);
                        }
                        Err(error) => result.warnings.push(error),
                    }
                }

                if result.model.as_deref() == Some("WH-1000XM6")
                    && result.firmware.as_deref() == Some("3.1.5")
                {
                    match session.equalizer_settings(connection, cancelled) {
                        Ok(equalizer) => {
                            result.controls.capabilities.push("equalizer".into());
                            result.controls.equalizer = Some(equalizer);
                        }
                        Err(error) => result.warnings.push(error),
                    }
                } else if let Some((_, inquiry)) = EQUALIZER_FUNCTIONS.iter().filter(|_| extras).find(|(function, _)| functions.contains(function)) {
                    // Any other Sony model describes its own equalizer (band count, level steps,
                    // presets and band frequencies), so the layout is read from the headphone
                    // instead of assumed. The raw replies are logged for diagnostics.
                    let mut notes = Vec::new();
                    match session.read_generic_equalizer(connection, cancelled, *inquiry, &mut notes) {
                        Ok((layout, info)) => {
                            result.controls.capabilities.push("equalizer".into());
                            result.controls.equalizer = Some(info);
                            result.controls.eq_layout = Some(layout.clone());
                            session.eq_layout = Some(layout);
                        }
                        Err(error) => result.warnings.push(format!("Equalizer layout was not confirmed: {error}")),
                    }
                    result.warnings.extend(notes);
                }
                if extras && session.table2_supported {
                    let mut notes = Vec::new();
                    match session.multipoint_discover(connection, cancelled, &mut notes) {
                        Ok(Some(list)) => {
                            result.controls.capabilities.push("multipointDevices".into());
                            result.controls.read_only_values.insert("multipointDevices".into(), ControlValue::Text(list.to_json_string()));
                        }
                        Ok(None) => {}
                        Err(error) => result.warnings.push(format!("Multipoint devices could not be read: {error}")),
                    }
                    result.warnings.extend(notes);
                }
            }
            Err(error) => result.warnings.push(error),
        }
    }
    session.install_device_controls(&result.controls.device_controls);
    (result, session)
}

#[derive(Debug, Eq, PartialEq)]
pub struct BatteryInfo {
    pub level: u8,
    pub charging: &'static str,
}

fn parse_battery(payload: &[u8]) -> Option<BatteryInfo> {
    if payload.len() != 4 || payload[..2] != [0x23, 0] || payload[2] > 100 {
        return None;
    }
    let charging = match payload[3] {
        0 => "notCharging",
        1 => "charging",
        2 => "unknown",
        3 => "charged",
        _ => return None,
    };
    Some(BatteryInfo {
        level: payload[2],
        charging,
    })
}

fn parse_upscaling_value(payload: &[u8], command: u8) -> Option<&'static str> {
    if payload.len() != 3 || payload[0] != command || payload[1] != 0x01 {
        return None;
    }
    match payload[2] {
        0 => Some("off"),
        1 => Some("auto"),
        _ => None,
    }
}

fn parse_power_off_options(payload: &[u8], inquiry: u8) -> Option<Vec<u8>> {
    if !matches!(inquiry, 0x04 | 0x05)
        || payload.len() < 4
        || payload.len() > 20
        || payload[..2] != [0x21, inquiry]
        || usize::from(payload[2]) != payload.len() - 3
        || payload[2] == 0
    {
        return None;
    }
    let supported = |value: u8| match inquiry {
        0x04 => matches!(value, 0..=4 | 17),
        0x05 => matches!(value, 0..=4 | 16 | 17),
        _ => false,
    };
    let options = payload[3..].to_vec();
    if options.iter().any(|value| !supported(*value))
        || options
            .iter()
            .enumerate()
            .any(|(index, value)| options[..index].contains(value))
    {
        return None;
    }
    Some(options)
}

fn parse_power_off_value(
    payload: &[u8],
    command: u8,
    inquiry: u8,
    options: &[u8],
) -> Option<(u8, u8)> {
    if !matches!(inquiry, 0x04 | 0x05)
        || payload.len() != 4
        || payload[..2] != [command, inquiry]
        || !options.contains(&payload[2])
        || !options.contains(&payload[3])
    {
        return None;
    }
    Some((payload[2], payload[3]))
}

fn power_off_options(values: &[u8]) -> Vec<ControlOption> {
    values
        .iter()
        .filter_map(|value| {
            let label = match value {
                0 => "5 minutes",
                1 => "30 minutes",
                2 => "60 minutes",
                3 => "180 minutes",
                4 => "15 minutes",
                16 => "When removed from ears",
                17 => "Disabled",
                _ => return None,
            };
            Some(ControlOption {
                value: ControlValue::Integer(i64::from(*value)),
                label: label.into(),
                enabled: None,
            })
        })
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_cinema_value(payload: &[u8], command: u8) -> Option<&'static str> {
    if payload.len() != 3 || payload[0] != command || payload[1] != 0x04 {
        return None;
    }
    match payload[2] {
        0 => Some("cinema"),
        1 => Some("standard"),
        _ => None,
    }
}

fn parse_bgm_value(payload: &[u8], command: u8, inquiry: u8) -> Option<BgmInfo> {
    if payload.len() != 4
        || payload[0] != command
        || payload[1] != inquiry
        || !matches!(inquiry, 0x03 | 0x09)
        || payload[2] > 1
        || payload[3] > 2
    {
        return None;
    }
    Some(BgmInfo {
        enabled: payload[2] == 0,
        room_size: payload[3],
    })
}

fn parse_speak_to_chat_value(payload: &[u8], command: u8) -> Option<(bool, u8)> {
    if payload.len() != 4
        || payload[0] != command
        || payload[1] != 0x0c
        || payload[2] > 1
        || payload[3] > 1
    {
        return None;
    }
    Some((payload[2] == 0, payload[3]))
}

fn parse_noise_payload(payload: &[u8], expected_command: Option<u8>) -> Option<NoiseInfo> {
    if payload.len() == 7 && payload[..2] == [0x67, 0x17] {
        if expected_command.is_some_and(|command| command != 0x67)
            || payload[2] > 1
            || payload[3] > 1
            || payload[4] > 1
            || payload[5] > 1
        {
            return None;
        }
        let mode = if payload[3] == 0 {
            "off"
        } else if payload[4] == 1 {
            "ambient"
        } else {
            "cancelling"
        };
        return Some(NoiseInfo {
            inquiry: 0x17,
            value_change_status: payload[2],
            nc_asm_mode: payload[4],
            ambient_mode: payload[5],
            mode,
            ambient_level: payload[6],
            focus_on_voice: payload[5] == 1,
            secondary_enabled: false,
            adaptive_sensitivity: 0,
            raw_payload: payload.to_vec(),
        });
    }

    if payload.len() == 9
        && matches!(payload[0], 0x67 | 0x69)
        && payload[1] == 0x19
        && expected_command.is_none_or(|command| command == payload[0])
        && payload[2] <= 1 // ValueChangeStatus
        && payload[3] <= 1 // NcAsmOnOffValue
        && payload[4] <= 1 // NcAsmMode: NC=0, ASM=1
        && payload[5] <= 1 // AmbientSoundMode: NORMAL=0, VOICE=1
        && payload[7] <= 1 // NcAsmOnOffValue
        && payload[8] <= 2
    // NoiseAdaptiveSensitivity: STANDARD=0, HIGH=1, LOW=2
    {
        let mode = if payload[3] == 0 {
            "off"
        } else if payload[4] == 1 {
            "ambient"
        } else {
            "cancelling"
        };
        return Some(NoiseInfo {
            inquiry: 0x19,
            value_change_status: payload[2],
            nc_asm_mode: payload[4],
            ambient_mode: payload[5],
            mode,
            ambient_level: payload[6],
            focus_on_voice: payload[5] == 1,
            secondary_enabled: payload[7] == 1,
            adaptive_sensitivity: payload[8],
            raw_payload: payload.to_vec(),
        });
    }
    None
}

fn parse_device_string(payload: &[u8], inquiry: u8) -> Option<String> {
    if payload.len() < 4 || payload[..2] != [5, inquiry] || payload.len() != payload[2] as usize + 3
    {
        return None;
    }
    let value = std::str::from_utf8(&payload[3..]).ok()?;
    if value.len() > 128 || !value.bytes().all(|byte| (32..=126).contains(&byte)) {
        return None;
    }
    Some(value.into())
}

/// A general setting the headphone described itself (touch panel, multipoint, voice guidance...).
#[derive(Clone, Debug)]
struct GeneralSettingSlot {
    inquiry: u8,
    capability: GsCapability,
}

pub struct ReadSession {
    sequence: u8,
    synchronized: bool,
    decoder: Decoder,
    noise_observations: Vec<Vec<u8>>,
    noise_inquiry: Option<u8>,
    listening_bgm_inquiry: Option<u8>,
    listening_cinema_supported: bool,
    listening_series_supported: bool,
    listening_series_options: Vec<UpmixMode>,
    listening_series_mode: Option<UpmixMode>,
    listening_bgm_enabled: Option<bool>,
    listening_bgm_room_size: Option<u8>,
    listening_cinema_enabled: Option<bool>,
    speak_to_chat_supported: bool,
    speak_to_chat_aux: Option<u8>,
    wear_supported: bool,
    general_settings: BTreeMap<String, GeneralSettingSlot>,
    power_off_inquiry: Option<u8>,
    power_off_options: Vec<u8>,
    device_control_descriptors: BTreeMap<String, ControlDescriptor>,
    device_control_values: BTreeMap<String, ControlValue>,
    device_updates: Vec<DeviceControlConfirmation>,
    eq_layout: Option<EqLayout>,
    table2_supported: bool,
    multipoint_inquiry: Option<u8>,
    multipoint_json: Option<String>,
    multipoint_wait: Duration,
    multipoint_poll: Duration,
    multipoint_limit: Option<u8>,
    own_addresses: Vec<String>,
}

impl ReadSession {
    fn new() -> Self {
        Self {
            sequence: 0,
            synchronized: true,
            decoder: Decoder::default(),
            noise_observations: Vec::new(),
            noise_inquiry: None,
            listening_bgm_inquiry: None,
            listening_cinema_supported: false,
            listening_series_supported: false,
            listening_series_options: Vec::new(),
            listening_series_mode: None,
            listening_bgm_enabled: None,
            listening_bgm_room_size: None,
            listening_cinema_enabled: None,
            speak_to_chat_supported: false,
            speak_to_chat_aux: None,
            wear_supported: false,
            general_settings: BTreeMap::new(),
            power_off_inquiry: None,
            power_off_options: Vec::new(),
            device_control_descriptors: BTreeMap::new(),
            device_control_values: BTreeMap::new(),
            device_updates: Vec::new(),
            eq_layout: None,
            table2_supported: false,
            multipoint_inquiry: None,
            multipoint_json: None,
            multipoint_wait: Duration::from_secs(20),
            multipoint_poll: Duration::from_millis(600),
            multipoint_limit: None,
            own_addresses: Vec::new(),
        }
    }

    pub fn is_synchronized(&self) -> bool {
        self.synchronized
    }

    pub fn feed_frames(&mut self, bytes: &[u8]) -> Vec<Result<Frame, crate::protocol::CodecError>> {
        let frames = self.decoder.feed(bytes);
        for frame in &frames {
            if let Ok(frame) = frame {
                self.record_noise_observation(frame);
            }
        }
        frames
    }

    pub fn drain_noise_observations(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.noise_observations)
    }

    pub fn install_device_controls(&mut self, controls: &[ConfirmedControl]) {
        for control in controls {
            if control.validate().is_ok() {
                let key = control.descriptor.key.clone();
                self.device_control_descriptors
                    .insert(key.clone(), control.descriptor.clone());
                self.device_control_values
                    .insert(key, control.value.clone());
            }
        }
    }

    fn configure_listening_modes(&mut self, bgm_inquiry: Option<u8>, cinema: bool, series: bool) {
        self.listening_bgm_inquiry = bgm_inquiry.filter(|inquiry| matches!(inquiry, 0x03 | 0x09));
        self.listening_cinema_supported = cinema;
        self.listening_series_supported = series;
    }

    pub fn drain_device_updates(&mut self) -> Vec<DeviceControlConfirmation> {
        std::mem::take(&mut self.device_updates)
    }

    /// Read an NCASM inquiry while retaining its full payload. Inquiry 0x19 is
    /// currently treated as raw data only; no field semantics are inferred.
    pub fn noise_parameter(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
    ) -> Result<Vec<u8>, String> {
        if !matches!(inquiry, 0x17 | 0x19) {
            return Err("Noise-control inquiry is not validated for raw readback.".into());
        }
        self.exchange(connection, cancelled, vec![0x66, inquiry], |payload| {
            payload.len() >= 2 && payload.len() <= 128 && payload[..2] == [0x67, inquiry]
        })
    }

    fn noise_capability(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
    ) -> Result<Vec<u8>, String> {
        if !matches!(inquiry, 0x17 | 0x19) {
            return Err("Noise-control capability inquiry is not validated.".into());
        }
        self.exchange(connection, cancelled, vec![0x60, inquiry], |payload| {
            payload.len() >= 2 && payload.len() <= 128 && payload[..2] == [0x61, inquiry]
        })
    }

    fn power_off_capability(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        if !matches!(inquiry, 0x04 | 0x05) {
            return Err("Automatic power-off inquiry is not validated.".into());
        }
        let response = self.exchange(connection, cancelled, vec![0x20, inquiry], |payload| {
            payload.len() >= 3 && payload.len() <= 20 && payload[..2] == [0x21, inquiry]
        })?;
        let options = parse_power_off_options(&response, inquiry)
            .ok_or_else(|| "Automatic power-off capability fields are invalid.".to_string())?;
        Ok((response, options))
    }

    fn power_off_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
    ) -> Result<PowerOffInfo, String> {
        if !matches!(inquiry, 0x04 | 0x05) {
            return Err("Editable automatic power off is only validated for inquiries 0x04 and 0x05.".into());
        }
        let (_capability, options) = self.power_off_capability(connection, cancelled, inquiry)?;
        let payload = self.exchange(connection, cancelled, vec![0x26, inquiry], |payload| {
            parse_power_off_value(payload, 0x27, inquiry, &options).is_some()
        })?;
        let (active, selected_time) = parse_power_off_value(&payload, 0x27, inquiry, &options)
            .ok_or_else(|| "Automatic power-off parameter fields are invalid.".to_string())?;
        self.power_off_inquiry = Some(inquiry);
        self.power_off_options = options.clone();
        Ok(PowerOffInfo {
            active,
            selected_time,
            options,
        })
    }

    fn power_off_value(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<(u8, u8), String> {
        let inquiry = self
            .power_off_inquiry
            .ok_or_else(|| "Automatic power-off control has no confirmed inquiry.".to_string())?;
        let options = self.power_off_options.clone();
        let payload = self.exchange(connection, cancelled, vec![0x26, inquiry], |payload| {
            parse_power_off_value(payload, 0x27, inquiry, &options).is_some()
        })?;
        parse_power_off_value(&payload, 0x27, inquiry, &options)
            .ok_or_else(|| "Automatic power-off readback fields are invalid.".into())
    }

    /// Parse the validated XM6 inquiry-0x19 NTFY_PARAM frame for passive state
    /// updates. Inquiry 0x17 NTFY field semantics have not been validated.
    pub fn parse_noise_notification(&self, payload: &[u8]) -> Option<NoiseInfo> {
        if self.noise_inquiry != Some(0x19) {
            return None;
        }
        parse_noise_payload(payload, Some(0x69))
    }

    pub fn apply_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        change: SettingChange,
    ) -> Result<SettingConfirmation, String> {
        match change {
            SettingChange::ProbeNoiseSchema => {
                let capability =
                    self.exchange(connection, cancelled, vec![0x60, 0x17], |payload| {
                        payload.len() >= 2 && payload[..2] == [0x61, 0x17]
                    })?;
                let status = self.exchange(connection, cancelled, vec![0x62, 0x17], |payload| {
                    payload.len() >= 2 && payload[..2] == [0x63, 0x17]
                })?;
                let parameter =
                    self.exchange(connection, cancelled, vec![0x66, 0x17], |payload| {
                        payload.len() >= 2 && payload[..2] == [0x67, 0x17]
                    })?;
                let parameter19 = self.noise_parameter(connection, cancelled, 0x19)?;
                Ok(SettingConfirmation::NoiseSchemaProbe {
                    capability,
                    status,
                    parameter,
                    parameter19,
                })
            }
            SettingChange::NoiseMode(mode) => self.apply_noise_mode(
                connection,
                cancelled,
                &mode,
                Duration::from_secs(8),
                Duration::from_millis(200),
            ),
            SettingChange::DeviceControl(change) => {
                self.apply_device_control(connection, cancelled, change)
            }
            SettingChange::OnePlusEqProfile(_) => {
                Err("OnePlus EQ profile actions cannot be sent through the Sony adapter.".into())
            }
            SettingChange::MultipointDevice { address, connect } => {
                self.apply_multipoint_device(connection, cancelled, &address, connect)
            }
            SettingChange::MultipointRefresh => {
                let inquiry = self.multipoint_inquiry.ok_or("This headphone has not described its paired devices.")?;
                let list = self.multipoint_list(connection, cancelled, inquiry)?;
                Ok(self.multipoint_confirmation(&list))
            }
            SettingChange::EqualizerPreset(id) => {
                let layout = self
                    .eq_layout
                    .clone()
                    .ok_or("This headphone has not described an equalizer with presets.")?;
                self.apply_generic_equalizer(connection, cancelled, &layout, GenericEqRequest::Preset(id))
            }
            SettingChange::Equalizer(bands) if self.eq_layout.is_some() => {
                let layout = self.eq_layout.clone().expect("checked above");
                self.apply_generic_equalizer(connection, cancelled, &layout, GenericEqRequest::Bands(bands))
            }
            SettingChange::Equalizer(bands) => {
                self.require_standard_processing(connection, cancelled)?;
                if bands.len() != 10 || bands.iter().any(|value| *value > 12) {
                    return Err(
                        "XM6 custom EQ requires ten verified values in the range 0–12.".into(),
                    );
                }
                let mut payload = Vec::with_capacity(14);
                payload.extend_from_slice(&[0x58, 0x04, 0xa0, 0x0a]);
                payload.extend_from_slice(&bands);
                let before = self.equalizer_settings(connection, cancelled)?;
                if before.bands == bands {
                    return Ok(SettingConfirmation::Equalizer(before));
                }
                self.write_acknowledged(connection, cancelled, &payload)?;
                let confirmed = self.wait_for_equalizer_bands(connection, cancelled, &bands);
                let Ok(confirmed) = confirmed else {
                    let mut restore = Vec::with_capacity(14);
                    restore.extend_from_slice(&[0x58, 0x04, 0xa0, 0x0a]);
                    restore.extend_from_slice(&before.bands);
                    if self
                        .write_acknowledged(connection, cancelled, &restore)
                        .is_ok()
                        && self
                            .wait_for_equalizer_bands(connection, cancelled, &before.bands)
                            .is_ok()
                    {
                        return Err("The headphone did not apply the requested equalizer values; the previous profile was restored and verified.".into());
                    }
                    return Err("The headphone did not confirm the requested equalizer values, and the previous profile could not be verified after restoration. Reconnect and inspect the headphone before retrying.".into());
                };
                Ok(SettingConfirmation::Equalizer(confirmed))
            }
        }
    }

    fn apply_device_control(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        change: DeviceControlChange,
    ) -> Result<SettingConfirmation, String> {
        let descriptor = self
            .device_control_descriptors
            .get(&change.key)
            .ok_or_else(|| format!("The device did not confirm the {} control.", change.key))?;
        validate_change(descriptor, change.value.clone())?;
        if change.key == "dsee" {
            self.require_standard_processing(connection, cancelled)?;
            return self.apply_upscaling_setting(connection, cancelled, change.value);
        }
        if change.key == "listeningMode" {
            return self.apply_listening_mode(connection, cancelled, change.value);
        }
        if change.key == "speakToChat" {
            return self.apply_speak_to_chat(connection, cancelled, change.value);
        }
        if change.key == "wear" {
            return self.apply_wear(connection, cancelled, change.value);
        }
        if self.general_settings.contains_key(&change.key) {
            return self.apply_general_setting(connection, cancelled, change);
        }
        if change.key == "bgmRoomSize" {
            return self.apply_bgm_room_size(connection, cancelled, change.value);
        }
        if matches!(change.key.as_str(), "autoPowerOff" | "autoPowerOffTime") {
            return self.apply_power_off_setting(connection, cancelled, change);
        }
        if self.noise_inquiry != Some(0x19) {
            return Err("This device control requires the verified inquiry 0x19 session.".into());
        }
        let before = self.noise_settings(connection, cancelled)?;
        if before.value_change_status != 1 {
            return Err("The current noise-control state is still changing; reconnect or wait for a final read before updating it.".into());
        }
        let mut requested = before.clone();
        match (change.key.as_str(), &change.value) {
            ("focusOnVoice", ControlValue::Boolean(value)) => {
                // The voice option belongs to Ambient sound mode. In Noise cancelling or Off the
                // headphone keeps its previous value, so a write could never be confirmed.
                if before.mode != "ambient" {
                    return Err("Focus on voice only applies in Ambient sound mode. Switch Noise control to Ambient sound, then try again.".into());
                }
                requested.ambient_mode = u8::from(*value);
                requested.focus_on_voice = *value;
            }
            _ => {
                return Err(
                    "The device setting value does not match its confirmed control type.".into(),
                )
            }
        }
        let payload = [
            0x68,
            0x19,
            1,
            u8::from(requested.mode != "off"),
            requested.nc_asm_mode,
            requested.ambient_mode,
            requested.ambient_level,
            u8::from(requested.secondary_enabled),
            requested.adaptive_sensitivity,
        ];
        self.write_acknowledged(connection, cancelled, &payload)?;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let current = self.noise_settings(connection, cancelled)?;
            let matched = match change.key.as_str() {
                "focusOnVoice" => current.focus_on_voice == requested.focus_on_voice,
                _ => false,
            };
            if matched && current.value_change_status == 1 {
                let before_value = self
                    .device_control_values
                    .get(&change.key)
                    .cloned()
                    .unwrap_or_else(|| change.value.clone());
                self.device_control_values
                    .insert(change.key.clone(), change.value.clone());
                return Ok(SettingConfirmation::DeviceControl(
                    DeviceControlConfirmation {
                        key: change.key,
                        before: before_value,
                        confirmed: change.value,
                    },
                ));
            }
            if Instant::now() >= deadline {
                return Err("The headphone did not confirm the requested control state.".into());
            }
            if cancelled.load(Ordering::Acquire) {
                return Err("The device control readback was cancelled.".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn apply_power_off_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        change: DeviceControlChange,
    ) -> Result<SettingConfirmation, String> {
        if !matches!(self.power_off_inquiry, Some(0x04 | 0x05)) {
            return Err(
                "This writable power-off control requires a validated power-off inquiry.".into(),
            );
        }
        let ControlValue::Integer(value) = change.value else {
            return Err("Automatic power-off options must be integer values from the device capability response.".into());
        };
        let selected = u8::try_from(value)
            .map_err(|_| "Automatic power-off option is outside the protocol range.")?;
        if !self.power_off_options.contains(&selected) {
            return Err(
                "The requested power-off option was not advertised by the headphone.".into(),
            );
        }
        let before = self.power_off_value(connection, cancelled)?;
        let mut requested = before;
        match change.key.as_str() {
            "autoPowerOff" => requested.0 = selected,
            "autoPowerOffTime" => requested.1 = selected,
            _ => return Err("Unknown automatic power-off setting.".into()),
        }
        if requested == before {
            return Ok(SettingConfirmation::DeviceControl(
                DeviceControlConfirmation {
                    key: change.key,
                    before: ControlValue::Integer(i64::from(selected)),
                    confirmed: ControlValue::Integer(i64::from(selected)),
                },
            ));
        }
        if let Err(error) = self.write_power_off_and_confirm(connection, cancelled, requested) {
            if self
                .write_power_off_and_confirm(connection, cancelled, before)
                .is_ok()
            {
                return Err(format!(
                    "{error} The previous automatic power-off settings were restored and verified."
                ));
            }
            return Err(format!("{error} The previous automatic power-off settings could not be verified after restoration; reconnect and inspect the headphone before retrying."));
        }
        let previous = if change.key == "autoPowerOff" {
            before.0
        } else {
            before.1
        };
        self.device_control_values.insert(
            change.key.clone(),
            ControlValue::Integer(i64::from(selected)),
        );
        Ok(SettingConfirmation::DeviceControl(
            DeviceControlConfirmation {
                key: change.key,
                before: ControlValue::Integer(i64::from(previous)),
                confirmed: ControlValue::Integer(i64::from(selected)),
            },
        ))
    }

    fn write_power_off_and_confirm(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        requested: (u8, u8),
    ) -> Result<(), String> {
        let inquiry = self
            .power_off_inquiry
            .ok_or_else(|| "Automatic power-off inquiry is no longer available.".to_string())?;
        if !matches!(inquiry, 0x04 | 0x05) || requested.0 > 17 || requested.1 > 17 {
            return Err("Automatic power-off SET values do not match the verified schema.".into());
        }
        self.write_acknowledged(
            connection,
            cancelled,
            &[0x28, inquiry, requested.0, requested.1],
        )?;
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            match self.power_off_value(connection, cancelled) {
                Ok(value) if value == requested => return Ok(()),
                Ok(_) => {}
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Err(
                    "The headphone did not confirm the requested automatic power-off settings."
                        .into(),
                );
            }
            if cancelled.load(Ordering::Acquire) {
                return Err("Automatic power-off readback was cancelled.".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn upscaling_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<&'static str, String> {
        let payload = self.exchange(connection, cancelled, vec![0xe6, 0x01], |payload| {
            matches!(payload, [0xe7, 0x01, 0] | [0xe7, 0x01, 1])
        })?;
        parse_upscaling_value(&payload, 0xe7)
            .ok_or_else(|| "DSEE setting response fields are invalid.".into())
    }

    fn speak_to_chat_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<(bool, u8), String> {
        self.speak_to_chat_supported = true;
        let payload = self.exchange(connection, cancelled, vec![0xf6, 0x0c], |payload| {
            parse_speak_to_chat_value(payload, 0xf7).is_some()
        })?;
        let (enabled, aux) = parse_speak_to_chat_value(&payload, 0xf7)
            .ok_or_else(|| "Speak-to-Chat response fields are invalid.".to_string())?;
        self.speak_to_chat_aux = Some(aux);
        Ok((enabled, aux))
    }

    fn apply_speak_to_chat(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        value: ControlValue,
    ) -> Result<SettingConfirmation, String> {
        if !self.speak_to_chat_supported {
            return Err(
                "Speak-to-Chat Type 2 was not advertised by the connected headphone.".into(),
            );
        }
        let ControlValue::Boolean(enabled) = value else {
            return Err("Speak-to-Chat requires a confirmed on/off value.".into());
        };
        let (before_enabled, aux) = self.speak_to_chat_setting(connection, cancelled)?;
        if before_enabled == enabled {
            return Ok(SettingConfirmation::DeviceControl(
                DeviceControlConfirmation {
                    key: "speakToChat".into(),
                    before: ControlValue::Boolean(before_enabled),
                    confirmed: ControlValue::Boolean(enabled),
                },
            ));
        }
        if let Err(error) =
            self.write_speak_to_chat_and_confirm(connection, cancelled, enabled, aux)
        {
            if self
                .write_speak_to_chat_and_confirm(connection, cancelled, before_enabled, aux)
                .is_ok()
            {
                return Err(format!(
                    "{error} The previous Speak-to-Chat setting was restored and verified."
                ));
            }
            return Err(format!("{error} The previous Speak-to-Chat setting could not be verified after restoration; reconnect and inspect the headphone before retrying."));
        }
        let before = ControlValue::Boolean(before_enabled);
        let confirmed = ControlValue::Boolean(enabled);
        self.device_control_values
            .insert("speakToChat".into(), confirmed.clone());
        Ok(SettingConfirmation::DeviceControl(
            DeviceControlConfirmation {
                key: "speakToChat".into(),
                before,
                confirmed,
            },
        ))
    }

    fn write_speak_to_chat_and_confirm(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        enabled: bool,
        aux: u8,
    ) -> Result<(), String> {
        self.write_acknowledged(
            connection,
            cancelled,
            &[0xf8, 0x0c, if enabled { 0 } else { 1 }, aux],
        )?;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match self.speak_to_chat_setting(connection, cancelled) {
                Ok(current) if current == (enabled, aux) => {
                    return Ok(());
                }
                Ok(_) => {}
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Err(
                    "The headphone did not confirm its requested Speak-to-Chat setting.".into(),
                );
            }
            if cancelled.load(Ordering::Acquire) {
                return Err("The Speak-to-Chat readback was cancelled.".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Record a device-reported value and queue an update only when it changed.
    fn observe_device_value(&mut self, key: &str, value: ControlValue) {
        if !self.device_control_descriptors.contains_key(key) {
            return;
        }
        let before = self.device_control_values.insert(key.into(), value.clone());
        if let Some(before) = before.filter(|before| *before != value) {
            self.device_updates.push(DeviceControlConfirmation {
                key: key.into(),
                before,
                confirmed: value,
            });
        }
    }

    /// Wear detection (function 0xF1): returns whether the setting is currently enabled and its value.
    fn wear_read(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<(bool, bool), String> {
        let status = self.exchange(connection, cancelled, sys::wear_status_request(), |payload| {
            sys::parse_wear_status(payload, 0xf3).is_some()
        })?;
        let enabled = sys::parse_wear_status(&status, 0xf3)
            .ok_or_else(|| "Wear-detection status fields are invalid.".to_string())?;
        let on = self.wear_value(connection, cancelled)?;
        self.wear_supported = true;
        Ok((enabled, on))
    }

    fn wear_value(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<bool, String> {
        let value = self.exchange(connection, cancelled, sys::wear_param_request(), |payload| {
            sys::parse_wear_param(payload, 0xf7).is_some()
        })?;
        sys::parse_wear_param(&value, 0xf7)
            .ok_or_else(|| "Wear-detection value fields are invalid.".to_string())
    }

    fn apply_wear(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        value: ControlValue,
    ) -> Result<SettingConfirmation, String> {
        if !self.wear_supported {
            return Err("Wear detection was not confirmed by the connected headphone.".into());
        }
        let ControlValue::Boolean(on) = value else {
            return Err("Wear detection requires an on/off value.".into());
        };
        let before = self.wear_value(connection, cancelled)?;
        if before != on {
            self.write_acknowledged(connection, cancelled, &sys::wear_set_request(on))?;
            self.wait_for(cancelled, "wear detection", |session| {
                session.wear_value(connection, cancelled).map(|current| current == on)
            })?;
        }
        let confirmed = ControlValue::Boolean(on);
        self.device_control_values.insert("wear".into(), confirmed.clone());
        Ok(SettingConfirmation::DeviceControl(DeviceControlConfirmation {
            key: "wear".into(),
            before: ControlValue::Boolean(before),
            confirmed,
        }))
    }

    /// Poll a readback until it matches or eight seconds pass. No restoring write is ever sent.
    fn wait_for(
        &mut self,
        cancelled: &AtomicBool,
        what: &str,
        mut matches: impl FnMut(&mut Self) -> Result<bool, String>,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match matches(self) {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Err(format!("The headphone did not confirm the requested {what} setting. No restoration command was sent."));
            }
            if cancelled.load(Ordering::Acquire) {
                return Err(format!("The {what} readback was cancelled."));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Read general setting `slot` (1..=4): capability, status and current value. Registers the
    /// slot for later writes. The returned flag is whether the headphone currently allows changes.
    fn general_setting_read(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        slot: u8,
    ) -> Result<(ConfirmedControl, bool), String> {
        let inquiry = sys::general_setting_inquiry(slot)
            .ok_or_else(|| "General setting slot is out of range.".to_string())?;
        let reply = self.exchange(connection, cancelled, sys::gs_capability_request(inquiry), |payload| {
            payload.len() >= 2 && payload[..2] == [0xd1, inquiry]
        })?;
        let capability = sys::parse_gs_capability(&reply, inquiry)
            .ok_or_else(|| format!("General setting {slot} capability fields are invalid."))?;
        let status = self.exchange(connection, cancelled, sys::gs_status_request(inquiry), |payload| {
            sys::parse_gs_status(payload, 0xd3, inquiry).is_some()
        })?;
        let enabled = sys::parse_gs_status(&status, 0xd3, inquiry)
            .ok_or_else(|| format!("General setting {slot} status fields are invalid."))?;
        let mut descriptor = sys::gs_descriptor(slot, &capability);
        // Keys stay unique if two slots resolve to the same name.
        if self.general_settings.contains_key(&descriptor.key) {
            descriptor.key = format!("gs{slot}");
        }
        let slot_state = GeneralSettingSlot { inquiry, capability };
        let value = self.general_setting_value(connection, cancelled, &slot_state)?;
        let control = ConfirmedControl {
            descriptor,
            value: sys::gs_value_as_control(value),
        };
        control.validate()?;
        self.general_settings.insert(control.descriptor.key.clone(), slot_state);
        Ok((control, enabled))
    }

    fn general_setting_value(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        slot: &GeneralSettingSlot,
    ) -> Result<GsValue, String> {
        let inquiry = slot.inquiry;
        let reply = self.exchange(connection, cancelled, sys::gs_param_request(inquiry), |payload| {
            sys::parse_gs_param(payload, 0xd7, inquiry).is_some()
        })?;
        let value = sys::parse_gs_param(&reply, 0xd7, inquiry)
            .ok_or_else(|| "General setting value fields are invalid.".to_string())?;
        match (slot.capability.kind, value) {
            (GsKind::Boolean, GsValue::Boolean(_)) => Ok(value),
            (GsKind::List, GsValue::List(index)) if usize::from(index) < slot.capability.elements.len() => Ok(value),
            _ => Err("The headphone returned a value of a different type than it advertised.".into()),
        }
    }

    fn apply_general_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        change: DeviceControlChange,
    ) -> Result<SettingConfirmation, String> {
        let slot = self
            .general_settings
            .get(&change.key)
            .cloned()
            .ok_or_else(|| "The headphone did not confirm this setting.".to_string())?;
        let wanted = match (slot.capability.kind, &change.value) {
            (GsKind::Boolean, ControlValue::Boolean(on)) => GsValue::Boolean(*on),
            (GsKind::List, ControlValue::Integer(index))
                if usize::try_from(*index).is_ok_and(|index| index < slot.capability.elements.len()) =>
            {
                GsValue::List(*index as u8)
            }
            _ => return Err("The requested value is not one of the options the headphone advertised.".into()),
        };
        let status = self.exchange(connection, cancelled, sys::gs_status_request(slot.inquiry), |payload| {
            sys::parse_gs_status(payload, 0xd3, slot.inquiry).is_some()
        })?;
        if sys::parse_gs_status(&status, 0xd3, slot.inquiry) != Some(true) {
            return Err("The headphone currently does not allow changing this setting.".into());
        }
        let before = self.general_setting_value(connection, cancelled, &slot)?;
        if before != wanted {
            self.write_acknowledged(connection, cancelled, &sys::gs_set_request(slot.inquiry, wanted))?;
            self.wait_for(cancelled, &change.key, |session| {
                session.general_setting_value(connection, cancelled, &slot).map(|current| current == wanted)
            })?;
        }
        let confirmed = sys::gs_value_as_control(wanted);
        self.device_control_values.insert(change.key.clone(), confirmed.clone());
        Ok(SettingConfirmation::DeviceControl(DeviceControlConfirmation {
            key: change.key,
            before: sys::gs_value_as_control(before),
            confirmed,
        }))
    }

    fn require_standard_processing(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<(), String> {
        if self.listening_series_supported
            && self.series_setting(connection, cancelled)? != UpmixMode::Standard
        {
            return Err("Switch to Standard listening mode before changing EQ or DSEE. Music, Cinema and Game suspend those controls.".into());
        }
        Ok(())
    }

    fn discover_series_control(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        bgm: Option<BgmInfo>,
    ) -> Result<ConfirmedControl, String> {
        if self.listening_bgm_inquiry.is_some() && bgm.is_none() {
            return Err("Listening modes require a readable Background Music state.".into());
        }
        let payload = self.exchange(connection, cancelled, vec![0xe0, 0x0a], |p| {
            sony_listening::parse_capability(p).is_some()
        })?;
        self.listening_series_options = sony_listening::parse_capability(&payload)
            .ok_or("Unknown upmix-series capability layout.")?;
        let mode = self.series_setting(connection, cancelled)?;
        let value = self.series_mode_key(mode, bgm)?;
        let mut options = vec![ControlOption {
            value: ControlValue::Text("standard".into()),
            label: "Standard".into(),
            enabled: None,
        }];
        for mode in [UpmixMode::Music, UpmixMode::Cinema, UpmixMode::Game] {
            if self.listening_series_options.contains(&mode) {
                options.push(ControlOption {
                    value: ControlValue::Text(mode.key().into()),
                    label: mode.label().into(),
                    enabled: None,
                });
            }
        }
        if self.listening_bgm_inquiry.is_some() {
            options.push(ControlOption {
                value: ControlValue::Text("backgroundMusic".into()),
                label: "Background Music".into(),
                enabled: None,
            });
        }
        Ok(ConfirmedControl {
            descriptor: ControlDescriptor {
                key: "listeningMode".into(),
                label: "Listening mode".into(),
                description:
                    "Device-reported listening modes. Music, Cinema and Game suspend EQ and DSEE."
                        .into(),
                kind: ControlKind::Select,
                options: Some(options),
                min: None,
                max: None,
                step: None,
            },
            value: ControlValue::Text(value.into()),
        })
    }

    fn series_mode_key(
        &self,
        mode: UpmixMode,
        bgm: Option<BgmInfo>,
    ) -> Result<&'static str, String> {
        if bgm.is_some_and(|v| v.enabled) {
            if mode != UpmixMode::Standard {
                return Err("The headphone is transitioning between Background Music and upmix. Readback is not yet consistent.".into());
            }
            return Ok("backgroundMusic");
        }
        Ok(mode.key())
    }

    fn series_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<UpmixMode, String> {
        if !self.listening_series_supported || self.listening_series_options.is_empty() {
            return Err("Upmix-series capabilities have not been confirmed.".into());
        }
        let payload = self.exchange(connection, cancelled, vec![0xe6, 0x0a], |p| {
            sony_listening::parse_state(p, 0xe7).is_some()
        })?;
        let mode =
            sony_listening::parse_state(&payload, 0xe7).ok_or("Invalid upmix-series state.")?;
        if mode != UpmixMode::Standard && !self.listening_series_options.contains(&mode) {
            return Err("Current upmix mode was not advertised by this headphone.".into());
        }
        self.listening_series_mode = Some(mode);
        Ok(mode)
    }

    fn set_series_mode(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        mode: UpmixMode,
    ) -> Result<(), String> {
        if mode != UpmixMode::Standard && !self.listening_series_options.contains(&mode) {
            return Err("This upmix mode was not advertised by the headphone.".into());
        }
        self.write_acknowledged(connection, cancelled, &sony_listening::set_payload(mode))?;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if self.series_setting(connection, cancelled)? == mode {
                return Ok(());
            }
            if cancelled.load(Ordering::Acquire) {
                return Err("Listening-mode confirmation was cancelled.".into());
            }
            if Instant::now() >= deadline {
                return Err("The headphone did not confirm the requested upmix mode. No restoration command was sent.".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn apply_series_mode(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        value: ControlValue,
    ) -> Result<SettingConfirmation, String> {
        let ControlValue::Text(requested) = value else {
            return Err("Listening mode requires a supported menu option.".into());
        };
        let target = if requested == "backgroundMusic" {
            UpmixMode::Standard
        } else {
            UpmixMode::from_key(&requested).ok_or("Unknown listening mode.")?
        };
        let before_mode = self.series_setting(connection, cancelled)?;
        let mut bgm = if let Some(inquiry) = self.listening_bgm_inquiry {
            Some(self.bgm_setting(connection, cancelled, inquiry)?)
        } else {
            None
        };
        let before = self.series_mode_key(before_mode, bgm)?.to_string();
        if requested == "backgroundMusic" {
            let room = bgm
                .ok_or("Background Music was not advertised by this headphone.")?
                .room_size;
            if before_mode != UpmixMode::Standard {
                self.set_series_mode(connection, cancelled, UpmixMode::Standard)?;
            }
            if !bgm.is_some_and(|v| v.enabled) {
                self.set_bgm_enabled(connection, cancelled, true, room)?;
            }
        } else {
            if let Some(active) = bgm.filter(|v| v.enabled) {
                self.set_bgm_enabled(connection, cancelled, false, active.room_size)?;
            }
            if before_mode != target {
                self.set_series_mode(connection, cancelled, target)?;
            }
        }
        let confirmed_mode = self.series_setting(connection, cancelled)?;
        if let Some(inquiry) = self.listening_bgm_inquiry {
            bgm = Some(self.bgm_setting(connection, cancelled, inquiry)?);
        }
        let confirmed = self.series_mode_key(confirmed_mode, bgm)?;
        if confirmed != requested {
            return Err("The final listening-mode readback did not match the request. No restoration command was sent.".into());
        }
        let confirmed = ControlValue::Text(confirmed.into());
        self.device_control_values
            .insert("listeningMode".into(), confirmed.clone());
        Ok(SettingConfirmation::DeviceControl(
            DeviceControlConfirmation {
                key: "listeningMode".into(),
                before: ControlValue::Text(before),
                confirmed,
            },
        ))
    }

    fn cinema_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<&'static str, String> {
        let payload = self.exchange(connection, cancelled, vec![0xe6, 0x04], |payload| {
            parse_cinema_value(payload, 0xe7).is_some()
        })?;
        let value = parse_cinema_value(&payload, 0xe7)
            .ok_or_else(|| "Cinema listening-mode response fields are invalid.".to_string())?;
        self.listening_cinema_enabled = Some(value == "cinema");
        Ok(value)
    }

    fn bgm_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
    ) -> Result<BgmInfo, String> {
        if !matches!(inquiry, 0x03 | 0x09) || self.listening_bgm_inquiry != Some(inquiry) {
            return Err("BGM inquiry was not advertised by the connected headphone.".into());
        }
        let payload = self.exchange(connection, cancelled, vec![0xe6, inquiry], |payload| {
            parse_bgm_value(payload, 0xe7, inquiry).is_some()
        })?;
        let info = parse_bgm_value(&payload, 0xe7, inquiry)
            .ok_or_else(|| "BGM listening-mode response fields are invalid.".to_string())?;
        self.listening_bgm_enabled = Some(info.enabled);
        self.listening_bgm_room_size = Some(info.room_size);
        Ok(info)
    }

    fn read_listening_pair(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<(Option<BgmInfo>, Option<bool>), String> {
        let bgm = if let Some(inquiry) = self.listening_bgm_inquiry {
            Some(self.bgm_setting(connection, cancelled, inquiry)?)
        } else {
            None
        };
        let cinema = if self.listening_cinema_supported {
            Some(self.cinema_setting(connection, cancelled)? == "cinema")
        } else {
            None
        };
        Ok((bgm, cinema))
    }

    fn set_bgm_enabled(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        enabled: bool,
        room_size: u8,
    ) -> Result<BgmInfo, String> {
        let inquiry = self
            .listening_bgm_inquiry
            .ok_or_else(|| "BGM mode was not advertised by the connected headphone.".to_string())?;
        if room_size > 2 {
            return Err("The headphone returned an unknown BGM room size.".into());
        }
        self.write_acknowledged(
            connection,
            cancelled,
            &[0xe8, inquiry, if enabled { 0 } else { 1 }, room_size],
        )?;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let current = self.bgm_setting(connection, cancelled, inquiry)?;
            if current.enabled == enabled && current.room_size == room_size {
                return Ok(current);
            }
            if Instant::now() >= deadline {
                return Err("The headphone did not confirm its requested BGM setting.".into());
            }
            if cancelled.load(Ordering::Acquire) {
                return Err("The BGM setting readback was cancelled.".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn apply_bgm_room_size(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        value: ControlValue,
    ) -> Result<SettingConfirmation, String> {
        let ControlValue::Integer(requested) = value else {
            return Err("Background Music distance requires a supported room option.".into());
        };
        let requested = u8::try_from(requested)
            .ok()
            .filter(|value| *value <= 2)
            .ok_or_else(|| "Background Music distance option is invalid.".to_string())?;
        let inquiry = self.listening_bgm_inquiry.ok_or_else(|| {
            "Background Music was not advertised by the connected headphone.".to_string()
        })?;
        let before = self.bgm_setting(connection, cancelled, inquiry)?;
        if before.room_size == requested {
            return Ok(SettingConfirmation::DeviceControl(
                DeviceControlConfirmation {
                    key: "bgmRoomSize".into(),
                    before: ControlValue::Integer(i64::from(before.room_size)),
                    confirmed: ControlValue::Integer(i64::from(before.room_size)),
                },
            ));
        }
        match self.set_bgm_enabled(connection, cancelled, before.enabled, requested) {
            Ok(confirmed) => Ok(SettingConfirmation::DeviceControl(
                DeviceControlConfirmation {
                    key: "bgmRoomSize".into(),
                    before: ControlValue::Integer(i64::from(before.room_size)),
                    confirmed: ControlValue::Integer(i64::from(confirmed.room_size)),
                },
            )),
            Err(error) => {
                let restored = self
                    .set_bgm_enabled(connection, cancelled, before.enabled, before.room_size)
                    .is_ok_and(|current| {
                        current.enabled == before.enabled && current.room_size == before.room_size
                    });
                if restored {
                    Err(format!("{error} The previous Background Music environment was restored and verified."))
                } else {
                    Err(format!("{error} The previous Background Music environment could not be verified after restoration; reconnect and inspect the headphone before retrying."))
                }
            }
        }
    }

    fn set_cinema_enabled(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        enabled: bool,
    ) -> Result<(), String> {
        if !self.listening_cinema_supported {
            return Err("Cinema mode was not advertised by the connected headphone.".into());
        }
        self.write_acknowledged(
            connection,
            cancelled,
            &[0xe8, 0x04, if enabled { 0 } else { 1 }],
        )?;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let current = self.cinema_setting(connection, cancelled)? == "cinema";
            if current == enabled {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("The headphone did not confirm its requested Cinema setting.".into());
            }
            if cancelled.load(Ordering::Acquire) {
                return Err("The Cinema setting readback was cancelled.".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn transition_listening_pair(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        target_bgm: bool,
        target_cinema: bool,
    ) -> Result<(Option<BgmInfo>, Option<bool>), String> {
        let (mut bgm, mut cinema) = self.read_listening_pair(connection, cancelled)?;
        if bgm.is_some() && target_bgm && !target_cinema {
            if cinema == Some(true) {
                self.set_cinema_enabled(connection, cancelled, false)?;
                cinema = Some(false);
            }
            let current = bgm.expect("BGM support checked above");
            if !current.enabled {
                bgm = Some(self.set_bgm_enabled(connection, cancelled, true, current.room_size)?);
            }
        } else if cinema.is_some() && target_cinema && !target_bgm {
            if bgm.is_some_and(|current| current.enabled) {
                let current = bgm.expect("BGM support checked above");
                bgm =
                    Some(self.set_bgm_enabled(connection, cancelled, false, current.room_size)?);
            }
            if cinema != Some(true) {
                self.set_cinema_enabled(connection, cancelled, true)?;
                cinema = Some(true);
            }
        } else if target_bgm && target_cinema {
            // Restore a previously observed dual-active pair only during rollback.
            if let Some(current) = bgm.filter(|current| current.enabled) {
                bgm =
                    Some(self.set_bgm_enabled(connection, cancelled, false, current.room_size)?);
            }
            if cinema != Some(true) {
                self.set_cinema_enabled(connection, cancelled, true)?;
                cinema = Some(true);
            }
            let current = bgm.expect("BGM support checked above");
            bgm = Some(self.set_bgm_enabled(connection, cancelled, true, current.room_size)?);
        } else {
            if let Some(current) = bgm.filter(|current| current.enabled) {
                bgm =
                    Some(self.set_bgm_enabled(connection, cancelled, false, current.room_size)?);
            }
            if cinema == Some(true) {
                self.set_cinema_enabled(connection, cancelled, false)?;
                cinema = Some(false);
            }
        }

        let confirmed = self.read_listening_pair(connection, cancelled)?;
        if confirmed.0.map(|value| value.enabled) != bgm.map(|_| target_bgm)
            || confirmed.0.map(|value| value.room_size) != bgm.map(|value| value.room_size)
            || confirmed.1 != cinema.map(|_| target_cinema)
        {
            return Err(
                "The headphone readback did not match the requested listening-mode pair.".into(),
            );
        }
        Ok(confirmed)
    }

    fn apply_listening_mode(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        value: ControlValue,
    ) -> Result<SettingConfirmation, String> {
        if self.listening_series_supported {
            return self.apply_series_mode(connection, cancelled, value);
        }
        let ControlValue::Text(value) = value else {
            return Err("Listening mode requires a supported menu option.".into());
        };
        let (before_bgm, before_cinema) = self.read_listening_pair(connection, cancelled)?;
        let target_bgm = match value.as_str() {
            "standard" => false,
            "backgroundMusic" if before_bgm.is_some() => true,
            "cinema" if before_cinema.is_some() => false,
            _ => return Err("Listening mode option is invalid.".into()),
        };
        let target_cinema = match value.as_str() {
            "standard" | "backgroundMusic" => false,
            "cinema" if before_cinema.is_some() => true,
            _ => return Err("Listening mode option is invalid.".into()),
        };
        let before_mode = if before_bgm.is_some_and(|state| state.enabled) {
            "backgroundMusic"
        } else if before_cinema == Some(true) {
            "cinema"
        } else {
            "standard"
        };
        match self.transition_listening_pair(connection, cancelled, target_bgm, target_cinema) {
            Ok((bgm, cinema)) => {
                let confirmed_mode = if bgm.is_some_and(|state| state.enabled) {
                    "backgroundMusic"
                } else if cinema == Some(true) {
                    "cinema"
                } else {
                    "standard"
                };
                let before = ControlValue::Text(before_mode.into());
                let confirmed = ControlValue::Text(confirmed_mode.into());
                self.device_control_values
                    .insert("listeningMode".into(), confirmed.clone());
                Ok(SettingConfirmation::DeviceControl(
                    DeviceControlConfirmation {
                        key: "listeningMode".into(),
                        before,
                        confirmed,
                    },
                ))
            }
            Err(error) => {
                let restore_bgm = before_bgm.is_some_and(|state| state.enabled);
                let restore_cinema = before_cinema == Some(true);
                let restored = self
                    .transition_listening_pair(connection, cancelled, restore_bgm, restore_cinema)
                    .is_ok_and(|(bgm, cinema)| {
                        bgm.map(|state| state.enabled) == before_bgm.map(|state| state.enabled)
                            && bgm.map(|state| state.room_size)
                                == before_bgm.map(|state| state.room_size)
                            && cinema == before_cinema
                    });
                if restored {
                    Err(format!(
                        "{error} The previous listening-mode state was restored and verified."
                    ))
                } else {
                    Err(format!("{error} The previous listening-mode state could not be verified after restoration; reconnect and inspect the headphone before retrying."))
                }
            }
        }
    }

    fn apply_upscaling_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        value: ControlValue,
    ) -> Result<SettingConfirmation, String> {
        let ControlValue::Text(value) = value else {
            return Err("DSEE requires the headphone's Off or Auto option.".into());
        };
        let encoded = match value.as_str() {
            "off" => 0,
            "auto" => 1,
            _ => return Err("DSEE option is invalid.".into()),
        };
        let before = self.upscaling_setting(connection, cancelled)?;
        if before != value {
            self.write_acknowledged(connection, cancelled, &[0xe8, 0x01, encoded])?;
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let current = self.upscaling_setting(connection, cancelled)?;
            if current == value {
                let key = "dsee".to_string();
                let before = ControlValue::Text(before.into());
                let confirmed = ControlValue::Text(current.into());
                self.device_control_values
                    .insert(key.clone(), confirmed.clone());
                return Ok(SettingConfirmation::DeviceControl(
                    DeviceControlConfirmation {
                        key,
                        before,
                        confirmed,
                    },
                ));
            }
            if Instant::now() >= deadline {
                return Err(
                    "The headphone did not confirm its DSEE mode after the setting request.".into(),
                );
            }
            if cancelled.load(Ordering::Acquire) {
                return Err("The DSEE readback was cancelled.".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn apply_noise_mode(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        mode: &str,
        confirmation_timeout: Duration,
        poll_interval: Duration,
    ) -> Result<SettingConfirmation, String> {
        if !["cancelling", "ambient", "off"].contains(&mode) {
            return Err("Noise-control mode is invalid.".into());
        }
        let before = self.noise_settings(connection, cancelled)?;
        self.noise_observations.clear();

        // A final CHANGED update. Preserve fields that are independent from the
        // requested mode, including the XM6-specific secondary toggle and
        // adaptive-sensitivity selection on inquiry 0x19.
        let payload = if before.inquiry == 0x19 {
            vec![
                0x68,
                0x19,
                1,
                u8::from(mode != "off"),
                if mode == "off" {
                    before.nc_asm_mode
                } else {
                    u8::from(mode == "ambient")
                },
                before.ambient_mode,
                before.ambient_level,
                u8::from(before.secondary_enabled),
                before.adaptive_sensitivity,
            ]
        } else {
            vec![
                0x68,
                0x17,
                1,
                u8::from(mode != "off"),
                u8::from(mode == "ambient"),
                u8::from(before.focus_on_voice),
                before.ambient_level,
            ]
        };
        self.write_acknowledged(connection, cancelled, &payload)?;

        let (noise, confirmed) = self.wait_for_noise_mode(
            connection,
            cancelled,
            mode,
            confirmation_timeout,
            poll_interval,
        )?;
        let observations = std::mem::take(&mut self.noise_observations);
        Ok(SettingConfirmation::Noise {
            before,
            noise,
            requested_mode: mode.to_owned(),
            confirmed,
            observations,
        })
    }

    fn protocol_info(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<ProtocolInfo, String> {
        let payload = self.exchange(connection, cancelled, vec![0, 0], |payload| {
            parse_protocol_info(payload).is_some()
        })?;
        let mut info =
            parse_protocol_info(&payload).ok_or("Protocol-info reply shape was not recognized.")?;
        info.ack_observed = true;
        Ok(info)
    }

    fn device_info(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
    ) -> Result<String, String> {
        if ![1, 2].contains(&inquiry) {
            return Err("Device-info inquiry is not validated.".into());
        }
        let payload = self.exchange(connection, cancelled, vec![4, inquiry], |payload| {
            parse_device_string(payload, inquiry).is_some()
        })?;
        parse_device_string(&payload, inquiry)
            .ok_or_else(|| "Device-info response is invalid.".into())
    }

    fn battery_info(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<BatteryInfo, String> {
        let payload = self.exchange(connection, cancelled, vec![0x22, 0], |payload| {
            parse_battery(payload).is_some()
        })?;
        parse_battery(&payload).ok_or_else(|| "Battery status response is invalid.".into())
    }

    fn supported_functions(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, String> {
        let payload = self.exchange(connection, cancelled, vec![0x06, 0x00], |payload| {
            payload.len() >= 2 && payload[..2] == [0x07, 0x00]
        })?;
        if payload.len() < 3 || payload.len() != 3 + usize::from(payload[2]) * 2 {
            return Err("Supported-function response length is invalid.".into());
        }
        Ok(payload[3..]
            .chunks_exact(2)
            .map(|pair| pair[0])
            .filter(|function| *function != 0)
            .collect())
    }

    fn noise_settings(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<NoiseInfo, String> {
        self.noise_settings_for(connection, cancelled, self.noise_inquiry.unwrap_or(0x17))
    }

    fn noise_settings_for(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
    ) -> Result<NoiseInfo, String> {
        let payload = self.noise_parameter(connection, cancelled, inquiry)?;
        let noise = parse_noise_payload(&payload, Some(0x67)).ok_or_else(|| {
            format!("Noise-control inquiry 0x{inquiry:02x} response fields are invalid.")
        })?;
        self.noise_inquiry = Some(inquiry);
        Ok(noise)
    }

    /// Table 2 support functions (`[06 00]` on data type 0x0e). A short deadline keeps a headphone that
    /// ignores table 2 from delaying the connection.
    fn supported_functions_table2(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, String> {
        let payload = self.exchange_as(connection, cancelled, mp::TABLE2, Duration::from_millis(1500), vec![0x06, 0x00], |payload| {
            payload.len() >= 2 && payload[..2] == [0x07, 0x00]
        })?;
        if payload.len() < 3 || payload.len() != 3 + usize::from(payload[2]) * 2 {
            return Err("Table 2 supported-function response length is invalid.".into());
        }
        Ok(payload[3..].chunks(2).map(|pair| pair[0]).filter(|function| *function != 0).collect())
    }

    fn multipoint_list(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
    ) -> Result<mp::DeviceList, String> {
        let reply = self.exchange_as(connection, cancelled, mp::TABLE2, Duration::from_secs(4), mp::list_request(inquiry), move |payload| {
            payload.len() >= 2 && payload[0] == 0x37 && payload[1] == inquiry
        })?;
        let mut list = mp::parse_list(&reply, 0x37, inquiry)
            .ok_or_else(|| "The paired-device list reply does not match the layout the headphone advertised.".to_string())?;
        list.annotate(&self.own_addresses, self.multipoint_limit);
        Ok(list)
    }

    /// Find out whether the headphone manages paired devices (multipoint) and read the list. `Ok(None)`
    /// means it does not advertise the feature. Read-only. `notes` receives what the headphone said.
    fn multipoint_discover(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        notes: &mut Vec<String>,
    ) -> Result<Option<mp::DeviceList>, String> {
        let functions = match self.supported_functions_table2(connection, cancelled) {
            Ok(functions) => functions,
            Err(error) => {
                notes.push(format!("Table 2 support functions could not be read ({error}); multipoint devices are not offered."));
                return Ok(None);
            }
        };
        let Some(inquiry) = mp::inquiry_for_functions(&functions) else {
            return Ok(None);
        };
        notes.push(format!(
            "The headphone advertised table 2 functions {} and pairing-device management type {inquiry}.",
            functions.iter().map(|function| format!("0x{function:02x}")).collect::<Vec<_>>().join(", ")
        ));
        // Capability is informational (limits and file-transfer rules); it is logged, not interpreted.
        if let Ok(reply) = self.exchange_as(connection, cancelled, mp::TABLE2, Duration::from_secs(2), mp::capability_request(inquiry), move |payload| {
            payload.len() >= 2 && payload[0] == 0x31 && payload[1] == inquiry
        }) {
            notes.push(format!("Pairing-device capability: {}.", reply.iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(" ")));
            // `[31, type, a, b, fileTransfer]`: the Sony app refuses to start pairing once the number of
            // connected devices reaches `b` ("MULTIPOINT_MAXIMUM_DEVICES_ARE_CONNECTED"), so `b` is the
            // connection limit.
            if reply.len() == 5 && reply[3] > 0 {
                self.multipoint_limit = Some(reply[3]);
            }
        }
        self.own_addresses = crate::local_radio::addresses();
        let list = self.multipoint_list(connection, cancelled, inquiry)?;
        self.multipoint_inquiry = Some(inquiry);
        self.multipoint_json = Some(list.to_json_string());
        Ok(Some(list))
    }

    fn multipoint_confirmation(&mut self, list: &mp::DeviceList) -> SettingConfirmation {
        let after = list.to_json_string();
        let before = self.multipoint_json.replace(after.clone()).unwrap_or_else(|| after.clone());
        SettingConfirmation::DeviceControl(DeviceControlConfirmation {
            key: "multipointDevices".into(),
            before: ControlValue::Text(before),
            confirmed: ControlValue::Text(after),
        })
    }

    /// Ask for one connect or disconnect and wait until the headphone's own list shows it. No rollback.
    fn multipoint_change(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
        address: &str,
        connect: bool,
    ) -> Result<mp::DeviceList, MultipointFailure> {
        self.write_acknowledged_as(connection, cancelled, mp::TABLE2, &mp::action_request(inquiry, connect, address))
            .map_err(|error| if is_link_loss(&error) { MultipointFailure::LinkReset { request_sent: false } } else { MultipointFailure::Other(error) })?;
        // Connecting another device can take several seconds; confirmation is the list itself.
        let deadline = Instant::now() + self.multipoint_wait;
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err(MultipointFailure::Other("The multipoint change was cancelled.".into()));
            }
            std::thread::sleep(self.multipoint_poll);
            let current = self.multipoint_list(connection, cancelled, inquiry)?;
            if current.find(address).is_some_and(|device| device.connected == connect) {
                return Ok(current);
            }
            if Instant::now() >= deadline {
                // The list is refreshed so what the headphone reports stays visible.
                let _ = self.multipoint_confirmation(&current);
                return Err(MultipointFailure::Other(format!(
                    "The headphone did not report that device as {} within {} seconds. The list shows what it reports now.",
                    if connect { "connected" } else { "disconnected" },
                    self.multipoint_wait.as_secs().max(1)
                )));
            }
        }
    }

    fn apply_multipoint_device(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        address: &str,
        connect: bool,
    ) -> Result<SettingConfirmation, String> {
        let inquiry = self.multipoint_inquiry.ok_or("This headphone has not described its paired devices.")?;
        if !mp::valid_address(address) {
            return Err("That is not a Bluetooth address.".into());
        }
        let before = self.multipoint_list(connection, cancelled, inquiry)?;
        let device = before.find(address).ok_or("That device is not in the headphone's paired list. Refresh the list.")?;
        let (target_address, target_name) = (device.address.clone(), device.name.clone());
        if !connect {
            if !device.connected {
                return Ok(self.multipoint_confirmation(&before));
            }
            let current = self
                .multipoint_change(connection, cancelled, inquiry, &target_address, false)
                .map_err(|failure| failure.explain(&format!("disconnecting {target_name}"), &target_name, None))?;
            return Ok(self.multipoint_confirmation(&current));
        }
        let mut dropped: Option<String> = None;
        match mp::plan_connect(&before, &target_address)? {
            mp::ConnectPlan::AlreadyConnected => return Ok(self.multipoint_confirmation(&before)),
            mp::ConnectPlan::Connect => {}
            mp::ConnectPlan::SwapOut { address: victim, name } => {
                // The headphone is at its connection limit: free a slot first by disconnecting the
                // device that is not playing audio (never this computer), and connect only after that
                // disconnection is confirmed.
                self.multipoint_change(connection, cancelled, inquiry, &victim, false).map_err(|failure| match failure {
                    MultipointFailure::LinkReset { .. } => format!(
                        "The headphone briefly reset its control link while {name} was disconnecting to make room for {target_name}. Sound Connect is reconnecting; once the list refreshes, connect {target_name} again if it is not connected."
                    ),
                    MultipointFailure::Other(error) => {
                        format!("{target_name} was not connected: {name} would not disconnect to make room. {error}")
                    }
                })?;
                dropped = Some(name);
            }
        }
        match self.multipoint_change(connection, cancelled, inquiry, &target_address, true) {
            Ok(current) => Ok(self.multipoint_confirmation(&current)),
            Err(failure) => Err(failure.explain(&format!("connecting {target_name}"), &target_name, dropped.as_deref())),
        }
    }

    /// Read the equalizer a headphone describes: capability (bands, level steps, presets), the band
    /// frequencies (extended info) and the current values. Read-only. `notes` receives the raw replies.
    fn read_generic_equalizer(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
        notes: &mut Vec<String>,
    ) -> Result<(EqLayout, EqualizerInfo), String> {
        let hex = |bytes: &[u8]| -> String {
            if bytes.is_empty() {
                "(empty)".into()
            } else {
                bytes.iter().take(64).map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(" ")
            }
        };
        let capability = self.exchange(connection, cancelled, vec![0x50, inquiry, 0x01], move |payload| {
            payload.len() >= 2 && payload[0] == 0x51 && payload[1] == inquiry
        })?;
        notes.push(format!("Equalizer capability (type 0x{inquiry:02x}): reply {}.", hex(&capability)));
        let (band_count, level_steps, presets) = parse_eq_capability(&capability, inquiry)?;

        let band_info = self.exchange(connection, cancelled, vec![0x5a, inquiry], move |payload| {
            payload.len() >= 2 && payload[0] == 0x5b && payload[1] == inquiry
        });
        let (labels, clear_bass_index) = match band_info {
            Ok(reply) => {
                notes.push(format!("Equalizer band information: reply {}.", hex(&reply)));
                parse_eq_band_info(&reply, inquiry, band_count).unwrap_or_else(|| {
                    notes.push("Equalizer band information did not match the band count; bands are numbered instead.".into());
                    ((1..=band_count).map(|n| format!("Band {n}")).collect(), None)
                })
            }
            Err(error) => {
                notes.push(format!("Equalizer band information could not be read ({error}); bands are numbered instead."));
                ((1..=band_count).map(|n| format!("Band {n}")).collect(), None)
            }
        };

        let layout = EqLayout {
            inquiry,
            band_count,
            level_steps,
            presets,
            labels,
            clear_bass_index,
            customizable: inquiry != 2 && band_count > 0,
        };
        let info = self.generic_equalizer_values(connection, cancelled, &layout, notes)?;
        Ok((layout, info))
    }

    fn generic_equalizer_values(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        layout: &EqLayout,
        notes: &mut Vec<String>,
    ) -> Result<EqualizerInfo, String> {
        let inquiry = layout.inquiry;
        let reply = self.exchange(connection, cancelled, vec![0x56, inquiry], move |payload| {
            payload.len() >= 2 && payload[0] == 0x57 && payload[1] == inquiry
        })?;
        if notes.len() < 8 {
            notes.push(format!(
                "Equalizer values (type 0x{inquiry:02x}): reply {}.",
                reply.iter().take(64).map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(" ")
            ));
        }
        parse_eq_params(&reply, inquiry, layout.band_count, layout.level_steps)
    }

    fn wait_for_generic_equalizer(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        layout: &EqLayout,
        matches: &dyn Fn(&EqualizerInfo) -> bool,
    ) -> Result<EqualizerInfo, String> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let current = self.generic_equalizer_values(connection, cancelled, layout, &mut Vec::new())?;
            if matches(&current) {
                return Ok(current);
            }
            if Instant::now() >= deadline {
                return Err("Equalizer readback did not reach the requested values.".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn eq_write_payload(layout: &EqLayout, preset: u8, bands: &[u8]) -> Vec<u8> {
        let mut payload = vec![0x58, layout.inquiry, preset, bands.len() as u8];
        payload.extend_from_slice(bands);
        payload
    }

    fn apply_generic_equalizer(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        layout: &EqLayout,
        request: GenericEqRequest,
    ) -> Result<SettingConfirmation, String> {
        self.require_standard_processing(connection, cancelled)?;
        let (payload, wanted): (Vec<u8>, EqMatcher) = match request {
            GenericEqRequest::Bands(bands) => {
                if !layout.customizable {
                    return Err("This headphone only offers equalizer presets; its bands cannot be edited.".into());
                }
                if bands.len() != usize::from(layout.band_count) || bands.iter().any(|step| *step >= layout.level_steps) {
                    return Err(format!(
                        "This headphone's equalizer needs {} bands with steps from 0 to {}.",
                        layout.band_count,
                        layout.level_steps - 1
                    ));
                }
                (
                    Self::eq_write_payload(layout, EQ_CUSTOM_PRESET, &bands),
                    Box::new(move |info: &EqualizerInfo| info.bands == bands),
                )
            }
            GenericEqRequest::Preset(id) => {
                if !layout.presets.iter().any(|preset| preset.id == id) {
                    return Err("This headphone does not offer that equalizer preset.".into());
                }
                (Self::eq_write_payload(layout, id, &[]), Box::new(move |info: &EqualizerInfo| info.preset_code == id))
            }
        };
        let before = self.generic_equalizer_values(connection, cancelled, layout, &mut Vec::new())?;
        if wanted(&before) {
            return Ok(SettingConfirmation::Equalizer(before));
        }
        self.write_acknowledged(connection, cancelled, &payload)?;
        let Ok(confirmed) = self.wait_for_generic_equalizer(connection, cancelled, layout, wanted.as_ref()) else {
            // No matching readback: put the previous profile back and verify it, never assume.
            let restore = if before.preset_code == EQ_CUSTOM_PRESET && !before.bands.is_empty() {
                Self::eq_write_payload(layout, EQ_CUSTOM_PRESET, &before.bands)
            } else {
                Self::eq_write_payload(layout, before.preset_code, &[])
            };
            let previous = before.clone();
            if self.write_acknowledged(connection, cancelled, &restore).is_ok()
                && self
                    .wait_for_generic_equalizer(connection, cancelled, layout, &|info: &EqualizerInfo| {
                        info.preset_code == previous.preset_code && (previous.bands.is_empty() || info.bands == previous.bands)
                    })
                    .is_ok()
            {
                return Err("The headphone did not apply the requested equalizer values; the previous profile was restored and verified.".into());
            }
            return Err("The headphone did not confirm the requested equalizer values, and the previous profile could not be verified after restoration. Reconnect and inspect the headphone before retrying.".into());
        };
        Ok(SettingConfirmation::Equalizer(confirmed))
    }

    fn equalizer_settings(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<EqualizerInfo, String> {
        let payload = self.exchange(connection, cancelled, vec![0x56, 0x04], |payload| {
            payload.len() >= 2 && payload[..2] == [0x57, 0x04]
        })?;
        if payload.len() != 14 || payload[3] != 10 || payload[4..].iter().any(|value| *value > 12) {
            return Err(
                "XM6 equalizer response does not match the validated ten-band layout.".into(),
            );
        }
        Ok(EqualizerInfo {
            preset_code: payload[2],
            bands: payload[4..].to_vec(),
        })
    }

    fn wait_for_noise_mode(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        mode: &str,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<(NoiseInfo, bool), String> {
        let deadline = Instant::now() + timeout;
        loop {
            let current = self.noise_settings(connection, cancelled)?;
            // A matching mode is not final while the device reports
            // UNDER_CHANGING. Only CHANGED (1) can confirm the user request.
            if current.mode == mode && current.raw_payload[2] == 1 {
                return Ok((current, true));
            }
            if Instant::now() >= deadline {
                return Ok((current, false));
            }
            if cancelled.load(Ordering::Acquire) {
                return Err("Noise-control readback was cancelled.".into());
            }
            std::thread::sleep(
                poll_interval.min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    fn wait_for_equalizer_bands(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        bands: &[u8],
    ) -> Result<EqualizerInfo, String> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let current = self.equalizer_settings(connection, cancelled)?;
            if current.bands == bands {
                return Ok(current);
            }
            if Instant::now() >= deadline {
                return Err("Equalizer readback did not reach the requested values.".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn write_acknowledged(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        payload: &[u8],
    ) -> Result<(), String> {
        self.write_acknowledged_as(connection, cancelled, 0x0c, payload)
    }

    fn write_acknowledged_as(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        data_type: u8,
        payload: &[u8],
    ) -> Result<(), String> {
        if cancelled.load(Ordering::Acquire) || !self.synchronized {
            return Err("The device session is cancelled or unsynchronized; reconnect before changing settings.".into());
        }
        let request = Frame {
            data_type,
            sequence: self.sequence,
            payload: payload.to_vec(),
        };
        let expected_ack = 1 - self.sequence;
        // NC/ASM and custom EQ SET bodies exceed the two-byte GET command size.
        // Individual commands validate their typed payloads before this bound.
        let wire = encode(&request, 64).map_err(|error| error.to_string())?;
        self.synchronized = false;
        connection
            .write_all(&wire, Duration::from_secs(1))
            .map_err(|_| "The device setting request could not be sent.".to_string())?;
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut buffer = [0u8; 4096];
        while Instant::now() < deadline {
            if cancelled.load(Ordering::Acquire) {
                return Err("The device setting request was cancelled.".into());
            }
            match connection.read(&mut buffer, Duration::from_millis(200)) {
                Ok(0) => {
                    return Err(
                        "The device closed its control connection during a setting change.".into(),
                    )
                }
                Ok(count) => {
                    let mut matched_ack_observed = false;
                    for frame in self.decoder.feed(&buffer[..count]) {
                        let frame = frame
                            .map_err(|error| format!("Invalid setting response frame: {error}"))?;
                        self.record_noise_observation(&frame);
                        let matched_ack = frame.data_type == 1
                            && frame.sequence == expected_ack
                            && frame.payload.is_empty();
                        acknowledge(connection, &frame)?;
                        if matched_ack {
                            matched_ack_observed = true;
                        }
                    }
                    if matched_ack_observed {
                        self.sequence = expected_ack;
                        self.synchronized = true;
                        return Ok(());
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return Err("The control stream failed during a setting change.".into()),
            }
        }
        Err("No transport ACK arrived for the setting request. Reconnect before retrying.".into())
    }

    fn exchange(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        payload: Vec<u8>,
        matches: impl Fn(&[u8]) -> bool,
    ) -> Result<Vec<u8>, String> {
        self.exchange_as(connection, cancelled, 0x0c, Duration::from_secs(4), payload, matches)
    }

    /// Like `exchange` for a chosen frame data type (`0x0c` table 1, `0x0e` table 2) and deadline.
    fn exchange_as(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        data_type: u8,
        timeout: Duration,
        payload: Vec<u8>,
        matches: impl Fn(&[u8]) -> bool,
    ) -> Result<Vec<u8>, String> {
        if cancelled.load(Ordering::Acquire) {
            return Err("Read-only discovery was cancelled.".into());
        }
        if !self.synchronized {
            return Err(
                "Read-only request sequence is unconfirmed; reconnect before retrying.".into(),
            );
        }
        // The command schema selects message table 1, independently of the service UUID.
        let request = Frame {
            data_type,
            sequence: self.sequence,
            payload,
        };
        let expected_ack = 1 - self.sequence;
        // Setting bodies are longer than the two-byte GET requests. This
        // limit is only an encoder bound; callers validate each typed schema.
        let wire = encode(&request, 64).map_err(|error| error.to_string())?;
        self.synchronized = false;
        connection
            .write_all(&wire, Duration::from_secs(1))
            .map_err(|_| "Read-only discovery request could not be sent.".to_string())?;
        let deadline = Instant::now() + timeout;
        let mut reply = None;
        let mut buffer = [0u8; 4096];
        while Instant::now() < deadline {
            if cancelled.load(Ordering::Acquire) {
                return Err("Read-only discovery was cancelled.".into());
            }
            match connection.read(&mut buffer, Duration::from_millis(200)) {
                Ok(0) => {
                    return Err(CLOSED_DURING_DISCOVERY.into())
                }
                Ok(count) => {
                    for frame in self.decoder.feed(&buffer[..count]) {
                        let frame =
                            frame.map_err(|error| format!("Invalid discovery frame: {error}"))?;
                        self.record_noise_observation(&frame);
                        if frame.data_type == 1
                            && frame.sequence == expected_ack
                            && frame.payload.is_empty()
                        {
                            self.sequence = expected_ack;
                            self.synchronized = true;
                        }
                        acknowledge(connection, &frame)?;
                        if frame.data_type == data_type && matches(&frame.payload) {
                            reply = Some(frame.payload);
                        }
                    }
                    if self.synchronized {
                        if let Some(reply) = reply.take() {
                            return Ok(reply);
                        }
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => {
                    return Err("The control stream failed during read-only discovery.".into())
                }
            }
        }
        Err("No matched response and transport ACK arrived within four seconds. An ACK alone does not confirm a request.".into())
    }

    fn update_listening_mode_notification(&mut self) {
        if !self
            .device_control_descriptors
            .contains_key("listeningMode")
        {
            return;
        }
        let Some(value) = (if self.listening_series_supported {
            self.listening_series_mode.and_then(|mode| {
                if self.listening_bgm_inquiry.is_some() && self.listening_bgm_enabled.is_none() {
                    return None;
                }
                if self.listening_bgm_enabled == Some(true) {
                    (mode == UpmixMode::Standard).then_some("backgroundMusic")
                } else {
                    Some(mode.key())
                }
            })
        } else if self.listening_bgm_enabled == Some(true) {
            Some("backgroundMusic")
        } else if self.listening_cinema_enabled == Some(true) {
            Some("cinema")
        } else if self.listening_bgm_enabled.is_some() || self.listening_cinema_enabled.is_some() {
            Some("standard")
        } else {
            None
        }) else {
            return;
        };
        let value = ControlValue::Text(value.into());
        let before = self
            .device_control_values
            .insert("listeningMode".into(), value.clone());
        if let Some(before) = before.filter(|before| *before != value) {
            self.device_updates.push(DeviceControlConfirmation {
                key: "listeningMode".into(),
                before,
                confirmed: value,
            });
        }
    }

    fn record_noise_observation(&mut self, frame: &Frame) {
        // The headphone announces a changed paired-device list (another phone connected, a device
        // went out of range). Only a list that parses against the advertised layout is kept.
        if frame.data_type == mp::TABLE2 {
            if let Some(inquiry) = self.multipoint_inquiry {
                if let Some(list) = mp::parse_list(&frame.payload, 0x39, inquiry) {
                    let after = list.to_json_string();
                    if let Some(before) = self.multipoint_json.replace(after.clone()).filter(|before| *before != after) {
                        self.device_updates.push(DeviceControlConfirmation {
                            key: "multipointDevices".into(),
                            before: ControlValue::Text(before),
                            confirmed: ControlValue::Text(after),
                        });
                    }
                }
            }
        }
        // Preserve NCASM-family payloads as raw observations. Command-specific
        // parsers are intentionally deferred until their layouts are verified.
        if frame.data_type == 0x0c
            && (2..=128).contains(&frame.payload.len())
            && (0x60..=0x69).contains(&frame.payload[0])
            && self.noise_observations.len() < 16
            && self.noise_observations.last() != Some(&frame.payload)
        {
            self.noise_observations.push(frame.payload.clone());
        }
        if frame.data_type == 0x0c {
            if self.listening_series_supported {
                if let Some(mode) = sony_listening::parse_state(&frame.payload, 0xe9) {
                    if mode == UpmixMode::Standard || self.listening_series_options.contains(&mode)
                    {
                        self.listening_series_mode = Some(mode);
                        self.update_listening_mode_notification();
                    }
                }
            }
            if let Some((enabled, aux)) = parse_speak_to_chat_value(&frame.payload, 0xf9) {
                self.speak_to_chat_aux = Some(aux);
                if self.device_control_descriptors.contains_key("speakToChat") {
                    let value = ControlValue::Boolean(enabled);
                    let before = self
                        .device_control_values
                        .insert("speakToChat".into(), value.clone());
                    if let Some(before) = before.filter(|before| *before != value) {
                        self.device_updates.push(DeviceControlConfirmation {
                            key: "speakToChat".into(),
                            before,
                            confirmed: value,
                        });
                    }
                }
            }
            if self.wear_supported {
                if let Some(on) = sys::parse_wear_param(&frame.payload, 0xf9) {
                    self.observe_device_value("wear", ControlValue::Boolean(on));
                }
            }
            let observed: Vec<(String, GeneralSettingSlot)> = self
                .general_settings
                .iter()
                .map(|(key, slot)| (key.clone(), slot.clone()))
                .collect();
            for (key, slot) in observed {
                if let Some(value) = sys::parse_gs_param(&frame.payload, 0xd9, slot.inquiry) {
                    let valid = match (slot.capability.kind, value) {
                        (GsKind::Boolean, GsValue::Boolean(_)) => true,
                        (GsKind::List, GsValue::List(index)) => usize::from(index) < slot.capability.elements.len(),
                        _ => false,
                    };
                    if valid {
                        self.observe_device_value(&key, sys::gs_value_as_control(value));
                    }
                }
            }
            if let Some(value) = parse_upscaling_value(&frame.payload, 0xe9) {
                if self.device_control_descriptors.contains_key("dsee") {
                    let value = ControlValue::Text(value.into());
                    let before = self
                        .device_control_values
                        .insert("dsee".into(), value.clone());
                    if let Some(before) = before.filter(|before| *before != value) {
                        self.device_updates.push(DeviceControlConfirmation {
                            key: "dsee".into(),
                            before,
                            confirmed: value,
                        });
                    }
                }
            }
            if let Some(inquiry) = self.power_off_inquiry {
                if let Some((active, selected_time)) =
                    parse_power_off_value(&frame.payload, 0x29, inquiry, &self.power_off_options)
                {
                    for (key, value) in [
                        ("autoPowerOff", active),
                        ("autoPowerOffTime", selected_time),
                    ] {
                        if self.device_control_descriptors.contains_key(key) {
                            let value = ControlValue::Integer(i64::from(value));
                            let before =
                                self.device_control_values.insert(key.into(), value.clone());
                            if let Some(before) = before.filter(|before| *before != value) {
                                self.device_updates.push(DeviceControlConfirmation {
                                    key: key.into(),
                                    before,
                                    confirmed: value,
                                });
                            }
                        }
                    }
                }
            }
            if let Some(inquiry) = self.listening_bgm_inquiry {
                if let Some(value) = parse_bgm_value(&frame.payload, 0xe9, inquiry) {
                    self.listening_bgm_enabled = Some(value.enabled);
                    self.listening_bgm_room_size = Some(value.room_size);
                    if self.device_control_descriptors.contains_key("bgmRoomSize") {
                        let value = ControlValue::Integer(i64::from(value.room_size));
                        let before = self
                            .device_control_values
                            .insert("bgmRoomSize".into(), value.clone());
                        if let Some(before) = before.filter(|before| *before != value) {
                            self.device_updates.push(DeviceControlConfirmation {
                                key: "bgmRoomSize".into(),
                                before,
                                confirmed: value,
                            });
                        }
                    }
                    self.update_listening_mode_notification();
                }
            }
            if let Some(value) = parse_cinema_value(&frame.payload, 0xe9) {
                self.listening_cinema_enabled = Some(value == "cinema");
                self.update_listening_mode_notification();
            }
            if let Some(noise) = parse_noise_payload(&frame.payload, Some(0x69)) {
                if noise.value_change_status == 1 {
                    let observed = [
                        ("focusOnVoice", ControlValue::Boolean(noise.focus_on_voice)),
                        (
                            "ambient",
                            ControlValue::Integer(i64::from(noise.ambient_level)),
                        ),
                    ];
                    for (key, value) in observed {
                        if self.device_control_descriptors.contains_key(key) {
                            let before =
                                self.device_control_values.insert(key.into(), value.clone());
                            if let Some(before) = before.filter(|before| *before != value) {
                                self.device_updates.push(DeviceControlConfirmation {
                                    key: key.into(),
                                    before,
                                    confirmed: value,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn wire_frames(frames: &[Frame]) -> Vec<u8> {
        frames
            .iter()
            .flat_map(|frame| encode(frame, 64).unwrap())
            .collect()
    }

    fn noise_response(mode: u8, sequence: u8) -> Frame {
        Frame {
            data_type: 0x0c,
            sequence,
            payload: vec![0x67, 0x17, 1, 1, mode, 0, 8],
        }
    }

    fn noise_exchange(ack_sequence: u8, response_mode: u8, response_sequence: u8) -> Vec<u8> {
        wire_frames(&[
            Frame {
                data_type: 1,
                sequence: ack_sequence,
                payload: vec![],
            },
            noise_response(response_mode, response_sequence),
        ])
    }

    fn noise_off_exchange(ack_sequence: u8, response_sequence: u8, change_status: u8) -> Vec<u8> {
        wire_frames(&[
            Frame {
                data_type: 1,
                sequence: ack_sequence,
                payload: vec![],
            },
            Frame {
                data_type: 0x0c,
                sequence: response_sequence,
                payload: vec![0x67, 0x17, change_status, 0, 0, 0, 0],
            },
        ])
    }

    fn noise19_exchange(ack_sequence: u8, response: Frame) -> Vec<u8> {
        wire_frames(&[
            Frame {
                data_type: 1,
                sequence: ack_sequence,
                payload: vec![],
            },
            response,
        ])
    }

    fn audio_exchange(ack_sequence: u8, mode: u8) -> Vec<u8> {
        wire_frames(&[
            Frame {
                data_type: 1,
                sequence: ack_sequence,
                payload: vec![],
            },
            Frame {
                data_type: 0x0c,
                sequence: 0,
                payload: vec![0xe7, 0x01, mode],
            },
        ])
    }

    fn dsee_control(value: &'static str) -> ConfirmedControl {
        ConfirmedControl {
            descriptor: ControlDescriptor {
                key: "dsee".into(),
                label: "DSEE upscaling".into(),
                description: "Automatic DSEE upscaling".into(),
                kind: ControlKind::Select,
                options: Some(vec![
                    ControlOption {
                        value: ControlValue::Text("off".into()),
                        label: "Off".into(),
                        enabled: None,
                    },
                    ControlOption {
                        value: ControlValue::Text("auto".into()),
                        label: "Auto".into(),
                        enabled: None,
                    },
                ]),
                min: None,
                max: None,
                step: None,
            },
            value: ControlValue::Text(value.into()),
        }
    }

    fn focus_control(value: bool) -> ConfirmedControl {
        ConfirmedControl {
            descriptor: ControlDescriptor {
                key: "focusOnVoice".into(),
                label: "Focus on voice".into(),
                description: "Prioritize voices in ambient sound mode.".into(),
                kind: ControlKind::Toggle,
                options: None,
                min: None,
                max: None,
                step: None,
            },
            value: ControlValue::Boolean(value),
        }
    }

    fn setting_requests(connection: &FixtureConnection, command: u8) -> usize {
        let mut decoder = Decoder::default();
        connection
            .writes
            .iter()
            .flat_map(|wire| decoder.feed(wire))
            .map(Result::unwrap)
            .filter(|frame| frame.data_type == 0x0c && frame.payload.first() == Some(&command))
            .count()
    }

    struct FixtureConnection {
        reads: VecDeque<Vec<u8>>,
        writes: Vec<Vec<u8>>,
        service: &'static str,
    }

    /// Stateful wire-level replay; never opens a real Bluetooth connection.
    struct SeriesFixture {
        reads: VecDeque<Vec<u8>>,
        commands: Vec<Vec<u8>>,
        upmix: u8,
        bgm: bool,
        room: u8,
        options: Vec<u8>,
        ack_only_after_set: bool,
        wrote_setting: bool,
    }
    impl Default for SeriesFixture {
        fn default() -> Self {
            Self {
                reads: VecDeque::new(),
                commands: vec![],
                upmix: 0,
                bgm: false,
                room: 2,
                options: vec![1, 2, 3],
                ack_only_after_set: false,
                wrote_setting: false,
            }
        }
    }
    impl Connection for SeriesFixture {
        fn service_uuid(&self) -> &'static str {
            "96cc203e-5068-46ad-b32d-e316f5e069ba"
        }
        fn read(&mut self, buffer: &mut [u8], _timeout: Duration) -> std::io::Result<usize> {
            let Some(bytes) = self.reads.pop_front() else {
                return Ok(0);
            };
            buffer[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
        fn write_all(&mut self, bytes: &[u8], _timeout: Duration) -> std::io::Result<()> {
            for frame in Decoder::default().feed(bytes) {
                let frame = frame.unwrap();
                if frame.data_type != 0x0c {
                    continue;
                }
                self.commands.push(frame.payload.clone());
                let mut frames = vec![Frame {
                    data_type: 1,
                    sequence: 1 - frame.sequence,
                    payload: vec![],
                }];
                let reply = match frame.payload.as_slice() {
                    [0xe0, 0x0a] => {
                        let mut p = vec![0xe1, 0x0a, self.options.len() as u8];
                        p.extend(&self.options);
                        p.push(0);
                        Some(p)
                    }
                    [0xe6, 0x0a] if !(self.ack_only_after_set && self.wrote_setting) => {
                        Some(vec![0xe7, 0x0a, self.upmix])
                    }
                    [0xe6, 0x09] => Some(vec![0xe7, 0x09, if self.bgm { 0 } else { 1 }, self.room]),
                    [0xe8, 0x0a, mode] => {
                        self.upmix = *mode;
                        self.wrote_setting = true;
                        None
                    }
                    [0xe8, 0x09, enabled, room] => {
                        self.bgm = *enabled == 0;
                        self.room = *room;
                        self.wrote_setting = true;
                        None
                    }
                    _ => None,
                };
                if let Some(payload) = reply {
                    frames.push(Frame {
                        data_type: 0x0c,
                        sequence: 0,
                        payload,
                    });
                }
                self.reads.push_back(wire_frames(&frames));
            }
            Ok(())
        }
    }
    fn series_session(connection: &mut SeriesFixture) -> ReadSession {
        let cancelled = AtomicBool::new(false);
        let mut session = ReadSession::new();
        session.configure_listening_modes(Some(0x09), false, true);
        let bgm = session.bgm_setting(connection, &cancelled, 0x09).unwrap();
        let control = session
            .discover_series_control(connection, &cancelled, Some(bgm))
            .unwrap();
        session.install_device_controls(&[control]);
        session
    }
    #[test]
    fn series_modes_discover_apply_and_confirm_all_five_options() {
        let mut connection = SeriesFixture::default();
        let mut session = series_session(&mut connection);
        let descriptor = &session.device_control_descriptors["listeningMode"];
        assert_eq!(descriptor.options.as_ref().unwrap().len(), 5);
        for target in [
            "music",
            "cinema",
            "game",
            "backgroundMusic",
            "music",
            "standard",
        ] {
            let confirmation = session
                .apply_device_control(
                    &mut connection,
                    &AtomicBool::new(false),
                    DeviceControlChange {
                        key: "listeningMode".into(),
                        value: ControlValue::Text(target.into()),
                    },
                )
                .unwrap();
            let SettingConfirmation::DeviceControl(value) = confirmation else {
                panic!("wrong confirmation");
            };
            assert_eq!(value.confirmed, ControlValue::Text(target.into()));
        }
        let writes: Vec<_> = connection
            .commands
            .iter()
            .filter(|p| p.first() == Some(&0xe8))
            .cloned()
            .collect();
        assert_eq!(
            writes,
            vec![
                vec![0xe8, 0x0a, 3],
                vec![0xe8, 0x0a, 1],
                vec![0xe8, 0x0a, 2],
                vec![0xe8, 0x0a, 0],
                vec![0xe8, 0x09, 0, 2],
                vec![0xe8, 0x09, 1, 2],
                vec![0xe8, 0x0a, 3],
                vec![0xe8, 0x0a, 0]
            ]
        );
        assert_eq!(connection.room, 2);
    }
    #[test]
    fn series_ack_only_never_confirms_or_sends_automatic_restoration() {
        let mut connection = SeriesFixture::default();
        let mut session = series_session(&mut connection);
        connection.ack_only_after_set = true;
        assert!(session
            .apply_device_control(
                &mut connection,
                &AtomicBool::new(false),
                DeviceControlChange {
                    key: "listeningMode".into(),
                    value: ControlValue::Text("music".into())
                }
            )
            .is_err());
        assert_eq!(
            session.device_control_values["listeningMode"],
            ControlValue::Text("standard".into())
        );
        assert_eq!(
            connection
                .commands
                .iter()
                .filter(|p| p.first() == Some(&0xe8))
                .count(),
            1
        );
    }
    #[test]
    fn series_capabilities_restrict_options_and_unadvertised_modes_send_no_commands() {
        let mut connection = SeriesFixture {
            options: vec![1],
            ..Default::default()
        };
        let mut session = series_session(&mut connection);
        assert_eq!(
            session.device_control_descriptors["listeningMode"]
                .options
                .as_ref()
                .unwrap()
                .len(),
            3
        );
        let before = connection.commands.len();
        assert!(session
            .apply_device_control(
                &mut connection,
                &AtomicBool::new(false),
                DeviceControlChange {
                    key: "listeningMode".into(),
                    value: ControlValue::Text("game".into())
                }
            )
            .is_err());
        assert_eq!(connection.commands.len(), before);
    }
    #[test]
    fn series_notifications_follow_the_hardware_button_and_ignore_unknown_modes() {
        let mut connection = SeriesFixture::default();
        let mut session = series_session(&mut connection);
        session.record_noise_observation(&Frame {
            data_type: 0x0c,
            sequence: 0,
            payload: vec![0xe9, 0x0a, 3],
        });
        let updates = session.drain_device_updates();
        assert_eq!(
            updates.last().unwrap().confirmed,
            ControlValue::Text("music".into())
        );
        session.record_noise_observation(&Frame {
            data_type: 0x0c,
            sequence: 0,
            payload: vec![0xe9, 0x0a, 99],
        });
        assert!(session.drain_device_updates().is_empty());
    }
    #[test]
    fn series_upmix_blocks_eq_and_dsee_before_any_setting_write() {
        let mut connection = SeriesFixture {
            upmix: 3,
            ..Default::default()
        };
        let mut session = series_session(&mut connection);
        session.install_device_controls(&[dsee_control("auto")]);
        assert!(session
            .apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::Equalizer(vec![6; 10])
            )
            .is_err());
        assert!(session
            .apply_device_control(
                &mut connection,
                &AtomicBool::new(false),
                DeviceControlChange {
                    key: "dsee".into(),
                    value: ControlValue::Text("off".into())
                }
            )
            .is_err());
        assert!(!connection
            .commands
            .iter()
            .any(|p| matches!(p.first(), Some(0xe8 | 0x58))));
    }
    impl Connection for FixtureConnection {
        fn service_uuid(&self) -> &'static str {
            self.service
        }
        fn read(&mut self, buffer: &mut [u8], _timeout: Duration) -> std::io::Result<usize> {
            let Some(bytes) = self.reads.pop_front() else {
                return Ok(0);
            };
            buffer[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
        fn write_all(&mut self, bytes: &[u8], _timeout: Duration) -> std::io::Result<()> {
            self.writes.push(bytes.into());
            Ok(())
        }
    }

    #[test]
    fn source_derived_response_shapes_are_strict() {
        assert_eq!(
            parse_protocol_info(&[1, 0, 0x12, 0x34]).unwrap().version,
            0x1234
        );
        assert_eq!(
            parse_protocol_info(&[1, 0, 0x12, 0x34, 0x56, 0x78, 0, 1])
                .unwrap()
                .version,
            0x12345678
        );
        for payload in [
            vec![1],
            vec![1, 1, 0, 0],
            vec![1, 0, 0, 0, 0],
            vec![1, 0, 0, 0, 0, 0, 2, 0],
        ] {
            assert!(parse_protocol_info(&payload).is_none());
        }
    }

    #[test]
    fn upscaling_parser_accepts_only_exact_known_values() {
        assert_eq!(parse_upscaling_value(&[0xe7, 1, 0], 0xe7), Some("off"));
        assert_eq!(parse_upscaling_value(&[0xe7, 1, 1], 0xe7), Some("auto"));
        for invalid in [
            vec![0xe7, 1],
            vec![0xe7, 1, 2],
            vec![0xe9, 1, 1],
            vec![0xe7, 2, 1],
            vec![0xe7, 1, 1, 0],
        ] {
            assert!(parse_upscaling_value(&invalid, 0xe7).is_none());
        }
    }

    #[test]
    fn unadvertised_or_unreadable_dsee_has_no_command_descriptor() {
        let mut session = ReadSession::new();
        let mut connection = FixtureConnection {
            reads: VecDeque::new(),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let error = session.apply_setting(
            &mut connection,
            &AtomicBool::new(false),
            SettingChange::DeviceControl(DeviceControlChange {
                key: "dsee".into(),
                value: ControlValue::Text("auto".into()),
            }),
        );
        assert!(error.is_err());
        assert!(connection.writes.is_empty());
    }

    #[test]
    fn dsee_ack_is_not_confirmation_and_get_readback_confirms_set() {
        let mut connection = FixtureConnection {
            reads: VecDeque::from([
                audio_exchange(1, 0),
                wire_frames(&[Frame {
                    data_type: 1,
                    sequence: 0,
                    payload: vec![],
                }]),
                audio_exchange(1, 1),
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        session.install_device_controls(&[dsee_control("off")]);
        let confirmation = session
            .apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(DeviceControlChange {
                    key: "dsee".into(),
                    value: ControlValue::Text("auto".into()),
                }),
            )
            .unwrap();
        let SettingConfirmation::DeviceControl(confirmation) = confirmation else {
            panic!("expected a typed DSEE confirmation");
        };
        assert_eq!(confirmation.before, ControlValue::Text("off".into()));
        assert_eq!(confirmation.confirmed, ControlValue::Text("auto".into()));
        let mut decoder = Decoder::default();
        let commands: Vec<_> = connection
            .writes
            .iter()
            .flat_map(|wire| decoder.feed(wire))
            .map(Result::unwrap)
            .filter(|frame| frame.data_type == 0x0c)
            .map(|frame| frame.payload)
            .collect();
        assert_eq!(commands, [vec![0xe6, 1], vec![0xe8, 1, 1], vec![0xe6, 1]]);

        let mut no_readback = FixtureConnection {
            reads: VecDeque::from([
                audio_exchange(1, 0),
                wire_frames(&[Frame {
                    data_type: 1,
                    sequence: 0,
                    payload: vec![],
                }]),
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        session.install_device_controls(&[dsee_control("off")]);
        assert!(session
            .apply_setting(
                &mut no_readback,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(DeviceControlChange {
                    key: "dsee".into(),
                    value: ControlValue::Text("auto".into()),
                }),
            )
            .is_err());
        assert_eq!(setting_requests(&no_readback, 0xe8), 1);
    }


    /// Stateful wire-level replay of a headphone exposing wear detection and two general settings.
    struct SystemFixture {
        reads: VecDeque<Vec<u8>>,
        commands: Vec<Vec<u8>>,
        wear_on: bool,
        wear_enabled: bool,
        ignore_sets: bool,
        touch_on: bool,
        language: u8,
    }
    impl Default for SystemFixture {
        fn default() -> Self {
            Self { reads: VecDeque::new(), commands: vec![], wear_on: true, wear_enabled: true, ignore_sets: false, touch_on: true, language: 0 }
        }
    }
    fn gs_text(value: &str) -> Vec<u8> {
        let mut bytes = vec![value.len() as u8];
        bytes.extend(value.as_bytes());
        bytes
    }
    impl Connection for SystemFixture {
        fn service_uuid(&self) -> &'static str {
            "956c7b26-d49a-4ba8-b03f-b17d393cb6e2"
        }
        fn read(&mut self, buffer: &mut [u8], _timeout: Duration) -> std::io::Result<usize> {
            let Some(bytes) = self.reads.pop_front() else {
                return Ok(0);
            };
            buffer[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
        fn write_all(&mut self, bytes: &[u8], _timeout: Duration) -> std::io::Result<()> {
            for frame in Decoder::default().feed(bytes) {
                let frame = frame.unwrap();
                if frame.data_type != 0x0c {
                    continue;
                }
                self.commands.push(frame.payload.clone());
                let mut frames = vec![Frame { data_type: 1, sequence: 1 - frame.sequence, payload: vec![] }];
                let reply = match frame.payload.as_slice() {
                    [0xf2, 0x01] => Some(vec![0xf3, 0x01, u8::from(!self.wear_enabled)]),
                    [0xf6, 0x01] => Some(vec![0xf7, 0x01, u8::from(!self.wear_on)]),
                    [0xf8, 0x01, value] => {
                        if !self.ignore_sets {
                            self.wear_on = *value == 0;
                        }
                        None
                    }
                    [0xd0, 0xd1, 0x01] => {
                        let mut p = vec![0xd1, 0xd1, 0, 1];
                        p.extend(gs_text("TOUCH_PANEL_SETTING"));
                        p.extend(gs_text(""));
                        Some(p)
                    }
                    [0xd0, 0xd2, 0x01] => {
                        let mut p = vec![0xd1, 0xd2, 1, 1];
                        p.extend(gs_text("VOICE_GUIDANCE_SETTING"));
                        p.extend(gs_text(""));
                        p.push(2);
                        for name in ["VOICE_GUIDANCE_ELEM_ENGLISH", "VOICE_GUIDANCE_ELEM_FRENCH"] {
                            p.push(1);
                            p.extend(gs_text(name));
                            p.extend(gs_text(""));
                        }
                        Some(p)
                    }
                    [0xd2, inquiry] => Some(vec![0xd3, *inquiry, 0]),
                    [0xd6, 0xd1] => Some(vec![0xd7, 0xd1, 0, u8::from(!self.touch_on)]),
                    [0xd6, 0xd2] => Some(vec![0xd7, 0xd2, 1, self.language]),
                    [0xd8, 0xd1, 0, value] => {
                        if !self.ignore_sets {
                            self.touch_on = *value == 0;
                        }
                        None
                    }
                    [0xd8, 0xd2, 1, index] => {
                        if !self.ignore_sets {
                            self.language = *index;
                        }
                        None
                    }
                    _ => None,
                };
                if let Some(payload) = reply {
                    frames.push(Frame { data_type: 0x0c, sequence: 0, payload });
                }
                // Capability replies list every option name, so they exceed the 64-byte helper limit.
                self.reads.push_back(frames.iter().flat_map(|frame| encode(frame, 1024).unwrap()).collect());
            }
            Ok(())
        }
    }
    fn system_change(key: &str, value: ControlValue) -> SettingChange {
        SettingChange::DeviceControl(DeviceControlChange { key: key.into(), value })
    }
    fn count_command(connection: &SystemFixture, command: u8) -> usize {
        connection.commands.iter().filter(|payload| payload.first() == Some(&command)).count()
    }

    /// Stateful replay of a six-band headphone (Clear Bass plus 400 Hz, 1 kHz, 2.5 kHz, 6.3 kHz and
    /// 16 kHz) with 21 level steps per band, like the models the Sony app describes as "standard".
    struct EqFixture {
        reads: VecDeque<Vec<u8>>,
        commands: Vec<Vec<u8>>,
        preset: u8,
        bands: Vec<u8>,
        ignore_sets: bool,
        no_band_info: bool,
    }
    impl Default for EqFixture {
        fn default() -> Self {
            Self { reads: VecDeque::new(), commands: vec![], preset: 0, bands: vec![10; 6], ignore_sets: false, no_band_info: false }
        }
    }
    impl EqFixture {
        fn capability() -> Vec<u8> {
            // 6 bands, 21 steps, 4 presets: Off and Bass (named by id), Custom, and one named by the device.
            let mut reply = vec![0x51, 0x00, 6, 21, 4, 0, 0, 22, 0, 0xa0, 0, 0x70, 4];
            reply.extend(b"Warm");
            reply
        }
        fn band_info() -> Vec<u8> {
            vec![0x5b, 0x00, 6, 16, 0, 1, 1, 0x01, 0x90, 1, 0x03, 0xe8, 1, 0x09, 0xc4, 1, 0x18, 0x9c, 1, 0x3e, 0x80]
        }
    }
    impl Connection for EqFixture {
        fn service_uuid(&self) -> &'static str {
            "956c7b26-d49a-4ba8-b03f-b17d393cb6e2"
        }
        fn read(&mut self, buffer: &mut [u8], _timeout: Duration) -> std::io::Result<usize> {
            let Some(bytes) = self.reads.pop_front() else { return Ok(0) };
            buffer[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
        fn write_all(&mut self, bytes: &[u8], _timeout: Duration) -> std::io::Result<()> {
            for frame in Decoder::default().feed(bytes) {
                let frame = frame.unwrap();
                if frame.data_type != 0x0c {
                    continue;
                }
                self.commands.push(frame.payload.clone());
                let mut frames = vec![Frame { data_type: 1, sequence: 1 - frame.sequence, payload: vec![] }];
                let reply = match frame.payload.as_slice() {
                    [0x50, 0x00, 0x01] => Some(Self::capability()),
                    [0x5a, 0x00] if !self.no_band_info => Some(Self::band_info()),
                    [0x56, 0x00] => {
                        let mut reply = vec![0x57, 0x00, self.preset, self.bands.len() as u8];
                        reply.extend(&self.bands);
                        Some(reply)
                    }
                    [0x58, 0x00, preset, count, steps @ ..] => {
                        if !self.ignore_sets {
                            self.preset = *preset;
                            if usize::from(*count) == steps.len() && *count > 0 {
                                self.bands = steps.to_vec();
                            }
                        }
                        None
                    }
                    _ => None,
                };
                if let Some(payload) = reply {
                    frames.push(Frame { data_type: 0x0c, sequence: 0, payload });
                }
                self.reads.push_back(frames.iter().flat_map(|frame| encode(frame, 1024).unwrap()).collect());
            }
            Ok(())
        }
    }
    fn eq_session(connection: &mut EqFixture) -> (ReadSession, EqLayout, EqualizerInfo, Vec<String>) {
        let mut session = ReadSession::new();
        let mut notes = Vec::new();
        let (layout, info) = session.read_generic_equalizer(connection, &AtomicBool::new(false), 0, &mut notes).unwrap();
        session.eq_layout = Some(layout.clone());
        (session, layout, info, notes)
    }

    #[test]
    fn generic_equalizer_layout_is_read_from_the_headphone_not_assumed() {
        let mut connection = EqFixture::default();
        let (_, layout, info, notes) = eq_session(&mut connection);
        assert_eq!((layout.band_count, layout.level_steps, layout.customizable), (6, 21, true));
        assert_eq!(layout.labels, ["Clear Bass", "400 Hz", "1 kHz", "2.5 kHz", "6.3 kHz", "16 kHz"]);
        assert_eq!(layout.clear_bass_index, Some(0));
        let presets: Vec<_> = layout.presets.iter().map(|preset| (preset.id, preset.label.as_str())).collect();
        assert_eq!(presets, [(0, "Off"), (22, "Bass"), (0xa0, "Custom"), (0x70, "Warm")], "known ids use the standard names, others the device's");
        assert_eq!((info.preset_code, info.bands), (0, vec![10; 6]));
        assert!(notes.iter().any(|note| note.starts_with("Equalizer capability") && note.contains("51 00 06 15 04")), "{notes:?}");
        let sent: Vec<u8> = connection.commands.iter().map(|command| command[0]).collect();
        assert_eq!(sent, [0x50, 0x5a, 0x56], "discovery only reads");
    }

    #[test]
    fn missing_band_information_falls_back_to_numbered_bands() {
        let mut connection = EqFixture { no_band_info: true, ..EqFixture::default() };
        let (_, layout, _, notes) = eq_session(&mut connection);
        assert_eq!(layout.labels, ["Band 1", "Band 2", "Band 3", "Band 4", "Band 5", "Band 6"]);
        assert!(notes.iter().any(|note| note.contains("numbered instead")), "{notes:?}");
    }

    #[test]
    fn generic_equalizer_parsers_reject_replies_that_do_not_fit_their_own_declarations() {
        assert!(parse_eq_capability(&[0x51, 0, 6, 21, 2, 0, 0], 0).is_err(), "declares two presets, lists one");
        assert!(parse_eq_capability(&[0x51, 0, 6, 21, 1, 0, 9, b'x'], 0).is_err(), "name longer than the reply");
        assert!(parse_eq_capability(&[0x51, 0, 0, 21, 1, 0, 0], 0).is_err(), "zero bands on a customizable type");
        assert!(parse_eq_capability(&[0x51, 0, 6, 1, 1, 0, 0], 0).is_err(), "a single level step cannot be a range");
        assert!(parse_eq_capability(&[0x51, 2, 0, 13, 1, 0, 0], 2).is_ok(), "preset-only types may have no bands");
        assert!(parse_eq_band_info(&[0x5b, 0, 2, 1, 1, 0x90], 0, 2).is_none(), "truncated");
        assert!(parse_eq_params(&[0x57, 0, 0xa0, 2, 5, 21], 0, 2, 21).is_err(), "a step outside the reported range");
        assert!(parse_eq_params(&[0x57, 0, 0xa0, 3, 5, 6, 7], 0, 2, 21).is_err(), "wrong band count");
        assert_eq!(eq_frequency_label(1, 2500), "2.5 kHz");
        assert_eq!(eq_frequency_label(1, 16000), "16 kHz");
        assert_eq!(eq_frequency_label(2, 4), "4 kHz");
    }

    #[test]
    fn generic_equalizer_band_write_confirms_by_readback_with_the_custom_preset() {
        let mut connection = EqFixture::default();
        let (mut session, _, _, _) = eq_session(&mut connection);
        connection.commands.clear();
        let SettingConfirmation::Equalizer(info) = session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::Equalizer(vec![12, 10, 10, 8, 10, 14]))
            .unwrap()
        else {
            panic!("expected an equalizer confirmation");
        };
        assert_eq!((info.preset_code, info.bands), (0xa0, vec![12, 10, 10, 8, 10, 14]));
        let sets: Vec<_> = connection.commands.iter().filter(|command| command[0] == 0x58).collect();
        assert_eq!(sets, [&vec![0x58, 0x00, 0xa0, 6, 12, 10, 10, 8, 10, 14]], "one SET: custom preset, six steps");
    }

    #[test]
    fn generic_equalizer_preset_write_sends_only_the_preset_and_confirms_it() {
        let mut connection = EqFixture::default();
        let (mut session, _, _, _) = eq_session(&mut connection);
        connection.commands.clear();
        let SettingConfirmation::Equalizer(info) = session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::EqualizerPreset(22))
            .unwrap()
        else {
            panic!("expected an equalizer confirmation");
        };
        assert_eq!(info.preset_code, 22);
        let sets: Vec<_> = connection.commands.iter().filter(|command| command[0] == 0x58).collect();
        assert_eq!(sets, [&vec![0x58, 0x00, 22, 0]], "a preset change carries no band values");
    }

    #[test]
    fn generic_equalizer_rejects_out_of_range_values_and_unknown_presets_before_any_write() {
        let mut connection = EqFixture::default();
        let (mut session, _, _, _) = eq_session(&mut connection);
        connection.commands.clear();
        for change in [
            SettingChange::Equalizer(vec![10; 5]),
            SettingChange::Equalizer(vec![21, 10, 10, 10, 10, 10]),
            SettingChange::EqualizerPreset(99),
        ] {
            assert!(session.apply_setting(&mut connection, &AtomicBool::new(false), change).is_err());
        }
        assert!(connection.commands.iter().all(|command| command[0] != 0x58), "no SET was sent");
    }

    #[test]
    fn generic_equalizer_restores_and_verifies_the_previous_profile_when_readback_never_matches() {
        let mut connection = EqFixture { preset: 22, ..EqFixture::default() };
        let (mut session, _, _, _) = eq_session(&mut connection);
        connection.ignore_sets = true;
        let error = session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::Equalizer(vec![12, 10, 10, 8, 10, 14]))
            .unwrap_err();
        assert!(error.contains("previous profile was restored and verified"), "{error}");
        let sets: Vec<_> = connection.commands.iter().filter(|command| command[0] == 0x58).collect();
        assert_eq!(sets.len(), 2, "the request, then one restore of the previous preset");
        assert_eq!(sets[1], &vec![0x58, 0x00, 22, 0], "restore sends the previous preset without band values");
    }

    #[test]
    fn a_headphone_without_a_described_equalizer_rejects_preset_changes() {
        let mut connection = EqFixture::default();
        let mut session = ReadSession::new();
        let error = session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::EqualizerPreset(0))
            .unwrap_err();
        assert!(error.contains("has not described an equalizer"), "{error}");
        assert!(connection.commands.is_empty());
    }

    /// Stateful replay of a headphone that manages paired devices over message table 2 (frame type
    /// 0x0e). A connect or disconnect takes effect after `delay_polls` list reads, like a real one.
    struct MultipointFixture {
        reads: VecDeque<Vec<u8>>,
        commands: Vec<(u8, Vec<u8>)>,
        devices: Vec<(String, u8, String)>,
        playback: u8,
        pending: Option<(String, u8, u32)>,
        delay_polls: u32,
        ignore_actions: bool,
        answer_table2: bool,
        limit: u8,
        /// The headphone resets its control link right after it accepts this many connect/disconnect requests.
        reset_after_actions: usize,
        actions_seen: usize,
        link_closed: bool,
    }
    impl MultipointFixture {
        fn new() -> Self {
            Self {
                reads: VecDeque::new(),
                commands: vec![],
                devices: vec![
                    ("AA:BB:CC:00:11:22".into(), 1, "Pixel 9".into()),
                    ("AA:BB:CC:00:11:33".into(), 0, "Work laptop".into()),
                    ("AA:BB:CC:00:11:44".into(), 0, "iPad".into()),
                ],
                playback: 0,
                pending: None,
                delay_polls: 2,
                ignore_actions: false,
                answer_table2: true,
                limit: 2,
                reset_after_actions: 0,
                actions_seen: 0,
                link_closed: false,
            }
        }
        fn list_reply(&self, reply: u8) -> Vec<u8> {
            let mut payload = vec![reply, 2, self.devices.len() as u8];
            for (address, status, name) in &self.devices {
                payload.extend(address.as_bytes());
                payload.push(*status);
                payload.extend([0x5a, 0x02, 0x0c, name.len() as u8]);
                payload.extend(name.as_bytes());
            }
            // The last byte names the connection slot (status) of the device that plays.
            payload.push(self.devices.get(usize::from(self.playback)).map_or(0, |device| device.1));
            payload
        }
    }
    impl Connection for MultipointFixture {
        fn service_uuid(&self) -> &'static str {
            "956c7b26-d49a-4ba8-b03f-b17d393cb6e2"
        }
        fn read(&mut self, buffer: &mut [u8], _timeout: Duration) -> std::io::Result<usize> {
            let Some(bytes) = self.reads.pop_front() else { return Ok(0) };
            buffer[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
        fn write_all(&mut self, bytes: &[u8], _timeout: Duration) -> std::io::Result<()> {
            for frame in Decoder::default().feed(bytes) {
                let frame = frame.unwrap();
                if frame.data_type != 0x0c && frame.data_type != 0x0e {
                    continue;
                }
                if self.link_closed {
                    // The headphone has reset the link: nothing is answered any more.
                    self.commands.push((frame.data_type, frame.payload.clone()));
                    continue;
                }
                self.commands.push((frame.data_type, frame.payload.clone()));
                let mut frames = vec![Frame { data_type: 1, sequence: 1 - frame.sequence, payload: vec![] }];
                if frame.data_type == 0x0e && frame.payload.first() == Some(&0x3c) {
                    self.actions_seen += 1;
                    if self.actions_seen == self.reset_after_actions {
                        self.link_closed = true;
                    }
                }
                let reply = match (frame.data_type, frame.payload.as_slice()) {
                    (0x0e, [0x06, 0x00]) if self.answer_table2 => Some(vec![0x07, 0x00, 2, 0x31, 0, 0x33, 0]),
                    (0x0e, [0x30, 2]) => Some(vec![0x31, 2, 3, self.limit, 0]),
                    (0x0e, [0x36, 2]) => {
                        if let Some((address, status, polls)) = self.pending.take() {
                            if polls <= 1 {
                                // A connecting device takes the lowest free connection slot.
                                let taken: Vec<u8> = self.devices.iter().map(|device| device.1).filter(|slot| *slot > 0).collect();
                                let free = (1u8..).find(|slot| !taken.contains(slot)).unwrap_or(1);
                                for device in &mut self.devices {
                                    if device.0 == address {
                                        device.1 = if status > 0 { free } else { 0 };
                                    }
                                }
                            } else {
                                self.pending = Some((address, status, polls - 1));
                            }
                        }
                        Some(self.list_reply(0x37))
                    }
                    (0x0e, [0x3c, 2, action, address @ ..]) => {
                        if !self.ignore_actions {
                            self.pending = Some((String::from_utf8_lossy(address).to_string(), *action, self.delay_polls));
                        }
                        None
                    }
                    _ => None,
                };
                if let Some(payload) = reply {
                    frames.push(Frame { data_type: 0x0e, sequence: 0, payload });
                }
                self.reads.push_back(frames.iter().flat_map(|frame| encode(frame, 1024).unwrap()).collect());
            }
            Ok(())
        }
    }
    fn multipoint_session(connection: &mut MultipointFixture) -> (ReadSession, mp::DeviceList, Vec<String>) {
        let mut session = ReadSession::new();
        session.multipoint_wait = Duration::from_secs(2);
        session.multipoint_poll = Duration::from_millis(10);
        let mut notes = Vec::new();
        let list = session.multipoint_discover(connection, &AtomicBool::new(false), &mut notes).unwrap().expect("multipoint is offered");
        (session, list, notes)
    }
    fn mp_json(confirmation: SettingConfirmation) -> (serde_json::Value, serde_json::Value) {
        let SettingConfirmation::DeviceControl(confirmation) = confirmation else { panic!("expected a device-control confirmation") };
        assert_eq!(confirmation.key, "multipointDevices");
        let parse = |value: ControlValue| match value {
            ControlValue::Text(text) => serde_json::from_str(&text).unwrap(),
            other => panic!("expected text, got {other:?}"),
        };
        (parse(confirmation.before), parse(confirmation.confirmed))
    }

    #[test]
    fn multipoint_discovery_reads_the_table_2_functions_capability_and_paired_devices_over_frame_type_0e() {
        let mut connection = MultipointFixture::new();
        let (session, list, notes) = multipoint_session(&mut connection);
        assert_eq!(session.multipoint_inquiry, Some(2));
        let names: Vec<_> = list.devices.iter().map(|device| (device.name.as_str(), device.connected)).collect();
        assert_eq!(names, [("Pixel 9", true), ("Work laptop", false), ("iPad", false)]);
        assert!(connection.commands.iter().all(|(data_type, _)| *data_type == 0x0e), "table 2 requests use frame type 0x0e");
        let sent: Vec<u8> = connection.commands.iter().map(|(_, payload)| payload[0]).collect();
        assert_eq!(sent, [0x06, 0x30, 0x36], "functions, capability, list: all reads");
        assert!(notes.iter().any(|note| note.contains("0x31, 0x33")), "{notes:?}");
        assert!(notes.iter().any(|note| note.contains("Pairing-device capability: 31 02 03 02 00")), "{notes:?}");
        assert_eq!(session.multipoint_limit, Some(2), "the fourth capability byte is the connection limit");
        assert_eq!(list.max_connections, Some(2));
    }

    #[test]
    fn a_headphone_that_ignores_table_2_is_simply_not_offered_multipoint() {
        let mut connection = MultipointFixture { answer_table2: false, ..MultipointFixture::new() };
        let mut session = ReadSession::new();
        let mut notes = Vec::new();
        assert!(session.multipoint_discover(&mut connection, &AtomicBool::new(false), &mut notes).unwrap().is_none());
        assert!(notes.iter().any(|note| note.contains("not offered")), "{notes:?}");
    }

    #[test]
    fn connecting_a_device_sends_one_request_and_confirms_only_when_the_list_shows_it_connected() {
        let mut connection = MultipointFixture::new();
        let (mut session, _, _) = multipoint_session(&mut connection);
        connection.commands.clear();
        let (before, after) = mp_json(
            session
                .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: "AA:BB:CC:00:11:33".into(), connect: true })
                .unwrap(),
        );
        assert_eq!(before["devices"][1]["connected"], false);
        assert_eq!(after["devices"][1]["connected"], true);
        assert_eq!(after["devices"][0]["connected"], true, "the other device stays connected");
        let actions: Vec<_> = connection.commands.iter().filter(|(_, payload)| payload[0] == 0x3c).collect();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, 0x0e);
        let mut expected = vec![0x3c, 2, 1];
        expected.extend(b"AA:BB:CC:00:11:33");
        assert_eq!(actions[0].1, expected);
        assert!(connection.commands.iter().filter(|(_, payload)| payload[0] == 0x36).count() >= 3, "it polled the list until the change appeared");
    }

    fn swap_fixture(playing: u8) -> MultipointFixture {
        MultipointFixture {
            devices: vec![
                ("AA:BB:CC:00:11:22".into(), 1, "Pixel 9".into()),
                ("AA:BB:CC:00:11:33".into(), 2, "Work laptop".into()),
                ("AA:BB:CC:00:11:44".into(), 0, "iPad".into()),
            ],
            playback: playing,
            delay_polls: 1,
            ..MultipointFixture::new()
        }
    }
    fn action_log(connection: &MultipointFixture) -> Vec<(u8, String)> {
        connection
            .commands
            .iter()
            .filter(|(_, payload)| payload[0] == 0x3c)
            .map(|(_, payload)| (payload[2], String::from_utf8_lossy(&payload[3..]).to_string()))
            .collect()
    }

    #[test]
    fn connecting_a_third_device_first_disconnects_the_one_that_is_not_active() {
        // The laptop (this computer) plays audio, the phone does not, and the headphone allows two devices.
        let mut connection = swap_fixture(1);
        let (mut session, _, _) = multipoint_session(&mut connection);
        session.own_addresses = vec!["aa:bb:cc:00:11:33".into()];
        connection.commands.clear();
        let (before, after) = mp_json(
            session
                .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: "AA:BB:CC:00:11:44".into(), connect: true })
                .unwrap(),
        );
        assert_eq!(before["devices"][0]["connected"], true);
        let states: Vec<_> = after["devices"].as_array().unwrap().iter().map(|d| (d["name"].as_str().unwrap(), d["connected"].as_bool().unwrap())).collect();
        assert_eq!(states, [("Pixel 9", false), ("Work laptop", true), ("iPad", true)]);
        assert_eq!(
            action_log(&connection),
            [(0, "AA:BB:CC:00:11:22".to_string()), (1, "AA:BB:CC:00:11:44".to_string())],
            "the phone is disconnected first, then the iPad is connected, and nothing else is touched"
        );
        assert_eq!(after["devices"][1]["thisComputer"], true);
        assert_eq!(after["maxConnections"], 2);
    }

    #[test]
    fn the_playing_device_and_this_computer_are_never_disconnected_to_make_room() {
        // The phone plays audio and the laptop is this computer: there is nothing safe to drop.
        let mut connection = swap_fixture(0);
        let (mut session, _, _) = multipoint_session(&mut connection);
        session.own_addresses = vec!["AA:BB:CC:00:11:33".into()];
        connection.commands.clear();
        let error = session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: "AA:BB:CC:00:11:44".into(), connect: true })
            .unwrap_err();
        assert!(error.contains("none can be dropped automatically"), "{error}");
        assert!(action_log(&connection).is_empty(), "no disconnect or connect request was sent");
    }

    #[test]
    fn an_unknown_connection_limit_never_drops_a_device() {
        let mut connection = MultipointFixture { limit: 0, delay_polls: 1, ..swap_fixture(1) };
        let (mut session, _, _) = multipoint_session(&mut connection);
        session.own_addresses = vec!["AA:BB:CC:00:11:33".into()];
        connection.commands.clear();
        session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: "AA:BB:CC:00:11:44".into(), connect: true })
            .unwrap();
        assert_eq!(action_log(&connection), [(1, "AA:BB:CC:00:11:44".to_string())], "only the connect request");
    }

    #[test]
    fn if_the_device_to_drop_does_not_disconnect_the_new_one_is_never_connected() {
        let mut connection = MultipointFixture { ignore_actions: true, ..swap_fixture(1) };
        let (mut session, _, _) = multipoint_session(&mut connection);
        session.own_addresses = vec!["AA:BB:CC:00:11:33".into()];
        session.multipoint_wait = Duration::from_millis(200);
        connection.commands.clear();
        let error = session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: "AA:BB:CC:00:11:44".into(), connect: true })
            .unwrap_err();
        assert!(error.contains("iPad was not connected") && error.contains("Pixel 9 would not disconnect"), "{error}");
        assert_eq!(action_log(&connection), [(0, "AA:BB:CC:00:11:22".to_string())], "the connect request was never sent");
    }

    #[test]
    fn a_link_reset_after_the_connect_request_is_explained_as_probably_succeeding_not_as_a_failure() {
        // The headphone accepts the connect request, then resets the control link (as it does when its
        // connections change). The new device does connect a few seconds later.
        let mut connection = MultipointFixture { reset_after_actions: 1, ..MultipointFixture::new() };
        let (mut session, _, _) = multipoint_session(&mut connection);
        connection.commands.clear();
        let error = session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: "AA:BB:CC:00:11:44".into(), connect: true })
            .unwrap_err();
        assert!(error.contains("briefly reset its control link while connecting iPad"), "{error}");
        assert!(error.contains("most likely went through") && error.contains("reconnecting"), "{error}");
        assert!(!error.contains("did not connect"), "a link reset must not be reported as a failed connection: {error}");
        assert_eq!(action_log(&connection).len(), 1, "no second request is sent into a dead link");
    }

    #[test]
    fn a_link_reset_while_the_swapped_out_device_disconnects_says_to_connect_the_new_one_again() {
        let mut connection = MultipointFixture { reset_after_actions: 1, ..swap_fixture(1) };
        let (mut session, _, _) = multipoint_session(&mut connection);
        session.own_addresses = vec!["AA:BB:CC:00:11:33".into()];
        connection.commands.clear();
        let error = session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: "AA:BB:CC:00:11:44".into(), connect: true })
            .unwrap_err();
        assert!(error.contains("briefly reset its control link while Pixel 9 was disconnecting to make room for iPad"), "{error}");
        assert!(error.contains("connect iPad again"), "{error}");
        assert_eq!(action_log(&connection), [(0, "AA:BB:CC:00:11:22".to_string())], "the connect request was not sent");
    }

    #[test]
    fn a_link_reset_after_the_swap_connect_request_is_also_treated_as_probably_succeeding() {
        let mut connection = MultipointFixture { reset_after_actions: 2, ..swap_fixture(1) };
        let (mut session, _, _) = multipoint_session(&mut connection);
        session.own_addresses = vec!["AA:BB:CC:00:11:33".into()];
        connection.commands.clear();
        let error = session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: "AA:BB:CC:00:11:44".into(), connect: true })
            .unwrap_err();
        assert!(error.contains("briefly reset its control link while connecting iPad"), "{error}");
        assert_eq!(action_log(&connection).len(), 2, "disconnect, then connect: both were sent");
    }

    #[test]
    fn disconnecting_a_device_uses_action_zero() {
        let mut connection = MultipointFixture::new();
        let (mut session, _, _) = multipoint_session(&mut connection);
        connection.commands.clear();
        let (_, after) = mp_json(
            session
                .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: "AA:BB:CC:00:11:22".into(), connect: false })
                .unwrap(),
        );
        assert_eq!(after["devices"][0]["connected"], false);
        let action = connection.commands.iter().find(|(_, payload)| payload[0] == 0x3c).unwrap();
        assert_eq!(&action.1[..3], [0x3c, 2, 0]);
    }

    #[test]
    fn asking_for_the_state_a_device_already_has_sends_nothing() {
        let mut connection = MultipointFixture::new();
        let (mut session, _, _) = multipoint_session(&mut connection);
        connection.commands.clear();
        session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: "AA:BB:CC:00:11:22".into(), connect: true })
            .unwrap();
        assert!(connection.commands.iter().all(|(_, payload)| payload[0] != 0x3c), "no connect request for a connected device");
    }

    #[test]
    fn unknown_or_malformed_devices_are_rejected_before_any_request() {
        let mut connection = MultipointFixture::new();
        let (mut session, _, _) = multipoint_session(&mut connection);
        connection.commands.clear();
        for address in ["AA:BB:CC:00:11:99", "not-an-address"] {
            let error = session
                .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: address.into(), connect: true })
                .unwrap_err();
            assert!(error.contains("paired list") || error.contains("Bluetooth address"), "{error}");
        }
        assert!(connection.commands.iter().all(|(_, payload)| payload[0] != 0x3c));
    }

    #[test]
    fn a_change_the_headphone_never_reports_is_an_error_and_the_list_is_left_as_reported() {
        let mut connection = MultipointFixture { ignore_actions: true, ..MultipointFixture::new() };
        let (mut session, _, _) = multipoint_session(&mut connection);
        session.multipoint_wait = Duration::from_millis(200);
        let error = session
            .apply_setting(&mut connection, &AtomicBool::new(false), SettingChange::MultipointDevice { address: "AA:BB:CC:00:11:33".into(), connect: true })
            .unwrap_err();
        assert!(error.contains("did not report that device as connected"), "{error}");
        assert!(connection.commands.iter().filter(|(_, payload)| payload[0] == 0x3c).count() == 1, "no retry or rollback request");
    }

    #[test]
    fn a_passive_device_list_notification_updates_the_list_and_ignores_garbage() {
        let mut connection = MultipointFixture::new();
        let (mut session, _, _) = multipoint_session(&mut connection);
        connection.devices[2].1 = 1;
        let notification = Frame { data_type: 0x0e, sequence: 0, payload: connection.list_reply(0x39) };
        session.record_noise_observation(&notification);
        let updates = session.drain_device_updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].key, "multipointDevices");
        let ControlValue::Text(after) = &updates[0].confirmed else { panic!("text") };
        assert!(after.contains("\"iPad\"") && after.contains("\"connected\":true"));
        session.record_noise_observation(&Frame { data_type: 0x0e, sequence: 0, payload: vec![0x39, 2, 9, 1, 2, 3] });
        session.record_noise_observation(&notification);
        assert!(session.drain_device_updates().is_empty(), "malformed or unchanged notifications change nothing");
    }

    #[test]
    fn a_link_the_headphone_closes_during_discovery_is_recognised_so_the_connection_can_be_retried() {
        // A headphone that closes the link before answering anything.
        let mut connection = MultipointFixture { answer_table2: false, ..MultipointFixture::new() };
        connection.reads.clear();
        let (info, _) = read_basics(&mut connection, &AtomicBool::new(false));
        assert_eq!(info.protocol.as_ref().err().map(String::as_str), Some(CLOSED_DURING_DISCOVERY));
        assert!(closed_during_discovery(&info));
        let mut later = BasicInfo { protocol: Ok(ProtocolInfo { format: "v2", version: 1, ack_observed: true, table2: false }), model: None, firmware: None, battery: None, controls: ControlInfo::default(), warnings: vec![] };
        assert!(!closed_during_discovery(&later));
        later.warnings.push(format!("General setting 1 could not be read: {CLOSED_DURING_DISCOVERY}"));
        assert!(closed_during_discovery(&later), "a drop partway through discovery counts too");
    }

    #[test]
    fn the_optional_equalizer_and_multipoint_reads_can_be_skipped_for_a_last_attempt() {
        // Table 2 is advertised, but with extras off no table 2 frame is ever sent.
        let mut connection = MultipointFixture::new();
        let mut session = ReadSession::new();
        session.table2_supported = true;
        let extras = false;
        assert!(!(extras && session.table2_supported), "the gate used by read_basics_with");
        let (info, _) = read_basics_with(&mut connection, &AtomicBool::new(false), false);
        assert!(connection.commands.iter().all(|(data_type, _)| *data_type != 0x0e), "no table 2 request without extras");
        assert!(!info.controls.capabilities.contains(&"multipointDevices".to_string()));
    }

    #[test]
    fn protocol_info_reports_whether_message_table_2_is_supported() {
        let info = |last: u8| parse_protocol_info(&[0x01, 0x00, 0, 0, 0, 1, 0, last]).unwrap();
        assert!(info(0).table2, "EnableDisable ENABLE is 0");
        assert!(!info(1).table2);
        assert!(!parse_protocol_info(&[0x01, 0x00, 0, 1]).unwrap().table2, "V1 has no table 2");
    }

    #[test]
    fn wear_detection_reads_status_and_value_then_writes_and_confirms_by_readback() {
        let mut connection = SystemFixture::default();
        let cancelled = AtomicBool::new(false);
        let mut session = ReadSession::new();
        assert_eq!(session.wear_read(&mut connection, &cancelled).unwrap(), (true, true));
        session.install_device_controls(&[ConfirmedControl {
            descriptor: sys::wear_descriptor(),
            value: ControlValue::Boolean(true),
        }]);
        let SettingConfirmation::DeviceControl(confirmation) = session
            .apply_setting(&mut connection, &cancelled, system_change("wear", ControlValue::Boolean(false)))
            .unwrap()
        else {
            panic!("expected a typed wear confirmation");
        };
        assert_eq!(confirmation.before, ControlValue::Boolean(true));
        assert_eq!(confirmation.confirmed, ControlValue::Boolean(false));
        assert!(!connection.wear_on);
        let sets: Vec<_> = connection.commands.iter().filter(|c| c[0] == 0xf8).collect();
        assert_eq!(sets, [&vec![0xf8, 0x01, 1]]);
    }

    #[test]
    fn wear_detection_without_matching_readback_is_never_confirmed_and_sends_no_restore() {
        let mut connection = SystemFixture { ignore_sets: true, ..SystemFixture::default() };
        let cancelled = AtomicBool::new(false);
        let mut session = ReadSession::new();
        session.wear_read(&mut connection, &cancelled).unwrap();
        session.install_device_controls(&[ConfirmedControl { descriptor: sys::wear_descriptor(), value: ControlValue::Boolean(true) }]);
        let error = session
            .apply_setting(&mut connection, &cancelled, system_change("wear", ControlValue::Boolean(false)))
            .unwrap_err();
        assert!(error.contains("did not confirm"), "{error}");
        assert_eq!(count_command(&connection, 0xf8), 1, "one SET, no automatic restoration");
    }

    #[test]
    fn unavailable_wear_status_is_reported_without_an_error() {
        let mut connection = SystemFixture { wear_enabled: false, wear_on: false, ..SystemFixture::default() };
        let mut session = ReadSession::new();
        assert_eq!(session.wear_read(&mut connection, &AtomicBool::new(false)).unwrap(), (false, false));
    }

    #[test]
    fn general_settings_are_described_by_the_headphone_and_written_with_readback() {
        let mut connection = SystemFixture::default();
        let cancelled = AtomicBool::new(false);
        let mut session = ReadSession::new();
        let (touch, enabled) = session.general_setting_read(&mut connection, &cancelled, 1).unwrap();
        assert!(enabled);
        assert_eq!(touch.descriptor.key, "touch");
        assert_eq!(touch.value, ControlValue::Boolean(true));
        let (guidance, _) = session.general_setting_read(&mut connection, &cancelled, 2).unwrap();
        assert_eq!(guidance.descriptor.key, "guidance");
        assert_eq!(guidance.value, ControlValue::Integer(0));
        session.install_device_controls(&[touch, guidance]);

        let SettingConfirmation::DeviceControl(done) = session
            .apply_setting(&mut connection, &cancelled, system_change("touch", ControlValue::Boolean(false)))
            .unwrap()
        else {
            panic!("expected a typed confirmation");
        };
        assert_eq!(done.confirmed, ControlValue::Boolean(false));
        assert!(!connection.touch_on);
        session
            .apply_setting(&mut connection, &cancelled, system_change("guidance", ControlValue::Integer(1)))
            .unwrap();
        assert_eq!(connection.language, 1);
        let sets: Vec<_> = connection.commands.iter().filter(|c| c[0] == 0xd8).cloned().collect();
        assert_eq!(sets, [vec![0xd8, 0xd1, 0, 1], vec![0xd8, 0xd2, 1, 1]]);
        assert!(
            session
                .apply_setting(&mut connection, &cancelled, system_change("guidance", ControlValue::Integer(2)))
                .is_err(),
            "an option the headphone never listed is rejected before any write"
        );
        assert_eq!(count_command(&connection, 0xd8), 2);
    }

    #[test]
    fn general_setting_without_matching_readback_is_never_confirmed() {
        let mut connection = SystemFixture { ignore_sets: true, ..SystemFixture::default() };
        let cancelled = AtomicBool::new(false);
        let mut session = ReadSession::new();
        let (touch, _) = session.general_setting_read(&mut connection, &cancelled, 1).unwrap();
        session.install_device_controls(&[touch]);
        assert!(session
            .apply_setting(&mut connection, &cancelled, system_change("touch", ControlValue::Boolean(false)))
            .is_err());
        assert_eq!(count_command(&connection, 0xd8), 1);
    }

    #[test]
    fn wear_and_general_setting_notifications_update_confirmed_values() {
        let mut connection = SystemFixture::default();
        let cancelled = AtomicBool::new(false);
        let mut session = ReadSession::new();
        session.wear_read(&mut connection, &cancelled).unwrap();
        let (touch, _) = session.general_setting_read(&mut connection, &cancelled, 1).unwrap();
        session.install_device_controls(&[ConfirmedControl { descriptor: sys::wear_descriptor(), value: ControlValue::Boolean(true) }, touch]);
        for payload in [vec![0xf9, 0x01, 1], vec![0xd9, 0xd1, 0, 1], vec![0xd9, 0xd2, 1, 0]] {
            session.record_noise_observation(&Frame { data_type: 0x0c, sequence: 0, payload });
        }
        let updates = session.drain_device_updates();
        let keys: Vec<_> = updates.iter().map(|update| (update.key.as_str(), update.confirmed.clone())).collect();
        assert_eq!(keys, [("wear", ControlValue::Boolean(false)), ("touch", ControlValue::Boolean(false))]);
    }

    #[test]
    fn wearing_aware_power_off_accepts_the_inquiry_0x05_path_and_rejects_unadvertised_options() {
        let mut session = ReadSession::new();
        session.power_off_inquiry = Some(0x05);
        session.power_off_options = vec![0, 1, 16, 17];
        let mut connection = FixtureConnection { reads: VecDeque::new(), writes: vec![], service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2" };
        let error = session
            .apply_power_off_setting(
                &mut connection,
                &AtomicBool::new(false),
                DeviceControlChange { key: "autoPowerOff".into(), value: ControlValue::Integer(4) },
            )
            .unwrap_err();
        assert!(error.contains("not advertised"), "{error}");
        assert!(connection.writes.is_empty());
    }

    #[test]
    fn dsee_final_notification_updates_confirmed_control_value() {
        let mut session = ReadSession::new();
        session.install_device_controls(&[dsee_control("off")]);
        session.feed_frames(&wire_frames(&[Frame {
            data_type: 0x0c,
            sequence: 0,
            payload: vec![0xe9, 1, 1],
        }]));
        assert_eq!(
            session.drain_device_updates(),
            [DeviceControlConfirmation {
                key: "dsee".into(),
                before: ControlValue::Text("off".into()),
                confirmed: ControlValue::Text("auto".into()),
            }]
        );
        assert!(session.drain_device_updates().is_empty());
    }

    #[test]
    fn focus_mode_only_follows_final_inquiry19_notifications() {
        let mut session = ReadSession::new();
        session.noise_inquiry = Some(0x19);
        session.install_device_controls(&[focus_control(false)]);
        session.feed_frames(&wire_frames(&[Frame {
            data_type: 0x0c,
            sequence: 0,
            payload: vec![0x69, 0x19, 0, 1, 0, 1, 9, 1, 2],
        }]));
        assert!(session.drain_device_updates().is_empty());
        session.feed_frames(&wire_frames(&[Frame {
            data_type: 0x0c,
            sequence: 1,
            payload: vec![0x69, 0x19, 1, 1, 0, 1, 9, 1, 2],
        }]));
        assert_eq!(
            session.drain_device_updates(),
            [DeviceControlConfirmation {
                key: "focusOnVoice".into(),
                before: ControlValue::Boolean(false),
                confirmed: ControlValue::Boolean(true),
            }]
        );
    }

    #[test]
    fn focus_mode_read_modify_write_requires_final_baseline_and_preserves_other_fields() {
        let mut changing = FixtureConnection {
            reads: VecDeque::from([noise19_exchange(
                1,
                Frame {
                    data_type: 0x0c,
                    sequence: 0,
                    payload: vec![0x67, 0x19, 0, 1, 0, 0, 9, 1, 2],
                },
            )]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        session.noise_inquiry = Some(0x19);
        session.install_device_controls(&[focus_control(false)]);
        assert!(session
            .apply_setting(
                &mut changing,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(DeviceControlChange {
                    key: "focusOnVoice".into(),
                    value: ControlValue::Boolean(true),
                }),
            )
            .is_err());
        assert_eq!(setting_requests(&changing, 0x68), 0);

        let mut connection = FixtureConnection {
            reads: VecDeque::from([
                noise19_exchange(
                    1,
                    Frame {
                        data_type: 0x0c,
                        sequence: 0,
                        payload: vec![0x67, 0x19, 1, 1, 1, 0, 9, 1, 2],
                    },
                ),
                wire_frames(&[Frame {
                    data_type: 1,
                    sequence: 0,
                    payload: vec![],
                }]),
                noise19_exchange(
                    1,
                    Frame {
                        data_type: 0x0c,
                        sequence: 0,
                        payload: vec![0x67, 0x19, 1, 1, 1, 1, 9, 1, 2],
                    },
                ),
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        session.noise_inquiry = Some(0x19);
        session.install_device_controls(&[focus_control(false)]);
        let confirmation = session
            .apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(DeviceControlChange {
                    key: "focusOnVoice".into(),
                    value: ControlValue::Boolean(true),
                }),
            )
            .unwrap();
        let SettingConfirmation::DeviceControl(confirmation) = confirmation else {
            panic!("expected focus-on-voice confirmation");
        };
        assert_eq!(confirmation.before, ControlValue::Boolean(false));
        assert_eq!(confirmation.confirmed, ControlValue::Boolean(true));
        let mut decoder = Decoder::default();
        let request = connection
            .writes
            .iter()
            .flat_map(|wire| decoder.feed(wire))
            .map(Result::unwrap)
            .find(|frame| frame.data_type == 0x0c && frame.payload.first() == Some(&0x68))
            .unwrap();
        assert_eq!(request.payload, [0x68, 0x19, 1, 1, 1, 1, 9, 1, 2]);
    }

    #[test]
    fn focus_on_voice_is_refused_without_a_write_outside_ambient_sound_mode() {
        for ncasm_mode in [0u8] {
            let mut connection = FixtureConnection {
                reads: VecDeque::from([noise19_exchange(
                    1,
                    Frame {
                        data_type: 0x0c,
                        sequence: 0,
                        payload: vec![0x67, 0x19, 1, 1, ncasm_mode, 0, 9, 1, 2],
                    },
                )]),
                writes: vec![],
                service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
            };
            let mut session = ReadSession::new();
            session.noise_inquiry = Some(0x19);
            session.install_device_controls(&[focus_control(false)]);
            let error = session
                .apply_setting(
                    &mut connection,
                    &AtomicBool::new(false),
                    SettingChange::DeviceControl(DeviceControlChange {
                        key: "focusOnVoice".into(),
                        value: ControlValue::Boolean(true),
                    }),
                )
                .unwrap_err();
            assert!(error.contains("Ambient sound"), "{error}");
            assert_eq!(setting_requests(&connection, 0x68), 0, "no SET is sent");
        }
    }

    #[test]
    fn matched_response_and_transport_ack_are_separate() {
        let ack = encode(
            &Frame {
                data_type: 1,
                sequence: 1,
                payload: vec![],
            },
            64,
        )
        .unwrap();
        let response = encode(
            &Frame {
                data_type: 0x0c,
                sequence: 0,
                payload: vec![1, 0, 0, 2],
            },
            64,
        )
        .unwrap();
        let mut connection = FixtureConnection {
            reads: VecDeque::from([ack, response]),
            writes: vec![],
            service: "96cc203e-5068-46ad-b32d-e316f5e069ba",
        };
        let info = query_protocol_info(&mut connection, &AtomicBool::new(false)).unwrap();
        assert_eq!(info.version, 2);
        assert!(info.ack_observed);
        assert_eq!(
            connection.writes[0],
            [0x3e, 0x0c, 0, 0, 0, 0, 2, 0, 0, 0x0e, 0x3c]
        );
        assert_eq!(connection.writes[1], [0x3e, 1, 1, 0, 0, 0, 0, 2, 0x3c]);
    }

    #[test]
    fn ack_alone_never_confirms_discovery() {
        let ack = encode(
            &Frame {
                data_type: 1,
                sequence: 1,
                payload: vec![],
            },
            64,
        )
        .unwrap();
        let mut connection = FixtureConnection {
            reads: VecDeque::from([ack]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        assert!(query_protocol_info(&mut connection, &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn cancelled_discovery_never_writes() {
        let mut connection = FixtureConnection {
            reads: VecDeque::new(),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        assert!(query_protocol_info(&mut connection, &AtomicBool::new(true)).is_err());
        assert!(connection.writes.is_empty());
    }

    #[test]
    fn acknowledged_setting_frames_accept_nc_and_equalizer_payload_lengths() {
        for payload in [
            vec![0x68, 0x17, 0x01, 0x01, 0x00, 0x00, 0x00],
            vec![0x58, 0x04, 0xa0, 10, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        ] {
            let ack = encode(
                &Frame {
                    data_type: 1,
                    sequence: 1,
                    payload: vec![],
                },
                64,
            )
            .unwrap();
            let mut connection = FixtureConnection {
                reads: VecDeque::from([ack]),
                writes: vec![],
                service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
            };
            let mut session = ReadSession::new();
            session
                .write_acknowledged(&mut connection, &AtomicBool::new(false), &payload)
                .unwrap();

            let mut decoder = Decoder::default();
            let request = connection
                .writes
                .iter()
                .flat_map(|wire| decoder.feed(wire))
                .map(Result::unwrap)
                .find(|frame| frame.data_type == 0x0c)
                .unwrap();
            assert_eq!(request.payload, payload);
        }
    }

    #[test]
    fn noise_mode_confirmation_waits_for_a_delayed_matching_readback() {
        let mut connection = FixtureConnection {
            reads: VecDeque::from([
                noise_exchange(1, 1, 0), // initial ambient baseline
                wire_frames(&[Frame {
                    data_type: 1,
                    sequence: 0,
                    payload: vec![],
                }]), // NC/ASM SET transport ACK
                noise_exchange(1, 1, 0), // first GET still reports ambient
                noise_exchange(0, 0, 1), // next GET confirms cancelling
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        let confirmation = session
            .apply_noise_mode(
                &mut connection,
                &AtomicBool::new(false),
                "cancelling",
                Duration::from_millis(100),
                Duration::from_millis(1),
            )
            .unwrap();

        let SettingConfirmation::Noise {
            before,
            noise,
            requested_mode,
            confirmed,
            ..
        } = confirmation
        else {
            panic!("expected an NC/ASM confirmation");
        };
        assert_eq!(before.mode, "ambient");
        assert_eq!(noise.mode, "cancelling");
        assert_eq!(noise.raw_payload, [0x67, 0x17, 1, 1, 0, 0, 8]);
        assert_eq!(requested_mode, "cancelling");
        assert!(confirmed);
        assert_eq!(setting_requests(&connection, 0x68), 1);
    }

    #[test]
    fn inquiry19_notification_states_and_pending_status_are_decoded_strictly() {
        let mut session = ReadSession::new();
        session.noise_inquiry = Some(0x19);
        let cases = [
            ([0x69, 0x19, 1, 0, 0, 0, 1, 0, 0], "off"),
            ([0x69, 0x19, 1, 1, 0, 0, 1, 0, 0], "cancelling"),
            ([0x69, 0x19, 1, 1, 1, 0, 1, 0, 0], "ambient"),
            ([0x69, 0x19, 0, 0, 0, 0, 1, 0, 0], "off"),
        ];
        for (payload, expected_mode) in cases {
            let parsed = session.parse_noise_notification(&payload).unwrap();
            assert_eq!(parsed.mode, expected_mode);
            assert_eq!(parsed.inquiry, 0x19);
        }
        let pending = session
            .parse_noise_notification(&[0x69, 0x19, 0, 0, 0, 0, 1, 0, 0])
            .unwrap();
        assert_eq!(pending.mode, "off");
        assert_eq!(pending.value_change_status, 0);

        for payload in [
            vec![0x69, 0x19, 1, 0, 0, 0, 1, 0],
            vec![0x69, 0x18, 1, 0, 0, 0, 1, 0, 0],
            vec![0x67, 0x19, 1, 0, 0, 0, 1, 0, 0],
            vec![0x69, 0x19, 2, 0, 0, 0, 1, 0, 0],
            vec![0x69, 0x19, 1, 2, 0, 0, 1, 0, 0],
            vec![0x69, 0x19, 1, 1, 2, 0, 1, 0, 0],
            vec![0x69, 0x19, 1, 1, 0, 2, 1, 0, 0],
            vec![0x69, 0x19, 1, 1, 0, 0, 1, 2, 0],
            vec![0x69, 0x19, 1, 1, 0, 0, 1, 0, 3],
        ] {
            assert!(session.parse_noise_notification(&payload).is_none());
        }
        assert!(ReadSession::new()
            .parse_noise_notification(&[0x69, 0x19, 1, 1, 0, 0, 1, 0, 0])
            .is_none());
    }

    #[test]
    fn xm6_inquiry19_set_preserves_ambient_level_and_secondary_controls() {
        let baseline = vec![0x67, 0x19, 1, 1, 0, 0, 9, 1, 2];
        let mut connection = FixtureConnection {
            reads: VecDeque::from([
                noise19_exchange(
                    1,
                    Frame {
                        data_type: 0x0c,
                        sequence: 0,
                        payload: baseline.clone(),
                    },
                ),
                wire_frames(&[Frame {
                    data_type: 1,
                    sequence: 0,
                    payload: vec![],
                }]),
                noise19_exchange(
                    1,
                    Frame {
                        data_type: 0x0c,
                        sequence: 0,
                        payload: vec![0x67, 0x19, 1, 1, 1, 0, 9, 1, 2],
                    },
                ),
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        session.noise_inquiry = Some(0x19);
        let confirmation = session
            .apply_noise_mode(
                &mut connection,
                &AtomicBool::new(false),
                "ambient",
                Duration::from_millis(100),
                Duration::from_millis(1),
            )
            .unwrap();
        let SettingConfirmation::Noise {
            before,
            noise,
            confirmed,
            ..
        } = confirmation
        else {
            panic!("expected NC/ASM confirmation");
        };
        assert!(confirmed);
        assert_eq!(before.mode, "cancelling");
        assert_eq!(before.ambient_level, 9);
        assert!(before.secondary_enabled);
        assert_eq!(before.adaptive_sensitivity, 2);
        assert_eq!(noise.mode, "ambient");
        assert_eq!(noise.inquiry, 0x19);
        assert_eq!(noise.raw_payload, [0x67, 0x19, 1, 1, 1, 0, 9, 1, 2]);

        let mut decoder = Decoder::default();
        let set = connection
            .writes
            .iter()
            .flat_map(|wire| decoder.feed(wire))
            .map(Result::unwrap)
            .find(|frame| frame.data_type == 0x0c && frame.payload.first() == Some(&0x68))
            .unwrap();
        assert_eq!(set.payload, [0x68, 0x19, 1, 1, 1, 0, 9, 1, 2]);
        assert_eq!(before.ambient_level, 9);
        assert!(before.secondary_enabled);
        assert_eq!(before.adaptive_sensitivity, 2);
    }

    #[test]
    fn unconfirmed_noise_mode_never_sends_a_rollback_set() {
        let mut connection = FixtureConnection {
            reads: VecDeque::from([
                noise_exchange(1, 1, 0), // initial ambient baseline
                wire_frames(&[Frame {
                    data_type: 1,
                    sequence: 0,
                    payload: vec![],
                }]), // NC/ASM SET transport ACK
                noise_exchange(1, 1, 0), // readback remains ambient
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        let confirmation = session
            .apply_noise_mode(
                &mut connection,
                &AtomicBool::new(false),
                "cancelling",
                Duration::ZERO,
                Duration::ZERO,
            )
            .unwrap();

        let SettingConfirmation::Noise {
            before,
            noise,
            requested_mode,
            confirmed,
            ..
        } = confirmation
        else {
            panic!("expected an NC/ASM confirmation");
        };
        assert_eq!(before.mode, "ambient");
        assert_eq!(noise.mode, "ambient");
        assert_eq!(requested_mode, "cancelling");
        assert!(!confirmed);
        assert_eq!(setting_requests(&connection, 0x68), 1);
    }

    #[test]
    fn pending_off_readback_is_unconfirmed_but_explicit_request_is_sent() {
        let mut connection = FixtureConnection {
            reads: VecDeque::from([
                noise_off_exchange(1, 0, 0), // UNDER_CHANGING; GET reports off
                wire_frames(&[Frame {
                    data_type: 1,
                    sequence: 0,
                    payload: vec![],
                }]), // explicit OFF SET transport ACK
                noise_off_exchange(1, 0, 0), // all-zero UNDER_CHANGING response repeats
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        let confirmation = session
            .apply_noise_mode(
                &mut connection,
                &AtomicBool::new(false),
                "off",
                Duration::ZERO,
                Duration::from_millis(1),
            )
            .unwrap();

        let SettingConfirmation::Noise {
            noise, confirmed, ..
        } = confirmation
        else {
            panic!("expected an NC/ASM confirmation");
        };
        assert!(!confirmed);
        assert_eq!(noise.raw_payload, [0x67, 0x17, 0, 0, 0, 0, 0]);
        assert_eq!(setting_requests(&connection, 0x68), 1);
    }

    #[test]
    fn coalesced_setting_ack_and_device_update_are_both_processed() {
        let ack = Frame {
            data_type: 1,
            sequence: 1,
            payload: vec![],
        };
        let update = noise_response(0, 0);
        let mut connection = FixtureConnection {
            reads: VecDeque::from([wire_frames(&[ack, update])]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        session
            .write_acknowledged(
                &mut connection,
                &AtomicBool::new(false),
                &[0x68, 0x17, 1, 1, 0, 0, 8],
            )
            .unwrap();

        let mut decoder = Decoder::default();
        let frames: Vec<_> = connection
            .writes
            .iter()
            .flat_map(|wire| decoder.feed(wire))
            .map(Result::unwrap)
            .collect();
        assert!(frames.iter().any(|frame| {
            frame.data_type == 1 && frame.sequence == 1 && frame.payload.is_empty()
        }));
    }

    #[test]
    fn ncasm_notification_arriving_with_set_ack_is_retained_as_raw_observation() {
        let notification = vec![0x69, 0x17, 1, 1, 0, 0, 8];
        let mut connection = FixtureConnection {
            reads: VecDeque::from([
                noise_exchange(1, 1, 0),
                wire_frames(&[
                    Frame {
                        data_type: 1,
                        sequence: 0,
                        payload: vec![],
                    },
                    Frame {
                        data_type: 0x0c,
                        sequence: 1,
                        payload: notification.clone(),
                    },
                ]),
                noise_exchange(1, 0, 0),
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let confirmation = ReadSession::new()
            .apply_noise_mode(
                &mut connection,
                &AtomicBool::new(false),
                "cancelling",
                Duration::from_millis(100),
                Duration::from_millis(1),
            )
            .unwrap();

        let SettingConfirmation::Noise {
            noise,
            confirmed,
            observations,
            ..
        } = confirmation
        else {
            panic!("expected NC/ASM confirmation");
        };
        assert!(confirmed);
        assert_eq!(noise.mode, "cancelling");
        assert_eq!(
            observations,
            [notification, vec![0x67, 0x17, 1, 1, 0, 0, 8]]
        );
    }

    #[test]
    fn ncasm_notification_interleaved_with_get_response_is_retained() {
        let notification = vec![0x69, 0x17, 1, 1, 0, 0, 8];
        let mut connection = FixtureConnection {
            reads: VecDeque::from([
                noise_exchange(1, 1, 0),
                wire_frames(&[Frame {
                    data_type: 1,
                    sequence: 0,
                    payload: vec![],
                }]),
                wire_frames(&[
                    Frame {
                        data_type: 1,
                        sequence: 1,
                        payload: vec![],
                    },
                    Frame {
                        data_type: 0x0c,
                        sequence: 0,
                        payload: notification.clone(),
                    },
                    noise_response(0, 1),
                ]),
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let confirmation = ReadSession::new()
            .apply_noise_mode(
                &mut connection,
                &AtomicBool::new(false),
                "cancelling",
                Duration::from_millis(100),
                Duration::from_millis(1),
            )
            .unwrap();

        let SettingConfirmation::Noise { observations, .. } = confirmation else {
            panic!("expected NC/ASM confirmation");
        };
        assert_eq!(
            observations,
            [notification, vec![0x67, 0x17, 1, 1, 0, 0, 8]]
        );
    }

    #[test]
    fn passive_reader_captures_all_ncasm_commands_and_inquiries_raw() {
        let mut session = ReadSession::new();
        let wire = wire_frames(&[
            Frame {
                data_type: 0x0c,
                sequence: 0,
                payload: vec![0x69, 0x19, 0xaa, 0xbb],
            },
            Frame {
                data_type: 0x0c,
                sequence: 1,
                payload: vec![0x67, 0x17, 0, 0, 0, 0, 0],
            },
        ]);

        let decoded = session.feed_frames(&wire);
        assert_eq!(decoded.len(), 2);
        assert_eq!(
            session.drain_noise_observations(),
            [
                vec![0x69, 0x19, 0xaa, 0xbb],
                vec![0x67, 0x17, 0, 0, 0, 0, 0],
            ]
        );
        assert!(session.drain_noise_observations().is_empty());
    }

    #[test]
    fn raw_noise_inquiry_19_requires_matching_verified_get_response() {
        let response = vec![0x67, 0x19, 1, 1, 0, 0, 1, 0, 0];
        let mut connection = FixtureConnection {
            reads: VecDeque::from([wire_frames(&[
                Frame {
                    data_type: 1,
                    sequence: 1,
                    payload: vec![],
                },
                Frame {
                    data_type: 0x0c,
                    sequence: 0,
                    payload: response.clone(),
                },
            ])]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        assert_eq!(
            session
                .noise_parameter(&mut connection, &AtomicBool::new(false), 0x19)
                .unwrap(),
            response
        );
        let mut decoder = Decoder::default();
        let request = connection
            .writes
            .iter()
            .flat_map(|wire| decoder.feed(wire))
            .map(Result::unwrap)
            .find(|frame| frame.data_type == 0x0c)
            .unwrap();
        assert_eq!(request.payload, [0x66, 0x19]);
    }

    #[test]
    fn both_service_uuids_use_the_commands_message_table() {
        for service in [
            "96cc203e-5068-46ad-b32d-e316f5e069ba",
            "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        ] {
            let reply = encode(
                &Frame {
                    data_type: 0x0c,
                    sequence: 0,
                    payload: vec![1, 0, 0, 0, 0, 1, 0, 1],
                },
                64,
            )
            .unwrap();
            let mut connection = FixtureConnection {
                reads: VecDeque::from([vec![0x3e, 1, 1, 0, 0, 0, 0, 2, 0x3c], reply]),
                writes: vec![],
                service,
            };
            assert!(query_protocol_info(&mut connection, &AtomicBool::new(false)).is_ok());
            assert_eq!(
                connection.writes[0][1], 0x0c,
                "service UUID cannot select the message DataType"
            );
        }
    }

    #[test]
    fn device_strings_require_exact_inquiry_length_and_printable_utf8() {
        assert_eq!(
            parse_device_string(&[5, 2, 5, b'3', b'.', b'1', b'.', b'5'], 2),
            Some("3.1.5".into())
        );
        assert!(parse_device_string(&[5, 1, 1, b'x'], 2).is_none());
        assert!(parse_device_string(&[5, 2, 2, b'x'], 2).is_none());
        assert!(parse_device_string(&[5, 2, 1, 0xff], 2).is_none());
        assert!(parse_device_string(&[5, 2, 1, b'\n'], 2).is_none());
    }

    #[test]
    fn serialized_requests_advance_only_with_inverted_ack_and_matching_reply() {
        let frames = [
            Frame {
                data_type: 1,
                sequence: 1,
                payload: vec![],
            },
            Frame {
                data_type: 0x0c,
                sequence: 0,
                payload: vec![1, 0, 0, 0, 0, 1, 0, 1],
            },
            Frame {
                data_type: 0x0c,
                sequence: 1,
                payload: vec![5, 1, 2, b'X', b'M'],
            },
            Frame {
                data_type: 1,
                sequence: 0,
                payload: vec![],
            },
            Frame {
                data_type: 1,
                sequence: 1,
                payload: vec![],
            },
            Frame {
                data_type: 0x0c,
                sequence: 0,
                payload: vec![5, 2, 5, b'3', b'.', b'1', b'.', b'5'],
            },
            Frame {
                data_type: 1,
                sequence: 0,
                payload: vec![],
            },
            Frame {
                data_type: 0x0c,
                sequence: 1,
                payload: vec![0x23, 0, 82, 0],
            },
            Frame {
                data_type: 1,
                sequence: 1,
                payload: vec![],
            },
            Frame {
                data_type: 0x0c,
                sequence: 0,
                payload: vec![0x07, 0x00, 0],
            },
        ];
        let mut connection = FixtureConnection {
            reads: frames
                .iter()
                .map(|frame| encode(frame, 64).unwrap())
                .collect(),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let (info, _) = read_basics(&mut connection, &AtomicBool::new(false));
        assert!(info.protocol.is_ok());
        assert_eq!(info.model.as_deref(), Some("XM"));
        assert_eq!(info.firmware.as_deref(), Some("3.1.5"));
        assert_eq!(
            info.battery,
            Some(BatteryInfo {
                level: 82,
                charging: "notCharging"
            })
        );
        assert!(info.warnings.is_empty());
        let mut decoder = Decoder::default();
        let requests: Vec<_> = connection
            .writes
            .iter()
            .flat_map(|wire| decoder.feed(wire))
            .map(Result::unwrap)
            .filter(|frame| frame.data_type == 0x0c)
            .collect();
        assert_eq!(
            requests
                .iter()
                .map(|frame| frame.sequence)
                .collect::<Vec<_>>(),
            [0, 1, 0, 1, 0]
        );
        assert_eq!(requests[1].payload, [4, 1]);
        assert_eq!(requests[2].payload, [4, 2]);
        assert_eq!(requests[3].payload, [0x22, 0]);
        assert_eq!(requests[4].payload, [0x06, 0x00]);
    }

    #[test]
    fn battery_status_rejects_invalid_level_charge_and_wrong_command() {
        assert_eq!(
            parse_battery(&[0x23, 0, 100, 3]),
            Some(BatteryInfo {
                level: 100,
                charging: "charged"
            })
        );
        assert_eq!(
            parse_battery(&[0x23, 0, 0, 2]),
            Some(BatteryInfo {
                level: 0,
                charging: "unknown"
            })
        );
        for payload in [
            vec![0x23, 0, 101, 0],
            vec![0x23, 0, 50, 4],
            vec![0x27, 0, 50, 0],
            vec![0x23, 0, 50],
            vec![0x23, 1, 50, 0],
        ] {
            assert!(parse_battery(&payload).is_none());
        }
    }
}
