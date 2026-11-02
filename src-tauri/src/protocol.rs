//! Independently authored codec from the APK framing observations in the research report.
//! This module deliberately contains no guessed setting payload schemas.

use std::fmt;

const START: u8 = 0x3e;
const END: u8 = 0x3c;
const ESCAPE: u8 = 0x3d;
pub const DEFAULT_MAX_PAYLOAD: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub data_type: u8,
    pub sequence: u8,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    TooLarge,
    InvalidEscape,
    Truncated,
    LengthMismatch,
    ChecksumMismatch,
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "frame exceeds configured payload limit",
            Self::InvalidEscape => "invalid or dangling frame escape",
            Self::Truncated => "frame header is truncated",
            Self::LengthMismatch => "frame length does not match header",
            Self::ChecksumMismatch => "frame checksum does not match",
        })
    }
}

impl std::error::Error for CodecError {}

pub fn encode(frame: &Frame, max_payload: usize) -> Result<Vec<u8>, CodecError> {
    if frame.payload.len() > max_payload || frame.payload.len() > u32::MAX as usize {
        return Err(CodecError::TooLarge);
    }
    let length = (frame.payload.len() as u32).to_be_bytes();
    let mut wire = Vec::with_capacity(frame.payload.len() + 9);
    wire.push(START);
    let mut checksum = 0u8;
    for byte in [frame.data_type, frame.sequence]
        .into_iter()
        .chain(length)
        .chain(frame.payload.iter().copied())
    {
        checksum = checksum.wrapping_add(byte);
        push_escaped(&mut wire, byte);
    }
    push_escaped(&mut wire, checksum);
    wire.push(END);
    Ok(wire)
}

fn push_escaped(output: &mut Vec<u8>, byte: u8) {
    match byte {
        END | ESCAPE | START => output.extend_from_slice(&[ESCAPE, byte ^ 0x10]),
        _ => output.push(byte),
    }
}

/// Bytes are unescaped incrementally, so partial reads retain at most one bounded body.
pub struct Decoder {
    body: Vec<u8>,
    in_frame: bool,
    escaped: bool,
    max_payload: usize,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_PAYLOAD)
    }
}

impl Decoder {
    pub fn new(max_payload: usize) -> Self {
        Self {
            body: Vec::new(),
            in_frame: false,
            escaped: false,
            max_payload: max_payload.min(DEFAULT_MAX_PAYLOAD),
        }
    }

    pub fn feed(&mut self, input: &[u8]) -> Vec<Result<Frame, CodecError>> {
        let mut frames = Vec::new();
        for &byte in input {
            if byte == START {
                if self.in_frame && self.escaped {
                    frames.push(Err(CodecError::InvalidEscape));
                }
                self.body.clear();
                self.in_frame = true;
                self.escaped = false;
                continue;
            }
            if !self.in_frame {
                continue;
            }
            if byte == END {
                frames.push(if self.escaped {
                    Err(CodecError::InvalidEscape)
                } else {
                    decode_body(&self.body, self.max_payload)
                });
                self.reset();
                continue;
            }
            let decoded = if self.escaped {
                self.escaped = false;
                match byte {
                    0x2c..=0x2e => byte ^ 0x10,
                    _ => {
                        frames.push(Err(CodecError::InvalidEscape));
                        self.reset();
                        continue;
                    }
                }
            } else if byte == ESCAPE {
                self.escaped = true;
                continue;
            } else {
                byte
            };
            self.body.push(decoded);
            if self.body.len() == 6 && declared_length(&self.body) > self.max_payload {
                frames.push(Err(CodecError::TooLarge));
                self.reset();
            } else if self.body.len() > self.max_payload + 7 {
                frames.push(Err(CodecError::TooLarge));
                self.reset();
            } else if self.body.len() > 6 && self.body.len() > declared_length(&self.body) + 7 {
                frames.push(Err(CodecError::LengthMismatch));
                self.reset();
            }
        }
        frames
    }

    pub fn buffered_len(&self) -> usize {
        self.body.len()
    }

    fn reset(&mut self) {
        self.body.clear();
        self.in_frame = false;
        self.escaped = false;
    }
}

fn declared_length(body: &[u8]) -> usize {
    u32::from_be_bytes([body[2], body[3], body[4], body[5]]) as usize
}

fn decode_body(body: &[u8], max_payload: usize) -> Result<Frame, CodecError> {
    if body.len() < 7 {
        return Err(CodecError::Truncated);
    }
    let length = declared_length(body);
    if length > max_payload {
        return Err(CodecError::TooLarge);
    }
    if body.len() != length + 7 {
        return Err(CodecError::LengthMismatch);
    }
    let checksum = body[..body.len() - 1]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    if checksum != body[body.len() - 1] {
        return Err(CodecError::ChecksumMismatch);
    }
    Ok(Frame {
        data_type: body[0],
        sequence: body[1],
        payload: body[6..body.len() - 1].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Frame {
        Frame {
            data_type: 0x0c,
            sequence: 1,
            payload: vec![0x3c, 0x3d, 0x3e],
        }
    }

    #[test]
    fn independently_calculated_golden_vector() {
        let wire = encode(&sample(), 64).unwrap();
        assert_eq!(
            wire,
            [0x3e, 0x0c, 1, 0, 0, 0, 3, 0x3d, 0x2c, 0x3d, 0x2d, 0x3d, 0x2e, 0xc7, 0x3c]
        );
        assert_eq!(Decoder::default().feed(&wire), [Ok(sample())]);
    }

    #[test]
    fn every_possible_read_split_and_single_byte_reads() {
        let wire = encode(&sample(), 64).unwrap();
        for split in 0..=wire.len() {
            let mut decoder = Decoder::default();
            let mut result = decoder.feed(&wire[..split]);
            result.extend(decoder.feed(&wire[split..]));
            assert_eq!(result, [Ok(sample())], "split {split}");
        }
        let mut decoder = Decoder::default();
        let result: Vec<_> = wire.iter().flat_map(|b| decoder.feed(&[*b])).collect();
        assert_eq!(result, [Ok(sample())]);
    }

    #[test]
    fn coalesced_frames_and_noise() {
        let wire = encode(&sample(), 64).unwrap();
        let combined = [vec![0, 2, 8], wire.clone(), wire].concat();
        assert_eq!(
            Decoder::default().feed(&combined),
            [Ok(sample()), Ok(sample())]
        );
    }

    #[test]
    fn malformed_frames_recover_at_next_start() {
        let good = encode(&sample(), 64).unwrap();
        for (bad, error) in [
            (vec![START, ESCAPE, 0x00, END], CodecError::InvalidEscape),
            (vec![START, 0, ESCAPE, END], CodecError::InvalidEscape),
            (vec![START, 0, END], CodecError::Truncated),
            (
                vec![START, 0x0c, 0, 0, 0, 0, 2, 0, END],
                CodecError::LengthMismatch,
            ),
            (
                vec![START, 0x0c, 0, 0, 0, 0, 0, 0, END],
                CodecError::ChecksumMismatch,
            ),
        ] {
            let mut decoder = Decoder::default();
            let result = decoder.feed(&[bad, good.clone()].concat());
            assert_eq!(result, [Err(error), Ok(sample())]);
        }
    }

    #[test]
    fn malicious_length_is_rejected_before_payload_allocation() {
        let mut decoder = Decoder::new(16);
        assert_eq!(
            decoder.feed(&[START, 0x0c, 0, 0xff, 0xff, 0xff, 0xff]),
            [Err(CodecError::TooLarge)]
        );
        assert_eq!(decoder.buffered_len(), 0);
        assert!(decoder.feed(&vec![0; 100_000]).is_empty());
        assert_eq!(decoder.buffered_len(), 0);
    }

    #[test]
    fn excess_body_and_unfinished_frame_are_bounded() {
        let mut decoder = Decoder::new(2);
        let mut bytes = vec![START, 0x0c, 0, 0, 0, 0, 2];
        bytes.extend(vec![0; 1000]);
        assert_eq!(decoder.feed(&bytes), [Err(CodecError::TooLarge)]);
        assert_eq!(decoder.buffered_len(), 0);
    }

    #[test]
    fn all_byte_values_round_trip_and_checksum_wraps() {
        let frame = Frame {
            data_type: 0x0e,
            sequence: 0xff,
            payload: (0..=255).collect(),
        };
        let wire = encode(&frame, 1024).unwrap();
        assert_eq!(Decoder::default().feed(&wire), [Ok(frame)]);
    }

    #[test]
    fn zero_payload_and_encoding_limit() {
        let frame = Frame {
            data_type: 1,
            sequence: 0,
            payload: vec![],
        };
        assert_eq!(
            encode(&frame, 0).unwrap(),
            [START, 1, 0, 0, 0, 0, 0, 1, END]
        );
        assert_eq!(encode(&sample(), 2), Err(CodecError::TooLarge));
    }
}
