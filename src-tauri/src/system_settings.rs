//! Sony MDR V2 "system" settings decoded from the shipped Sound Connect 13.3.0 APK (static,
//! read-only analysis; see docs/research/v2-system-settings.md). Pure request builders and
//! strict response parsers. Nothing here talks to a device.
//!
//! * Wear detection ("Pause when headphones are removed"), function 0xF1: GET_STATUS
//!   `[F2,01]` -> `[F3,01,enable]`, GET_PARAM `[F6,01]` -> `[F7,01,on]`, SET_PARAM `[F8,01,on]`,
//!   NTFY_PARAM `[F9,01,on]`. `enable` is 0=ENABLE/1=DISABLE and `on` is 0=ON/1=OFF.
//! * General settings 1-4 (functions 0xD1-0xD4, inquiries 0xD1-0xD4): the headphone describes each
//!   setting itself (a boolean or a list, with a title and option names), so touch-panel control,
//!   multipoint, voice guidance and similar settings all share this one protocol.

use crate::device_controls::{ControlDescriptor, ControlKind, ControlOption, ControlValue};

pub const WEAR_FUNCTION: u8 = 0xf1;
const WEAR_INQUIRY: u8 = 0x01;
const ENGLISH: u8 = 0x01;

pub fn wear_status_request() -> Vec<u8> {
    vec![0xf2, WEAR_INQUIRY]
}
pub fn wear_param_request() -> Vec<u8> {
    vec![0xf6, WEAR_INQUIRY]
}
pub fn wear_set_request(on: bool) -> Vec<u8> {
    vec![0xf8, WEAR_INQUIRY, u8::from(!on)]
}
/// `[command, 01, 0=ENABLE | 1=DISABLE]`; command 0xF3 (reply) or 0xF5 (notification).
pub fn parse_wear_status(payload: &[u8], command: u8) -> Option<bool> {
    match payload {
        [c, WEAR_INQUIRY, value @ (0 | 1)] if *c == command => Some(*value == 0),
        _ => None,
    }
}
/// `[command, 01, 0=ON | 1=OFF]`; command 0xF7 (reply) or 0xF9 (notification).
pub fn parse_wear_param(payload: &[u8], command: u8) -> Option<bool> {
    match payload {
        [c, WEAR_INQUIRY, value @ (0 | 1)] if *c == command => Some(*value == 0),
        _ => None,
    }
}
pub fn wear_descriptor() -> ControlDescriptor {
    ControlDescriptor {
        key: "wear".into(),
        label: "Pause when headphones are removed".into(),
        description: "Pause playback when you take the headphones off and resume it when you put them back on.".into(),
        kind: ControlKind::Toggle,
        options: None,
        min: None,
        max: None,
        step: None,
    }
}

/// Function IDs 0xD1..=0xD4 advertise general settings 1..=4.
pub fn general_setting_inquiry(slot: u8) -> Option<u8> {
    (1..=4).contains(&slot).then(|| 0xd0 + slot)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GsKind {
    Boolean,
    List,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GsFormat {
    /// The text is shown as sent by the headphone.
    Raw,
    /// The text is one of Sony's enum names (for example `TOUCH_PANEL_SETTING`).
    Enum,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GsElement {
    pub format: GsFormat,
    pub subject: String,
    pub summary: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GsCapability {
    pub kind: GsKind,
    pub format: GsFormat,
    pub title: String,
    pub summary: String,
    pub elements: Vec<GsElement>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GsValue {
    Boolean(bool),
    List(u8),
}

pub fn gs_capability_request(inquiry: u8) -> Vec<u8> {
    vec![0xd0, inquiry, ENGLISH]
}
pub fn gs_status_request(inquiry: u8) -> Vec<u8> {
    vec![0xd2, inquiry]
}
pub fn gs_param_request(inquiry: u8) -> Vec<u8> {
    vec![0xd6, inquiry]
}
pub fn gs_set_request(inquiry: u8, value: GsValue) -> Vec<u8> {
    match value {
        GsValue::Boolean(on) => vec![0xd8, inquiry, 0, u8::from(!on)],
        GsValue::List(index) => vec![0xd8, inquiry, 1, index],
    }
}

fn format_of(byte: u8) -> Option<GsFormat> {
    match byte {
        0 => Some(GsFormat::Raw),
        1 => Some(GsFormat::Enum),
        _ => None,
    }
}
/// Length-prefixed UTF-8 text at `at`; returns the text and the index after it.
fn text_at(payload: &[u8], at: usize) -> Option<(String, usize)> {
    let length = usize::from(*payload.get(at)?);
    let end = at + 1 + length;
    let text = String::from_utf8(payload.get(at + 1..end)?.to_vec()).ok()?;
    Some((text, end))
}

/// RET_CAPABILITY `[D1, inquiry, type, format, titleLen, title, summaryLen, summary, (list: count, elements...)]`.
/// A list element is `[format, subjectLen, subject, summaryLen, summary]`, one to 64 of them.
pub fn parse_gs_capability(payload: &[u8], inquiry: u8) -> Option<GsCapability> {
    if payload.len() < 5 || payload[0] != 0xd1 || payload[1] != inquiry {
        return None;
    }
    let kind = match payload[2] {
        0 => GsKind::Boolean,
        1 => GsKind::List,
        _ => return None,
    };
    let format = format_of(payload[3])?;
    let (title, next) = text_at(payload, 4)?;
    let (summary, next) = text_at(payload, next)?;
    let mut elements = Vec::new();
    let mut end = next;
    if kind == GsKind::List {
        let count = usize::from(*payload.get(next)?);
        if !(1..=64).contains(&count) {
            return None;
        }
        end = next + 1;
        for _ in 0..count {
            let element_format = format_of(*payload.get(end)?)?;
            let (subject, after_subject) = text_at(payload, end + 1)?;
            let (element_summary, after_summary) = text_at(payload, after_subject)?;
            elements.push(GsElement { format: element_format, subject, summary: element_summary });
            end = after_summary;
        }
    }
    (end == payload.len()).then_some(GsCapability { kind, format, title, summary, elements })
}
/// RET_STATUS (0xD3) or NTFY_STATUS (0xD5): `[command, inquiry, 0=ENABLE | 1=DISABLE]`.
pub fn parse_gs_status(payload: &[u8], command: u8, inquiry: u8) -> Option<bool> {
    match payload {
        [c, i, value @ (0 | 1)] if *c == command && *i == inquiry => Some(*value == 0),
        _ => None,
    }
}
/// RET_PARAM (0xD7) or NTFY_PARAM (0xD9): `[command, inquiry, 0, 0=ON | 1=OFF]` or `[command, inquiry, 1, index<64]`.
pub fn parse_gs_param(payload: &[u8], command: u8, inquiry: u8) -> Option<GsValue> {
    match payload {
        [c, i, 0, value @ (0 | 1)] if *c == command && *i == inquiry => Some(GsValue::Boolean(*value == 0)),
        [c, i, 1, index] if *c == command && *i == inquiry && *index < 64 => Some(GsValue::List(*index)),
        _ => None,
    }
}

/// Stable key, label and description for the settings Sony's app names. Unknown enum titles and
/// device-supplied text fall back to a slot-based key and the text itself.
pub fn gs_identity(slot: u8, capability: &GsCapability) -> (String, String, String) {
    let fallback_key = format!("gs{slot}");
    if capability.format == GsFormat::Raw {
        let label = if capability.title.trim().is_empty() { format!("Headphone setting {slot}") } else { capability.title.clone() };
        return (fallback_key, label, capability.summary.clone());
    }
    let known = match capability.title.as_str() {
        "TOUCH_PANEL_SETTING" => Some(("touch", "Touch sensor control panel", "Control playback and calls with touch gestures on the earcup.")),
        "MULTIPOINT_SETTING" => Some(("multipoint", "Multipoint connection", "Stay connected to two devices at the same time.")),
        "VOICE_GUIDANCE_SETTING" => Some(("guidance", "Voice guidance", "Choose the headphone's voice guidance setting.")),
        "FACETAP_SETTING" | "SELECTABLE_FACETAP_SETTING" => Some(("faceTap", "Face tap", "Tap the headphone to control playback.")),
        "SIDETONE_SETTING" => Some(("sidetone", "Sidetone", "Hear your own voice during calls.")),
        "TWS_ONE_SIDE_USE_NCASM_SETTING" => Some(("oneSideAmbient", "Single earbud ambient control", "Choose how noise control behaves when one earbud is used.")),
        "TAP_SENSITIVITY_SETTING" => Some(("tapSensitivity", "Tap sensitivity", "How firmly you need to tap the headphone.")),
        title if title.starts_with("ASSIGNABLE_KEY_SETTING") => Some((fallback_key.as_str(), "Button assignment", "Choose what the assignable button does.")),
        _ => None,
    };
    match known {
        Some((key, label, description)) => (key.into(), label.into(), description.into()),
        None => (fallback_key, humanize(&capability.title), capability.summary.clone()),
    }
}

fn humanize(name: &str) -> String {
    let mut words = name.trim_end_matches("_SETTING").split('_').filter(|word| !word.is_empty());
    let first = words.next().map(|word| {
        let mut chars = word.chars();
        chars.next().map(|c| c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()).unwrap_or_default()
    });
    first.into_iter().chain(words.map(|word| word.to_lowercase())).collect::<Vec<_>>().join(" ")
}

/// Option label for a list element: device text as sent, or a readable name for Sony's enum names.
pub fn gs_element_label(element: &GsElement) -> String {
    if element.format == GsFormat::Raw {
        return element.subject.clone();
    }
    let name = element.subject.as_str();
    match name {
        "ASSIGNABLE_KEY_ELEM_NCASM" => return "Noise control / Ambient sound".into(),
        "ASSIGNABLE_KEY_ELEM_NC" => return "Noise control".into(),
        "ASSIGNABLE_KEY_ELEM_VR" => return "Voice recognition".into(),
        "ASSIGNABLE_KEY_ELEM_PLAYBACK_CONTROL" => return "Playback control".into(),
        "ASSIGNABLE_KEY_ELEM_GOOGLE_ASSISTANT" => return "Google Assistant".into(),
        "ASSIGNABLE_KEY_ELEM_AMAZON_ALEXA" => return "Amazon Alexa".into(),
        "ASSIGNABLE_KEY_ELEM_TENCENT_XIAOWEI" => return "Tencent Xiaowei".into(),
        _ => {}
    }
    let suffix = name.split_once("_ELEM_").map_or(name, |(_, rest)| rest);
    if name.starts_with("VOICE_GUIDANCE_ELEM_") {
        return suffix
            .split('_')
            .map(|word| {
                let mut chars = word.chars();
                chars.next().map(|c| c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()).unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    humanize(suffix)
}

/// Editable control for a general setting. Booleans are toggles; lists are selects by element index.
pub fn gs_descriptor(slot: u8, capability: &GsCapability) -> ControlDescriptor {
    let (key, label, description) = gs_identity(slot, capability);
    match capability.kind {
        GsKind::Boolean => ControlDescriptor { key, label, description, kind: ControlKind::Toggle, options: None, min: None, max: None, step: None },
        GsKind::List => ControlDescriptor {
            key,
            label,
            description,
            kind: ControlKind::Select,
            options: Some(
                capability
                    .elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| ControlOption {
                        value: ControlValue::Integer(index as i64),
                        label: gs_element_label(element),
                        enabled: None,
                    })
                    .collect(),
            ),
            min: None,
            max: None,
            step: None,
        },
    }
}

pub fn gs_value_as_control(value: GsValue) -> ControlValue {
    match value {
        GsValue::Boolean(on) => ControlValue::Boolean(on),
        GsValue::List(index) => ControlValue::Integer(i64::from(index)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Vec<u8> {
        let mut bytes = vec![value.len() as u8];
        bytes.extend(value.as_bytes());
        bytes
    }
    fn boolean_capability(inquiry: u8, title: &str) -> Vec<u8> {
        let mut p = vec![0xd1, inquiry, 0, 1];
        p.extend(text(title));
        p.extend(text("summary"));
        p
    }
    fn list_capability(inquiry: u8, title: &str, elements: &[&str]) -> Vec<u8> {
        let mut p = vec![0xd1, inquiry, 1, 1];
        p.extend(text(title));
        p.extend(text(""));
        p.push(elements.len() as u8);
        for element in elements {
            p.push(1);
            p.extend(text(element));
            p.extend(text(""));
        }
        p
    }

    #[test]
    fn wear_requests_and_values_follow_the_apk_schema() {
        assert_eq!(wear_status_request(), [0xf2, 0x01]);
        assert_eq!(wear_param_request(), [0xf6, 0x01]);
        assert_eq!(wear_set_request(true), [0xf8, 0x01, 0]);
        assert_eq!(wear_set_request(false), [0xf8, 0x01, 1]);
        assert_eq!(parse_wear_status(&[0xf3, 1, 0], 0xf3), Some(true));
        assert_eq!(parse_wear_status(&[0xf3, 1, 1], 0xf3), Some(false));
        assert_eq!(parse_wear_param(&[0xf7, 1, 0], 0xf7), Some(true));
        assert_eq!(parse_wear_param(&[0xf9, 1, 1], 0xf9), Some(false));
    }

    #[test]
    fn wear_parsers_reject_wrong_command_inquiry_value_or_length() {
        assert_eq!(parse_wear_param(&[0xf7, 1, 0], 0xf9), None);
        assert_eq!(parse_wear_param(&[0xf7, 2, 0], 0xf7), None);
        assert_eq!(parse_wear_param(&[0xf7, 1, 2], 0xf7), None);
        assert_eq!(parse_wear_param(&[0xf7, 1, 0, 0], 0xf7), None);
        assert_eq!(parse_wear_status(&[0xf3, 1], 0xf3), None);
    }

    #[test]
    fn general_setting_requests_use_the_slot_inquiry() {
        assert_eq!(general_setting_inquiry(1), Some(0xd1));
        assert_eq!(general_setting_inquiry(4), Some(0xd4));
        assert_eq!(general_setting_inquiry(0), None);
        assert_eq!(general_setting_inquiry(5), None);
        assert_eq!(gs_capability_request(0xd2), [0xd0, 0xd2, 1]);
        assert_eq!(gs_status_request(0xd2), [0xd2, 0xd2]);
        assert_eq!(gs_param_request(0xd2), [0xd6, 0xd2]);
        assert_eq!(gs_set_request(0xd2, GsValue::Boolean(true)), [0xd8, 0xd2, 0, 0]);
        assert_eq!(gs_set_request(0xd2, GsValue::Boolean(false)), [0xd8, 0xd2, 0, 1]);
        assert_eq!(gs_set_request(0xd2, GsValue::List(3)), [0xd8, 0xd2, 1, 3]);
    }

    #[test]
    fn boolean_capability_names_the_touch_panel() {
        let capability = parse_gs_capability(&boolean_capability(0xd1, "TOUCH_PANEL_SETTING"), 0xd1).unwrap();
        assert_eq!(capability.kind, GsKind::Boolean);
        let descriptor = gs_descriptor(1, &capability);
        assert_eq!((descriptor.key.as_str(), descriptor.label.as_str()), ("touch", "Touch sensor control panel"));
        assert_eq!(descriptor.kind, ControlKind::Toggle);
    }

    #[test]
    fn list_capability_decodes_options_and_labels_voice_guidance_languages() {
        let bytes = list_capability(0xd2, "VOICE_GUIDANCE_SETTING", &["VOICE_GUIDANCE_ELEM_ENGLISH", "VOICE_GUIDANCE_ELEM_BRAZILIAN_PORTUGUESE"]);
        let capability = parse_gs_capability(&bytes, 0xd2).unwrap();
        let descriptor = gs_descriptor(2, &capability);
        assert_eq!(descriptor.key, "guidance");
        let labels: Vec<_> = descriptor.options.unwrap().into_iter().map(|option| option.label).collect();
        assert_eq!(labels, ["English", "Brazilian Portuguese"]);
    }

    #[test]
    fn device_supplied_text_and_unknown_titles_keep_their_own_wording() {
        let mut raw = vec![0xd1, 0xd3, 0, 0];
        raw.extend(text("Wind noise reduction"));
        raw.extend(text("Reduce wind noise."));
        let capability = parse_gs_capability(&raw, 0xd3).unwrap();
        let (key, label, description) = gs_identity(3, &capability);
        assert_eq!((key.as_str(), label.as_str(), description.as_str()), ("gs3", "Wind noise reduction", "Reduce wind noise."));
        let unknown = parse_gs_capability(&boolean_capability(0xd4, "SOME_NEW_SETTING"), 0xd4).unwrap();
        assert_eq!(gs_identity(4, &unknown).1, "Some new");
    }

    #[test]
    fn multipoint_and_assignable_button_titles_are_recognised() {
        let multipoint = parse_gs_capability(&boolean_capability(0xd1, "MULTIPOINT_SETTING"), 0xd1).unwrap();
        assert_eq!(gs_identity(1, &multipoint).0, "multipoint");
        let button = parse_gs_capability(&list_capability(0xd2, "ASSIGNABLE_KEY_SETTING_NCAMB", &["ASSIGNABLE_KEY_ELEM_NCASM", "ASSIGNABLE_KEY_ELEM_PLAYBACK_CONTROL"]), 0xd2).unwrap();
        let descriptor = gs_descriptor(2, &button);
        assert_eq!((descriptor.key.as_str(), descriptor.label.as_str()), ("gs2", "Button assignment"));
        assert_eq!(descriptor.options.unwrap()[0].label, "Noise control / Ambient sound");
    }

    #[test]
    fn capability_parser_is_strict() {
        let good = list_capability(0xd1, "TOUCH_PANEL_SETTING", &["A", "B"]);
        assert!(parse_gs_capability(&good, 0xd1).is_some());
        assert!(parse_gs_capability(&good, 0xd2).is_none(), "wrong inquiry");
        let mut trailing = good.clone();
        trailing.push(0);
        assert!(parse_gs_capability(&trailing, 0xd1).is_none(), "trailing byte");
        assert!(parse_gs_capability(&good[..good.len() - 1], 0xd1).is_none(), "truncated");
        let mut zero_elements = list_capability(0xd1, "T", &[]);
        assert!(parse_gs_capability(&zero_elements, 0xd1).is_none(), "list needs 1..=64 elements");
        zero_elements[2] = 2;
        assert!(parse_gs_capability(&zero_elements, 0xd1).is_none(), "unknown type");
        let mut bad_text = boolean_capability(0xd1, "TOUCH_PANEL_SETTING");
        bad_text[5] = 0xff;
        assert!(parse_gs_capability(&bad_text, 0xd1).is_none(), "invalid UTF-8");
        assert!(parse_gs_capability(&[0xd1, 0xd1, 0, 1], 0xd1).is_none(), "too short");
    }

    #[test]
    fn status_and_param_replies_are_matched_by_command_inquiry_and_type() {
        assert_eq!(parse_gs_status(&[0xd3, 0xd1, 0], 0xd3, 0xd1), Some(true));
        assert_eq!(parse_gs_status(&[0xd5, 0xd1, 1], 0xd5, 0xd1), Some(false));
        assert_eq!(parse_gs_status(&[0xd3, 0xd2, 0], 0xd3, 0xd1), None);
        assert_eq!(parse_gs_param(&[0xd7, 0xd1, 0, 0], 0xd7, 0xd1), Some(GsValue::Boolean(true)));
        assert_eq!(parse_gs_param(&[0xd9, 0xd1, 0, 1], 0xd9, 0xd1), Some(GsValue::Boolean(false)));
        assert_eq!(parse_gs_param(&[0xd7, 0xd1, 1, 5], 0xd7, 0xd1), Some(GsValue::List(5)));
        assert_eq!(parse_gs_param(&[0xd7, 0xd1, 1, 64], 0xd7, 0xd1), None);
        assert_eq!(parse_gs_param(&[0xd7, 0xd1, 0, 2], 0xd7, 0xd1), None);
        assert_eq!(parse_gs_param(&[0xd7, 0xd1, 2, 0], 0xd7, 0xd1), None);
        assert_eq!(parse_gs_param(&[0xd7, 0xd1, 0], 0xd7, 0xd1), None);
    }
}
