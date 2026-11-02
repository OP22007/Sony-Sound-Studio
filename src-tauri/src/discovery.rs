//! Read-only protocol-info exchange, independently implemented from static REA evidence.
//! V1/V2 request [00,00], strict response layouts, initial TX0 and inverted empty ACKs
//! are recorded in the private protocol request / ACK evidence exports.

use crate::protocol::{encode, Decoder, Frame};
use crate::transport::Connection;
use crate::device_controls::{
    ConfirmedControl, ControlDescriptor, ControlKind, ControlOption, ControlValue,
    DeviceControlChange, DeviceControlConfirmation, validate_change,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Eq, PartialEq)]
pub struct ProtocolInfo {
    pub format: &'static str,
    pub version: u32,
    pub ack_observed: bool,
}

fn parse_protocol_info(payload: &[u8]) -> Option<ProtocolInfo> {
    if payload.len() < 2 || payload[..2] != [0x01, 0x00] {
        return None;
    }
    let (format, version) = match payload.len() {
        4 => ("v1", u16::from_be_bytes([payload[2], payload[3]]) as u32),
        8 if payload[6] <= 1 && payload[7] <= 1 => (
            "v2",
            u32::from_be_bytes([payload[2], payload[3], payload[4], payload[5]]),
        ),
        _ => return None,
    };
    Some(ProtocolInfo {
        format,
        version,
        ack_observed: false,
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
    DeviceControl(DeviceControlChange),
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
    NoiseSchemaProbe {
        capability: Vec<u8>,
        status: Vec<u8>,
        parameter: Vec<u8>,
        parameter19: Vec<u8>,
    },
}

pub fn read_basics(
    connection: &mut dyn Connection,
    cancelled: &AtomicBool,
) -> (BasicInfo, ReadSession) {
    let mut session = ReadSession::new();
    let protocol = session.protocol_info(connection, cancelled);
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
                if functions.contains(&0xf1) {
                    result.controls.capabilities.push("wear".into());
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
                // Table-1 AUTO_POWER_OFF is function 0x24. It uses inquiry
                // 0x04 and has a fully validated option, GET, SET and readback
                // path in the shipped Sony APK. Do not route the XM6's distinct
                // 0x25 wearing-detection variant through this setter.
                if functions.contains(&0x24) {
                    match session.power_off_setting(connection, cancelled, 0x04) {
                        Ok(info) => {
                            result.controls.capabilities.push("autoPowerOff".into());
                            let options = power_off_options(&info.options);
                            result.controls.device_controls.push(ConfirmedControl {
                                descriptor: ControlDescriptor {
                                    key: "autoPowerOff".into(),
                                    label: "Automatic power off".into(),
                                    description: "Choose the power-off behavior reported by the headphone.".into(),
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
                                && timer_options.iter().any(|option| option.value == ControlValue::Integer(i64::from(info.selected_time)))
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
                // Function 0x25 is AUTO_POWER_OFF_WITH_WEARING_DETECTION,
                // not the standard 0x24 setter route. Record bounded read-only
                // capability/value replies for follow-up; its distinct SET
                // serializer is not yet verified.
                if functions.contains(&0x25) && !functions.contains(&0x24) {
                    match session.power_off_read_only(connection, cancelled, 0x05) {
                        Ok((capability, value, active, selected_time)) => {
                            if !result.controls.capabilities.iter().any(|key| key == "autoPowerOff") {
                                result.controls.capabilities.push("autoPowerOff".into());
                                result.controls.capabilities.push("autoPowerOffTime".into());
                            }
                            result.controls.read_only_values.insert(
                                "autoPowerOff".into(),
                                ControlValue::Integer(i64::from(active)),
                            );
                            result.controls.read_only_values.insert(
                                "autoPowerOffTime".into(),
                                ControlValue::Integer(i64::from(selected_time)),
                            );
                            result.warnings.push(format!(
                                "Read-only wearing-aware power setting: capability [{}], value [{}].",
                                to_hex(&capability), to_hex(&value)
                            ));
                        }
                        Err(error) => result.warnings.push(format!(
                            "Read-only wearing-aware power setting is not decoded: {error}"
                        )),
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
                                        ControlOption { value: ControlValue::Text("off".into()), label: "Off".into() },
                                        ControlOption { value: ControlValue::Text("auto".into()), label: "Auto".into() },
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
                            description: "Choose the room ambience used by Background Music Effect.".into(),
                            kind: ControlKind::Select,
                            options: Some(vec![
                                ControlOption { value: ControlValue::Integer(0), label: "My room".into() },
                                ControlOption { value: ControlValue::Integer(1), label: "Living room".into() },
                                ControlOption { value: ControlValue::Integer(2), label: "Cafe".into() },
                            ]),
                            min: None,
                            max: None,
                            step: None,
                        },
                        value: ControlValue::Integer(i64::from(info.room_size)),
                    });
                }
                if !series_supported && !bgm_ambiguous && (bgm_inquiry.is_some() || cinema_supported) {
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
                        }];
                        if bgm_inquiry.is_some() {
                            options.push(ControlOption {
                                value: ControlValue::Text("backgroundMusic".into()),
                                label: "Background Music".into(),
                            });
                        }
                        if cinema_supported {
                            options.push(ControlOption {
                                value: ControlValue::Text("cinema".into()),
                                label: "Cinema".into(),
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
                                        description: "Prioritize voices in ambient sound mode.".into(),
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
        || options.iter().enumerate().any(|(index, value)| options[..index].contains(value))
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
    values.iter().filter_map(|value| {
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
        })
    }).collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(" ")
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
    if payload.len() != 4 || payload[0] != command || payload[1] != 0x0c
        || payload[2] > 1 || payload[3] > 1
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

pub struct ReadSession {
    sequence: u8,
    synchronized: bool,
    decoder: Decoder,
    noise_observations: Vec<Vec<u8>>,
    noise_inquiry: Option<u8>,
    listening_bgm_inquiry: Option<u8>,
    listening_cinema_supported: bool,
    listening_series_supported: bool,
    listening_bgm_enabled: Option<bool>,
    listening_bgm_room_size: Option<u8>,
    listening_cinema_enabled: Option<bool>,
    speak_to_chat_supported: bool,
    speak_to_chat_aux: Option<u8>,
    power_off_inquiry: Option<u8>,
    power_off_options: Vec<u8>,
    device_control_descriptors: BTreeMap<String, ControlDescriptor>,
    device_control_values: BTreeMap<String, ControlValue>,
    device_updates: Vec<DeviceControlConfirmation>,
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
            listening_bgm_enabled: None,
            listening_bgm_room_size: None,
            listening_cinema_enabled: None,
            speak_to_chat_supported: false,
            speak_to_chat_aux: None,
            power_off_inquiry: None,
            power_off_options: Vec::new(),
            device_control_descriptors: BTreeMap::new(),
            device_control_values: BTreeMap::new(),
            device_updates: Vec::new(),
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
                self.device_control_descriptors.insert(key.clone(), control.descriptor.clone());
                self.device_control_values.insert(key, control.value.clone());
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

    fn power_off_read_only(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
    ) -> Result<(Vec<u8>, Vec<u8>, u8, u8), String> {
        let (capability, options) = self.power_off_capability(connection, cancelled, inquiry)?;
        let value = self.exchange(connection, cancelled, vec![0x26, inquiry], |payload| {
            parse_power_off_value(payload, 0x27, inquiry, &options).is_some()
        })?;
        let (active, selected_time) = parse_power_off_value(&value, 0x27, inquiry, &options)
            .ok_or_else(|| "Wearing-aware power parameter fields are invalid.".to_string())?;
        Ok((capability, value, active, selected_time))
    }

    fn power_off_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        inquiry: u8,
    ) -> Result<PowerOffInfo, String> {
        if inquiry != 0x04 {
            return Err("Editable automatic power off is only validated for inquiry 0x04.".into());
        }
        let (_capability, options) = self.power_off_capability(connection, cancelled, inquiry)?;
        let payload = self.exchange(connection, cancelled, vec![0x26, inquiry], |payload| {
            parse_power_off_value(payload, 0x27, inquiry, &options).is_some()
        })?;
        let (active, selected_time) = parse_power_off_value(&payload, 0x27, inquiry, &options)
            .ok_or_else(|| "Automatic power-off parameter fields are invalid.".to_string())?;
        self.power_off_inquiry = Some(inquiry);
        self.power_off_options = options.clone();
        Ok(PowerOffInfo { active, selected_time, options })
    }

    fn power_off_value(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<(u8, u8), String> {
        let inquiry = self.power_off_inquiry
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
            SettingChange::Equalizer(bands) => {
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
        let descriptor = self.device_control_descriptors.get(&change.key)
            .ok_or_else(|| format!("The device did not confirm the {} control.", change.key))?;
        validate_change(descriptor, change.value.clone())?;
        if change.key == "dsee" {
            return self.apply_upscaling_setting(connection, cancelled, change.value);
        }
        if change.key == "listeningMode" {
            return self.apply_listening_mode(connection, cancelled, change.value);
        }
        if change.key == "speakToChat" {
            return self.apply_speak_to_chat(connection, cancelled, change.value);
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
                requested.ambient_mode = u8::from(*value);
                requested.focus_on_voice = *value;
            }
            _ => return Err("The device setting value does not match its confirmed control type.".into()),
        }
        let payload = [
            0x68, 0x19, 1, u8::from(requested.mode != "off"), requested.nc_asm_mode,
            requested.ambient_mode, requested.ambient_level, u8::from(requested.secondary_enabled),
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
                let before_value = self.device_control_values.get(&change.key).cloned().unwrap_or_else(|| change.value.clone());
                self.device_control_values.insert(change.key.clone(), change.value.clone());
                return Ok(SettingConfirmation::DeviceControl(DeviceControlConfirmation {
                    key: change.key,
                    before: before_value,
                    confirmed: change.value,
                }));
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
        if self.power_off_inquiry != Some(0x04) {
            return Err("This writable power-off control requires the validated inquiry 0x04 path.".into());
        }
        let ControlValue::Integer(value) = change.value else {
            return Err("Automatic power-off options must be integer values from the device capability response.".into());
        };
        let selected = u8::try_from(value)
            .map_err(|_| "Automatic power-off option is outside the protocol range.")?;
        if !self.power_off_options.contains(&selected) {
            return Err("The requested power-off option was not advertised by the headphone.".into());
        }
        let before = self.power_off_value(connection, cancelled)?;
        let mut requested = before;
        match change.key.as_str() {
            "autoPowerOff" => requested.0 = selected,
            "autoPowerOffTime" => requested.1 = selected,
            _ => return Err("Unknown automatic power-off setting.".into()),
        }
        if requested == before {
            return Ok(SettingConfirmation::DeviceControl(DeviceControlConfirmation {
                key: change.key,
                before: ControlValue::Integer(i64::from(selected)),
                confirmed: ControlValue::Integer(i64::from(selected)),
            }));
        }
        if let Err(error) = self.write_power_off_and_confirm(connection, cancelled, requested) {
            if self.write_power_off_and_confirm(connection, cancelled, before).is_ok() {
                return Err(format!("{error} The previous automatic power-off settings were restored and verified."));
            }
            return Err(format!("{error} The previous automatic power-off settings could not be verified after restoration; reconnect and inspect the headphone before retrying."));
        }
        let previous = if change.key == "autoPowerOff" { before.0 } else { before.1 };
        self.device_control_values.insert(change.key.clone(), ControlValue::Integer(i64::from(selected)));
        Ok(SettingConfirmation::DeviceControl(DeviceControlConfirmation {
            key: change.key,
            before: ControlValue::Integer(i64::from(previous)),
            confirmed: ControlValue::Integer(i64::from(selected)),
        }))
    }

    fn write_power_off_and_confirm(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        requested: (u8, u8),
    ) -> Result<(), String> {
        let inquiry = self.power_off_inquiry
            .ok_or_else(|| "Automatic power-off inquiry is no longer available.".to_string())?;
        if inquiry != 0x04 || requested.0 > 17 || requested.1 > 17 {
            return Err("Automatic power-off SET values do not match the verified schema.".into());
        }
        self.write_acknowledged(connection, cancelled, &[0x28, inquiry, requested.0, requested.1])?;
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            match self.power_off_value(connection, cancelled) {
                Ok(value) if value == requested => return Ok(()),
                Ok(_) => {}
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Err("The headphone did not confirm the requested automatic power-off settings.".into());
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
            return Err("Speak-to-Chat Type 2 was not advertised by the connected headphone.".into());
        }
        let ControlValue::Boolean(enabled) = value else {
            return Err("Speak-to-Chat requires a confirmed on/off value.".into());
        };
        let (before_enabled, aux) = self.speak_to_chat_setting(connection, cancelled)?;
        if before_enabled == enabled {
            return Ok(SettingConfirmation::DeviceControl(DeviceControlConfirmation {
                key: "speakToChat".into(),
                before: ControlValue::Boolean(before_enabled),
                confirmed: ControlValue::Boolean(enabled),
            }));
        }
        if let Err(error) = self.write_speak_to_chat_and_confirm(connection, cancelled, enabled, aux) {
            if self.write_speak_to_chat_and_confirm(connection, cancelled, before_enabled, aux).is_ok() {
                return Err(format!("{error} The previous Speak-to-Chat setting was restored and verified."));
            }
            return Err(format!("{error} The previous Speak-to-Chat setting could not be verified after restoration; reconnect and inspect the headphone before retrying."));
        }
        let before = ControlValue::Boolean(before_enabled);
        let confirmed = ControlValue::Boolean(enabled);
        self.device_control_values.insert("speakToChat".into(), confirmed.clone());
        Ok(SettingConfirmation::DeviceControl(DeviceControlConfirmation {
            key: "speakToChat".into(), before, confirmed,
        }))
    }

    fn write_speak_to_chat_and_confirm(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        enabled: bool,
        aux: u8,
    ) -> Result<(), String> {
        self.write_acknowledged(connection, cancelled, &[0xf8, 0x0c, if enabled { 0 } else { 1 }, aux])?;
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
                return Err("The headphone did not confirm its requested Speak-to-Chat setting.".into());
            }
            if cancelled.load(Ordering::Acquire) {
                return Err("The Speak-to-Chat readback was cancelled.".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
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
        let inquiry = self.listening_bgm_inquiry
            .ok_or_else(|| "BGM mode was not advertised by the connected headphone.".to_string())?;
        if room_size > 2 {
            return Err("The headphone returned an unknown BGM room size.".into());
        }
        self.write_acknowledged(connection, cancelled, &[
            0xe8, inquiry, if enabled { 0 } else { 1 }, room_size,
        ])?;
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
        let inquiry = self.listening_bgm_inquiry
            .ok_or_else(|| "Background Music was not advertised by the connected headphone.".to_string())?;
        let before = self.bgm_setting(connection, cancelled, inquiry)?;
        if before.room_size == requested {
            return Ok(SettingConfirmation::DeviceControl(DeviceControlConfirmation {
                key: "bgmRoomSize".into(),
                before: ControlValue::Integer(i64::from(before.room_size)),
                confirmed: ControlValue::Integer(i64::from(before.room_size)),
            }));
        }
        match self.set_bgm_enabled(connection, cancelled, before.enabled, requested) {
            Ok(confirmed) => Ok(SettingConfirmation::DeviceControl(DeviceControlConfirmation {
                key: "bgmRoomSize".into(),
                before: ControlValue::Integer(i64::from(before.room_size)),
                confirmed: ControlValue::Integer(i64::from(confirmed.room_size)),
            })),
            Err(error) => {
                let restored = self.set_bgm_enabled(connection, cancelled, before.enabled, before.room_size)
                    .is_ok_and(|current| current.enabled == before.enabled && current.room_size == before.room_size);
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
        self.write_acknowledged(connection, cancelled, &[0xe8, 0x04, if enabled { 0 } else { 1 }])?;
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
                bgm = Some(self.set_bgm_enabled(connection, cancelled, false, current.room_size)?);
            }
            if cinema != Some(true) {
                self.set_cinema_enabled(connection, cancelled, true)?;
                cinema = Some(true);
            }
        } else if target_bgm && target_cinema {
            // Restore a previously observed dual-active pair only during rollback.
            if let Some(current) = bgm.filter(|current| current.enabled) {
                bgm = Some(self.set_bgm_enabled(connection, cancelled, false, current.room_size)?);
            }
            if cinema != Some(true) {
                self.set_cinema_enabled(connection, cancelled, true)?;
                cinema = Some(true);
            }
            let current = bgm.expect("BGM support checked above");
            bgm = Some(self.set_bgm_enabled(connection, cancelled, true, current.room_size)?);
        } else {
            if let Some(current) = bgm.filter(|current| current.enabled) {
                bgm = Some(self.set_bgm_enabled(connection, cancelled, false, current.room_size)?);
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
            return Err("The headphone readback did not match the requested listening-mode pair.".into());
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
            return Err("Listening mode cannot be changed until the headphone's series-mode state is fully read.".into());
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
                self.device_control_values.insert("listeningMode".into(), confirmed.clone());
                Ok(SettingConfirmation::DeviceControl(DeviceControlConfirmation {
                    key: "listeningMode".into(), before, confirmed,
                }))
            }
            Err(error) => {
                let restore_bgm = before_bgm.is_some_and(|state| state.enabled);
                let restore_cinema = before_cinema == Some(true);
                let restored = self.transition_listening_pair(
                    connection, cancelled, restore_bgm, restore_cinema,
                ).is_ok_and(|(bgm, cinema)| {
                    bgm.map(|state| state.enabled) == before_bgm.map(|state| state.enabled)
                        && bgm.map(|state| state.room_size) == before_bgm.map(|state| state.room_size)
                        && cinema == before_cinema
                });
                if restored {
                    Err(format!("{error} The previous listening-mode state was restored and verified."))
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
                self.device_control_values.insert(key.clone(), confirmed.clone());
                return Ok(SettingConfirmation::DeviceControl(DeviceControlConfirmation {
                    key, before, confirmed,
                }));
            }
            if Instant::now() >= deadline {
                return Err("The headphone did not confirm its DSEE mode after the setting request.".into());
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
        if cancelled.load(Ordering::Acquire) || !self.synchronized {
            return Err("The device session is cancelled or unsynchronized; reconnect before changing settings.".into());
        }
        let request = Frame {
            data_type: 0x0c,
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
            data_type: 0x0c,
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
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut reply = None;
        let mut buffer = [0u8; 4096];
        while Instant::now() < deadline {
            if cancelled.load(Ordering::Acquire) {
                return Err("Read-only discovery was cancelled.".into());
            }
            match connection.read(&mut buffer, Duration::from_millis(200)) {
                Ok(0) => {
                    return Err(
                        "The device closed its control connection during read-only discovery."
                            .into(),
                    )
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
                        if frame.data_type == 0x0c && matches(&frame.payload) {
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
        if !self.device_control_descriptors.contains_key("listeningMode") {
            return;
        }
        let Some(value) = (if self.listening_bgm_enabled == Some(true) {
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
        let before = self.device_control_values.insert("listeningMode".into(), value.clone());
        if let Some(before) = before.filter(|before| *before != value) {
            self.device_updates.push(DeviceControlConfirmation {
                key: "listeningMode".into(), before, confirmed: value,
            });
        }
    }

    fn record_noise_observation(&mut self, frame: &Frame) {
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
            if let Some((enabled, aux)) = parse_speak_to_chat_value(&frame.payload, 0xf9) {
                self.speak_to_chat_aux = Some(aux);
                if self.device_control_descriptors.contains_key("speakToChat") {
                    let value = ControlValue::Boolean(enabled);
                    let before = self.device_control_values.insert("speakToChat".into(), value.clone());
                    if let Some(before) = before.filter(|before| *before != value) {
                        self.device_updates.push(DeviceControlConfirmation {
                            key: "speakToChat".into(), before, confirmed: value,
                        });
                    }
                }
            }
            if let Some(value) = parse_upscaling_value(&frame.payload, 0xe9) {
                if self.device_control_descriptors.contains_key("dsee") {
                    let value = ControlValue::Text(value.into());
                    let before = self.device_control_values.insert("dsee".into(), value.clone());
                    if let Some(before) = before.filter(|before| *before != value) {
                        self.device_updates.push(DeviceControlConfirmation {
                            key: "dsee".into(), before, confirmed: value,
                        });
                    }
                }
            }
            if let Some(inquiry) = self.power_off_inquiry {
                if let Some((active, selected_time)) = parse_power_off_value(
                    &frame.payload,
                    0x29,
                    inquiry,
                    &self.power_off_options,
                ) {
                    for (key, value) in [
                        ("autoPowerOff", active),
                        ("autoPowerOffTime", selected_time),
                    ] {
                        if self.device_control_descriptors.contains_key(key) {
                            let value = ControlValue::Integer(i64::from(value));
                            let before = self.device_control_values.insert(key.into(), value.clone());
                            if let Some(before) = before.filter(|before| *before != value) {
                                self.device_updates.push(DeviceControlConfirmation {
                                    key: key.into(), before, confirmed: value,
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
                        let before = self.device_control_values.insert("bgmRoomSize".into(), value.clone());
                        if let Some(before) = before.filter(|before| *before != value) {
                            self.device_updates.push(DeviceControlConfirmation {
                                key: "bgmRoomSize".into(), before, confirmed: value,
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
                        ("ambient", ControlValue::Integer(i64::from(noise.ambient_level))),
                    ];
                    for (key, value) in observed {
                        if self.device_control_descriptors.contains_key(key) {
                            let before = self.device_control_values.insert(key.into(), value.clone());
                            if let Some(before) = before.filter(|before| *before != value) {
                                self.device_updates.push(DeviceControlConfirmation {
                                    key: key.into(), before, confirmed: value,
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
            Frame { data_type: 1, sequence: ack_sequence, payload: vec![] },
            Frame { data_type: 0x0c, sequence: 0, payload: vec![0xe7, 0x01, mode] },
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
                    ControlOption { value: ControlValue::Text("off".into()), label: "Off".into() },
                    ControlOption { value: ControlValue::Text("auto".into()), label: "Auto".into() },
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
                wire_frames(&[Frame { data_type: 1, sequence: 0, payload: vec![] }]),
                audio_exchange(1, 1),
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        session.install_device_controls(&[dsee_control("off")]);
        let confirmation = session.apply_setting(
            &mut connection,
            &AtomicBool::new(false),
            SettingChange::DeviceControl(DeviceControlChange {
                key: "dsee".into(),
                value: ControlValue::Text("auto".into()),
            }),
        ).unwrap();
        let SettingConfirmation::DeviceControl(confirmation) = confirmation else {
            panic!("expected a typed DSEE confirmation");
        };
        assert_eq!(confirmation.before, ControlValue::Text("off".into()));
        assert_eq!(confirmation.confirmed, ControlValue::Text("auto".into()));
        let mut decoder = Decoder::default();
        let commands: Vec<_> = connection.writes.iter()
            .flat_map(|wire| decoder.feed(wire))
            .map(Result::unwrap)
            .filter(|frame| frame.data_type == 0x0c)
            .map(|frame| frame.payload)
            .collect();
        assert_eq!(commands, [vec![0xe6, 1], vec![0xe8, 1, 1], vec![0xe6, 1]]);

        let mut no_readback = FixtureConnection {
            reads: VecDeque::from([
                audio_exchange(1, 0),
                wire_frames(&[Frame { data_type: 1, sequence: 0, payload: vec![] }]),
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        session.install_device_controls(&[dsee_control("off")]);
        assert!(session.apply_setting(
            &mut no_readback,
            &AtomicBool::new(false),
            SettingChange::DeviceControl(DeviceControlChange {
                key: "dsee".into(),
                value: ControlValue::Text("auto".into()),
            }),
        ).is_err());
        assert_eq!(setting_requests(&no_readback, 0xe8), 1);
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
        assert_eq!(session.drain_device_updates(), [DeviceControlConfirmation {
            key: "dsee".into(),
            before: ControlValue::Text("off".into()),
            confirmed: ControlValue::Text("auto".into()),
        }]);
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
        assert_eq!(session.drain_device_updates(), [DeviceControlConfirmation {
            key: "focusOnVoice".into(),
            before: ControlValue::Boolean(false),
            confirmed: ControlValue::Boolean(true),
        }]);
    }

    #[test]
    fn focus_mode_read_modify_write_requires_final_baseline_and_preserves_other_fields() {
        let mut changing = FixtureConnection {
            reads: VecDeque::from([noise19_exchange(1, Frame {
                data_type: 0x0c, sequence: 0,
                payload: vec![0x67, 0x19, 0, 1, 0, 0, 9, 1, 2],
            })]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        session.noise_inquiry = Some(0x19);
        session.install_device_controls(&[focus_control(false)]);
        assert!(session.apply_setting(
            &mut changing,
            &AtomicBool::new(false),
            SettingChange::DeviceControl(DeviceControlChange {
                key: "focusOnVoice".into(), value: ControlValue::Boolean(true),
            }),
        ).is_err());
        assert_eq!(setting_requests(&changing, 0x68), 0);

        let mut connection = FixtureConnection {
            reads: VecDeque::from([
                noise19_exchange(1, Frame {
                    data_type: 0x0c, sequence: 0,
                    payload: vec![0x67, 0x19, 1, 1, 0, 0, 9, 1, 2],
                }),
                wire_frames(&[Frame { data_type: 1, sequence: 0, payload: vec![] }]),
                noise19_exchange(1, Frame {
                    data_type: 0x0c, sequence: 0,
                    payload: vec![0x67, 0x19, 1, 1, 0, 1, 9, 1, 2],
                }),
            ]),
            writes: vec![],
            service: "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
        };
        let mut session = ReadSession::new();
        session.noise_inquiry = Some(0x19);
        session.install_device_controls(&[focus_control(false)]);
        let confirmation = session.apply_setting(
            &mut connection,
            &AtomicBool::new(false),
            SettingChange::DeviceControl(DeviceControlChange {
                key: "focusOnVoice".into(), value: ControlValue::Boolean(true),
            }),
        ).unwrap();
        let SettingConfirmation::DeviceControl(confirmation) = confirmation else {
            panic!("expected focus-on-voice confirmation");
        };
        assert_eq!(confirmation.before, ControlValue::Boolean(false));
        assert_eq!(confirmation.confirmed, ControlValue::Boolean(true));
        let mut decoder = Decoder::default();
        let request = connection.writes.iter().flat_map(|wire| decoder.feed(wire))
            .map(Result::unwrap)
            .find(|frame| frame.data_type == 0x0c && frame.payload.first() == Some(&0x68))
            .unwrap();
        assert_eq!(request.payload, [0x68, 0x19, 1, 1, 0, 1, 9, 1, 2]);
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
