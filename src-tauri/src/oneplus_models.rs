//! Independently assembled compatibility facts from HeyMelody 117.6.6.
//! A catalog entry selects a schema; live product identity and capabilities still gate controls.
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
pub struct NoiseProfile {
    pub index: u8,
    pub parent_index: Option<u8>,
    pub mode: String,
    pub label: String,
}

#[derive(Debug, Deserialize)]
pub struct EqPreset {
    pub index: u8,
    pub label: String,
}

#[derive(Debug, Deserialize)]
pub struct Model {
    pub product_id: String,
    pub name: String,
    pub service_uuid: String,
    pub aliases: Vec<String>,
    pub noise_profiles: Vec<NoiseProfile>,
    pub eq_presets: Vec<EqPreset>,
    pub conditional_eq_presets: Vec<EqPreset>,
    pub wear_detection: bool,
    #[serde(default)]
    pub spatial_types: Vec<u8>,
    #[serde(default)]
    pub game_sound_types: Vec<u8>,
    pub game_feature: Option<u8>,
    #[serde(default)]
    pub custom_eq_supported: bool,
    #[serde(default)]
    pub custom_eq_frequencies: Option<Vec<u16>>,
    #[serde(default)]
    pub bass_wave_supported: bool,
}

pub fn models() -> &'static [Model] {
    static MODELS: OnceLock<Vec<Model>> = OnceLock::new();
    MODELS.get_or_init(|| {
        let mut models: Vec<Model> = serde_json::from_str(include_str!("oneplus_models.json"))
            .expect("the embedded OnePlus model registry must be valid");
        const CUSTOM_EQ_SUPPORTED: &[&str] = &[
            "067414", "067814", "064014", "062014", "063414", "063014", "060C14", "065414",
            "063C14", "066414", "064C14", "061014", "062814", "066814", "067014", "064414",
            "065014", "062414", "066054", "064814", "061814", "061414", "065854", "062C14",
            "050C14",
        ];
        const TEN_BAND_EQ: &[u16] = &[31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
        const BASS_WAVE_SUPPORTED: &[&str] = &[
            "067414", "067814", "064014", "062014", "063414", "063014", "065414", "063C14",
            "066414", "064C14", "062814", "066814", "067014", "064414", "065014", "062414",
            "051014", "050C14",
        ];
        const SPATIAL_MODELS: &[&str] = &[
            "067414", "067814", "064014", "06E410", "065414", "063C14", "066414", "064C14",
            "066814", "067014", "064414", "065014", "066054", "065854", "051014",
        ];
        for model in &mut models {
            model.spatial_types = if matches!(model.product_id.as_str(), "067814" | "064014") {
                vec![0, 1, 2]
            } else if SPATIAL_MODELS.contains(&model.product_id.as_str()) {
                vec![0, 1]
            } else {
                vec![]
            };
            // Verified GameSound product allowlist from the current HeyMelody
            // compatibility table. Live support still gates discovery and writes.
            model.game_sound_types = if matches!(
                model.product_id.as_str(),
                "067414" | "067814" | "066414" | "066814" | "067014"
            ) {
                vec![0, 3]
            } else {
                vec![]
            };
            model.custom_eq_supported = CUSTOM_EQ_SUPPORTED.contains(&model.product_id.as_str());
            model.bass_wave_supported = BASS_WAVE_SUPPORTED.contains(&model.product_id.as_str());
            model.custom_eq_frequencies = match model.product_id.as_str() {
                "067414" | "067814" | "066414" | "066814" | "067014" => Some(TEN_BAND_EQ.to_vec()),
                _ => None,
            };
        }
        models
    })
}

fn normalized(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn by_name(name: &str) -> Option<&'static Model> {
    let wanted = normalized(name);
    models().iter().find(|model| {
        normalized(&model.name) == wanted
            || model
                .aliases
                .iter()
                .any(|alias| normalized(alias) == wanted)
    })
}

/// A name is only a discovery hint, including for models absent from this APK.
/// Unknown product IDs are rejected by the control adapter before settings queries.
pub fn is_oneplus_name(name: &str) -> bool {
    if by_name(name).is_some() {
        return true;
    }
    let name = normalized(name);
    name.strip_prefix("oneplus").is_some_and(|suffix| {
        [
            "buds",
            "nordbuds",
            "bullets",
            "flowbuds",
            "headphones",
            "earbuds",
        ]
        .iter()
        .any(|family| suffix.starts_with(family))
    })
}

pub fn by_product_bytes(bytes: &[u8]) -> Option<&'static Model> {
    if bytes.len() != 3 {
        return None;
    }
    let id = format!("{:02X}{:02X}{:02X}", bytes[2], bytes[1], bytes[0]);
    models().iter().find(|model| model.product_id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn catalog_has_unique_product_ids_and_bounded_unambiguous_controls() {
        assert_eq!(models().len(), 32);
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for model in models() {
            assert!(ids.insert(&model.product_id));
            assert_eq!(model.product_id.len(), 6);
            assert!(model
                .product_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()));
            assert!(model.name.starts_with("OnePlus "));
            assert!(matches!(
                model.service_uuid.as_str(),
                "0000079a-d102-11e1-9b23-00025b00a5a5" | "00001107-d102-11e1-9b23-00025b00a5a5"
            ));
            names.insert(&model.name);
            assert!(by_name(&model.name).is_some());
            for alias in &model.aliases {
                assert_eq!(by_name(alias).unwrap().product_id, model.product_id);
            }
            let mut noise_indices = BTreeSet::new();
            for profile in &model.noise_profiles {
                assert!(noise_indices.insert(profile.index));
                assert!(profile.index < 32);
                if let Some(parent) = profile.parent_index {
                    assert_ne!(parent, profile.index);
                    assert!(model
                        .noise_profiles
                        .iter()
                        .any(|profile| profile.index == parent));
                }
                assert!(matches!(
                    profile.mode.as_str(),
                    "off" | "cancelling" | "ambient" | "adaptive" | "unknown"
                ));
                assert!(!profile.label.is_empty());
            }
            let mut eq_indices = BTreeSet::new();
            for preset in model.eq_presets.iter().chain(&model.conditional_eq_presets) {
                assert!(eq_indices.insert(preset.index));
                assert!(!preset.label.is_empty());
            }
            assert!(model.game_feature.is_none() || matches!(model.game_feature, Some(6 | 40)));
            assert!(model.game_sound_types.is_empty() || model.game_sound_types == [0, 3]);
        }
        assert_eq!(names.len(), 30);
        for product_id in ["067414", "067814", "066414", "066814", "067014"] {
            assert_eq!(
                models()
                    .iter()
                    .find(|model| model.product_id == product_id)
                    .unwrap()
                    .game_sound_types,
                [0, 3]
            );
        }
    }

    #[test]
    fn custom_equalizer_support_and_band_templates_follow_verified_model_config() {
        let enabled = models()
            .iter()
            .filter(|model| model.custom_eq_supported)
            .count();
        assert_eq!(enabled, 25);
        let nord3pro = by_name("OnePlus Nord Buds 3 Pro").unwrap();
        assert!(nord3pro.custom_eq_supported);
        assert_eq!(nord3pro.custom_eq_frequencies, None);
        let flow = by_name("OnePlus Flow Buds").unwrap();
        assert_eq!(
            flow.custom_eq_frequencies.as_deref(),
            Some(&[31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000][..])
        );
        for name in [
            "OnePlus Buds Clip",
            "OnePlus Buds Z",
            "OnePlus Nord Buds CE",
            "OnePlus Buds",
            "OnePlus Bullets Wireless Z3",
        ] {
            assert!(!by_name(name).unwrap().custom_eq_supported, "{name}");
        }
    }

    #[test]
    fn bass_wave_model_allowlist_matches_bundled_function_support() {
        assert_eq!(
            models()
                .iter()
                .filter(|model| model.bass_wave_supported)
                .count(),
            18
        );
        assert!(
            by_name("OnePlus Nord Buds 3 Pro")
                .unwrap()
                .bass_wave_supported
        );
        assert!(!by_name("OnePlus Buds Z").unwrap().bass_wave_supported);
        assert!(!by_name("OnePlus Nord Buds CE").unwrap().bass_wave_supported);
    }

    #[test]
    fn product_lookup_is_little_endian_and_preserves_regional_variants() {
        assert_eq!(
            by_product_bytes(&[0x14, 0x44, 0x06]).unwrap().name,
            "OnePlus Nord Buds 3 Pro"
        );
        for id in [[0x14, 0x04, 0x06], [0x14, 0x84, 0x06]] {
            assert_eq!(by_product_bytes(&id).unwrap().name, "OnePlus Buds");
        }
        assert!(by_product_bytes(&[0, 0, 0]).is_none());
        assert!(by_product_bytes(&[0x14, 0x44]).is_none());
        assert!(by_product_bytes(&[0x14, 0x44, 0x06, 0]).is_none());
    }

    #[test]
    fn models_preserve_modern_and_legacy_rfcomm_endpoints() {
        assert_eq!(
            by_name("OnePlus Nord Buds 3 Pro").unwrap().service_uuid,
            crate::oneplus::CURRENT_SERVICE_UUID
        );
        assert_eq!(
            by_name("OnePlus Buds Pro").unwrap().service_uuid,
            crate::oneplus::SERVICE_UUID
        );
        assert_eq!(
            by_name("OnePlus Buds Z").unwrap().service_uuid,
            crate::oneplus::SERVICE_UUID
        );
        assert_eq!(
            models()
                .iter()
                .filter(|model| model.service_uuid == crate::oneplus::CURRENT_SERVICE_UUID)
                .count(),
            18
        );
        assert_eq!(
            models()
                .iter()
                .filter(|model| model.service_uuid == crate::oneplus::SERVICE_UUID)
                .count(),
            14
        );
    }

    #[test]
    fn normalized_names_find_neckbands_but_do_not_match_sony_or_phones() {
        assert_eq!(
            by_name("oneplus bullets wireless z2 anc")
                .unwrap()
                .product_id,
            "050C14"
        );
        assert!(is_oneplus_name("OnePlus Buds Future"));
        for other in [
            "OnePlus 15",
            "OnePlus Open",
            "Sony WH-1000XM6",
            "NotOnePlus Buds",
            "Buds Pro 3",
            "",
        ] {
            assert!(
                !is_oneplus_name(other),
                "unexpected headphone match: {other}"
            );
        }
    }

    #[test]
    fn older_models_have_different_noise_and_eq_indices() {
        let nord2 = by_name("OnePlus Nord Buds 2").unwrap();
        let nord3pro = by_name("OnePlus Nord Buds 3 Pro").unwrap();
        assert_eq!(
            nord2
                .noise_profiles
                .iter()
                .find(|p| p.index == 0)
                .unwrap()
                .mode,
            "cancelling"
        );
        assert_eq!(
            nord3pro
                .noise_profiles
                .iter()
                .find(|p| p.index == 0)
                .unwrap()
                .mode,
            "off"
        );
        assert_eq!(
            by_name("OnePlus Buds Pro")
                .unwrap()
                .noise_profiles
                .iter()
                .find(|p| p.index == 1)
                .unwrap()
                .mode,
            "ambient"
        );
        assert!(by_name("OnePlus Buds").unwrap().noise_profiles.is_empty());
        assert!(by_name("OnePlus Nord Buds 2r")
            .unwrap()
            .noise_profiles
            .is_empty());
        assert_eq!(
            nord2
                .eq_presets
                .iter()
                .find(|p| p.label == "Balanced")
                .unwrap()
                .index,
            0
        );
        assert_eq!(
            nord3pro
                .eq_presets
                .iter()
                .find(|p| p.label == "Balanced")
                .unwrap()
                .index,
            3
        );
    }

    #[test]
    fn model_local_labels_and_conditional_profiles_preserve_original_semantics() {
        let nord2r = by_name("OnePlus Nord Buds 2r").unwrap();
        let nord2 = by_name("OnePlus Nord Buds 2").unwrap();
        assert_eq!(
            nord2r
                .eq_presets
                .iter()
                .find(|p| p.index == 1)
                .unwrap()
                .label,
            "Bold"
        );
        assert_eq!(
            nord2
                .eq_presets
                .iter()
                .find(|p| p.index == 1)
                .unwrap()
                .label,
            "Bass Plus"
        );
        assert_eq!(
            by_name("OnePlus Buds Pro 2")
                .unwrap()
                .conditional_eq_presets[0]
                .label,
            "Hans Zimmer Soundscape Tuning"
        );
        assert_eq!(
            by_name("OnePlus Buds Clip").unwrap().conditional_eq_presets[0].label,
            "Clear & crisp"
        );
        assert_eq!(
            by_name("OnePlus Buds 4")
                .unwrap()
                .noise_profiles
                .iter()
                .find(|p| p.index == 11)
                .unwrap()
                .mode,
            "adaptive"
        );
        for model in models() {
            for preset in model.eq_presets.iter().chain(&model.conditional_eq_presets) {
                assert!(
                    !preset.label.starts_with("Sound profile "),
                    "unmapped EQ label on {}",
                    model.name
                );
            }
            for profile in model.noise_profiles.iter().filter(|p| p.mode == "unknown") {
                assert_eq!(model.product_id, "067814");
                assert!((12..=14).contains(&profile.index));
                assert_eq!(
                    profile.label,
                    format!("Noise-control profile {}", profile.index)
                );
            }
        }
    }
}
