//! HeyMelody EQ profile wire codec (commands 290 and 1048).
//!
//! This module only encodes and decodes payloads. Callers must check the
//! device's advertised command capability before sending these commands.

use serde::{Deserialize, Serialize};

const MAX_PROFILES: usize = 32;
const MAX_BANDS: usize = 16;
const MAX_NAME_BYTES: usize = 255;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct EqProfile {
    pub selected: u8,
    pub min_value: i8,
    pub max_value: i8,
    pub eq_id: u8,
    pub name: String,
    pub frequencies: Vec<u16>,
    pub db_values: Vec<i8>,
}

/// A request to save edits to an already confirmed custom profile.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EqProfileChange {
    pub action: u8,
    pub profile: EqProfile,
}

impl EqProfileChange {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.action, 1 | 2) {
            return Err(
                "Only creation or edits to an existing OnePlus EQ profile are supported.".into(),
            );
        }
        if self.profile.selected != 1 {
            return Err("A saved OnePlus EQ profile must be marked selected.".into());
        }
        if self.profile.name.trim().is_empty() {
            return Err("OnePlus EQ profile name cannot be empty.".into());
        }
        if self.profile.min_value >= self.profile.max_value {
            return Err(
                "OnePlus EQ profile range must have distinct minimum and maximum values.".into(),
            );
        }
        if self.action == 1
            && (self.profile.eq_id != 0
                || self.profile.min_value != -6
                || self.profile.max_value != 6)
        {
            return Err(
                "New OnePlus EQ profiles must use the device template ID and -6 to +6 dB range."
                    .into(),
            );
        }
        encode_set_profile(&self.profile, self.action).map(|_| ())
    }
}

/// Decode the command 290 response payload: status, count, then packed profiles.
pub fn parse_profiles(data: &[u8]) -> Result<Vec<EqProfile>, String> {
    let status = *data.first().ok_or("EQ response is missing status")? as i8;
    if status != 0 {
        return Err(format!("EQ response status is {status}"));
    }
    let count = *data.get(1).ok_or("EQ response is missing profile count")? as usize;
    if count > MAX_PROFILES {
        return Err(format!(
            "EQ profile count {count} exceeds limit {MAX_PROFILES}"
        ));
    }

    let mut cursor = 2;
    let mut profiles = Vec::with_capacity(count);
    for index in 0..count {
        let selected = take_u8(data, &mut cursor, "selected flag")?;
        if selected > 1 {
            return Err(format!("EQ profile {index} has an invalid selected flag"));
        }
        let min_value = take_i8(data, &mut cursor, "minimum value")?;
        let max_value = take_i8(data, &mut cursor, "maximum value")?;
        let eq_id = take_u8(data, &mut cursor, "EQ id")?;
        let name_len = take_u8(data, &mut cursor, "name length")? as usize;
        if name_len > MAX_NAME_BYTES {
            return Err(format!("EQ profile {index} name exceeds limit"));
        }
        let name_bytes = take_bytes(data, &mut cursor, name_len, "name")?;
        let name = String::from_utf8(name_bytes.to_vec())
            .map_err(|_| format!("EQ profile {index} name is not valid UTF-8"))?;
        let band_count = take_u8(data, &mut cursor, "band count")? as usize;
        if band_count == 0 || band_count > MAX_BANDS {
            return Err(format!(
                "EQ profile {index} has invalid band count {band_count}"
            ));
        }

        let mut frequencies = Vec::with_capacity(band_count);
        let mut db_values = Vec::with_capacity(band_count);
        for _ in 0..band_count {
            let bytes = take_bytes(data, &mut cursor, 2, "frequency")?;
            frequencies.push(u16::from_le_bytes([bytes[0], bytes[1]]));
            db_values.push(take_i8(data, &mut cursor, "band dB value")?);
        }
        let unique_frequencies = frequencies
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if frequencies.iter().any(|frequency| *frequency == 0)
            || unique_frequencies.len() != frequencies.len()
        {
            return Err(format!(
                "EQ profile {index} has invalid or duplicate band frequencies"
            ));
        }
        if min_value > max_value {
            return Err(format!("EQ profile {index} has minimum above maximum"));
        }
        if db_values
            .iter()
            .any(|value| *value < min_value || *value > max_value)
        {
            return Err(format!(
                "EQ profile {index} has a band outside its declared range"
            ));
        }

        profiles.push(EqProfile {
            selected,
            min_value,
            max_value,
            eq_id,
            name,
            frequencies,
            db_values,
        });
    }
    if cursor != data.len() {
        return Err(format!(
            "EQ response has {} trailing bytes",
            data.len() - cursor
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    if profiles.iter().any(|profile| !ids.insert(profile.eq_id)) {
        return Err("EQ response contains duplicate profile IDs".into());
    }
    Ok(profiles)
}

/// Encode command 1048's profile body. The leading action byte is 1 to create,
/// 2 to select/update/rename, or 3 to delete. Callers restrict exposed actions
/// to operations that have a confirmed profile identity and readback path.
pub fn encode_set_profile(profile: &EqProfile, selector: u8) -> Result<Vec<u8>, String> {
    if !(1..=3).contains(&selector) {
        return Err("EQ action must be 1 (create), 2 (select/update), or 3 (delete)".into());
    }
    if profile.name.len() > MAX_NAME_BYTES {
        return Err("EQ profile name exceeds 255 UTF-8 bytes".into());
    }
    if profile.frequencies.is_empty()
        || profile.frequencies.len() > MAX_BANDS
        || profile.frequencies.len() != profile.db_values.len()
    {
        return Err("EQ frequency and dB arrays must have equal length from 1 to 16".into());
    }
    if profile.min_value > profile.max_value {
        return Err("EQ minimum value exceeds maximum value".into());
    }
    if profile
        .db_values
        .iter()
        .any(|value| *value < profile.min_value || *value > profile.max_value)
    {
        return Err("EQ dB value is outside the profile's declared range".into());
    }

    let name = profile.name.as_bytes();
    let mut payload = Vec::with_capacity(6 + name.len() + 3 * profile.frequencies.len());
    payload.push(selector);
    payload.push(profile.min_value as u8);
    payload.push(profile.max_value as u8);
    payload.push(profile.eq_id);
    payload.push(name.len() as u8);
    payload.extend_from_slice(name);
    payload.push(profile.frequencies.len() as u8);
    for (frequency, db_value) in profile.frequencies.iter().zip(&profile.db_values) {
        payload.extend_from_slice(&frequency.to_le_bytes());
        payload.push(*db_value as u8);
    }
    Ok(payload)
}

fn take_u8(data: &[u8], cursor: &mut usize, field: &str) -> Result<u8, String> {
    let value = *data
        .get(*cursor)
        .ok_or_else(|| format!("EQ response is truncated at {field}"))?;
    *cursor += 1;
    Ok(value)
}

fn take_i8(data: &[u8], cursor: &mut usize, field: &str) -> Result<i8, String> {
    Ok(take_u8(data, cursor, field)? as i8)
}

fn take_bytes<'a>(
    data: &'a [u8],
    cursor: &mut usize,
    length: usize,
    field: &str,
) -> Result<&'a [u8], String> {
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| format!("EQ response length overflow at {field}"))?;
    let bytes = data
        .get(*cursor..end)
        .ok_or_else(|| format!("EQ response is truncated at {field}"))?;
    *cursor = end;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{encode_set_profile, parse_profiles, EqProfile, EqProfileChange};

    // status=0, one profile, selected, -6..6 dB, id 128, UTF-8 "Bäs",
    // then two interleaved [frequency LE, signed dB] bands.
    const GOLDEN_PROFILE: &[u8] = &[
        0x00, 0x01, 0x01, 0xfa, 0x06, 0x80, 0x04, 0x42, 0xc3, 0xa4, 0x73, 0x02, 0xe8, 0x03, 0xfa,
        0x10, 0x27, 0x03,
    ];

    fn golden_profile() -> EqProfile {
        EqProfile {
            selected: 1,
            min_value: -6,
            max_value: 6,
            eq_id: 128,
            name: "Bäs".into(),
            frequencies: vec![1000, 10000],
            db_values: vec![-6, 3],
        }
    }

    #[test]
    fn parses_golden_interleaved_profile_with_signed_values_and_utf8() {
        let profiles = parse_profiles(GOLDEN_PROFILE).unwrap();
        assert_eq!(profiles, vec![golden_profile()]);
    }

    #[test]
    fn encodes_golden_interleaved_profile_for_known_actions() {
        let profile = golden_profile();
        for action in 1..=3 {
            let mut expected = GOLDEN_PROFILE[2..].to_vec();
            expected[0] = action;
            assert_eq!(encode_set_profile(&profile, action).unwrap(), expected);
        }
    }

    #[test]
    fn rejects_unsupported_action_values() {
        let profile = golden_profile();
        for action in [0, 4, u8::MAX] {
            assert!(encode_set_profile(&profile, action).is_err());
        }
    }

    #[test]
    fn rejects_missing_status_count_and_truncated_profile_fields() {
        for data in [
            &[][..],
            &[0][..],
            &[0, 1][..],
            &[0, 1, 1, 0xfa][..],
            &GOLDEN_PROFILE[..GOLDEN_PROFILE.len() - 1],
        ] {
            assert!(parse_profiles(data).is_err(), "accepted {data:?}");
        }
    }

    #[test]
    fn rejects_bad_count_trailing_bytes_and_out_of_range_db_values() {
        assert!(parse_profiles(&[0, 33]).is_err());

        let mut trailing = GOLDEN_PROFILE.to_vec();
        trailing.push(0);
        assert!(parse_profiles(&trailing).is_err());

        let mut out_of_range = GOLDEN_PROFILE.to_vec();
        *out_of_range.last_mut().unwrap() = 7;
        assert!(parse_profiles(&out_of_range).is_err());
    }

    #[test]
    fn rejects_invalid_utf8_and_invalid_band_count() {
        let mut invalid_name = GOLDEN_PROFILE.to_vec();
        invalid_name[8] = 0xff;
        assert!(parse_profiles(&invalid_name).is_err());

        let mut no_bands = GOLDEN_PROFILE.to_vec();
        no_bands[11] = 0;
        assert!(parse_profiles(&no_bands).is_err());
    }

    #[test]
    fn rejects_invalid_selected_flags_duplicate_ids_and_duplicate_frequencies() {
        let mut invalid_selected = GOLDEN_PROFILE.to_vec();
        invalid_selected[2] = 2;
        assert!(parse_profiles(&invalid_selected).is_err());

        let mut duplicate_id = vec![0, 2];
        duplicate_id.extend_from_slice(&GOLDEN_PROFILE[2..]);
        duplicate_id.extend_from_slice(&GOLDEN_PROFILE[2..]);
        assert!(parse_profiles(&duplicate_id).is_err());

        let mut duplicate_frequency = GOLDEN_PROFILE.to_vec();
        duplicate_frequency[15..17].copy_from_slice(&[0xe8, 0x03]);
        assert!(parse_profiles(&duplicate_frequency).is_err());
    }

    #[test]
    fn typed_profile_actions_restrict_creation_and_existing_profile_updates() {
        let profile = golden_profile();
        assert!(EqProfileChange {
            action: 2,
            profile: profile.clone()
        }
        .validate()
        .is_ok());
        assert!(EqProfileChange {
            action: 3,
            profile: profile.clone()
        }
        .validate()
        .is_err());
        assert!(EqProfileChange {
            action: 1,
            profile: EqProfile {
                eq_id: 0,
                min_value: -6,
                max_value: 6,
                ..profile.clone()
            }
        }
        .validate()
        .is_ok());
        assert!(EqProfileChange { action: 1, profile }.validate().is_err());
    }

    #[test]
    fn encoder_rejects_range_or_array_mismatches() {
        let mut profile = golden_profile();
        profile.db_values[0] = -7;
        assert!(encode_set_profile(&profile, 2).is_err());

        profile = golden_profile();
        profile.db_values.pop();
        assert!(encode_set_profile(&profile, 2).is_err());
    }
}
