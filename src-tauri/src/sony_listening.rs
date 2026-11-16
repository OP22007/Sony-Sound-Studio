//! Sony MDR V2 upmix-series codec, independently implemented from Sound Connect
//! 13.3.0 command builders/readers. See docs/research/collexion-listening-modes.md.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpmixMode {
    Standard,
    Cinema,
    Game,
    Music,
}

impl UpmixMode {
    pub fn decode(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Standard),
            1 => Some(Self::Cinema),
            2 => Some(Self::Game),
            3 => Some(Self::Music),
            _ => None,
        }
    }
    pub fn code(self) -> u8 {
        match self {
            Self::Standard => 0,
            Self::Cinema => 1,
            Self::Game => 2,
            Self::Music => 3,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Cinema => "cinema",
            Self::Game => "game",
            Self::Music => "music",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Cinema => "360 Upmix for Cinema",
            Self::Game => "360 Upmix for Game",
            Self::Music => "Music",
        }
    }
    pub fn from_key(value: &str) -> Option<Self> {
        match value {
            "standard" => Some(Self::Standard),
            "cinema" => Some(Self::Cinema),
            "game" => Some(Self::Game),
            "music" => Some(Self::Music),
            _ => None,
        }
    }
}

pub fn parse_state(payload: &[u8], command: u8) -> Option<UpmixMode> {
    match payload {
        [cmd, 0x0a, value] if *cmd == command => UpmixMode::decode(*value),
        _ => None,
    }
}

pub fn set_payload(mode: UpmixMode) -> [u8; 3] {
    [0xe8, 0x0a, mode.code()]
}

/// AUDIO_RET_CAPABILITY: inquiry, counted item IDs, counted exclusive functions.
pub fn parse_capability(payload: &[u8]) -> Option<Vec<UpmixMode>> {
    if payload.len() < 4 || payload[..2] != [0xe1, 0x0a] {
        return None;
    }
    let count = usize::from(payload[2]);
    let end = 3usize.checked_add(count)?;
    let exclusive = usize::from(*payload.get(end)?);
    if payload.len() != end.checked_add(1)?.checked_add(exclusive)? || count == 0 || count > 4 {
        return None;
    }
    let mut modes = Vec::new();
    for id in &payload[3..end] {
        let mode = UpmixMode::decode(*id)?;
        if modes.contains(&mode) {
            return None;
        }
        modes.push(mode);
    }
    Some(modes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upmix_series_uses_the_shipped_wire_ids_not_menu_order() {
        for (code, key) in [(0, "standard"), (1, "cinema"), (2, "game"), (3, "music")] {
            let mode = UpmixMode::from_key(key).unwrap();
            assert_eq!(parse_state(&[0xe7, 0x0a, code], 0xe7), Some(mode));
            assert_eq!(parse_state(&[0xe9, 0x0a, code], 0xe9), Some(mode));
            assert_eq!(set_payload(mode), [0xe8, 0x0a, code]);
        }
    }
    #[test]
    fn rejects_unknown_modes_wrong_inquiry_truncation_and_trailing_data() {
        for payload in [
            &[0xe7, 0x0a, 4][..],
            &[0xe7, 4, 1],
            &[0xe7, 0x0a],
            &[0xe7, 0x0a, 1, 0],
            &[0xe9, 0x0a, 1],
        ] {
            assert_eq!(parse_state(payload, 0xe7), None);
        }
    }
    #[test]
    fn capability_lists_are_counted_and_never_inferred_from_a_model_name() {
        assert_eq!(
            parse_capability(&[0xe1, 0x0a, 3, 1, 2, 3, 0]),
            Some(vec![UpmixMode::Cinema, UpmixMode::Game, UpmixMode::Music])
        );
        assert_eq!(
            parse_capability(&[0xe1, 0x0a, 1, 1, 2, 0, 1]),
            Some(vec![UpmixMode::Cinema])
        );
        for payload in [
            &[0xe1, 0x0a, 2, 1][..],
            &[0xe1, 0x0a, 2, 1, 1, 0],
            &[0xe1, 0x0a, 1, 99, 0],
            &[0xe1, 0x0a, 1, 1, 1],
            &[0xe1, 0x0a, 1, 1, 0, 0],
        ] {
            assert_eq!(parse_capability(payload), None);
        }
    }
}
