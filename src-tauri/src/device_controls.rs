//! Typed contract shared by device discovery, settings IPC, and the React view.
//!
//! A control is included in a snapshot only after its descriptor, current value,
//! and device-specific capability have all been decoded from a matching reply.
//! This module deliberately contains no guessed Sony command IDs.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ControlValue {
    Boolean(bool),
    Integer(i64),
    Text(String),
}

impl ControlValue {
    pub fn as_json(&self) -> Value {
        serde_json::to_value(self).expect("ControlValue is always JSON serializable")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlOption {
    pub value: ControlValue,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlKind {
    Toggle,
    Select,
    Range,
}

/// JSON shape consumed by `DeviceControlDescriptor` in `src/bridge.ts`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlDescriptor {
    pub key: String,
    pub label: String,
    pub description: String,
    pub kind: ControlKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<ControlOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmedControl {
    pub descriptor: ControlDescriptor,
    pub value: ControlValue,
}

impl ConfirmedControl {
    /// Reject malformed descriptors and values before they reach the UI.
    pub fn validate(&self) -> Result<(), String> {
        let d = &self.descriptor;
        if d.key.is_empty()
            || d.key.len() > 64
            || !d
                .key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            || d.label.trim().is_empty()
            || d.label.len() > 96
            || d.description.len() > 240
        {
            return Err("Device setting descriptor has invalid text fields.".into());
        }

        match d.kind {
            ControlKind::Toggle => {
                if !matches!(self.value, ControlValue::Boolean(_))
                    || d.options.is_some()
                    || d.min.is_some()
                    || d.max.is_some()
                    || d.step.is_some()
                {
                    return Err("Toggle control has an invalid value or range.".into());
                }
            }
            ControlKind::Select => {
                let Some(options) = d.options.as_ref() else {
                    return Err("Select control has no options.".into());
                };
                if options.is_empty()
                    || options.len() > 32
                    || !options.iter().any(|option| option.value == self.value)
                    || d.min.is_some()
                    || d.max.is_some()
                    || d.step.is_some()
                {
                    return Err("Select control options do not contain its current value.".into());
                }
                let mut seen = Vec::new();
                for option in options {
                    if option.label.trim().is_empty()
                        || option.label.len() > 64
                        || seen.contains(&option.value)
                    {
                        return Err("Select control has invalid or duplicate options.".into());
                    }
                    seen.push(option.value.clone());
                }
            }
            ControlKind::Range => {
                let (Some(min), Some(max), Some(step)) = (d.min, d.max, d.step) else {
                    return Err("Range control has incomplete device-reported bounds.".into());
                };
                let ControlValue::Integer(value) = &self.value else {
                    return Err("Range control value must be an integer.".into());
                };
                if min > max
                    || step <= 0
                    || *value < min
                    || *value > max
                    || (*value).checked_sub(min).is_none_or(|offset| offset % step != 0)
                    || d.options.is_some()
                {
                    return Err("Range control value is outside the device-reported range.".into());
                }
            }
        }
        Ok(())
    }
}

/// A request from the UI. It is not authoritative until a matching GET or
/// final-state notification confirms the new value.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceControlChange {
    pub key: String,
    pub value: ControlValue,
}

/// The confirmation sent after a SET has been matched to a final device state.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceControlConfirmation {
    pub key: String,
    pub before: ControlValue,
    pub confirmed: ControlValue,
}

/// Build the stable JSON contract stored under `settings` by native discovery.
/// Invalid or duplicate controls fail closed rather than being partially shown.
pub fn settings_map(controls: impl IntoIterator<Item = ConfirmedControl>) -> Result<BTreeMap<String, Value>, String> {
    let mut controls = controls.into_iter().collect::<Vec<_>>();
    if controls.len() > 64 {
        return Err("Device returned too many settings controls.".into());
    }
    controls.sort_by(|left, right| left.descriptor.key.cmp(&right.descriptor.key));
    let mut descriptors = Vec::with_capacity(controls.len());
    let mut settings = BTreeMap::new();
    for control in controls {
        control.validate()?;
        let key = control.descriptor.key.clone();
        if settings.insert(key.clone(), control.value.as_json()).is_some() {
            return Err("Device returned duplicate settings controls.".into());
        }
        let descriptor = serde_json::to_value(control.descriptor)
            .map_err(|_| "Device settings descriptor could not be serialized.")?;
        descriptors.push(descriptor);
    }
    // Keep the descriptor array in the settings map without exposing an
    // alternate source of truth for each setting's confirmed value.
    settings.insert("controlDescriptors".into(), Value::Array(descriptors));
    Ok(settings)
}

/// Validate an incoming UI value against the last device-confirmed descriptor.
pub fn validate_change(
    descriptor: &ControlDescriptor,
    value: ControlValue,
) -> Result<DeviceControlChange, String> {
    let candidate = ConfirmedControl {
        descriptor: descriptor.clone(),
        value: value.clone(),
    };
    candidate.validate()?;
    Ok(DeviceControlChange {
        key: descriptor.key.clone(),
        value,
    })
}

/// Parse a JSON descriptor received back from the frontend snapshot.
pub fn descriptor_from_json(value: &Value) -> Result<ControlDescriptor, String> {
    if !value.is_object() {
        return Err("Device setting descriptor is not an object.".into());
    }
    serde_json::from_value(value.clone())
        .map_err(|_| "Device setting descriptor has an invalid schema.".into())
}
