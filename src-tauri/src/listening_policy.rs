//! Maps a detected content category to the settings a connected device actually supports.
//! Pure: it reads only the device's confirmed capabilities, control descriptors and values.

use crate::Device;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Movie,
    Music,
    Game,
    Other,
}

impl Category {
    pub fn key(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Music => "music",
            Self::Game => "game",
            Self::Other => "other",
        }
    }
    pub fn from_kind(kind: &str) -> Self {
        match kind {
            "movie" => Self::Movie,
            "music" => Self::Music,
            "game" => Self::Game,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub key: &'static str,
    pub value: Value,
}

fn has_capability(device: &Device, key: &str) -> bool {
    device.capabilities.iter().any(|capability| capability == key)
}

fn descriptor<'a>(device: &'a Device, key: &str) -> Option<&'a Value> {
    device
        .settings
        .get("controlDescriptors")?
        .as_array()?
        .iter()
        .find(|descriptor| descriptor.get("key").and_then(Value::as_str) == Some(key))
}

/// True when the device advertises `key` and, for select controls, offers `value` as an enabled option.
fn offers(device: &Device, key: &str, value: &Value) -> bool {
    if !has_capability(device, key) {
        return false;
    }
    let Some(descriptor) = descriptor(device, key) else {
        return false;
    };
    match descriptor.get("options").and_then(Value::as_array) {
        Some(options) => options.iter().any(|option| {
            option.get("value") == Some(value)
                && option.get("enabled").and_then(Value::as_bool) != Some(false)
        }),
        None => descriptor.get("kind").and_then(Value::as_str) == Some("toggle") && value.is_boolean(),
    }
}

fn push_if_offered(out: &mut Vec<Setting>, device: &Device, key: &'static str, value: Value) {
    if offers(device, key, &value) {
        out.push(Setting { key, value });
    }
}

/// Settings to apply for `category`, in order, using only what `device` has confirmed.
pub fn desired_settings(device: &Device, category: Category) -> Vec<Setting> {
    let mut out = Vec::new();
    if has_capability(device, "listeningMode") {
        let preferred = match category {
            Category::Movie => "cinema",
            Category::Music => "music",
            Category::Game => "game",
            Category::Other => "standard",
        };
        let mode = [preferred, "standard"]
            .into_iter()
            .find(|mode| offers(device, "listeningMode", &Value::from(*mode)));
        if let Some(mode) = mode {
            out.push(Setting { key: "listeningMode", value: Value::from(mode) });
        }
    }
    // OnePlus: head tracking (2) is never selected automatically.
    let spatial = match category {
        Category::Movie => 1,
        _ => 0,
    };
    push_if_offered(&mut out, device, "spatialAudio", Value::from(spatial));
    push_if_offered(&mut out, device, "gameMode", Value::from(category == Category::Game));
    if category == Category::Game {
        push_if_offered(&mut out, device, "gameSoundProfile", Value::from(3));
    }
    out
}

/// Short user-facing description of a planned setting.
pub fn describe(setting: &Setting) -> String {
    let on_off = |on: bool| if on { "on" } else { "off" };
    match (setting.key, &setting.value) {
        ("listeningMode", Value::String(mode)) => format!("Listening mode: {mode}"),
        ("spatialAudio", Value::Number(n)) => format!(
            "Spatial audio: {}",
            match n.as_i64() { Some(0) => "off", Some(1) => "fixed", _ => "head tracking" }
        ),
        ("gameMode", Value::Bool(on)) => format!("Game mode: {}", on_off(*on)),
        ("gameSoundProfile", _) => "Game sound profile".into(),
        (key, _) => key.into(),
    }
}

/// The subset of `desired_settings` the device has not already confirmed.
pub fn pending_settings(device: &Device, category: Category) -> Vec<Setting> {
    desired_settings(device, category)
        .into_iter()
        .filter(|setting| device.settings.get(setting.key) != Some(&setting.value))
        .collect()
}

/// Keys whose manual change should pause automation.
pub fn is_automated_key(key: &str) -> bool {
    matches!(key, "listeningMode" | "spatialAudio" | "gameMode" | "gameSoundProfile")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConnectionState;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn device(capabilities: &[&str], descriptors: Value, values: Value) -> Device {
        let mut settings = BTreeMap::new();
        settings.insert("controlDescriptors".to_string(), descriptors);
        if let Value::Object(map) = values {
            settings.extend(map);
        }
        Device {
            id: "d".into(),
            name: "d".into(),
            model: "m".into(),
            connection: ConnectionState::Connected,
            battery: None,
            firmware: None,
            capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
            settings,
        }
    }
    fn sony(modes: &[&str], current: &str) -> Device {
        let options: Vec<_> = modes.iter().map(|m| json!({"value": m, "label": m})).collect();
        device(
            &["listeningMode"],
            json!([{"key": "listeningMode", "kind": "select", "options": options}]),
            json!({"listeningMode": current}),
        )
    }
    fn oneplus(values: Value) -> Device {
        device(
            &["spatialAudio", "gameMode", "gameSoundProfile"],
            json!([
                {"key": "spatialAudio", "kind": "select", "options": [{"value": 0}, {"value": 1}, {"value": 2}]},
                {"key": "gameMode", "kind": "toggle"},
                {"key": "gameSoundProfile", "kind": "select", "options": [{"value": 0}, {"value": 3}]}
            ]),
            values,
        )
    }
    fn keys(settings: &[Setting]) -> Vec<(&'static str, Value)> {
        settings.iter().map(|s| (s.key, s.value.clone())).collect()
    }

    #[test]
    fn sony_upmix_series_maps_every_category() {
        let d = sony(&["standard", "cinema", "game", "music"], "standard");
        for (category, mode) in [
            (Category::Movie, "cinema"),
            (Category::Music, "music"),
            (Category::Game, "game"),
            (Category::Other, "standard"),
        ] {
            assert_eq!(keys(&desired_settings(&d, category)), vec![("listeningMode", json!(mode))]);
        }
    }

    #[test]
    fn sony_without_game_or_music_falls_back_to_standard() {
        let d = sony(&["standard", "cinema"], "cinema");
        assert_eq!(keys(&desired_settings(&d, Category::Game)), vec![("listeningMode", json!("standard"))]);
        assert_eq!(keys(&desired_settings(&d, Category::Music)), vec![("listeningMode", json!("standard"))]);
        assert_eq!(keys(&desired_settings(&d, Category::Movie)), vec![("listeningMode", json!("cinema"))]);
    }

    #[test]
    fn sony_without_any_offered_mode_sends_nothing() {
        let d = sony(&["cinema"], "cinema");
        assert!(desired_settings(&d, Category::Other).is_empty());
    }

    #[test]
    fn oneplus_maps_spatial_and_game_mode() {
        let d = oneplus(json!({}));
        assert_eq!(
            keys(&desired_settings(&d, Category::Movie)),
            vec![("spatialAudio", json!(1)), ("gameMode", json!(false))]
        );
        assert_eq!(
            keys(&desired_settings(&d, Category::Music)),
            vec![("spatialAudio", json!(0)), ("gameMode", json!(false))]
        );
        assert_eq!(
            keys(&desired_settings(&d, Category::Game)),
            vec![("spatialAudio", json!(0)), ("gameMode", json!(true)), ("gameSoundProfile", json!(3))]
        );
    }

    #[test]
    fn oneplus_never_selects_head_tracking_and_skips_unadvertised_controls() {
        let d = device(
            &["spatialAudio"],
            json!([{"key": "spatialAudio", "kind": "select", "options": [{"value": 0}, {"value": 2}]}]),
            json!({}),
        );
        // Fixed (1) is not offered, so movie sends nothing; head tracking is never chosen.
        assert!(desired_settings(&d, Category::Movie).is_empty());
        assert_eq!(keys(&desired_settings(&d, Category::Music)), vec![("spatialAudio", json!(0))]);
        assert_eq!(keys(&desired_settings(&d, Category::Game)), vec![("spatialAudio", json!(0))]);
    }

    #[test]
    fn disabled_options_and_missing_capabilities_are_skipped() {
        let d = device(
            &["listeningMode"],
            json!([{"key": "listeningMode", "kind": "select", "options": [
                {"value": "standard"}, {"value": "game", "enabled": false}]}]),
            json!({}),
        );
        assert_eq!(keys(&desired_settings(&d, Category::Game)), vec![("listeningMode", json!("standard"))]);
        let no_capability = device(&[], json!([{"key": "listeningMode", "kind": "select", "options": [{"value": "standard"}]}]), json!({}));
        assert!(desired_settings(&no_capability, Category::Other).is_empty());
    }

    #[test]
    fn describe_is_readable() {
        let d = oneplus(json!({}));
        let text: Vec<_> = desired_settings(&d, Category::Game).iter().map(describe).collect();
        assert_eq!(text, ["Spatial audio: off", "Game mode: on", "Game sound profile"]);
    }

    #[test]
    fn pending_settings_omit_values_the_device_already_confirmed() {
        let d = oneplus(json!({"spatialAudio": 0, "gameMode": true, "gameSoundProfile": 0}));
        assert_eq!(keys(&pending_settings(&d, Category::Game)), vec![("gameSoundProfile", json!(3))]);
        assert_eq!(
            keys(&pending_settings(&d, Category::Movie)),
            vec![("spatialAudio", json!(1)), ("gameMode", json!(false))]
        );
    }
}
