//! Independently implemented HeyMelody OPO v1 adapter. See docs/research/oneplus-heymelody.md.
//! Model names select a discovery path; only a matching product ID and live replies unlock writes.
use crate::device_controls::{
    ConfirmedControl, ControlDescriptor, ControlKind, ControlOption, ControlValue,
    DeviceControlConfirmation,
};
use crate::discovery::{
    BasicInfo, ControlInfo, NoiseInfo, ProtocolInfo, SettingChange, SettingConfirmation,
};
use crate::oneplus_models::{by_product_bytes, EqPreset, Model, NoiseProfile};
use crate::transport::Connection;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(test)]
pub const MODEL: &str = "OnePlus Nord Buds 3 Pro";
pub const SERVICE_UUID: &str = "00001107-d102-11e1-9b23-00025b00a5a5";
pub const CURRENT_SERVICE_UUID: &str = "0000079a-d102-11e1-9b23-00025b00a5a5";
const MAX_PACKET: usize = 8192;
const TIMEOUT: Duration = Duration::from_secs(3);
const BASS_WAVE_GET: u16 = 0x0124; // command 292; reply is status plus signed min/max/current
const BASS_WAVE_STATUS: u16 = 0x8508; // normalized request-manager dispatch key 1288
const BASS_WAVE_SET: u16 = 0x041b; // command 1051

#[derive(Clone, Debug, PartialEq)]
struct Packet {
    command: u16,
    sequence: u8,
    data: Vec<u8>,
}

fn encode(packet: &Packet) -> Result<Vec<u8>, String> {
    if packet.data.len() > MAX_PACKET - 5 {
        return Err("OnePlus packet exceeds the configured limit.".into());
    }
    let mut inner = Vec::with_capacity(packet.data.len() + 5);
    inner.extend_from_slice(&packet.command.to_le_bytes());
    inner.push(packet.sequence);
    inner.extend_from_slice(&(packet.data.len() as u16).to_le_bytes());
    inner.extend_from_slice(&packet.data);
    // Stay within the original wrapper's initial 20-byte frame size without an MTU write.
    let mut wire = Vec::new();
    if inner.len() <= 16 {
        wire.extend_from_slice(&[0xaa, (inner.len() + 2) as u8, 0, 0]);
        wire.extend_from_slice(&inner);
    } else {
        let count = inner.len().div_ceil(15);
        if count > 256 {
            return Err("OnePlus outgoing packet needs too many fragments.".into());
        }
        for (index, chunk) in inner.chunks(15).enumerate() {
            let flag = if index == 0 {
                1
            } else if index + 1 == count {
                3
            } else {
                2
            };
            wire.extend_from_slice(&[0xaa, (chunk.len() + 3) as u8, flag, 0, index as u8]);
            wire.extend_from_slice(chunk);
        }
    }
    Ok(wire)
}

#[derive(Default)]
struct Decoder {
    buffer: Vec<u8>,
    fragments: Vec<u8>,
    next_fragment: Option<u16>,
    recovering: bool,
}

impl Decoder {
    fn feed(&mut self, bytes: &[u8]) -> Vec<Packet> {
        let mut packets = Vec::new();
        // Bound both retained state and work for arbitrary stream input.
        for chunk in bytes.chunks(MAX_PACKET) {
            if self.buffer.len() + chunk.len() > MAX_PACKET * 2 {
                self.buffer.clear();
                self.reset_fragments();
                self.recovering = true;
            }
            self.buffer.extend_from_slice(chunk);
            loop {
                let Some(start) = self.buffer.iter().position(|byte| *byte == 0xaa) else {
                    self.buffer.clear();
                    break;
                };
                if start > 0 {
                    self.buffer.drain(..start);
                }
                if self.buffer.len() < 2 {
                    break;
                }
                let mut length = 0usize;
                let mut header = 1usize;
                let mut complete = false;
                let mut invalid = false;
                for index in 0..2 {
                    let Some(&byte) = self.buffer.get(1 + index) else {
                        break;
                    };
                    length |= usize::from(byte & 0x7f) << (7 * index);
                    header += 1;
                    if byte & 0x80 == 0 {
                        complete = true;
                        break;
                    }
                    if index == 1 {
                        invalid = true;
                    }
                }
                if invalid || (complete && !(2..=MAX_PACKET).contains(&length)) {
                    self.buffer.drain(..1);
                    self.reset_fragments();
                    self.recovering = true;
                    continue;
                }
                if !complete || self.buffer.len() < header + length {
                    // After rejected framing only, skip an incomplete false start
                    // when a later sync byte exists. Ordinary partial payloads retain AA bytes.
                    if self.recovering && self.buffer[1..].contains(&0xaa) {
                        self.buffer.drain(..1);
                        continue;
                    }
                    break;
                }
                let body = self.buffer[header..header + length].to_vec();
                self.buffer.drain(..header + length);
                if body[0] > 3 || body[1] != 0 {
                    self.reset_fragments();
                    self.recovering = true;
                    continue;
                }
                self.recovering = false;
                let inner = match body[0] {
                    0 => {
                        self.reset_fragments();
                        Some(body[2..].to_vec())
                    }
                    1 if body.len() > 3 && body[2] == 0 => {
                        self.fragments = body[3..].to_vec();
                        self.next_fragment = Some(1);
                        None
                    }
                    2 | 3 if body.len() > 3 && self.next_fragment == Some(u16::from(body[2])) => {
                        if self.fragments.len() + body.len() - 3 > MAX_PACKET {
                            self.reset_fragments();
                            continue;
                        }
                        self.fragments.extend_from_slice(&body[3..]);
                        self.next_fragment = Some(u16::from(body[2]) + 1);
                        if body[0] == 3 {
                            self.next_fragment = None;
                            Some(std::mem::take(&mut self.fragments))
                        } else {
                            None
                        }
                    }
                    _ => {
                        self.reset_fragments();
                        None
                    }
                };
                if let Some(inner) = inner {
                    if inner.len() < 5 {
                        continue;
                    }
                    let len = usize::from(u16::from_le_bytes([inner[3], inner[4]]));
                    if len != inner.len() - 5 {
                        continue;
                    }
                    packets.push(Packet {
                        command: u16::from_le_bytes([inner[0], inner[1]]),
                        sequence: inner[2],
                        data: inner[5..].to_vec(),
                    });
                }
            }
        }
        packets
    }

    fn reset_fragments(&mut self) {
        self.fragments.clear();
        self.next_fragment = None;
    }
}

fn successful(data: &[u8]) -> Result<&[u8], String> {
    match data.split_first() {
        Some((0, rest)) => Ok(rest),
        Some((status, _)) => Err(format!("OnePlus rejected the request (status {status}).")),
        None => Err("OnePlus returned an empty status reply.".into()),
    }
}

fn hex(data: &[u8]) -> String {
    if data.is_empty() {
        return "(empty)".into();
    }
    data.iter().take(32).map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(" ")
}

fn spatial_support_summary(support: &[u8], path: Option<SpatialPath>) -> String {
    let path = match path {
        Some(SpatialPath::Typed) => "typed (GET 0x012a / SET 0x0422)",
        Some(SpatialPath::Legacy) => "legacy on/off (feature 27, GET 0x010d / SET 0x0403)",
        None => "none",
    };
    format!(
        "path {path}; capability 47 (typed) = {}, capability 7 (legacy) = {}; support bytes {}",
        capability(support, 47),
        capability(support, 7),
        hex(support)
    )
}

fn capability(mask: &[u8], bit: usize) -> bool {
    mask.get(bit / 8)
        .is_some_and(|byte| byte & (1 << (bit % 8)) != 0)
}

/// BassWave's numeric value is encoded as three signed bytes in protocol order:
/// minimum, maximum, current. This payload has no status prefix; a negative
/// minimum is valid and must never be interpreted as an error status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BassWaveInfo {
    min: i8,
    max: i8,
    current: i8,
}

fn parse_bass_wave(data: &[u8]) -> Result<BassWaveInfo, String> {
    let [min, max, current] = data else {
        return Err("OnePlus BassWave reply must contain exactly three signed bytes.".into());
    };
    let info = BassWaveInfo {
        min: *min as i8,
        max: *max as i8,
        current: *current as i8,
    };
    if info.min > info.max || info.current < info.min || info.current > info.max {
        return Err("OnePlus BassWave reply contains an invalid range or current value.".into());
    }
    Ok(info)
}

fn encode_bass_wave(info: BassWaveInfo) -> Result<[u8; 3], String> {
    if info.min > info.max || info.current < info.min || info.current > info.max {
        return Err("OnePlus BassWave value is outside its live device range.".into());
    }
    Ok([info.min as u8, info.max as u8, info.current as u8])
}

fn bass_wave_set_supported(model: &Model, support: &[u8]) -> bool {
    model.bass_wave_supported && capability(support, 38)
}

/// Parse GET_FEATURE_SWITCH_STATUS (0x010d): status, count, then
/// `(feature id, boolean status)` pairs. Reject partial, duplicate, or
/// non-boolean records instead of publishing a guessed setting state.
fn feature_switches(data: &[u8]) -> Option<BTreeMap<u8, bool>> {
    let body = successful(data).ok()?;
    let count = usize::from(*body.first()?);
    if count == 0 || body.len() != 1 + count * 2 {
        return None;
    }
    let mut switches = BTreeMap::new();
    for pair in body[1..].chunks_exact(2) {
        let enabled = match pair[1] {
            0 => false,
            1 => true,
            _ => return None,
        };
        if switches.insert(pair[0], enabled).is_some() {
            return None;
        }
    }
    Some(switches)
}

fn toggle_control(key: &str, label: &str, description: &str, value: bool) -> ConfirmedControl {
    ConfirmedControl {
        descriptor: ControlDescriptor {
            key: key.into(),
            label: label.into(),
            description: description.into(),
            kind: ControlKind::Toggle,
            options: None,
            min: None,
            max: None,
            step: None,
        },
        value: ControlValue::Boolean(value),
    }
}

fn bass_wave_control(info: BassWaveInfo) -> ConfirmedControl {
    ConfirmedControl {
        descriptor: ControlDescriptor {
            key: "bassWave".into(),
            label: "BassWave".into(),
            description: "Adjust the BassWave intensity within the range reported by the earbuds."
                .into(),
            kind: ControlKind::Range,
            options: None,
            min: Some(i64::from(info.min)),
            max: Some(i64::from(info.max)),
            step: Some(1),
        },
        value: ControlValue::Integer(i64::from(info.current)),
    }
}

fn profile(model: &'static Model, index: u8) -> Option<&'static NoiseProfile> {
    model.noise_profiles.iter().find(|item| item.index == index)
}

fn mode_name(model: &'static Model, index: u8) -> Option<&'static str> {
    profile(model, index).map(|item| item.mode.as_str())
}

fn normalized_mode(mode: &str) -> Option<&'static str> {
    match mode {
        "off" => Some("off"),
        "cancelling" => Some("cancelling"),
        "ambient" => Some("ambient"),
        _ => None,
    }
}

fn ancestor_or_self(model: &'static Model, ancestor: u8, mut descendant: u8) -> bool {
    // Bound traversal even if a future catalog edit introduces a parent cycle.
    for _ in 0..model.noise_profiles.len() {
        if ancestor == descendant {
            return true;
        }
        let Some(parent) = profile(model, descendant).and_then(|item| item.parent_index) else {
            return false;
        };
        descendant = parent;
    }
    false
}

fn bounded_hex(data: &[u8]) -> String {
    let hex = data
        .iter()
        .take(32)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    if data.len() > 32 {
        format!("{hex} ... ({} bytes)", data.len())
    } else if data.is_empty() {
        "(empty)".into()
    } else {
        hex
    }
}

fn parse_current(data: &[u8], supported: &BTreeSet<u8>, model: &'static Model) -> Option<u8> {
    // CurrentNoiseModeInfo uses the same little-endian bitmap layout for types 1 and 5.
    if !(3..=6).contains(&data.len()) || data[0] != 1 || ![1, 5].contains(&data[1]) {
        return None;
    }
    let mut active = Vec::new();
    for (byte_index, byte) in data[2..].iter().enumerate() {
        for bit in 0..8 {
            if byte & (1 << bit) != 0 {
                let index = (byte_index * 8 + bit) as u8;
                profile(model, index)?;
                active.push(index);
            }
        }
    }
    // A parent category and its active detail can be reported together. Two unrelated
    // categories or sibling strengths are still conflicting, never a confirmed setting.
    let mut selected = *active.iter().find(|leaf| {
        active
            .iter()
            .all(|index| ancestor_or_self(model, *index, **leaf))
    })?;
    for _ in 0..model.noise_profiles.len() {
        if supported.contains(&selected) {
            return Some(selected);
        }
        // A detail readback can identify its advertised parent without unlocking detail writes.
        selected = profile(model, selected)?.parent_index?;
    }
    None
}

fn noise_info(model: &'static Model, index: u8, data: Vec<u8>) -> NoiseInfo {
    NoiseInfo {
        inquiry: 1,
        value_change_status: 1,
        nc_asm_mode: index,
        ambient_mode: 0,
        mode: mode_name(model, index).expect("validated noise index"),
        ambient_level: 0,
        focus_on_voice: false,
        secondary_enabled: false,
        adaptive_sensitivity: 0,
        raw_payload: data,
    }
}

// HeyMelody has two earbud-rendered spatial paths. The modern enum uses
// capability 47 and dedicated GET/SET; older firmware uses feature switch 27.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SpatialPath {
    Typed,
    Legacy,
}
fn spatial_path(model: &Model, support: &[u8]) -> Option<SpatialPath> {
    if model.spatial_types.is_empty() {
        None
    } else if capability(support, 47) {
        Some(SpatialPath::Typed)
    } else if capability(support, 7) {
        Some(SpatialPath::Legacy)
    } else {
        None
    }
}
fn spatial_control(model: &Model, path: SpatialPath, value: u8) -> Option<ConfirmedControl> {
    let options: Vec<_> = model
        .spatial_types
        .iter()
        .copied()
        .filter(|v| *v <= 2 && (path == SpatialPath::Typed || *v <= 1))
        .map(|v| ControlOption {
            value: ControlValue::Integer(i64::from(v)),
            label: match v {
                0 => "Off",
                1 => "Fixed spatial audio",
                _ => "Head-tracked spatial audio",
            }
            .into(),
            enabled: None,
        })
        .collect();
    if !options
        .iter()
        .any(|o| o.value == ControlValue::Integer(i64::from(value)))
    {
        return None;
    }
    Some(ConfirmedControl { descriptor: ControlDescriptor { key: "spatialAudio".into(), label: "Spatial audio".into(),
        description: "Choose the earbuds' reported spatial audio mode. Availability depends on the model and firmware.".into(),
        kind: ControlKind::Select, options: Some(options), min: None, max: None, step: None },
        value: ControlValue::Integer(i64::from(value)) })
}
fn spatial_readback(data: &[u8], path: SpatialPath) -> Option<u8> {
    match path {
        SpatialPath::Typed => match data {
            [0, value @ 0..=2] => Some(*value),
            _ => None,
        },
        SpatialPath::Legacy => feature_switches(data)?.get(&27).map(|v| u8::from(*v)),
    }
}

fn game_sound_readback(data: &[u8], model: &Model) -> Option<(u8, Vec<u8>)> {
    let body = successful(data).ok()?;
    // GET_GAME_SOUND_PROFILE_STATUS is status, current type, count, then the
    // live supported type ids. Keep a small explicit limit as well as an exact
    // count check so malformed device lengths never influence UI state.
    if model.game_sound_types.is_empty() || body.len() < 3 || body.len() > 34 {
        return None;
    }
    let current = body[0];
    let count = usize::from(body[1]);
    if count == 0 || count > 32 || body.len() != count + 2 {
        return None;
    }
    let mut live = BTreeSet::new();
    for value in &body[2..] {
        if !matches!(*value, 0 | 3) || !live.insert(*value) {
            return None;
        }
    }
    let options: Vec<_> = model
        .game_sound_types
        .iter()
        .copied()
        .filter(|value| live.contains(value))
        .collect();
    if options.is_empty() || !options.contains(&current) {
        return None;
    }
    Some((current, options))
}

fn game_sound_control(model: &Model, current: u8, options: &[u8]) -> Option<ConfirmedControl> {
    if !options.contains(&current)
        || options
            .iter()
            .any(|value| !model.game_sound_types.contains(value))
    {
        return None;
    }
    let mut controls = Vec::with_capacity(options.len());
    for &value in options {
        let label = match value {
            0 => "Off",
            3 => "Sound effects for shooting games",
            _ => return None,
        };
        controls.push(ControlOption {
            value: ControlValue::Integer(i64::from(value)),
            label: label.into(),
            enabled: None,
        });
    }
    Some(select_control_with_options(
        "gameSoundProfile",
        "Game sound profile",
        "Choose a GameSound profile advertised by this model and firmware.",
        current,
        controls,
    ))
}

fn selected_game_feature(model: &'static Model, _support: &[u8]) -> Option<u8> {
    model.game_feature
}

fn switch_query(model: &'static Model, support: &[u8]) -> Vec<u8> {
    let mut features = Vec::new();
    if model.wear_detection {
        features.push(4);
    }
    if let Some(feature) = selected_game_feature(model, support) {
        features.push(feature);
    }
    if model.bass_wave_supported {
        features.push(29);
    }
    let mut request = Vec::with_capacity(features.len() + 1);
    request.push(features.len() as u8);
    request.extend(features);
    request
}

fn eq_preset(model: &'static Model, index: u8) -> Option<&'static EqPreset> {
    model.eq_presets.iter().find(|item| item.index == index)
}

fn eq_template(model: &'static Model) -> Option<crate::oneplus_eq::EqProfile> {
    if !model.custom_eq_supported {
        return None;
    }
    let frequencies = model
        .custom_eq_frequencies
        .clone()
        .unwrap_or_else(|| vec![62, 250, 1000, 4000, 8000, 16000]);
    if frequencies.is_empty()
        || frequencies.len() > 16
        || frequencies.contains(&0)
        || frequencies.iter().copied().collect::<BTreeSet<_>>().len() != frequencies.len()
    {
        return None;
    }
    Some(crate::oneplus_eq::EqProfile {
        selected: 1,
        min_value: -6,
        max_value: 6,
        eq_id: 0,
        name: String::new(),
        db_values: vec![0; frequencies.len()],
        frequencies,
    })
}

fn select_control(
    key: &str,
    label: &str,
    description: &str,
    current: u8,
    options: Vec<(u8, String)>,
) -> ConfirmedControl {
    select_control_with_options(
        key,
        label,
        description,
        current,
        options
            .into_iter()
            .map(|(value, label)| ControlOption {
                value: ControlValue::Integer(i64::from(value)),
                label,
                enabled: None,
            })
            .collect(),
    )
}

fn select_control_with_options(
    key: &str,
    label: &str,
    description: &str,
    current: u8,
    options: Vec<ControlOption>,
) -> ConfirmedControl {
    ConfirmedControl {
        descriptor: ControlDescriptor {
            key: key.into(),
            label: label.into(),
            description: description.into(),
            kind: ControlKind::Select,
            options: Some(options),
            min: None,
            max: None,
            step: None,
        },
        value: ControlValue::Integer(i64::from(current)),
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Battery {
    device_type: u8,
    level: u8,
    charging: bool,
}

fn batteries(data: &[u8]) -> Option<Vec<Battery>> {
    let count = usize::from(*data.first()?);
    if !(1..=3).contains(&count) || data.len() != 1 + count * 2 {
        return None;
    }
    let mut entries = Vec::new();
    for pair in data[1..].chunks_exact(2) {
        if !(1..=3).contains(&pair[0])
            || pair[1] & 0x7f > 100
            || entries
                .iter()
                .any(|entry: &Battery| entry.device_type == pair[0])
        {
            return None;
        }
        entries.push(Battery {
            device_type: pair[0],
            level: pair[1] & 0x7f,
            charging: pair[1] & 0x80 != 0,
        });
    }
    Some(entries)
}

fn firmware(data: &[u8]) -> Option<String> {
    let count = usize::from(*data.first()?);
    if !(1..=16).contains(&count) || data.len() > 2048 {
        return None;
    }
    let text = std::str::from_utf8(&data[1..]).ok()?.trim_end_matches('\0');
    let fields: Vec<_> = text.split(',').collect();
    if fields.len() != count * 3 {
        return None;
    }
    let mut versions = BTreeSet::new();
    for record in fields.chunks_exact(3) {
        if matches!(record[0], "1" | "2") && matches!(record[1], "2" | "3") {
            let value = record[2].trim();
            if value.is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
                return None;
            }
            versions.insert(value);
        }
    }
    if versions.is_empty() {
        None
    } else {
        Some(versions.into_iter().collect::<Vec<_>>().join(" / "))
    }
}

pub struct Session {
    decoder: Decoder,
    inbox: VecDeque<Packet>,
    sequence: u8,
    expected: Option<(u16, u8)>,
    synchronized: bool,
    support: Vec<u8>,
    model: Option<&'static Model>,
    bass_wave: Option<BassWaveInfo>,
    bass_wave_revision: u64,
    noise_supported: BTreeSet<u8>,
    noise_observations: Vec<Vec<u8>>,
    updates: Vec<DeviceControlConfirmation>,
    controls: BTreeMap<String, ControlDescriptor>,
    values: BTreeMap<String, ControlValue>,
    eq_profiles: Vec<crate::oneplus_eq::EqProfile>,
    metadata: BTreeMap<String, serde_json::Value>,
    metadata_updates: BTreeMap<String, serde_json::Value>,
    /// Human-readable protocol notes for the diagnostics log. Bounded, drained by the connection loop.
    notes: Vec<String>,
}

impl Session {
    fn new() -> Self {
        Self {
            decoder: Decoder::default(),
            inbox: VecDeque::new(),
            sequence: 0,
            expected: None,
            synchronized: true,
            support: vec![],
            model: None,
            bass_wave: None,
            bass_wave_revision: 0,
            noise_supported: BTreeSet::new(),
            noise_observations: vec![],
            updates: vec![],
            controls: BTreeMap::new(),
            values: BTreeMap::new(),
            eq_profiles: Vec::new(),
            metadata: BTreeMap::new(),
            metadata_updates: BTreeMap::new(),
            notes: Vec::new(),
        }
    }

    fn note(&mut self, text: String) {
        if self.notes.len() == 32 {
            self.notes.remove(0);
        }
        self.notes.push(text);
    }

    pub fn drain_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    pub fn is_synchronized(&self) -> bool {
        self.synchronized
    }
    pub fn metadata(&self) -> BTreeMap<String, serde_json::Value> {
        self.metadata.clone()
    }
    pub fn drain_metadata_updates(&mut self) -> BTreeMap<String, serde_json::Value> {
        std::mem::take(&mut self.metadata_updates)
    }
    pub fn drain_noise_observations(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.noise_observations)
    }
    pub fn drain_device_updates(&mut self) -> Vec<DeviceControlConfirmation> {
        std::mem::take(&mut self.updates)
    }
    pub fn parse_noise_notification(&self, data: &[u8]) -> Option<NoiseInfo> {
        let model = self.model?;
        Some(noise_info(
            model,
            parse_current(data, &self.noise_supported, model)?,
            data.to_vec(),
        ))
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        for packet in self.decoder.feed(bytes) {
            let matched = self.expected == Some((packet.command & 0x7fff, packet.sequence));
            if packet.command == 0x810c && matched && packet.data.first() == Some(&0) {
                if packet.data.get(1) == Some(&1) {
                    self.observe_noise(&packet.data[1..]);
                }
            }
            // CMD_NOTIFICATION_EVENT contains [event, event-specific fields], without a status byte.
            let event = if packet.command == 0x0204 {
                Some(packet.data.as_slice())
            } else if packet.command == 0x8201 && matched && packet.data.first() == Some(&0) {
                Some(&packet.data[1..])
            } else {
                None
            };
            if let Some(event) = event {
                if event.first() == Some(&3) {
                    let data = &event[1..];
                    self.observe_noise(data);
                } else if event.first() == Some(&1) {
                    if let Some(entries) = batteries(&event[1..]) {
                        self.record_batteries(&entries);
                    }
                }
            }
            if packet.command == BASS_WAVE_STATUS
                && self
                    .model
                    .is_some_and(|model| bass_wave_set_supported(model, &self.support))
            {
                self.observe_bass_wave(&packet.data);
            }
            if self.inbox.len() == 64 {
                self.inbox.pop_front();
            }
            self.inbox.push_back(packet);
        }
    }

    fn observe_noise(&mut self, data: &[u8]) {
        if self.noise_observations.len() == 32 {
            self.noise_observations.remove(0);
        }
        // Keep malformed readbacks visible too; diagnostics must not hide the bytes
        // that caused a failed confirmation. Unparsed values never update controls.
        self.noise_observations
            .push(data.iter().take(32).copied().collect());
        if let Some(index) = self
            .model
            .and_then(|model| parse_current(data, &self.noise_supported, model))
        {
            self.record_profile(index);
        }
    }

    fn observe_bass_wave(&mut self, data: &[u8]) {
        if let Ok(info) = parse_bass_wave(data) {
            self.record_bass_wave(info);
        }
    }

    fn record_bass_wave(&mut self, info: BassWaveInfo) {
        self.bass_wave_revision = self.bass_wave_revision.wrapping_add(1);
        self.bass_wave = Some(info);
        if self.controls.contains_key("bassWave") {
            let confirmed = ControlValue::Integer(i64::from(info.current));
            let Some(before) = self.values.insert("bassWave".into(), confirmed.clone()) else {
                return;
            };
            if before != confirmed {
                if self.updates.len() == 32 {
                    self.updates.remove(0);
                }
                self.updates.push(DeviceControlConfirmation {
                    key: "bassWave".into(),
                    before,
                    confirmed,
                });
            }
            if let Some(descriptor) = self.controls.get_mut("bassWave") {
                descriptor.min = Some(i64::from(info.min));
                descriptor.max = Some(i64::from(info.max));
            }
        }
    }

    fn record_batteries(&mut self, entries: &[Battery]) {
        let mut update = BTreeMap::new();
        update.insert("oneplusBatteries".into(), serde_json::json!(entries));
        if let Some(level) = entries
            .iter()
            .filter(|entry| entry.device_type <= 2)
            .map(|entry| entry.level)
            .min()
        {
            update.insert("oneplusBatteryLevel".into(), serde_json::json!(level));
            let charging = entries
                .iter()
                .filter(|entry| entry.device_type <= 2)
                .all(|entry| entry.charging);
            update.insert(
                "batteryCharging".into(),
                serde_json::json!(if charging { "charging" } else { "notCharging" }),
            );
        }
        self.metadata.extend(update.clone());
        self.metadata_updates.extend(update);
    }

    fn record_profile(&mut self, index: u8) {
        if self.controls.contains_key("noiseProfile") {
            let confirmed = ControlValue::Integer(i64::from(index));
            let Some(before) = self.values.insert("noiseProfile".into(), confirmed.clone()) else {
                return;
            };
            if self.updates.len() == 32 {
                self.updates.remove(0);
            }
            self.updates.push(DeviceControlConfirmation {
                key: "noiseProfile".into(),
                before,
                confirmed,
            });
        }
    }

    fn exchange(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        command: u16,
        data: &[u8],
    ) -> Result<Vec<u8>, String> {
        if !self.synchronized {
            return Err("OnePlus protocol session needs reconnecting.".into());
        }
        if cancelled.load(Ordering::Acquire) {
            return Err("Connection was cancelled.".into());
        }
        let sequence = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        self.expected = Some((command, sequence));
        // Queued old replies can never satisfy a new request after sequence wrap.
        self.inbox.clear();
        let wire = encode(&Packet {
            command,
            sequence,
            data: data.to_vec(),
        })?;
        if connection.write_all(&wire, Duration::from_secs(1)).is_err() {
            self.synchronized = false;
            return Err("OnePlus command write failed; reconnect before retrying.".into());
        }
        let deadline = Instant::now() + TIMEOUT;
        let mut buffer = [0u8; 4096];
        while Instant::now() < deadline && !cancelled.load(Ordering::Acquire) {
            if let Some(index) = self.inbox.iter().position(|packet| {
                packet.command == (command | 0x8000) && packet.sequence == sequence
            }) {
                let packet = self.inbox.remove(index).expect("matched packet");
                self.expected = None;
                successful(&packet.data)?;
                return Ok(packet.data);
            }
            match connection.read(
                &mut buffer,
                Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())),
            ) {
                Ok(0) => {
                    self.synchronized = false;
                    return Err("OnePlus control connection closed.".into());
                }
                Ok(count) => self.feed(&buffer[..count]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => {
                    self.synchronized = false;
                    return Err("OnePlus control connection failed.".into());
                }
            }
        }
        self.synchronized = false;
        Err(format!("OnePlus command 0x{command:04x} was not confirmed before the deadline. Reconnect to discard late replies."))
    }

    fn current_noise(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<NoiseInfo, String> {
        let (data, noise) = self.query_noise(connection, cancelled)?;
        noise.ok_or_else(|| self.noise_readback_error(&data))
    }

    /// Command 292 returns `[status, min, max, current]`; the independent 0x8508
    /// status route is also retained as an unsolicited update source. The reply
    /// status is removed before the signed triple is decoded.
    fn query_bass_wave(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        after_revision: u64,
    ) -> Result<BassWaveInfo, String> {
        self.inbox
            .retain(|packet| packet.command != BASS_WAVE_STATUS);
        let reply = self.exchange(connection, cancelled, BASS_WAVE_GET, &[])?;
        let body = successful(&reply).map_err(|error| {
            format!(
                "{error} BassWave GET reply bytes: [{}].",
                bounded_hex(&reply)
            )
        })?;
        let info = parse_bass_wave(body).map_err(|error| {
            format!(
                "{error} BassWave GET reply bytes: [{}].",
                bounded_hex(&reply)
            )
        })?;
        self.record_bass_wave(info);
        if self.bass_wave_revision <= after_revision {
            return Err("OnePlus BassWave GET did not produce a fresh status observation.".into());
        }
        Ok(info)
    }

    fn noise_readback_error(&self, data: &[u8]) -> String {
        format!("OnePlus noise profile is not yet confirmed. Readback: [{}]; advertised profile indices: {:?}. No restoration command was sent.", bounded_hex(data), self.noise_supported)
    }

    fn query_noise(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
    ) -> Result<(Vec<u8>, Option<NoiseInfo>), String> {
        let model = self
            .model
            .ok_or("OnePlus product model was not selected.")?;
        let reply = self.exchange(connection, cancelled, 0x010c, &[1, 1])?;
        let data = successful(&reply)?;
        let noise = parse_current(data, &self.noise_supported, model)
            .map(|index| noise_info(model, index, data.to_vec()));
        Ok((data.to_vec(), noise))
    }

    fn set_noise(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        index: u8,
    ) -> Result<NoiseInfo, String> {
        let model = self
            .model
            .ok_or("OnePlus product model was not selected.")?;
        if !capability(&self.support, 8)
            || !self.noise_supported.contains(&index)
            || profile(model, index).is_none()
        {
            return Err("This OnePlus noise profile was not advertised.".into());
        }
        let bytes = usize::from(index) / 8 + 1;
        let mut data = vec![1, 1];
        data.resize(2 + bytes, 0);
        data[2 + usize::from(index) / 8] = 1 << (index % 8);
        self.exchange(connection, cancelled, 0x0404, &data)?;
        // SET status is acceptance only. Match a subsequent GET; never roll back a timeout.
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let (data, result) = self.query_noise(connection, cancelled)?;
            if let Some(result) = result {
                // Selecting a category may preserve the device's active child strength.
                // The UI must receive that actual advertised readback, not the requested index.
                if ancestor_or_self(model, index, result.nc_asm_mode) {
                    return Ok(result);
                }
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "Requested OnePlus noise profile {index}. {}",
                    self.noise_readback_error(&data)
                ));
            }
            if cancelled.load(Ordering::Acquire) {
                return Err("Connection was cancelled.".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn apply_setting(
        &mut self,
        connection: &mut dyn Connection,
        cancelled: &AtomicBool,
        change: SettingChange,
    ) -> Result<SettingConfirmation, String> {
        match change {
            SettingChange::NoiseMode(requested_mode) => {
                let model = self
                    .model
                    .ok_or("OnePlus product model was not selected.")?;
                let before = self.current_noise(connection, cancelled)?;
                if !["off", "ambient", "cancelling"].contains(&requested_mode.as_str()) {
                    return Err("Invalid OnePlus noise mode.".into());
                }
                let index = if before.mode == requested_mode {
                    before.nc_asm_mode
                } else {
                    *self
                        .noise_supported
                        .iter()
                        .find(|index| {
                            mode_name(model, **index).and_then(normalized_mode)
                                == Some(requested_mode.as_str())
                        })
                        .ok_or("Requested OnePlus noise mode is unavailable.")?
                };
                let noise = self.set_noise(connection, cancelled, index)?;
                Ok(SettingConfirmation::Noise {
                    before,
                    noise,
                    requested_mode,
                    confirmed: true,
                    observations: vec![],
                })
            }
            SettingChange::DeviceControl(change) => {
                let descriptor = self
                    .controls
                    .get(&change.key)
                    .ok_or("OnePlus setting was not negotiated.")?;
                let change = crate::device_controls::validate_change(descriptor, change.value)?;
                let mut before = self
                    .values
                    .get(&change.key)
                    .cloned()
                    .ok_or("OnePlus setting has no confirmed current value.")?;
                let confirmed = match change.key.as_str() {
                    "spatialAudio" => {
                        let model = self
                            .model
                            .ok_or("OnePlus product model was not selected.")?;
                        let path = spatial_path(model, &self.support).ok_or(
                            "Spatial audio was not advertised by this model and firmware.",
                        )?;
                        let ControlValue::Integer(value) = change.value else {
                            return Err("Spatial audio requires a numeric device mode.".into());
                        };
                        let value =
                            u8::try_from(value).map_err(|_| "Invalid spatial audio mode.")?;
                        if spatial_control(model, path, value).is_none() {
                            return Err("Spatial mode is not supported by this device path.".into());
                        }
                        self.note(format!(
                            "Spatial audio change to {value} requested: {}.",
                            spatial_support_summary(&self.support, Some(path))
                        ));
                        let (set_command, set_body): (u16, Vec<u8>) = match path {
                            SpatialPath::Typed => (0x0422, vec![value]),
                            SpatialPath::Legacy => (0x0403, vec![27, value]),
                        };
                        let set_reply = self.exchange(connection, cancelled, set_command, &set_body);
                        match &set_reply {
                            Ok(reply) => self.note(format!(
                                "Spatial audio SET 0x{set_command:04x} [{}] was acknowledged; reply bytes {}.",
                                hex(&set_body),
                                hex(reply)
                            )),
                            Err(error) => self.note(format!(
                                "Spatial audio SET 0x{set_command:04x} [{}] failed: {error}",
                                hex(&set_body)
                            )),
                        }
                        set_reply?;
                        let mut confirmed = false;
                        for attempt in 0..3 {
                            let (get_command, get_body): (u16, &[u8]) = match path {
                                SpatialPath::Typed => (0x012a, &[]),
                                SpatialPath::Legacy => (0x010d, &[1, 27]),
                            };
                            let reply = match self.exchange(connection, cancelled, get_command, get_body) {
                                Ok(reply) => reply,
                                Err(error) => {
                                    // A failed or timed-out readback must leave a trace, not just an error.
                                    self.note(format!(
                                        "Spatial audio readback {} of 3 failed after the SET was acknowledged: {error}",
                                        attempt + 1
                                    ));
                                    return Err(error);
                                }
                            };
                            let parsed = spatial_readback(&reply, path);
                            self.note(format!(
                                "Spatial audio readback {} of 3: bytes {} read as {}; wanted {value}.",
                                attempt + 1,
                                hex(&reply),
                                parsed.map_or("an unrecognised value".to_string(), |found| found.to_string())
                            ));
                            if parsed == Some(value) {
                                confirmed = true;
                                break;
                            }
                            if attempt < 2 {
                                if cancelled.load(Ordering::Acquire) {
                                    return Err("Spatial audio readback was cancelled.".into());
                                }
                                std::thread::sleep(Duration::from_millis(180));
                            }
                        }
                        if !confirmed {
                            return Err("OnePlus accepted the spatial audio request, but matching device readback was not received. No restoration command was sent.".into());
                        }
                        ControlValue::Integer(i64::from(value))
                    }
                    "noiseProfile" => {
                        let ControlValue::Integer(value) = change.value else {
                            return Err("OnePlus setting requires a device option.".into());
                        };
                        let value = u8::try_from(value).map_err(|_| "Invalid OnePlus option.")?;
                        let noise = self.set_noise(connection, cancelled, value)?;
                        ControlValue::Integer(i64::from(noise.nc_asm_mode))
                    }
                    "gameSoundProfile" => {
                        let model = self
                            .model
                            .ok_or("OnePlus product model was not selected.")?;
                        if model.game_sound_types.is_empty() || !capability(&self.support, 49) {
                            return Err(
                                "OnePlus GameSound profile control was not advertised.".into()
                            );
                        }
                        let ControlValue::Integer(value) = change.value else {
                            return Err("OnePlus GameSound requires a numeric profile.".into());
                        };
                        let value = u8::try_from(value)
                            .map_err(|_| "Invalid OnePlus GameSound profile.")?;
                        if game_sound_control(model, value, &[value]).is_none() {
                            return Err(
                                "Requested OnePlus GameSound profile is unavailable.".into()
                            );
                        }
                        let advertised = descriptor.options.as_ref().is_some_and(|options| {
                            options.iter().any(|option| {
                                option.enabled != Some(false)
                                    && option.value == ControlValue::Integer(i64::from(value))
                            })
                        });
                        if !advertised {
                            return Err(
                                "Requested OnePlus GameSound profile is unavailable.".into()
                            );
                        }
                        self.exchange(connection, cancelled, 0x0423, &[value, 1])?;
                        let mut matched = None;
                        for attempt in 0..3 {
                            let readback = self.exchange(connection, cancelled, 0x012b, &[])?;
                            let (current, options) = game_sound_readback(&readback, model)
                                .ok_or("OnePlus accepted GameSound SET, but returned a malformed status readback. No restoration command was sent.")?;
                            if current == value && options.contains(&value) {
                                matched = Some(current);
                                break;
                            }
                            if attempt < 2 {
                                if cancelled.load(Ordering::Acquire) {
                                    return Err("OnePlus GameSound readback was cancelled. No restoration command was sent.".into());
                                }
                                std::thread::sleep(Duration::from_millis(180));
                            }
                        }
                        let current = matched.ok_or("OnePlus accepted GameSound SET, but fresh profile readback did not match. No restoration command was sent.")?;
                        ControlValue::Integer(i64::from(current))
                    }
                    "eqPreset" => {
                        let model = self
                            .model
                            .ok_or("OnePlus product model was not selected.")?;
                        let ControlValue::Integer(value) = change.value else {
                            return Err("OnePlus setting requires a device option.".into());
                        };
                        let value = u8::try_from(value).map_err(|_| "Invalid OnePlus option.")?;
                        if eq_preset(model, value).is_none()
                            && !model
                                .conditional_eq_presets
                                .iter()
                                .any(|preset| preset.index == value)
                        {
                            return Err("OnePlus EQ preset is not listed for this model.".into());
                        }
                        if !capability(&self.support, 10) {
                            return Err("OnePlus EQ mode was not advertised.".into());
                        }
                        self.exchange(connection, cancelled, 0x0406, &[value])?;
                        let mut matched = false;
                        for attempt in 0..3 {
                            let reply = self.exchange(connection, cancelled, 0x010f, &[])?;
                            if reply.as_slice() == [0, value] {
                                matched = true;
                                break;
                            }
                            if attempt < 2 {
                                if cancelled.load(Ordering::Acquire) {
                                    return Err("OnePlus EQ preset readback was cancelled.".into());
                                }
                                std::thread::sleep(Duration::from_millis(200));
                            }
                        }
                        if !matched {
                            return Err("OnePlus equalizer preset readback did not match. No restoration command was sent.".into());
                        }
                        ControlValue::Integer(i64::from(value))
                    }
                    "bassWave" => {
                        let model = self
                            .model
                            .ok_or("OnePlus product model was not selected.")?;
                        if !bass_wave_set_supported(model, &self.support) {
                            return Err(
                                "OnePlus BassWave SET was not advertised for this model.".into()
                            );
                        }
                        let ControlValue::Integer(requested) = change.value else {
                            return Err(
                                "OnePlus BassWave setting requires an integer value.".into()
                            );
                        };
                        let cached = self
                            .bass_wave
                            .ok_or("OnePlus BassWave has no fresh confirmed device range.")?;
                        let prior_revision = self.bass_wave_revision;
                        let live = self.query_bass_wave(connection, cancelled, prior_revision)?;
                        if live.min != cached.min || live.max != cached.max {
                            return Err("OnePlus BassWave range changed since discovery; reconnect to refresh the control before writing.".into());
                        }
                        before = ControlValue::Integer(i64::from(live.current));
                        let requested = i8::try_from(requested)
                            .map_err(|_| "BassWave value is outside the signed-byte range.")?;
                        let baseline = self.bass_wave_revision;
                        let payload = encode_bass_wave(BassWaveInfo {
                            current: requested,
                            ..live
                        })?;
                        self.exchange(connection, cancelled, BASS_WAVE_SET, &payload)?;

                        let mut readback = None;
                        let mut readback_after = baseline;
                        for attempt in 0..3 {
                            match self.query_bass_wave(connection, cancelled, readback_after) {
                                Ok(info) if info.min == live.min
                                    && info.max == live.max
                                    && info.current == requested => {
                                        readback = Some(info);
                                        break;
                                    }
                                Ok(_) => readback_after = self.bass_wave_revision,
                                Err(error) if !self.synchronized || attempt == 2 => return Err(format!(
                                    "OnePlus accepted BassWave SET, but matching status readback was not received. {error} No restoration command was sent."
                                )),
                                Err(_) => {}
                            }
                            if attempt < 2 {
                                if cancelled.load(Ordering::Acquire) {
                                    return Err("OnePlus BassWave readback was cancelled.".into());
                                }
                                std::thread::sleep(Duration::from_millis(200));
                            }
                        }
                        let info = readback.ok_or("OnePlus accepted BassWave SET, but fresh range/value readback did not match. No restoration command was sent.")?;
                        self.record_bass_wave(info);
                        ControlValue::Integer(i64::from(info.current))
                    }
                    "wearDetection" | "gameMode" | "bassWaveEnabled" => {
                        let model = self
                            .model
                            .ok_or("OnePlus product model was not selected.")?;
                        if !capability(&self.support, 7) {
                            return Err("OnePlus feature-switch command was not advertised.".into());
                        }
                        let ControlValue::Boolean(enabled) = change.value else {
                            return Err("OnePlus setting requires a boolean value.".into());
                        };
                        let feature = match change.key.as_str() {
                            "wearDetection" if model.wear_detection => 4,
                            "gameMode" => selected_game_feature(model, &self.support)
                                .ok_or("Game mode is not listed for this OnePlus model.")?,
                            "bassWaveEnabled" if model.bass_wave_supported => 29,
                            _ => {
                                return Err(
                                    "Wear detection is not listed for this OnePlus model.".into()
                                )
                            }
                        };
                        self.exchange(
                            connection,
                            cancelled,
                            0x0403,
                            &[feature, u8::from(enabled)],
                        )?;
                        let reply = self.exchange(
                            connection,
                            cancelled,
                            0x010d,
                            &switch_query(model, &self.support),
                        )?;
                        let switches = feature_switches(&reply)
                            .ok_or("OnePlus returned an invalid feature-switch readback.")?;
                        match switches.get(&feature) {
                            Some(value) if *value == enabled => ControlValue::Boolean(*value),
                            Some(_) => return Err("OnePlus accepted the request, but feature-switch readback did not match. No restoration command was sent.".into()),
                            None => return Err("OnePlus accepted the request, but feature-switch readback omitted the requested setting. No restoration command was sent.".into()),
                        }
                    }
                    _ => return Err("This OnePlus setting has no implemented command.".into()),
                };
                self.values.insert(change.key.clone(), confirmed.clone());
                Ok(SettingConfirmation::DeviceControl(
                    DeviceControlConfirmation {
                        key: change.key,
                        before,
                        confirmed,
                    },
                ))
            }
            SettingChange::OnePlusEqProfile(change) => {
                change.validate()?;
                if !capability(&self.support, 34) {
                    return Err("OnePlus custom EQ profile support was not advertised.".into());
                }
                let requested = &change.profile;
                let model = self
                    .model
                    .ok_or("OnePlus product model was not selected.")?;
                if !model.custom_eq_supported {
                    return Err("Custom EQ is not configured for this OnePlus model.".into());
                }
                let before_reply = self.exchange(connection, cancelled, 0x0122, &[])?;
                let before_profiles = crate::oneplus_eq::parse_profiles(&before_reply)?;
                match change.action {
                    1 => {
                        let template = eq_template(model)
                            .ok_or("This OnePlus model has no confirmed custom EQ template.")?;
                        if requested.frequencies != template.frequencies
                            || requested.min_value != template.min_value
                            || requested.max_value != template.max_value
                        {
                            return Err("New OnePlus EQ profile does not match the model's confirmed band template; no write was sent.".into());
                        }
                        if before_profiles.len() >= 32 {
                            return Err("OnePlus EQ profile list is at the native parsing limit; no create command was sent.".into());
                        }
                    }
                    2 => {
                        if !self
                            .eq_profiles
                            .iter()
                            .any(|profile| profile.eq_id == requested.eq_id)
                        {
                            return Err(
                                "OnePlus custom EQ profile ID was not returned by the device."
                                    .into(),
                            );
                        }
                        let live_profile = before_profiles
                            .iter()
                            .find(|profile| profile.eq_id == requested.eq_id)
                            .ok_or("The OnePlus EQ profile is no longer present on the device.")?;
                        if live_profile.min_value != requested.min_value
                            || live_profile.max_value != requested.max_value
                            || live_profile.frequencies != requested.frequencies
                        {
                            return Err("The OnePlus EQ profile's band layout or range changed since discovery; no write was sent.".into());
                        }
                    }
                    _ => return Err("Unsupported OnePlus EQ profile action.".into()),
                }
                let previous_ids = before_profiles
                    .iter()
                    .map(|profile| profile.eq_id)
                    .collect::<BTreeSet<_>>();
                self.eq_profiles = before_profiles;
                let payload = crate::oneplus_eq::encode_set_profile(requested, change.action)?;
                let accepted = self.exchange(connection, cancelled, 0x0418, &payload)?;
                let response_body = successful(&accepted)?;
                if response_body.is_empty() {
                    return Err("OnePlus custom EQ SET reply is missing its response value.".into());
                }
                let mut confirmed_profiles = None;
                for attempt in 0..3 {
                    let reply = self.exchange(connection, cancelled, 0x0122, &[])?;
                    let profiles = crate::oneplus_eq::parse_profiles(&reply)?;
                    let matched = match change.action {
                        1 => {
                            !previous_ids
                                .iter()
                                .any(|id| !profiles.iter().any(|profile| profile.eq_id == *id))
                                && profiles
                                    .iter()
                                    .filter(|profile| {
                                        !previous_ids.contains(&profile.eq_id)
                                            && profile.selected == requested.selected
                                            && profile.min_value == requested.min_value
                                            && profile.max_value == requested.max_value
                                            && profile.name == requested.name
                                            && profile.frequencies == requested.frequencies
                                            && profile.db_values == requested.db_values
                                    })
                                    .count()
                                    == 1
                        }
                        2 => profiles.iter().any(|profile| profile == requested),
                        _ => unreachable!("action validated before the request"),
                    };
                    if matched {
                        confirmed_profiles = Some(profiles);
                        break;
                    }
                    if attempt < 2 {
                        if cancelled.load(Ordering::Acquire) {
                            return Err("OnePlus custom EQ profile readback was cancelled.".into());
                        }
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
                let profiles = confirmed_profiles.ok_or_else(|| {
                    let action = if change.action == 1 { "create" } else { "update" };
                    format!("OnePlus accepted the EQ profile {action} request, but bounded readback did not match the requested profile. No restoration command was sent.")
                })?;
                self.eq_profiles = profiles.clone();
                let serialized = serde_json::json!(profiles);
                self.metadata
                    .insert("oneplusEqualizerProfiles".into(), serialized.clone());
                self.metadata_updates
                    .insert("oneplusEqualizerProfiles".into(), serialized);
                Ok(SettingConfirmation::OnePlusEqProfiles(profiles))
            }
            _ => Err(
                "Sony equalizer and diagnostic commands cannot be sent to OnePlus earbuds.".into(),
            ),
        }
    }
}

pub fn read_basics(
    connection: &mut dyn Connection,
    cancelled: &AtomicBool,
) -> Result<(BasicInfo, Session), String> {
    let mut session = Session::new();
    let product = session.exchange(connection, cancelled, 0x0103, &[])?;
    if product.len() != 4 {
        return Err("The OnePlus product-ID reply has an invalid length.".into());
    }
    let model = by_product_bytes(&product[1..]).ok_or(
        "The OnePlus product ID is not in the bundled model registry; controls remain unavailable.",
    )?;
    session.model = Some(model);
    let reply = session.exchange(connection, cancelled, 0x0100, &[])?;
    let support = successful(&reply)?;
    if support.is_empty() || support.len() > 64 {
        return Err("Invalid OnePlus capability bitmap.".into());
    }
    session.support = support.to_vec();
    session.metadata.insert(
        "oneplusProductId".into(),
        serde_json::json!(model.product_id),
    );
    session.metadata.insert(
        "hardwareValidation".into(),
        serde_json::json!("Protocol derived from HeyMelody 117.6.6; device testing pending"),
    );
    let mut result = BasicInfo { protocol: Ok(ProtocolInfo { format: "heymelody-opo-v1", version: 1, ack_observed: false }), model: Some(model.name.clone()), firmware: None, battery: None,
        controls: ControlInfo { support_reply_confirmed: true, ..Default::default() }, warnings: vec![format!("{} product ID and capability reply confirmed. Each setting still requires matching readback.", model.name)] };
    if capability(&session.support, 0) {
        match session.exchange(connection, cancelled, 0x0105, &[]) {
            Ok(reply) => result.firmware = firmware(successful(&reply)?),
            Err(error) => result.warnings.push(error),
        }
    }
    if capability(&session.support, 1) {
        match session.exchange(connection, cancelled, 0x0106, &[]) {
            Ok(reply) => {
                if let Some(entries) = batteries(successful(&reply)?) {
                    session.record_batteries(&entries);
                    if let Some(level) = entries
                        .iter()
                        .filter(|entry| entry.device_type <= 2)
                        .map(|entry| entry.level)
                        .min()
                    {
                        result.battery = Some(crate::discovery::BatteryInfo {
                            level,
                            charging: if entries
                                .iter()
                                .filter(|entry| entry.device_type <= 2)
                                .all(|entry| entry.charging)
                            {
                                "charging"
                            } else {
                                "notCharging"
                            },
                        });
                    }
                }
            }
            Err(error) => result.warnings.push(error),
        }
    }
    if capability(&session.support, 8) && !model.noise_profiles.is_empty() {
        match session.exchange(connection, cancelled, 0x010c, &[2, 1]) {
            Ok(reply) => {
                let body = successful(&reply).expect("exchange accepted a successful status");
                if (3..=6).contains(&body.len()) && body[..2] == [2, 1] {
                    for (byte_index, byte) in body[2..].iter().enumerate() {
                        for bit in 0..8 {
                            if byte & (1 << bit) != 0 {
                                let index = (byte_index * 8 + bit) as u8;
                                if profile(model, index).is_some() {
                                    session.noise_supported.insert(index);
                                }
                            }
                        }
                    }
                    result.warnings.push(format!(
                        "OnePlus NC support response: [{}]; recognized profile indices: {:?}.",
                        bounded_hex(body),
                        session.noise_supported
                    ));
                    match session.current_noise(connection, cancelled) {
                        Ok(noise) => {
                            let current = noise.nc_asm_mode;
                            if normalized_mode(mode_name(model, current).unwrap_or("")).is_some()
                                && ["off", "cancelling", "ambient"].iter().all(|mode| {
                                    session.noise_supported.iter().any(|index| {
                                        mode_name(model, *index).and_then(normalized_mode)
                                            == Some(*mode)
                                    })
                                })
                            {
                                result.controls.capabilities.push("noise".into());
                                result.controls.noise = Some(noise);
                            }
                            let options = session
                                .noise_supported
                                .iter()
                                .filter_map(|index| {
                                    profile(model, *index).map(|item| (*index, item.label.clone()))
                                })
                                .collect();
                            let control = select_control(
                                "noiseProfile",
                                "Noise-control profile",
                                "Choose a noise-control profile advertised for this model.",
                                current,
                                options,
                            );
                            if control.validate().is_ok() {
                                session
                                    .values
                                    .insert("noiseProfile".into(), control.value.clone());
                                session
                                    .controls
                                    .insert("noiseProfile".into(), control.descriptor.clone());
                                result.controls.capabilities.push("noiseProfile".into());
                                result.controls.device_controls.push(control);
                            }
                        }
                        Err(error) => {
                            result.warnings.push(error);
                            if !session.synchronized {
                                return Err("OnePlus discovery lost synchronization; reconnect before using controls.".into());
                            }
                        }
                    }
                } else {
                    result
                        .warnings
                        .push("OnePlus returned an invalid noise-profile support reply.".into());
                }
            }
            Err(error) => {
                result.warnings.push(error);
                if !session.synchronized {
                    return Err(
                        "OnePlus discovery lost synchronization; reconnect before using controls."
                            .into(),
                    );
                }
            }
        }
    }
    if capability(&session.support, 10)
        && (!model.eq_presets.is_empty() || !model.conditional_eq_presets.is_empty())
    {
        match session.exchange(connection, cancelled, 0x010f, &[]) {
            Ok(reply) if reply.len() == 2 => {
                let current = reply[1];
                result.warnings.push(format!(
                    "OnePlus EQ mode GET 010F raw {} bytes: {}",
                    reply.len(),
                    bounded_hex(&reply)
                ));
                let mut presets: Vec<_> = model.eq_presets.iter().collect();
                presets.extend(
                    model
                        .conditional_eq_presets
                        .iter()
                        .filter(|preset| preset.index == current),
                );
                let mut options: Vec<_> = presets
                    .iter()
                    .map(|preset| ControlOption {
                        value: ControlValue::Integer(i64::from(preset.index)),
                        label: preset.label.clone(),
                        enabled: None,
                    })
                    .collect();
                if !options
                    .iter()
                    .any(|option| option.value == ControlValue::Integer(i64::from(current)))
                {
                    options.push(ControlOption {
                        value: ControlValue::Integer(i64::from(current)),
                        label: format!("Current device profile ({current})"),
                        enabled: Some(false),
                    });
                    result.warnings.push(format!(
                        "OnePlus reported EQ mode {current}, which is not writable in this model's factory preset list; it is shown as read-only."
                    ));
                }
                let control = select_control_with_options(
                    "eqPreset",
                    "Equalizer preset",
                    "Select a built-in EQ profile listed for this model.",
                    current,
                    options,
                );
                if control.validate().is_ok() {
                    session
                        .values
                        .insert("eqPreset".into(), control.value.clone());
                    session
                        .controls
                        .insert("eqPreset".into(), control.descriptor.clone());
                    result.controls.capabilities.push("eqPreset".into());
                    result.controls.device_controls.push(control);
                }
            }
            Ok(reply) => result.warnings.push(format!(
                "OnePlus returned an invalid EQ-preset status reply ({} bytes): {}",
                reply.len(),
                bounded_hex(&reply)
            )),
            Err(error) => {
                result.warnings.push(error);
                if !session.synchronized {
                    return Err(
                        "OnePlus discovery lost synchronization; reconnect before using controls."
                            .into(),
                    );
                }
            }
        }
    }
    // The model whitelist uses GET_FEATURE_SWITCH_STATUS for the two verified
    // everyday switches. The returned feature entries, not the model name,
    // decide which controls can be exposed and subsequently written.
    if capability(&session.support, 7)
        && (model.wear_detection
            || selected_game_feature(model, &session.support).is_some()
            || model.bass_wave_supported)
    {
        match session.exchange(
            connection,
            cancelled,
            0x010d,
            &switch_query(model, &session.support),
        ) {
            Ok(reply) => match feature_switches(&reply) {
                Some(switches) => {
                    let mut requested = Vec::new();
                    if model.wear_detection {
                        requested.push((
                            4,
                            "wearDetection",
                            "Wear detection",
                            "Pause and resume audio when the earbuds are removed or worn.",
                        ));
                    }
                    if let Some(feature) = selected_game_feature(model, &session.support) {
                        requested.push((
                            feature,
                            "gameMode",
                            "Game mode",
                            "Toggle the earbuds' low-latency game mode.",
                        ));
                    }
                    if model.bass_wave_supported {
                        requested.push((
                            29,
                            "bassWaveEnabled",
                            "BassWave",
                            "Enable or disable OnePlus BassWave processing.",
                        ));
                    }
                    for (feature, key, label, description) in requested {
                        if let Some(&enabled) = switches.get(&feature) {
                            let control = toggle_control(key, label, description, enabled);
                            if control.validate().is_ok() {
                                session.values.insert(key.into(), control.value.clone());
                                session
                                    .controls
                                    .insert(key.into(), control.descriptor.clone());
                                result.controls.capabilities.push(key.into());
                                result.controls.device_controls.push(control);
                            }
                        }
                    }
                }
                None => result
                    .warnings
                    .push("OnePlus returned an invalid feature-switch status reply.".into()),
            },
            Err(error) => result.warnings.push(error),
        }
    }
    if capability(&session.support, 34) {
        match session.exchange(connection, cancelled, 0x0122, &[]) {
            Ok(reply) => {
                result.warnings.push(format!(
                    "OnePlus EQ profile GET 0122 raw {} bytes: {}",
                    reply.len(),
                    bounded_hex(&reply)
                ));
                match crate::oneplus_eq::parse_profiles(&reply) {
                    Ok(profiles) => {
                        if model.custom_eq_supported {
                            result.controls.capabilities.push("oneplusEqProfile".into());
                            session
                                .metadata
                                .insert("oneplusEqProfile".into(), serde_json::json!(true));
                            if let Some(template) = eq_template(model) {
                                session.metadata.insert(
                                    "oneplusEqTemplate".into(),
                                    serde_json::json!(template),
                                );
                            }
                        }
                        session.eq_profiles = profiles.clone();
                        session.metadata.insert(
                            "oneplusEqualizerProfiles".into(),
                            serde_json::json!(profiles),
                        );
                    }
                    Err(error) => result.warnings.push(format!(
                        "{error}; bounded raw payload: {}",
                        bounded_hex(&reply)
                    )),
                }
            }
            Err(error) => result.warnings.push(error),
        }
    }
    if bass_wave_set_supported(model, &session.support) {
        let baseline = session.bass_wave_revision;
        match session.query_bass_wave(connection, cancelled, baseline) {
            Ok(info) => {
                let control = bass_wave_control(info);
                if control.validate().is_ok() {
                    session
                        .values
                        .insert("bassWave".into(), control.value.clone());
                    session
                        .controls
                        .insert("bassWave".into(), control.descriptor.clone());
                    result.controls.capabilities.push("bassWave".into());
                    result.controls.device_controls.push(control);
                }
            }
            Err(error) => {
                result.warnings.push(error);
                if !session.synchronized {
                    return Err(
                        "OnePlus discovery lost synchronization; reconnect before using controls."
                            .into(),
                    );
                }
            }
        }
    }
    if spatial_path(model, &session.support).is_none() && !model.spatial_types.is_empty() {
        let summary = spatial_support_summary(&session.support, None);
        session.note(format!(
            "Spatial audio is not offered: {} lists spatial types {:?}, but {summary}.",
            model.name, model.spatial_types
        ));
    }
    if let Some(path) = spatial_path(model, &session.support) {
        let response = match path {
            SpatialPath::Typed => session.exchange(connection, cancelled, 0x012a, &[]),
            SpatialPath::Legacy => session.exchange(connection, cancelled, 0x010d, &[1, 27]),
        };
        let discovery_note = match &response {
            Ok(reply) => format!(
                "Spatial audio discovery: {}; current-value reply bytes {}, read as {}.",
                spatial_support_summary(&session.support, Some(path)),
                hex(reply),
                spatial_readback(reply, path).map_or("an unrecognised value".to_string(), |found| found.to_string())
            ),
            Err(error) => format!(
                "Spatial audio discovery: {}; the current-value request failed: {error}",
                spatial_support_summary(&session.support, Some(path))
            ),
        };
        session.note(discovery_note);
        match response {
            Ok(reply) => {
                if let Some(control) = spatial_readback(&reply, path)
                    .and_then(|value| spatial_control(model, path, value))
                {
                    if control.validate().is_ok() {
                        session
                            .values
                            .insert("spatialAudio".into(), control.value.clone());
                        session
                            .controls
                            .insert("spatialAudio".into(), control.descriptor.clone());
                        result.controls.capabilities.push("spatialAudio".into());
                        result.controls.device_controls.push(control);
                    }
                } else {
                    result.warnings.push("OnePlus spatial mode readback was invalid or not listed for this model; spatial controls remain unavailable.".into());
                }
            }
            Err(error) => result.warnings.push(error),
        }
        if !session.synchronized {
            return Err(
                "Spatial mode discovery lost synchronization; reconnect before using controls."
                    .into(),
            );
        }
    }
    if !model.game_sound_types.is_empty() && capability(&session.support, 49) {
        match session.exchange(connection, cancelled, 0x012b, &[]) {
            Ok(reply) => {
                if let Some((current, options)) = game_sound_readback(&reply, model) {
                    if let Some(control) = game_sound_control(model, current, &options) {
                        if control.validate().is_ok() {
                            session
                                .values
                                .insert("gameSoundProfile".into(), control.value.clone());
                            session
                                .controls
                                .insert("gameSoundProfile".into(), control.descriptor.clone());
                            result.controls.capabilities.push("gameSoundProfile".into());
                            result.controls.device_controls.push(control);
                        }
                    }
                } else {
                    result.warnings.push("OnePlus GameSound profile readback was invalid or not listed for this model; GameSound controls remain unavailable.".into());
                }
            }
            Err(error) => result.warnings.push(error),
        }
        if !session.synchronized {
            return Err("GameSound profile discovery lost synchronization; reconnect before using controls.".into());
        }
    }
    // Subscribe only to the original app's advertised battery and noise events.
    let notifications = session.exchange(connection, cancelled, 0x0200, &[])?;
    if notifications.len() >= 2 && usize::from(notifications[1]) == notifications.len() - 2 {
        for event in [1, 3] {
            if notifications[2..].contains(&event) {
                if let Err(error) = session.exchange(connection, cancelled, 0x0201, &[event]) {
                    result.warnings.push(error);
                }
            }
        }
    }
    if !session.synchronized {
        return Err(
            "OnePlus discovery lost synchronization; reconnect before using controls.".into(),
        );
    }
    Ok((result, session))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_reply_shapes_and_model_paths_are_strict() {
        assert_eq!(spatial_readback(&[0, 2], SpatialPath::Typed), Some(2));
        for invalid in [vec![], vec![0], vec![1, 1], vec![0, 3], vec![0, 1, 2]] {
            assert_eq!(spatial_readback(&invalid, SpatialPath::Typed), None);
        }
        assert_eq!(
            spatial_readback(&[0, 1, 27, 1], SpatialPath::Legacy),
            Some(1)
        );
        for invalid in [
            vec![0, 1, 27, 2],
            vec![0, 2, 27, 1, 27, 0],
            vec![0, 1, 4, 1],
        ] {
            assert_eq!(spatial_readback(&invalid, SpatialPath::Legacy), None);
        }
        let pro = crate::oneplus_models::by_name("OnePlus Buds Pro 3").unwrap();
        let mut typed = vec![0; 6];
        typed[5] = 0x80;
        assert_eq!(spatial_path(pro, &typed), Some(SpatialPath::Typed));
        assert!(spatial_control(pro, SpatialPath::Typed, 2).is_some());
        assert!(spatial_control(pro, SpatialPath::Legacy, 2).is_none());
        let nord = crate::oneplus_models::by_name(MODEL).unwrap();
        assert!(spatial_control(nord, SpatialPath::Typed, 2).is_none());
        let phone = crate::oneplus_models::by_name("OnePlus Buds Pro 2").unwrap();
        assert_eq!(spatial_path(phone, &typed), None);
    }

    #[test]
    fn typed_spatial_discovery_uses_its_advertised_command_and_model_options() {
        let mut connection = ScriptedConnection::new(vec![
            (0x0103, vec![], vec![reply(0x0103, &[0, 0x14, 0x40, 6])]),
            (
                0x0100,
                vec![],
                vec![reply(0x0100, &[0, 0, 0, 0, 0, 0, 0x80])],
            ),
            (0x012a, vec![], vec![reply(0x012a, &[0, 2])]),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, session) = read_basics(&mut connection, &AtomicBool::new(false)).unwrap();
        let control = info
            .controls
            .device_controls
            .iter()
            .find(|c| c.descriptor.key == "spatialAudio")
            .unwrap();
        assert_eq!(control.value, ControlValue::Integer(2));
        assert_eq!(control.descriptor.options.as_ref().unwrap().len(), 3);
        assert_eq!(
            session.values.get("spatialAudio"),
            Some(&ControlValue::Integer(2))
        );
        assert!(connection.script.is_empty());
    }

    #[test]
    fn spatial_discovery_records_the_path_support_bits_and_raw_reply_for_diagnostics() {
        let mut connection = ScriptedConnection::new(vec![
            (0x0103, vec![], vec![reply(0x0103, &[0, 0x14, 0x40, 6])]),
            (0x0100, vec![], vec![reply(0x0100, &[0, 0, 0, 0, 0, 0, 0x80])]),
            (0x012a, vec![], vec![reply(0x012a, &[0, 2])]),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (_, mut session) = read_basics(&mut connection, &AtomicBool::new(false)).unwrap();
        let notes = session.drain_diagnostics();
        let note = notes.iter().find(|note| note.starts_with("Spatial audio discovery")).expect("a discovery note");
        assert!(note.contains("typed (GET 0x012a / SET 0x0422)"), "{note}");
        assert!(note.contains("capability 47 (typed) = true"), "{note}");
        assert!(note.contains("current-value reply bytes 00 02, read as 2"), "{note}");
        assert!(session.drain_diagnostics().is_empty(), "notes are drained once");
    }

    #[test]
    fn a_failed_spatial_readback_after_an_acknowledged_set_leaves_a_diagnostic_note() {
        let model = crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap();
        let mut session = Session::new();
        session.model = Some(model);
        session.support = vec![0x80];
        let control = spatial_control(model, SpatialPath::Legacy, 1).unwrap();
        session.values.insert("spatialAudio".into(), control.value);
        session.controls.insert("spatialAudio".into(), control.descriptor);
        // The SET is acknowledged, then the earbuds answer the readback with an error status.
        let mut connection = ScriptedConnection::new(vec![
            (0x0403, vec![27, 0], vec![reply(0x0403, &[0])]),
            (0x010d, vec![1, 27], vec![reply(0x010d, &[1])]),
        ]);
        let result = session.apply_setting(
            &mut connection,
            &AtomicBool::new(false),
            SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                key: "spatialAudio".into(),
                value: ControlValue::Integer(0),
            }),
        );
        assert!(result.is_err());
        let notes = session.drain_diagnostics();
        assert!(notes.iter().any(|note| note.contains("was acknowledged")), "{notes:?}");
        assert!(
            notes.iter().any(|note| note.contains("readback 1 of 3 failed after the SET was acknowledged")),
            "the failed readback must not vanish: {notes:?}"
        );
        assert_eq!(session.values["spatialAudio"], ControlValue::Integer(1), "the last confirmed value is kept");
    }

    #[test]
    fn a_listed_model_without_a_live_spatial_capability_says_why_it_is_not_offered() {
        let mut connection = ScriptedConnection::new(vec![
            (0x0103, vec![], vec![reply(0x0103, &[0, 0x14, 0x40, 6])]),
            (0x0100, vec![], vec![reply(0x0100, &[0, 0, 0, 0, 0, 0, 0])]),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, mut session) = read_basics(&mut connection, &AtomicBool::new(false)).unwrap();
        assert!(!info.controls.capabilities.contains(&"spatialAudio".to_string()));
        let notes = session.drain_diagnostics();
        let note = notes.iter().find(|note| note.starts_with("Spatial audio is not offered")).expect("an explanation");
        assert!(note.contains("capability 47 (typed) = false") && note.contains("capability 7 (legacy) = false"), "{note}");
    }

    #[test]
    fn spatial_set_requires_matching_readback_and_never_sends_restoration() {
        for (path, requested, observed, succeeds) in [
            (SpatialPath::Typed, 2, 2, true),
            (SpatialPath::Typed, 1, 0, false),
            (SpatialPath::Legacy, 1, 1, true),
            (SpatialPath::Legacy, 1, 0, false),
        ] {
            let model = crate::oneplus_models::by_name("OnePlus Buds Pro 3").unwrap();
            let mut session = Session::new();
            session.model = Some(model);
            session.support = if path == SpatialPath::Typed {
                vec![0, 0, 0, 0, 0, 0x80]
            } else {
                vec![0x80]
            };
            let control = spatial_control(model, path, 0).unwrap();
            session.values.insert("spatialAudio".into(), control.value);
            session
                .controls
                .insert("spatialAudio".into(), control.descriptor);
            let (set, payload, get, query, status) = if path == SpatialPath::Typed {
                (0x0422, vec![requested], 0x012a, vec![], vec![0, observed])
            } else {
                (
                    0x0403,
                    vec![27, requested],
                    0x010d,
                    vec![1, 27],
                    vec![0, 1, 27, observed],
                )
            };
            let mut script = vec![(set, payload, vec![reply(set, &[0])])];
            for _ in 0..if succeeds { 1 } else { 3 } {
                script.push((get, query.clone(), vec![reply(get, &status)]));
            }
            let mut connection = ScriptedConnection::new(script);
            let result = session.apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                    key: "spatialAudio".into(),
                    value: ControlValue::Integer(i64::from(requested)),
                }),
            );
            assert_eq!(result.is_ok(), succeeds);
            let notes = session.drain_diagnostics();
            assert!(notes[0].contains(&format!("change to {requested} requested")), "{notes:?}");
            let support_bit = if path == SpatialPath::Typed { "capability 47 (typed) = true" } else { "capability 7 (legacy) = true" };
            assert!(notes[0].contains(support_bit), "{notes:?}");
            assert!(notes.iter().any(|note| note.contains(&format!("SET 0x{set:04x}")) && note.contains("was acknowledged")), "{notes:?}");
            assert_eq!(notes.iter().filter(|note| note.contains("readback")).count(), if succeeds { 1 } else { 3 }, "{notes:?}");
            assert_eq!(
                session.values["spatialAudio"],
                ControlValue::Integer(if succeeds { i64::from(requested) } else { 0 })
            );
            assert!(connection.script.is_empty());
            assert_eq!(
                connection
                    .writes
                    .iter()
                    .filter(|p| p.command == set)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn game_sound_readback_requires_a_bounded_unique_live_intersection() {
        let model = crate::oneplus_models::by_product_bytes(&[0x14, 0x78, 0x06]).unwrap();
        assert_eq!(model.game_sound_types, [0, 3]);
        assert_eq!(
            game_sound_readback(&[0, 3, 2, 0, 3], model),
            Some((3, vec![0, 3]))
        );
        assert_eq!(
            game_sound_control(model, 3, &[0, 3])
                .unwrap()
                .descriptor
                .options
                .as_ref()
                .unwrap()[1]
                .label,
            "Sound effects for shooting games"
        );
        assert_eq!(
            game_sound_readback(&[0, 0, 1, 0], model),
            Some((0, vec![0]))
        );
        for invalid in [
            vec![],
            vec![1, 3, 2, 0, 3],
            vec![0, 2, 0, 3],
            vec![0, 3, 2, 0, 0],
            vec![0, 2, 1, 0],
            vec![0, 3, 1, 0],
            vec![0, 3, 0],
            vec![0, 3, 1, 0, 3],
        ] {
            assert_eq!(game_sound_readback(&invalid, model), None, "{invalid:?}");
        }
        let plain = crate::oneplus_models::by_name("OnePlus Buds").unwrap();
        assert_eq!(game_sound_readback(&[0, 0, 1, 0], plain), None);
    }

    #[test]
    fn game_sound_discovery_uses_only_live_capability_and_profile_intersection() {
        let mut connection = ScriptedConnection::new(vec![
            (0x0103, vec![], vec![reply(0x0103, &[0, 0x14, 0x78, 0x06])]),
            (
                0x0100,
                vec![],
                vec![reply(0x0100, &[0, 0, 0, 0, 0, 0, 0, 2])],
            ),
            (0x012b, vec![], vec![reply(0x012b, &[0, 3, 2, 0, 3])]),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, session) = read_basics(&mut connection, &AtomicBool::new(false)).unwrap();
        let control = info
            .controls
            .device_controls
            .iter()
            .find(|c| c.descriptor.key == "gameSoundProfile")
            .unwrap();
        assert_eq!(control.value, ControlValue::Integer(3));
        let options = control.descriptor.options.as_ref().unwrap();
        assert_eq!(
            options
                .iter()
                .map(|option| option.value.clone())
                .collect::<Vec<_>>(),
            [ControlValue::Integer(0), ControlValue::Integer(3)]
        );
        assert_eq!(options[1].label, "Sound effects for shooting games");
        assert_eq!(
            session.values.get("gameSoundProfile"),
            Some(&ControlValue::Integer(3))
        );
        assert!(info
            .controls
            .capabilities
            .contains(&"gameSoundProfile".into()));
        assert!(connection.script.is_empty());

        let mut gated = ScriptedConnection::new(vec![
            (0x0103, vec![], vec![reply(0x0103, &[0, 0x14, 0x78, 0x06])]),
            (
                0x0100,
                vec![],
                vec![reply(0x0100, &[0, 0, 0, 0, 0, 0, 0, 0])],
            ),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, _) = read_basics(&mut gated, &AtomicBool::new(false)).unwrap();
        assert!(!info
            .controls
            .capabilities
            .contains(&"gameSoundProfile".into()));
        assert!(gated.script.is_empty());
        assert!(!gated.writes.iter().any(|packet| packet.command == 0x012b));
    }

    #[test]
    fn game_sound_set_is_single_and_requires_fresh_matching_readback() {
        let model = crate::oneplus_models::by_product_bytes(&[0x14, 0x78, 0x06]).unwrap();
        for (requested, readback, expected, attempts) in [
            (3, vec![0, 3, 2, 0, 3], true, 1),
            (3, vec![0, 0, 2, 0, 3], false, 3),
            (3, vec![0, 3, 2, 0, 0], false, 1),
            (0, vec![0, 0, 2, 0, 3], true, 1),
        ] {
            let mut session = Session::new();
            session.model = Some(model);
            session.support = vec![0, 0, 0, 0, 0, 0, 2];
            let control = game_sound_control(model, 3, &[0, 3]).unwrap();
            session
                .values
                .insert("gameSoundProfile".into(), control.value);
            session
                .controls
                .insert("gameSoundProfile".into(), control.descriptor);
            let mut script = vec![(0x0423, vec![requested, 1], vec![reply(0x0423, &[0])])];
            script.extend((0..attempts).map(|_| (0x012b, vec![], vec![reply(0x012b, &readback)])));
            let mut connection = ScriptedConnection::new(script);
            let result = session.apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                    key: "gameSoundProfile".into(),
                    value: ControlValue::Integer(i64::from(requested)),
                }),
            );
            assert_eq!(result.is_ok(), expected);
            assert_eq!(
                session.values["gameSoundProfile"],
                ControlValue::Integer(if expected { i64::from(requested) } else { 3 })
            );
            let writes: Vec<_> = connection
                .writes
                .iter()
                .map(|packet| (packet.command, packet.data.clone()))
                .collect();
            assert_eq!(writes.first(), Some(&(0x0423, vec![requested, 1])));
            assert!(writes[1..]
                .iter()
                .all(|(command, payload)| *command == 0x012b && payload.is_empty()));
            assert_eq!(writes.len(), attempts + 1);
            assert!(connection.script.is_empty());
        }
    }

    #[test]
    fn bass_wave_signed_triple_is_statusless_and_round_trips() {
        let parsed = parse_bass_wave(&[0xe2, 0x14, 0xff]).unwrap();
        assert_eq!(
            parsed,
            BassWaveInfo {
                min: -30,
                max: 20,
                current: -1
            }
        );
        assert_eq!(encode_bass_wave(parsed).unwrap(), [0xe2, 0x14, 0xff]);
    }

    #[test]
    fn bass_wave_codec_rejects_wrong_length_and_invalid_ranges() {
        assert!(parse_bass_wave(&[]).is_err());
        assert!(parse_bass_wave(&[0, 1]).is_err());
        assert!(parse_bass_wave(&[0, 1, 2, 3]).is_err());
        assert!(parse_bass_wave(&[2, 1, 1]).is_err());
        assert!(parse_bass_wave(&[0, 1, 2]).is_err());
    }

    #[test]
    fn bass_wave_0508_notifications_accept_reports_on_either_side_of_ack() {
        for packets in [
            vec![
                Packet {
                    command: BASS_WAVE_STATUS,
                    sequence: 0,
                    data: vec![0xf6, 0x0a, 0],
                },
                reply(BASS_WAVE_SET, &[0]),
            ],
            vec![
                reply(BASS_WAVE_SET, &[0]),
                Packet {
                    command: BASS_WAVE_STATUS,
                    sequence: 0,
                    data: vec![0xf6, 0x0a, 0],
                },
            ],
        ] {
            let mut session = Session::new();
            session.model =
                Some(crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap());
            session.support = vec![0, 0, 0, 0, 0x40];
            let wire = packets
                .iter()
                .flat_map(|packet| encode(packet).unwrap())
                .collect::<Vec<_>>();
            session.feed(&wire);
            assert_eq!(
                session.bass_wave,
                Some(BassWaveInfo {
                    min: -10,
                    max: 10,
                    current: 0
                })
            );
            assert_eq!(session.bass_wave_revision, 1);
        }

        let mut invalid = Session::new();
        invalid.model = Some(crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap());
        invalid.support = vec![0, 0, 0, 0, 0x40];
        invalid.feed(
            &encode(&Packet {
                command: BASS_WAVE_STATUS,
                sequence: 0,
                data: vec![20, 10, 0],
            })
            .unwrap(),
        );
        assert_eq!(invalid.bass_wave, None);
        assert_eq!(invalid.bass_wave_revision, 0);
    }

    #[test]
    fn bass_wave_set_requires_allowlisted_model_and_set_capability() {
        let supported = crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap();
        let unsupported = crate::oneplus_models::by_name("OnePlus Buds Z").unwrap();
        assert!(bass_wave_set_supported(supported, &[0, 0, 0, 0, 0x40]));
        assert!(!bass_wave_set_supported(supported, &[0; 8]));
        assert!(!bass_wave_set_supported(unsupported, &[0, 0, 0, 0, 0x40]));
    }

    fn bass_wave_test_session(info: BassWaveInfo) -> Session {
        let mut session = Session::new();
        session.model = Some(crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap());
        session.support = vec![0, 0, 0, 0, 0x40];
        session.bass_wave = Some(info);
        session.bass_wave_revision = 1;
        let control = bass_wave_control(info);
        session.values.insert("bassWave".into(), control.value);
        session
            .controls
            .insert("bassWave".into(), control.descriptor);
        session
    }

    #[test]
    fn bass_wave_out_of_range_value_is_rejected_before_device_io() {
        let mut session = bass_wave_test_session(BassWaveInfo {
            min: -30,
            max: 20,
            current: 0,
        });
        let mut connection = ScriptedConnection::new(vec![]);
        let result = session.apply_setting(
            &mut connection,
            &AtomicBool::new(false),
            SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                key: "bassWave".into(),
                value: ControlValue::Integer(21),
            }),
        );
        assert!(result
            .unwrap_err()
            .contains("outside the device-reported range"));
        assert!(connection.writes.is_empty());
    }

    #[test]
    fn bass_wave_changed_live_bounds_reject_set_without_writing() {
        let mut session = bass_wave_test_session(BassWaveInfo {
            min: -30,
            max: 20,
            current: -1,
        });
        let mut connection = ScriptedConnection::new(vec![(
            BASS_WAVE_GET,
            vec![],
            vec![reply(BASS_WAVE_GET, &[0, 0xec, 10, 0])],
        )]);
        let result = session.apply_setting(
            &mut connection,
            &AtomicBool::new(false),
            SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                key: "bassWave".into(),
                value: ControlValue::Integer(10),
            }),
        );
        assert!(result
            .unwrap_err()
            .contains("range changed since discovery"));
        assert_eq!(
            (
                session.controls["bassWave"].min,
                session.controls["bassWave"].max
            ),
            (Some(-20), Some(10))
        );
        assert_eq!(
            connection
                .writes
                .iter()
                .map(|packet| packet.command)
                .collect::<Vec<_>>(),
            [BASS_WAVE_GET]
        );
        assert!(connection.script.is_empty());
    }

    #[test]
    fn bass_wave_mismatched_readbacks_are_bounded_and_never_trigger_rollback() {
        let mut session = bass_wave_test_session(BassWaveInfo {
            min: -30,
            max: 20,
            current: -1,
        });
        let mismatch = reply(BASS_WAVE_GET, &[0, 0xe2, 0x14, 9]);
        let mut connection = ScriptedConnection::new(vec![
            (
                BASS_WAVE_GET,
                vec![],
                vec![reply(BASS_WAVE_GET, &[0, 0xe2, 0x14, 0xff])],
            ),
            (
                BASS_WAVE_SET,
                vec![0xe2, 0x14, 10],
                vec![reply(BASS_WAVE_SET, &[0])],
            ),
            (BASS_WAVE_GET, vec![], vec![mismatch.clone()]),
            (BASS_WAVE_GET, vec![], vec![mismatch.clone()]),
            (BASS_WAVE_GET, vec![], vec![mismatch]),
        ]);
        let result = session.apply_setting(
            &mut connection,
            &AtomicBool::new(false),
            SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                key: "bassWave".into(),
                value: ControlValue::Integer(10),
            }),
        );
        assert!(result.unwrap_err().contains("readback did not match"));
        let commands = connection
            .writes
            .iter()
            .map(|packet| packet.command)
            .collect::<Vec<_>>();
        assert_eq!(
            commands
                .iter()
                .filter(|command| **command == BASS_WAVE_SET)
                .count(),
            1
        );
        assert_eq!(
            commands
                .iter()
                .filter(|command| **command == BASS_WAVE_GET)
                .count(),
            4
        );
        assert!(connection.script.is_empty());
        assert_eq!(session.values["bassWave"], ControlValue::Integer(9));
    }

    #[test]
    fn bass_wave_enable_switch_requires_model_and_exact_live_readback() {
        let model = crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap();
        let query = switch_query(model, &[0x80]);
        assert_eq!(query, [3, 4, 6, 29]);
        let mut connection = ScriptedConnection::new(vec![
            (0x0403, vec![29, 1], vec![reply(0x0403, &[0])]),
            (
                0x010d,
                query.clone(),
                vec![reply(0x010d, &[0, 3, 4, 0, 6, 0, 29, 1])],
            ),
        ]);
        let mut session = Session::new();
        session.model = Some(model);
        session.support = vec![0x80];
        let control = toggle_control("bassWaveEnabled", "BassWave", "Enable BassWave.", false);
        session
            .values
            .insert("bassWaveEnabled".into(), control.value.clone());
        session
            .controls
            .insert("bassWaveEnabled".into(), control.descriptor);
        let confirmation = session
            .apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                    key: "bassWaveEnabled".into(),
                    value: ControlValue::Boolean(true),
                }),
            )
            .unwrap();
        let SettingConfirmation::DeviceControl(confirmation) = confirmation else {
            panic!("expected BassWave switch confirmation");
        };
        assert_eq!(confirmation.confirmed, ControlValue::Boolean(true));
        assert_eq!(
            connection
                .writes
                .iter()
                .map(|packet| (packet.command, packet.data.clone()))
                .collect::<Vec<_>>(),
            [(0x0403, vec![29, 1]), (0x010d, query),]
        );

        let unsupported = crate::oneplus_models::by_name("OnePlus Buds Z").unwrap();
        let mut guarded = Session::new();
        guarded.model = Some(unsupported);
        guarded.support = vec![0x80];
        let control = toggle_control("bassWaveEnabled", "BassWave", "Enable BassWave.", false);
        guarded
            .values
            .insert("bassWaveEnabled".into(), control.value.clone());
        guarded
            .controls
            .insert("bassWaveEnabled".into(), control.descriptor);
        let mut no_io = ScriptedConnection::new(vec![]);
        assert!(guarded
            .apply_setting(
                &mut no_io,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                    key: "bassWaveEnabled".into(),
                    value: ControlValue::Boolean(true),
                }),
            )
            .is_err());
        assert!(no_io.writes.is_empty());
    }

    #[test]
    fn bass_wave_discovery_and_set_confirm_from_fresh_prefixed_get_replies() {
        let model = crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap();
        let product_bytes = [
            u8::from_str_radix(&model.product_id[4..6], 16).unwrap(),
            u8::from_str_radix(&model.product_id[2..4], 16).unwrap(),
            u8::from_str_radix(&model.product_id[0..2], 16).unwrap(),
        ];
        let support = [0, 0, 0, 0, 0x40]; // command 1051 / bit 38
        let mut connection = ScriptedConnection::new(vec![
            (
                0x0103,
                vec![],
                vec![reply(
                    0x0103,
                    &[0, product_bytes[0], product_bytes[1], product_bytes[2]],
                )],
            ),
            (
                0x0100,
                vec![],
                vec![reply(
                    0x0100,
                    &[
                        0, support[0], support[1], support[2], support[3], support[4],
                    ],
                )],
            ),
            // The GET reply carries a status prefix; the signed bytes follow it.
            (
                BASS_WAVE_GET,
                vec![],
                vec![reply(BASS_WAVE_GET, &[0, 0xe2, 0x14, 0xff])],
            ),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
            (
                BASS_WAVE_GET,
                vec![],
                vec![reply(BASS_WAVE_GET, &[0, 0xe2, 0x14, 0xff])],
            ),
            (
                BASS_WAVE_SET,
                vec![0xe2, 0x14, 0x0a],
                vec![reply(BASS_WAVE_SET, &[0])],
            ),
            (
                BASS_WAVE_GET,
                vec![],
                vec![reply(BASS_WAVE_GET, &[0, 0xe2, 0x14, 0x0a])],
            ),
        ]);
        let (info, mut session) = read_basics(&mut connection, &AtomicBool::new(false)).unwrap();
        assert!(info.controls.capabilities.contains(&"bassWave".into()));
        let descriptor = &session.controls["bassWave"];
        assert_eq!((descriptor.min, descriptor.max), (Some(-30), Some(20)));
        assert_eq!(session.values["bassWave"], ControlValue::Integer(-1));

        let confirmation = session
            .apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                    key: "bassWave".into(),
                    value: ControlValue::Integer(10),
                }),
            )
            .unwrap();
        let SettingConfirmation::DeviceControl(confirmation) = confirmation else {
            panic!("expected BassWave range confirmation");
        };
        assert_eq!(confirmation.before, ControlValue::Integer(-1));
        assert_eq!(confirmation.confirmed, ControlValue::Integer(10));
        assert_eq!(
            connection
                .writes
                .iter()
                .map(|packet| (packet.command, packet.data.clone()))
                .collect::<Vec<_>>(),
            [
                (0x0103, vec![]),
                (0x0100, vec![]),
                (BASS_WAVE_GET, vec![]),
                (0x0200, vec![]),
                (BASS_WAVE_GET, vec![]),
                (BASS_WAVE_SET, vec![0xe2, 0x14, 0x0a]),
                (BASS_WAVE_GET, vec![]),
            ]
        );
        assert!(connection.script.is_empty());
    }

    #[test]
    fn original_small_request_layout_and_chunked_stream() {
        let request = Packet {
            command: 0x010c,
            sequence: 7,
            data: vec![1, 1],
        };
        let wire = encode(&request).unwrap();
        assert_eq!(wire, [0xaa, 9, 0, 0, 0x0c, 1, 7, 2, 0, 1, 1]);
        let mut decoder = Decoder::default();
        let mut output = vec![];
        for byte in wire {
            output.extend(decoder.feed(&[byte]));
        }
        assert_eq!(output, [request]);
    }
    #[test]
    fn fragments_roundtrip_and_bad_sequence_cannot_join_packets() {
        let packet = Packet {
            command: 0x8122,
            sequence: 255,
            data: (0..=255).collect(),
        };
        let wire = encode(&packet).unwrap();
        let mut decoder = Decoder::default();
        assert_eq!(decoder.feed(&wire), [packet]);
        let mut damaged = wire.clone();
        damaged[24] = 10;
        assert!(Decoder::default().feed(&damaged).is_empty());
    }
    #[test]
    fn nord_profile_map_rejects_ambiguous_or_unknown_states() {
        let supported = BTreeSet::from([0, 1, 2, 4, 5, 6, 7, 8]);
        let model = crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap();
        assert_eq!(parse_current(&[1, 1, 2], &supported, model), Some(1));
        assert_eq!(parse_current(&[1, 1, 0x80], &supported, model), Some(7));
        assert_eq!(parse_current(&[1, 1, 0, 1], &supported, model), Some(8));
        assert_eq!(parse_current(&[1, 1, 3], &supported, model), None);
        assert_eq!(parse_current(&[1, 1, 0, 2], &supported, model), None);
        assert_eq!(parse_current(&[1, 1], &supported, model), None);
    }
    #[test]
    fn model_noise_mappings_are_not_shared_between_products() {
        let nord2 = crate::oneplus_models::by_name("OnePlus Nord Buds 2").unwrap();
        let nord3pro = crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap();
        let supported = BTreeSet::from([0, 1, 2]);
        assert_eq!(
            mode_name(nord2, parse_current(&[1, 1, 1], &supported, nord2).unwrap()),
            Some("cancelling")
        );
        assert_eq!(
            mode_name(
                nord3pro,
                parse_current(&[1, 1, 1], &supported, nord3pro).unwrap()
            ),
            Some("off")
        );
        assert_eq!(mode_name(nord3pro, 0), Some("off"));
        assert_eq!(mode_name(nord2, 0), Some("cancelling"));
        let non_anc = crate::oneplus_models::by_name("OnePlus Buds Z").unwrap();
        assert!(parse_current(&[1, 1, 1], &supported, non_anc).is_none());
    }
    #[test]
    fn malformed_and_oversized_frames_recover_without_unbounded_state() {
        let mut decoder = Decoder::default();
        assert!(decoder.feed(&vec![0xaa; 50000]).is_empty());
        assert!(decoder.buffer.len() <= MAX_PACKET * 2);
        let packet = Packet {
            command: 0x8100,
            sequence: 0,
            data: vec![0, 0x01],
        };
        assert_eq!(decoder.feed(&encode(&packet).unwrap()), [packet]);
        assert!(encode(&Packet {
            command: 0,
            sequence: 0,
            data: vec![0; MAX_PACKET]
        })
        .is_err());
    }
    #[test]
    fn battery_and_firmware_validate_lengths_and_values() {
        let values = batteries(&[3, 1, 85, 2, 80, 3, 0x80 | 70]).unwrap();
        assert_eq!(values[0].level, 85);
        assert!(values[2].charging);
        assert!(batteries(&[1, 1, 127]).is_none());
        assert!(batteries(&[2, 1, 80, 1, 70]).is_none());
        assert_eq!(firmware(b"\x021,2,123.4,2,2,123.4"), Some("123.4".into()));
        assert!(firmware(b"\x031,2,123.4").is_none());
    }
    #[test]
    fn feature_switch_reply_requires_status_count_unique_ids_and_boolean_states() {
        assert_eq!(
            feature_switches(&[0, 2, 2, 1, 4, 0]),
            Some(BTreeMap::from([(2, true), (4, false)]))
        );
        for malformed in [
            vec![],
            vec![1, 1, 4, 1], // nonzero result, zero entries
            vec![0, 0],
            vec![0, 1, 4], // zero count, truncated pair
            vec![0, 1, 4, 2],
            vec![0, 2, 4, 1, 4, 0], // bad bool, duplicate ID
            vec![0, 1, 4, 1, 6, 0], // trailing record contradicts count
        ] {
            assert!(
                feature_switches(&malformed).is_none(),
                "accepted {malformed:?}"
            );
        }
    }

    struct ScriptedConnection {
        script: VecDeque<(u16, Vec<u8>, Vec<Packet>)>,
        reads: VecDeque<Vec<u8>>,
        writes: Vec<Packet>,
    }
    impl ScriptedConnection {
        fn new(script: Vec<(u16, Vec<u8>, Vec<Packet>)>) -> Self {
            Self {
                script: script.into(),
                reads: VecDeque::new(),
                writes: vec![],
            }
        }
    }
    fn reply(command: u16, data: &[u8]) -> Packet {
        Packet {
            command: command | 0x8000,
            sequence: 0,
            data: data.to_vec(),
        }
    }
    fn profile_reply(profiles: &[crate::oneplus_eq::EqProfile]) -> Vec<u8> {
        let mut data = vec![0, profiles.len() as u8];
        for profile in profiles {
            data.extend_from_slice(&[
                profile.selected,
                profile.min_value as u8,
                profile.max_value as u8,
                profile.eq_id,
                profile.name.len() as u8,
            ]);
            data.extend_from_slice(profile.name.as_bytes());
            data.push(profile.frequencies.len() as u8);
            for (frequency, db) in profile.frequencies.iter().zip(&profile.db_values) {
                data.extend_from_slice(&frequency.to_le_bytes());
                data.push(*db as u8);
            }
        }
        data
    }
    impl Connection for ScriptedConnection {
        fn service_uuid(&self) -> &'static str {
            SERVICE_UUID
        }
        fn read(&mut self, buffer: &mut [u8], _: Duration) -> std::io::Result<usize> {
            let Some(bytes) = self.reads.pop_front() else {
                return Ok(0);
            };
            buffer[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
        fn write_all(&mut self, bytes: &[u8], _: Duration) -> std::io::Result<()> {
            let packets = Decoder::default().feed(bytes);
            assert_eq!(packets.len(), 1);
            let request = &packets[0];
            let (command, data, responses) = self.script.pop_front().expect("unexpected command");
            assert_eq!((request.command, &request.data), (command, &data));
            self.writes.push(request.clone());
            for mut response in responses {
                response.sequence = request.sequence.wrapping_add(response.sequence);
                self.reads.push_back(encode(&response).unwrap());
            }
            Ok(())
        }
    }
    fn noise_session() -> Session {
        let mut session = Session::new();
        session.model = Some(crate::oneplus_models::by_name(MODEL).unwrap());
        session.support = vec![0, 1]; // bit 8
        session.noise_supported = BTreeSet::from([0, 1, 2]);
        session
    }

    #[test]
    fn noise_bitmaps_resolve_one_model_hierarchy_without_accepting_conflicting_modes() {
        let model = crate::oneplus_models::by_name(MODEL).unwrap();
        let all = BTreeSet::from([0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(parse_current(&[1, 1, 0x04], &all, model), Some(2)); // user-provided Transparency capture
        assert_eq!(parse_current(&[1, 1, 0x12], &all, model), Some(4)); // NC category plus Max detail
        assert_eq!(parse_current(&[1, 5, 0x12], &all, model), Some(4));
        assert_eq!(parse_current(&[1, 5, 0x80], &all, model), Some(7));
        let roots = BTreeSet::from([0, 1, 2]);
        assert_eq!(parse_current(&[1, 1, 0x10], &roots, model), Some(1));
        assert_eq!(parse_current(&[1, 1, 0x12], &roots, model), Some(1));
        for invalid in [
            vec![1, 1, 0],    // no final mode
            vec![1, 1, 3],    // Off and NC conflict
            vec![1, 1, 0x90], // Max and Smart are sibling details
            vec![1, 1, 0, 2], // index 9 has no model mapping
            vec![1, 2, 0x12], // level subtype is not a mode bitmap
        ] {
            assert!(
                parse_current(&invalid, &all, model).is_none(),
                "accepted conflicting state {invalid:?}"
            );
        }
        // The same numeric bitmap on another model cannot acquire Nord 3 Pro semantics.
        let nord2 = crate::oneplus_models::by_name("OnePlus Nord Buds 2").unwrap();
        assert!(parse_current(&[1, 1, 0x12], &all, nord2).is_none());
    }

    #[test]
    fn nc_change_waits_through_transition_and_reports_actual_child_without_restoration() {
        let mut session = noise_session();
        session.noise_supported.extend([4, 7]);
        let control = select_control(
            "noiseProfile",
            "Noise profile",
            "Device profiles",
            2,
            vec![
                (0, "Off".into()),
                (1, "NC".into()),
                (2, "Transparency".into()),
                (4, "Max".into()),
                (7, "Smart".into()),
            ],
        );
        session
            .controls
            .insert("noiseProfile".into(), control.descriptor);
        session.values.insert("noiseProfile".into(), control.value);
        let mut connection = ScriptedConnection::new(vec![
            (0x0404, vec![1, 1, 2], vec![reply(0x0404, &[0])]),
            (0x010c, vec![1, 1], vec![reply(0x010c, &[0, 1, 1, 0])]),
            (0x010c, vec![1, 1], vec![reply(0x010c, &[0, 1, 5, 0x12])]),
        ]);
        let confirmation = session
            .apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                    key: "noiseProfile".into(),
                    value: ControlValue::Integer(1),
                }),
            )
            .unwrap();
        let SettingConfirmation::DeviceControl(confirmation) = confirmation else {
            panic!("wrong confirmation type");
        };
        assert_eq!(confirmation.before, ControlValue::Integer(2));
        assert_eq!(confirmation.confirmed, ControlValue::Integer(4));
        assert_eq!(session.values["noiseProfile"], ControlValue::Integer(4));
        assert_eq!(
            session.drain_noise_observations(),
            [vec![1, 1, 0], vec![1, 5, 0x12]]
        );
        assert_eq!(
            connection
                .writes
                .iter()
                .filter(|packet| packet.command == 0x0404)
                .count(),
            1
        );
        assert!(session.is_synchronized());
    }

    #[test]
    fn reading_an_unadvertised_detail_does_not_unlock_writing_it() {
        let mut session = noise_session();
        let mut connection = ScriptedConnection::new(vec![]);
        assert!(session
            .set_noise(&mut connection, &AtomicBool::new(false), 4)
            .is_err());
        assert!(connection.writes.is_empty());
    }

    #[test]
    fn rejected_noise_observations_remain_bounded_and_never_update_confirmed_controls() {
        let mut session = noise_session();
        let control = select_control(
            "noiseProfile",
            "Noise profile",
            "Device profiles",
            2,
            vec![
                (0, "Off".into()),
                (1, "NC".into()),
                (2, "Transparency".into()),
            ],
        );
        session
            .controls
            .insert("noiseProfile".into(), control.descriptor);
        session.values.insert("noiseProfile".into(), control.value);
        let invalid = Packet {
            command: 0x0204,
            sequence: 0,
            data: vec![3, 1, 1, 3],
        };
        session.feed(&encode(&invalid).unwrap());
        assert_eq!(session.drain_noise_observations(), [vec![1, 1, 3]]);
        let mut large = vec![3];
        large.extend(vec![1; 1000]);
        let wire = encode(&Packet {
            command: 0x0204,
            sequence: 1,
            data: large,
        })
        .unwrap();
        for _ in 0..40 {
            session.feed(&wire);
        }
        let observations = session.drain_noise_observations();
        assert_eq!(observations.len(), 32);
        assert!(observations.iter().all(|bytes| bytes.len() <= 32));
        assert!(session.drain_device_updates().is_empty());
        assert_eq!(session.values["noiseProfile"], ControlValue::Integer(2));
    }
    #[test]
    fn wrong_product_stops_before_capabilities_or_any_settings() {
        let mut connection =
            ScriptedConnection::new(vec![(0x0103, vec![], vec![reply(0x0103, &[0, 1, 2, 3])])]);
        assert!(read_basics(&mut connection, &AtomicBool::new(false)).is_err());
        assert_eq!(connection.writes.len(), 1);
    }
    #[test]
    fn matching_product_without_feature_bits_never_creates_controls() {
        let mut connection = ScriptedConnection::new(vec![
            (0x0103, vec![], vec![reply(0x0103, &[0, 0x14, 0x44, 6])]),
            (0x0100, vec![], vec![reply(0x0100, &[0, 0])]),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, mut session) = read_basics(&mut connection, &AtomicBool::new(false)).unwrap();
        assert!(info.controls.capabilities.is_empty());
        assert!(session
            .set_noise(&mut connection, &AtomicBool::new(false), 1)
            .is_err());
        assert_eq!(connection.writes.len(), 3);
    }
    #[test]
    fn feature_switch_discovery_requires_bit_seven_and_each_live_entry() {
        let mut unsupported = ScriptedConnection::new(vec![
            (0x0103, vec![], vec![reply(0x0103, &[0, 0x14, 0x44, 6])]),
            (0x0100, vec![], vec![reply(0x0100, &[0, 0])]),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, _) = read_basics(&mut unsupported, &AtomicBool::new(false)).unwrap();
        assert!(info.controls.device_controls.is_empty());
        assert_eq!(
            unsupported
                .writes
                .iter()
                .map(|packet| packet.command)
                .collect::<Vec<_>>(),
            [0x0103, 0x0100, 0x0200]
        );

        let mut one_live_entry = ScriptedConnection::new(vec![
            (0x0103, vec![], vec![reply(0x0103, &[0, 0x14, 0x44, 6])]),
            (0x0100, vec![], vec![reply(0x0100, &[0, 0x80])]), // bit 7 advertises command 1027
            (
                0x010d,
                vec![3, 4, 6, 29],
                vec![reply(0x010d, &[0, 1, 4, 1])],
            ), // gameMode and BassWave entries absent
            (0x010d, vec![1, 27], vec![reply(0x010d, &[0, 0])]), // no live spatial entry
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, session) = read_basics(&mut one_live_entry, &AtomicBool::new(false)).unwrap();
        assert_eq!(info.controls.device_controls.len(), 1);
        assert_eq!(
            info.controls.device_controls[0].descriptor.key,
            "wearDetection"
        );
        assert_eq!(
            info.controls.device_controls[0].value,
            ControlValue::Boolean(true)
        );
        assert!(!session.controls.contains_key("gameMode"));
        assert_eq!(one_live_entry.writes.len(), 5);
    }
    #[test]
    fn every_registry_model_uses_its_own_live_control_mappings() {
        for model in crate::oneplus_models::models() {
            let mut product_bytes = Vec::new();
            for offset in [4, 2, 0] {
                product_bytes
                    .push(u8::from_str_radix(&model.product_id[offset..offset + 2], 16).unwrap());
            }
            let mut support = vec![0u8; 2];
            let has_switches =
                model.wear_detection || model.game_feature.is_some() || model.bass_wave_supported;
            if has_switches {
                support[0] |= 1 << 7;
            }
            if !model.noise_profiles.is_empty() {
                support[1] |= 1;
            }
            if !model.eq_presets.is_empty() || !model.conditional_eq_presets.is_empty() {
                support[1] |= 1 << 2;
            }

            let mut script = vec![
                (
                    0x0103,
                    vec![],
                    vec![reply(
                        0x0103,
                        &[0, product_bytes[0], product_bytes[1], product_bytes[2]],
                    )],
                ),
                (
                    0x0100,
                    vec![],
                    vec![reply(0x0100, &[0, support[0], support[1]])],
                ),
            ];
            if !model.noise_profiles.is_empty() {
                let max_index = model
                    .noise_profiles
                    .iter()
                    .map(|profile| profile.index)
                    .max()
                    .unwrap();
                let byte_count = usize::from(max_index) / 8 + 1;
                let mut available = vec![0, 2, 1];
                available.resize(3 + byte_count, 0);
                for profile in &model.noise_profiles {
                    available[3 + usize::from(profile.index) / 8] |= 1 << (profile.index % 8);
                }
                let current = model.noise_profiles[0].index;
                let mut current_reply = vec![0, 1, 1];
                current_reply.resize(3 + byte_count, 0);
                current_reply[3 + usize::from(current) / 8] |= 1 << (current % 8);
                script.push((0x010c, vec![2, 1], vec![reply(0x010c, &available)]));
                script.push((0x010c, vec![1, 1], vec![reply(0x010c, &current_reply)]));
            }
            if !model.eq_presets.is_empty() || !model.conditional_eq_presets.is_empty() {
                let current = model
                    .eq_presets
                    .first()
                    .or_else(|| model.conditional_eq_presets.first())
                    .unwrap()
                    .index;
                script.push((0x010f, vec![], vec![reply(0x010f, &[0, current])]));
            }
            if has_switches {
                let query = switch_query(model, &support);
                let mut states = vec![0, query[0]];
                for feature in &query[1..] {
                    states.extend_from_slice(&[*feature, u8::from(*feature == 4)]);
                }
                script.push((0x010d, query, vec![reply(0x010d, &states)]));
            }
            if spatial_path(model, &support).is_some() {
                script.push((0x010d, vec![1, 27], vec![reply(0x010d, &[0, 1, 27, 0])]));
            }
            script.push((0x0200, vec![], vec![reply(0x0200, &[0, 0])]));

            let mut connection = ScriptedConnection::new(script);
            let (info, session) = read_basics(&mut connection, &AtomicBool::new(false))
                .unwrap_or_else(|error| {
                    panic!("{} {} failed: {error}", model.name, model.product_id)
                });
            assert_eq!(info.model.as_deref(), Some(model.name.as_str()));
            assert_eq!(
                session.model.map(|selected| selected.product_id.as_str()),
                Some(model.product_id.as_str())
            );
            assert!(
                connection.script.is_empty(),
                "{} left scripted exchanges",
                model.name
            );

            let commands: Vec<_> = connection
                .writes
                .iter()
                .map(|packet| packet.command)
                .collect();
            if model.noise_profiles.is_empty() {
                assert!(
                    !commands.contains(&0x010c),
                    "{} must not query unsupported noise controls",
                    model.name
                );
                assert!(!session.controls.contains_key("noiseProfile"));
                assert!(session.parse_noise_notification(&[1, 1, 1]).is_none());
            } else {
                assert_eq!(
                    session.noise_supported,
                    model
                        .noise_profiles
                        .iter()
                        .map(|profile| profile.index)
                        .collect()
                );
                let selected = model.noise_profiles.first().unwrap();
                let bytes = usize::from(selected.index) / 8 + 1;
                let mut notification = vec![1, 1];
                notification.resize(2 + bytes, 0);
                notification[2 + usize::from(selected.index) / 8] |= 1 << (selected.index % 8);
                assert_eq!(
                    session
                        .parse_noise_notification(&notification)
                        .unwrap()
                        .mode,
                    selected.mode
                );
                let options = session.controls["noiseProfile"].options.as_ref().unwrap();
                assert_eq!(
                    options.len(),
                    model.noise_profiles.len(),
                    "{} noise option count",
                    model.name
                );
                for option in options {
                    let ControlValue::Integer(index) = option.value else {
                        panic!("{} returned nonnumeric profile", model.name);
                    };
                    let configured = model
                        .noise_profiles
                        .iter()
                        .find(|profile| i64::from(profile.index) == index)
                        .unwrap();
                    assert_eq!(option.label, configured.label);
                }
            }

            let expected_eq: Vec<_> = model.eq_presets.iter().map(|preset| preset.index).collect();
            if expected_eq.is_empty() && model.conditional_eq_presets.is_empty() {
                assert!(
                    !commands.contains(&0x010f),
                    "{} must not query unlisted EQ presets",
                    model.name
                );
                assert!(!session.controls.contains_key("eqPreset"));
            } else {
                let current = model
                    .eq_presets
                    .first()
                    .or_else(|| model.conditional_eq_presets.first())
                    .unwrap()
                    .index;
                let mut expected = expected_eq;
                expected.extend(
                    model
                        .conditional_eq_presets
                        .iter()
                        .filter(|preset| preset.index == current)
                        .map(|preset| preset.index),
                );
                let options = session.controls["eqPreset"].options.as_ref().unwrap();
                assert_eq!(
                    options
                        .iter()
                        .map(|option| match option.value {
                            ControlValue::Integer(value) => value as u8,
                            _ => panic!("EQ option must be integer"),
                        })
                        .collect::<BTreeSet<_>>(),
                    expected.into_iter().collect()
                );
            }
            assert_eq!(
                session.controls.contains_key("wearDetection"),
                model.wear_detection,
                "{} wear gate",
                model.name
            );
            assert_eq!(
                session.controls.contains_key("gameMode"),
                model.game_feature.is_some(),
                "{} game gate",
                model.name
            );
            if has_switches {
                assert_eq!(
                    commands.iter().find(|command| **command == 0x010d),
                    Some(&0x010d)
                );
                let query = switch_query(model, &support);
                let actual = connection
                    .writes
                    .iter()
                    .find(|packet| packet.command == 0x010d)
                    .unwrap();
                assert_eq!(actual.data, query, "{} feature request", model.name);
            } else {
                assert!(
                    !commands.contains(&0x010d),
                    "{} must not query unavailable switches",
                    model.name
                );
            }
        }
    }
    #[test]
    fn conditional_eq_preset_is_offered_only_when_currently_reported() {
        let model = crate::oneplus_models::by_name("OnePlus Buds Clip").unwrap();
        let conditional = model.conditional_eq_presets.first().unwrap();
        let base = model.eq_presets.first().unwrap();
        for (current, should_include_conditional) in
            [(conditional.index, true), (base.index, false)]
        {
            let mut product_bytes = Vec::new();
            for offset in [4, 2, 0] {
                product_bytes
                    .push(u8::from_str_radix(&model.product_id[offset..offset + 2], 16).unwrap());
            }
            let mut connection = ScriptedConnection::new(vec![
                (
                    0x0103,
                    vec![],
                    vec![reply(
                        0x0103,
                        &[0, product_bytes[0], product_bytes[1], product_bytes[2]],
                    )],
                ),
                (0x0100, vec![], vec![reply(0x0100, &[0, 0, 4])]),
                (0x010f, vec![], vec![reply(0x010f, &[0, current])]),
                (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
            ]);
            let (info, _) = read_basics(&mut connection, &AtomicBool::new(false)).unwrap();
            let control = info
                .controls
                .device_controls
                .iter()
                .find(|control| control.descriptor.key == "eqPreset")
                .unwrap();
            let indices: BTreeSet<_> = control
                .descriptor
                .options
                .as_ref()
                .unwrap()
                .iter()
                .map(|option| {
                    let ControlValue::Integer(index) = option.value else {
                        panic!("EQ option must be integer");
                    };
                    index as u8
                })
                .collect();
            assert_eq!(
                indices.contains(&conditional.index),
                should_include_conditional
            );
            assert!(indices.contains(&base.index));
        }
    }
    #[test]
    fn unknown_current_eq_mode_keeps_factory_presets_and_marks_current_read_only() {
        let model = crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap();
        let mut product_bytes = Vec::new();
        for offset in [4, 2, 0] {
            product_bytes
                .push(u8::from_str_radix(&model.product_id[offset..offset + 2], 16).unwrap());
        }
        let mut connection = ScriptedConnection::new(vec![
            (
                0x0103,
                vec![],
                vec![reply(
                    0x0103,
                    &[0, product_bytes[0], product_bytes[1], product_bytes[2]],
                )],
            ),
            (0x0100, vec![], vec![reply(0x0100, &[0, 0, 4])]),
            (0x010f, vec![], vec![reply(0x010f, &[0, 99])]),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, _) = read_basics(&mut connection, &AtomicBool::new(false)).unwrap();
        let control = info
            .controls
            .device_controls
            .iter()
            .find(|control| control.descriptor.key == "eqPreset")
            .unwrap();
        assert_eq!(control.value, ControlValue::Integer(99));
        let options = control.descriptor.options.as_ref().unwrap();
        assert!(options
            .iter()
            .any(|option| option.value == ControlValue::Integer(3) && option.label == "Balanced"));
        let current = options
            .iter()
            .find(|option| option.value == ControlValue::Integer(99))
            .unwrap();
        assert_eq!(current.label, "Current device profile (99)");
        assert_eq!(current.enabled, Some(false));
        assert!(crate::device_controls::validate_change(
            &control.descriptor,
            ControlValue::Integer(3)
        )
        .is_ok());
        assert!(crate::device_controls::validate_change(
            &control.descriptor,
            ControlValue::Integer(99)
        )
        .is_err());
        assert!(info
            .warnings
            .iter()
            .any(|warning| warning.contains("raw 2 bytes: 00 63")));
    }
    #[test]
    fn empty_custom_eq_list_exposes_template_only_for_configured_models() {
        let model = crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap();
        let mut product_bytes = Vec::new();
        for offset in [4, 2, 0] {
            product_bytes
                .push(u8::from_str_radix(&model.product_id[offset..offset + 2], 16).unwrap());
        }
        let mut connection = ScriptedConnection::new(vec![
            (
                0x0103,
                vec![],
                vec![reply(
                    0x0103,
                    &[0, product_bytes[0], product_bytes[1], product_bytes[2]],
                )],
            ),
            (0x0100, vec![], vec![reply(0x0100, &[0, 0, 0, 0, 0, 4])]),
            (0x0122, vec![], vec![reply(0x0122, &[0, 0])]),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, session) = read_basics(&mut connection, &AtomicBool::new(false)).unwrap();
        assert!(info
            .controls
            .capabilities
            .contains(&"oneplusEqProfile".into()));
        assert_eq!(
            session.metadata()["oneplusEqProfile"],
            serde_json::json!(true)
        );
        let template = &session.metadata()["oneplusEqTemplate"];
        assert_eq!(template["eqId"], serde_json::json!(0));
        assert_eq!(
            template["frequencies"],
            serde_json::json!([62, 250, 1000, 4000, 8000, 16000])
        );
        assert_eq!(template["dbValues"], serde_json::json!([0, 0, 0, 0, 0, 0]));
        assert_eq!(
            session.metadata()["oneplusEqualizerProfiles"],
            serde_json::json!([])
        );
    }
    #[test]
    fn custom_eq_action_two_requires_live_existing_layout_and_exact_profile_readback() {
        let original = crate::oneplus_eq::EqProfile {
            selected: 1,
            min_value: -6,
            max_value: 6,
            eq_id: 42,
            name: "Desk".into(),
            frequencies: vec![250, 1000, 8000],
            db_values: vec![-2, 0, 3],
        };
        let edited = crate::oneplus_eq::EqProfile {
            name: "Warm Desk".into(),
            db_values: vec![1, 0, -1],
            ..original.clone()
        };
        let update = crate::oneplus_eq::EqProfileChange {
            action: 2,
            profile: edited.clone(),
        };
        let payload = crate::oneplus_eq::encode_set_profile(&edited, 2).unwrap();
        let mut connection = ScriptedConnection::new(vec![
            (
                0x0122,
                vec![],
                vec![reply(0x0122, &profile_reply(&[original.clone()]))],
            ),
            (0x0418, payload.clone(), vec![reply(0x0418, &[0, 0])]),
            (
                0x0122,
                vec![],
                vec![reply(0x0122, &profile_reply(&[edited.clone()]))],
            ),
        ]);
        let mut session = Session::new();
        session.support = vec![0, 0, 0, 0, 4];
        session.model = Some(crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap());
        session.eq_profiles = vec![original.clone()];
        let confirmation = session
            .apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::OnePlusEqProfile(update),
            )
            .unwrap();
        let SettingConfirmation::OnePlusEqProfiles(confirmed_profiles) = confirmation else {
            panic!("expected refreshed profile list");
        };
        assert_eq!(confirmed_profiles, vec![edited.clone()]);
        assert!(connection.script.is_empty());
        assert_eq!(
            connection
                .writes
                .iter()
                .map(|packet| (packet.command, packet.data.clone()))
                .collect::<Vec<_>>(),
            [(0x0122, vec![]), (0x0418, payload), (0x0122, vec![]),]
        );
        assert_eq!(
            session.metadata["oneplusEqualizerProfiles"],
            serde_json::json!([edited])
        );

        let mut changed_layout = edited.clone();
        changed_layout.frequencies[0] = 300;
        let mut stale_connection = ScriptedConnection::new(vec![(
            0x0122,
            vec![],
            vec![reply(0x0122, &profile_reply(&[original]))],
        )]);
        let mut stale_session = Session::new();
        stale_session.support = vec![0, 0, 0, 0, 4];
        stale_session.model =
            Some(crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap());
        stale_session.eq_profiles = vec![changed_layout.clone()];
        assert!(stale_session
            .apply_setting(
                &mut stale_connection,
                &AtomicBool::new(false),
                SettingChange::OnePlusEqProfile(crate::oneplus_eq::EqProfileChange {
                    action: 2,
                    profile: changed_layout
                }),
            )
            .is_err());
        assert_eq!(
            stale_connection
                .writes
                .iter()
                .map(|packet| packet.command)
                .collect::<Vec<_>>(),
            [0x0122]
        );
    }
    #[test]
    fn custom_eq_create_uses_model_template_and_requires_new_profile_readback() {
        let model = crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap();
        let template = eq_template(model).unwrap();
        assert_eq!(template.frequencies, [62, 250, 1000, 4000, 8000, 16000]);
        let requested = crate::oneplus_eq::EqProfile {
            selected: 1,
            min_value: -6,
            max_value: 6,
            eq_id: 0,
            name: "Warm".into(),
            frequencies: template.frequencies,
            db_values: vec![0, 1, 2, 0, -1, -2],
        };
        let assigned = crate::oneplus_eq::EqProfile {
            eq_id: 7,
            ..requested.clone()
        };
        let payload = crate::oneplus_eq::encode_set_profile(&requested, 1).unwrap();
        let mut connection = ScriptedConnection::new(vec![
            (0x0122, vec![], vec![reply(0x0122, &[0, 0])]),
            (0x0418, payload.clone(), vec![reply(0x0418, &[0, 7])]),
            (
                0x0122,
                vec![],
                vec![reply(0x0122, &profile_reply(&[assigned.clone()]))],
            ),
        ]);
        let mut session = Session::new();
        session.support = vec![0, 0, 0, 0, 4];
        session.model = Some(model);
        let confirmation = session
            .apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::OnePlusEqProfile(crate::oneplus_eq::EqProfileChange {
                    action: 1,
                    profile: requested,
                }),
            )
            .unwrap();
        let SettingConfirmation::OnePlusEqProfiles(profiles) = confirmation else {
            panic!("expected new profile list");
        };
        assert_eq!(profiles, vec![assigned.clone()]);
        assert!(connection.script.is_empty());
        assert_eq!(
            connection
                .writes
                .iter()
                .map(|packet| (packet.command, packet.data.clone()))
                .collect::<Vec<_>>(),
            [(0x0122, vec![]), (0x0418, payload), (0x0122, vec![]),]
        );
        assert_eq!(
            session.metadata["oneplusEqualizerProfiles"],
            serde_json::json!([assigned])
        );

        let unsupported = crate::oneplus_models::by_name("OnePlus Buds Z").unwrap();
        assert!(eq_template(unsupported).is_none());
        let mut unsupported_session = Session::new();
        unsupported_session.support = vec![0, 0, 0, 0, 4];
        unsupported_session.model = Some(unsupported);
        let mut no_io = ScriptedConnection::new(vec![]);
        assert!(unsupported_session
            .apply_setting(
                &mut no_io,
                &AtomicBool::new(false),
                SettingChange::OnePlusEqProfile(crate::oneplus_eq::EqProfileChange {
                    action: 1,
                    profile: crate::oneplus_eq::EqProfile {
                        selected: 1,
                        min_value: -6,
                        max_value: 6,
                        eq_id: 0,
                        name: "x".into(),
                        frequencies: vec![62, 250, 1000, 4000, 8000, 16000],
                        db_values: vec![0; 6]
                    }
                }),
            )
            .is_err());
        assert!(no_io.writes.is_empty());
    }
    #[test]
    fn discovery_publishes_custom_eq_template_after_live_profile_query() {
        let model = crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap();
        let mut product_bytes = Vec::new();
        for offset in [4, 2, 0] {
            product_bytes
                .push(u8::from_str_radix(&model.product_id[offset..offset + 2], 16).unwrap());
        }
        let mut connection = ScriptedConnection::new(vec![
            (
                0x0103,
                vec![],
                vec![reply(
                    0x0103,
                    &[0, product_bytes[0], product_bytes[1], product_bytes[2]],
                )],
            ),
            (0x0100, vec![], vec![reply(0x0100, &[0, 0, 0, 0, 0, 4])]),
            (0x0122, vec![], vec![reply(0x0122, &[0, 0])]),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, session) = read_basics(&mut connection, &AtomicBool::new(false)).unwrap();
        assert!(info
            .controls
            .capabilities
            .contains(&"oneplusEqProfile".into()));
        let metadata = session.metadata();
        assert_eq!(metadata["oneplusEqProfile"], serde_json::json!(true));
        assert_eq!(
            metadata["oneplusEqTemplate"]["selected"],
            serde_json::json!(1)
        );
        assert_eq!(
            metadata["oneplusEqTemplate"]["frequencies"],
            serde_json::json!([62, 250, 1000, 4000, 8000, 16000])
        );
        assert_eq!(metadata["oneplusEqualizerProfiles"], serde_json::json!([]));
    }
    #[test]
    fn model_specific_game_feature_40_uses_registry_for_set_and_readback() {
        let model = crate::oneplus_models::by_name("OnePlus Flow Buds").unwrap();
        assert_eq!(model.game_feature, Some(40));
        let support = [0x80];
        let query = switch_query(model, &support);
        let mut connection = ScriptedConnection::new(vec![
            (0x0403, vec![40, 1], vec![reply(0x0403, &[0])]),
            (
                0x010d,
                query.clone(),
                vec![reply(0x010d, &[0, 2, 4, 0, 40, 1])],
            ),
        ]);
        let mut session = Session::new();
        session.model = Some(model);
        session.support = support.to_vec();
        let control = toggle_control(
            "gameMode",
            "Game mode",
            "Toggle the model-listed game feature.",
            false,
        );
        session.values.insert("gameMode".into(), control.value);
        session
            .controls
            .insert("gameMode".into(), control.descriptor);
        let confirmation = session
            .apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                    key: "gameMode".into(),
                    value: ControlValue::Boolean(true),
                }),
            )
            .unwrap();
        let SettingConfirmation::DeviceControl(confirmation) = confirmation else {
            panic!("expected game mode confirmation");
        };
        assert_eq!(confirmation.confirmed, ControlValue::Boolean(true));
        assert_eq!(
            connection
                .writes
                .iter()
                .map(|packet| (packet.command, packet.data.clone()))
                .collect::<Vec<_>>(),
            [(0x0403, vec![40, 1]), (0x010d, query),]
        );
    }
    #[test]
    fn game_feature_bit_41_does_not_promote_feature_6_and_keeps_unlisted_models_hidden() {
        let model = crate::oneplus_models::by_name("OnePlus Nord Buds 3 Pro").unwrap();
        assert_eq!(model.game_feature, Some(6));
        let support = [0x80, 0, 0, 0, 0, 0x02]; // command 1027 plus advertised command 1059
        assert_eq!(switch_query(model, &support), [3, 4, 6, 29]);
        let mut product_bytes = Vec::new();
        for offset in [4, 2, 0] {
            product_bytes
                .push(u8::from_str_radix(&model.product_id[offset..offset + 2], 16).unwrap());
        }
        let mut connection = ScriptedConnection::new(vec![
            (
                0x0103,
                vec![],
                vec![reply(
                    0x0103,
                    &[0, product_bytes[0], product_bytes[1], product_bytes[2]],
                )],
            ),
            (
                0x0100,
                vec![],
                vec![reply(
                    0x0100,
                    &[
                        0, support[0], support[1], support[2], support[3], support[4], support[5],
                    ],
                )],
            ),
            (
                0x010d,
                vec![3, 4, 6, 29],
                vec![reply(0x010d, &[0, 3, 4, 1, 6, 0, 29, 0])],
            ),
            (0x010d, vec![1, 27], vec![reply(0x010d, &[0, 0])]),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
            (0x0403, vec![6, 1], vec![reply(0x0403, &[0])]),
            (
                0x010d,
                vec![3, 4, 6, 29],
                vec![reply(0x010d, &[0, 3, 4, 1, 6, 1, 29, 0])],
            ),
        ]);
        let (info, mut session) = read_basics(&mut connection, &AtomicBool::new(false)).unwrap();
        assert!(info
            .controls
            .device_controls
            .iter()
            .any(|control| control.descriptor.key == "gameMode"));
        session
            .apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                    key: "gameMode".into(),
                    value: ControlValue::Boolean(true),
                }),
            )
            .unwrap();
        assert!(connection.script.is_empty());
        assert!(connection
            .writes
            .iter()
            .any(|packet| packet.command == 0x0403 && packet.data == [6, 1]));

        let no_game = crate::oneplus_models::by_name("OnePlus Buds").unwrap();
        assert_eq!(no_game.game_feature, None);
        assert_eq!(selected_game_feature(no_game, &support), None);
        let mut product_bytes = Vec::new();
        for offset in [4, 2, 0] {
            product_bytes
                .push(u8::from_str_radix(&no_game.product_id[offset..offset + 2], 16).unwrap());
        }
        let mut no_game_connection = ScriptedConnection::new(vec![
            (
                0x0103,
                vec![],
                vec![reply(
                    0x0103,
                    &[0, product_bytes[0], product_bytes[1], product_bytes[2]],
                )],
            ),
            (
                0x0100,
                vec![],
                vec![reply(
                    0x0100,
                    &[
                        0, support[0], support[1], support[2], support[3], support[4], support[5],
                    ],
                )],
            ),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, session) =
            read_basics(&mut no_game_connection, &AtomicBool::new(false)).unwrap();
        assert!(!info
            .controls
            .device_controls
            .iter()
            .any(|control| control.descriptor.key == "gameMode"));
        assert!(!session.controls.contains_key("gameMode"));
        assert!(no_game_connection
            .writes
            .iter()
            .all(|packet| packet.command != 0x010d));
    }
    #[test]
    fn rejected_optional_eq_get_warns_but_transport_desync_fails_discovery() {
        let model = crate::oneplus_models::by_name("OnePlus Buds Pro 3").unwrap();
        let mut product_bytes = Vec::new();
        for offset in [4, 2, 0] {
            product_bytes
                .push(u8::from_str_radix(&model.product_id[offset..offset + 2], 16).unwrap());
        }
        let mut rejected = ScriptedConnection::new(vec![
            (
                0x0103,
                vec![],
                vec![reply(
                    0x0103,
                    &[0, product_bytes[0], product_bytes[1], product_bytes[2]],
                )],
            ),
            (0x0100, vec![], vec![reply(0x0100, &[0, 0, 4])]),
            (0x010f, vec![], vec![reply(0x010f, &[3])]),
            (0x0200, vec![], vec![reply(0x0200, &[0, 0])]),
        ]);
        let (info, _) = read_basics(&mut rejected, &AtomicBool::new(false)).unwrap();
        assert!(info.controls.device_controls.is_empty());
        assert!(info
            .warnings
            .iter()
            .any(|warning| warning.contains("status 3")));

        let mut desynchronized = ScriptedConnection::new(vec![
            (
                0x0103,
                vec![],
                vec![reply(
                    0x0103,
                    &[0, product_bytes[0], product_bytes[1], product_bytes[2]],
                )],
            ),
            (0x0100, vec![], vec![reply(0x0100, &[0, 0, 4])]),
            (0x010f, vec![], vec![]),
        ]);
        assert!(read_basics(&mut desynchronized, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn exchange_requires_both_command_and_sequence_and_rejects_status() {
        let mut wrong_sequence = reply(0x0100, &[0, 99]);
        wrong_sequence.sequence = 1;
        let mut connection = ScriptedConnection::new(vec![
            (
                0x0100,
                vec![],
                vec![
                    wrong_sequence,
                    reply(0x010f, &[0, 3]),
                    reply(0x0100, &[0, 42]),
                ],
            ),
            (0x0404, vec![1, 1, 2], vec![reply(0x0404, &[5])]),
        ]);
        let mut session = noise_session();
        assert_eq!(
            session
                .exchange(&mut connection, &AtomicBool::new(false), 0x0100, &[])
                .unwrap(),
            [0, 42]
        );
        assert!(session
            .set_noise(&mut connection, &AtomicBool::new(false), 1)
            .unwrap_err()
            .contains("status 5"));
        assert_eq!(connection.writes.len(), 2);
    }
    #[test]
    fn accepted_noise_waits_for_matching_readback_and_never_restores_on_disconnect() {
        let mut connection = ScriptedConnection::new(vec![
            (0x0404, vec![1, 1, 2], vec![reply(0x0404, &[0])]),
            (0x010c, vec![1, 1], vec![reply(0x010c, &[0, 1, 1, 1])]),
            (0x010c, vec![1, 1], vec![reply(0x010c, &[0, 1, 1, 2])]),
            (0x0404, vec![1, 1, 4], vec![reply(0x0404, &[0])]),
            (0x010c, vec![1, 1], vec![]),
        ]);
        let mut session = noise_session();
        assert_eq!(
            session
                .set_noise(&mut connection, &AtomicBool::new(false), 1)
                .unwrap()
                .mode,
            "cancelling"
        );
        assert!(session
            .set_noise(&mut connection, &AtomicBool::new(false), 2)
            .is_err());
        assert!(!session.is_synchronized());
        assert_eq!(
            connection
                .writes
                .iter()
                .filter(|packet| packet.command == 0x0404)
                .count(),
            2
        );
        assert!(connection.script.is_empty());
    }
    #[test]
    fn feature_switch_set_is_confirmed_only_by_exact_boolean_readback() {
        for (key, feature, readback, should_confirm) in [
            ("wearDetection", 4, [0, 3, 4, 1, 6, 0, 29, 0], true),
            ("wearDetection", 4, [0, 3, 4, 0, 6, 0, 29, 0], false),
            ("gameMode", 6, [0, 3, 4, 0, 6, 1, 29, 0], true),
            ("gameMode", 6, [0, 3, 4, 0, 6, 0, 29, 0], false),
        ] {
            let mut connection = ScriptedConnection::new(vec![
                (0x0403, vec![feature, 1], vec![reply(0x0403, &[0])]),
                (0x010d, vec![3, 4, 6, 29], vec![reply(0x010d, &readback)]),
            ]);
            let mut session = Session::new();
            session.model = Some(crate::oneplus_models::by_name(MODEL).unwrap());
            session.support = vec![0x80];
            let control = toggle_control(key, key, "Toggle the feature switch.", false);
            session.values.insert(key.into(), control.value.clone());
            session.controls.insert(key.into(), control.descriptor);
            let result = session.apply_setting(
                &mut connection,
                &AtomicBool::new(false),
                SettingChange::DeviceControl(crate::device_controls::DeviceControlChange {
                    key: key.into(),
                    value: ControlValue::Boolean(true),
                }),
            );
            if should_confirm {
                let SettingConfirmation::DeviceControl(confirmation) = result.unwrap() else {
                    panic!("expected confirmed control");
                };
                assert_eq!(confirmation.before, ControlValue::Boolean(false));
                assert_eq!(confirmation.confirmed, ControlValue::Boolean(true));
                assert_eq!(session.values.get(key), Some(&ControlValue::Boolean(true)));
            } else {
                assert!(result.unwrap_err().contains("readback did not match"));
                assert_eq!(session.values.get(key), Some(&ControlValue::Boolean(false)));
            }
            assert_eq!(
                connection
                    .writes
                    .iter()
                    .map(|packet| (packet.command, packet.data.clone()))
                    .collect::<Vec<_>>(),
                [(0x0403, vec![feature, 1]), (0x010d, vec![3, 4, 6, 29]),]
            );
            assert!(
                connection.script.is_empty(),
                "mismatch must not trigger a restoration SET"
            );
        }
    }
    #[test]
    fn notifications_and_matched_noise_readback_keep_wire_order() {
        let mut session = noise_session();
        session.expected = Some((0x010c, 9));
        let old = Packet {
            command: 0x0204,
            sequence: 0,
            data: vec![3, 1, 1, 1],
        };
        let new = Packet {
            command: 0x810c,
            sequence: 9,
            data: vec![0, 1, 1, 2],
        };
        let mut wire = encode(&old).unwrap();
        wire.extend(encode(&new).unwrap());
        session.feed(&wire);
        assert_eq!(
            session.drain_noise_observations(),
            [vec![1, 1, 1], vec![1, 1, 2]]
        );
        let mut wire = encode(&new).unwrap();
        wire.extend(encode(&old).unwrap());
        session.feed(&wire);
        assert_eq!(
            session.drain_noise_observations(),
            [vec![1, 1, 2], vec![1, 1, 1]]
        );
    }
    #[test]
    fn cancelled_request_performs_no_io() {
        let mut connection = ScriptedConnection::new(vec![]);
        assert!(Session::new()
            .exchange(&mut connection, &AtomicBool::new(true), 0x0103, &[])
            .is_err());
        assert!(connection.writes.is_empty());
    }
}
