//! Bounded callback state shared by the macOS delegate and its owner thread.
//! This module has no Apple dependencies, so stream behavior is tested on every OS.
use std::collections::VecDeque;

pub(super) const MAX_RECEIVE: usize = 128 * 1024;

#[derive(Default)]
pub(super) struct ChannelState {
    pub opened: Option<i32>,
    pub closed: bool,
    pub overflow: bool,
    bytes: VecDeque<u8>,
}

impl ChannelState {
    pub fn receive(&mut self, bytes: &[u8]) {
        if self.closed || self.overflow { return; }
        if bytes.len() > MAX_RECEIVE.saturating_sub(self.bytes.len()) {
            // Losing bytes destroys framing: retire the stream instead of parsing a suffix.
            self.bytes.clear();
            self.overflow = true;
            self.closed = true;
            return;
        }
        self.bytes.extend(bytes);
    }

    pub fn read(&mut self, destination: &mut [u8]) -> usize {
        let count = destination.len().min(self.bytes.len());
        for value in &mut destination[..count] { *value = self.bytes.pop_front().unwrap(); }
        count
    }
}

pub(super) fn uuid_bytes(uuid: &str) -> Option<[u8; 16]> {
    if uuid.len() != 36 { return None; }
    let mut result = [0; 16];
    let mut index = 0;
    let mut high = None;
    for (offset, byte) in uuid.bytes().enumerate() {
        if [8, 13, 18, 23].contains(&offset) {
            if byte != b'-' { return None; }
            continue;
        }
        let digit = (byte as char).to_digit(16)? as u8;
        if let Some(first) = high.take() {
            result[index] = first * 16 + digit;
            index += 1;
        } else { high = Some(digit); }
    }
    (index == 16 && high.is_none()).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragmented_callbacks_and_small_reads_preserve_the_stream() {
        let mut state = ChannelState::default();
        state.receive(&[1, 2]); state.receive(&[3, 4, 5]);
        let mut small = [0; 3];
        assert_eq!(state.read(&mut small), 3); assert_eq!(small, [1, 2, 3]);
        state.closed = true;
        assert_eq!(state.read(&mut small), 2); assert_eq!(&small[..2], &[4, 5]);
        assert_eq!(state.read(&mut small), 0);
        state.receive(&[6]); assert_eq!(state.read(&mut small), 0);
    }
    #[test]
    fn overflow_fails_the_whole_stream_instead_of_dropping_partial_frames() {
        let mut state = ChannelState::default();
        state.receive(&vec![1; MAX_RECEIVE]);
        state.receive(&[2]);
        assert!(state.overflow && state.closed);
        assert_eq!(state.read(&mut [0; 8]), 0);
    }
    #[test]
    fn service_uuid_is_strict_and_keeps_network_byte_order() {
        assert_eq!(uuid_bytes("96cc203e-5068-46ad-b32d-e316f5e069ba"), Some([0x96,0xcc,0x20,0x3e,0x50,0x68,0x46,0xad,0xb3,0x2d,0xe3,0x16,0xf5,0xe0,0x69,0xba]));
        for invalid in ["96cc203e5068-46ad-b32d-e316f5e069ba", "96cc203e_5068-46ad-b32d-e316f5e069ba", "96cc203e-5068-46ad-b32d-e316f5e069bz", ""] { assert!(uuid_bytes(invalid).is_none()); }
    }
}
