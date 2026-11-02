//! Read-only observer for VLC's localhost Lua HTTP status endpoint.
//!
//! VLC 3.0's installed `lua/http/requests/status.json` calls
//! `httprequests.getstatus(true)` and serializes it as JSON. The status schema
//! has a top-level `state` and `information.category` entries with `Type`
//! metadata for each stream. This module deliberately returns no VLC metadata.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(150);
const IO_TIMEOUT: Duration = Duration::from_millis(150);
const TOTAL_TIMEOUT: Duration = Duration::from_millis(700);
const MAX_RESPONSE: usize = 256 * 1024;
const VLC_ADDR: SocketAddr = SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 8080);

#[derive(Debug)]
pub struct Playback {
    pub playing: bool,
    pub kind: &'static str,
}

pub fn read(password: &str) -> Result<Playback, String> {
    let started = Instant::now();
    let mut stream = TcpStream::connect_timeout(&VLC_ADDR, CONNECT_TIMEOUT)
        .map_err(|_| "Could not connect to VLC's local status endpoint.".to_string())?;
    let remaining = TOTAL_TIMEOUT.saturating_sub(started.elapsed());
    if remaining.is_zero() {
        return Err("VLC status request timed out.".into());
    }
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|_| "Could not read VLC status.".to_string())?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT.min(remaining)))
        .map_err(|_| "Could not query VLC status.".to_string())?;

    let auth = STANDARD.encode(format!(":{password}"));
    let request = format!(
        "GET /requests/status.json HTTP/1.0\r\nHost: 127.0.0.1\r\nAuthorization: Basic {auth}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| "Could not query VLC status.".to_string())?;

    let mut response = Vec::with_capacity(8192);
    let mut buf = [0u8; 8192];
    loop {
        if started.elapsed() >= TOTAL_TIMEOUT {
            return Err("VLC status request timed out.".into());
        }
        stream
            .set_read_timeout(Some(IO_TIMEOUT.min(TOTAL_TIMEOUT.saturating_sub(started.elapsed()))))
            .map_err(|_| "Could not read VLC status.".to_string())?;
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if response.len().saturating_add(n) > MAX_RESPONSE {
                    return Err("VLC status response is too large.".into());
                }
                response.extend_from_slice(&buf[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e)
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                return Err("VLC status request timed out.".into());
            }
            Err(_) => return Err("Could not read VLC status.".into()),
        }
    }
    if started.elapsed() > TOTAL_TIMEOUT {
        return Err("VLC status request timed out.".into());
    }
    parse_response(&response)
}

fn parse_response(response: &[u8]) -> Result<Playback, String> {
    if response.len() > MAX_RESPONSE {
        return Err("VLC status response is too large.".into());
    }
    let split = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "VLC returned an invalid status response.".to_string())?;
    let headers = std::str::from_utf8(&response[..split])
        .map_err(|_| "VLC returned an invalid status response.".to_string())?;
    let status = headers.lines().next().unwrap_or_default();
    if !status.starts_with("HTTP/1.") {
        return Err("VLC returned an invalid status response.".into());
    }
    if status.contains(" 401 ") || status.contains(" 403 ") {
        return Err("VLC rejected the HTTP password.".into());
    }
    if !status.contains(" 200 ") {
        return Err("VLC status endpoint returned an error.".into());
    }

    let raw_body = &response[split + 4..];
    let body = if headers.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("transfer-encoding")
                && value.trim().eq_ignore_ascii_case("chunked")
        })
    }) {
        decode_chunked(raw_body)?
    } else {
        raw_body.to_vec()
    };
    let json: Value = serde_json::from_slice(&body)
        .map_err(|_| "VLC returned an invalid status response.".to_string())?;
    parse_status(&json)
}

fn decode_chunked(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut rest = input;
    loop {
        let line_end = rest
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| "VLC returned an invalid status response.".to_string())?;
        let size_text = std::str::from_utf8(&rest[..line_end])
            .map_err(|_| "VLC returned an invalid status response.".to_string())?;
        let size_text = size_text.split(';').next().unwrap_or("");
        let size = usize::from_str_radix(size_text.trim(), 16)
            .map_err(|_| "VLC returned an invalid status response.".to_string())?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if out.len().saturating_add(size) > MAX_RESPONSE || rest.len() < size + 2 {
            return Err("VLC returned an invalid status response.".into());
        }
        out.extend_from_slice(&rest[..size]);
        if &rest[size..size + 2] != b"\r\n" {
            return Err("VLC returned an invalid status response.".into());
        }
        rest = &rest[size + 2..];
    }
}

fn parse_status(json: &Value) -> Result<Playback, String> {
    let state = json
        .get("state")
        .and_then(Value::as_str)
        .ok_or_else(|| "VLC returned an invalid status response.".to_string())?;
    let playing = match state {
        "playing" => true,
        "paused" | "stopped" => false,
        _ => return Err("VLC returned an invalid playback state.".into()),
    };
    let types = stream_types(json);
    let has_video = types.iter().any(|kind| kind.eq_ignore_ascii_case("video"));
    let has_audio = types.iter().any(|kind| kind.eq_ignore_ascii_case("audio"));
    let kind = if has_video {
        "movie"
    } else if has_audio {
        "music"
    } else {
        "other"
    };
    Ok(Playback { playing, kind })
}

fn stream_types(value: &Value) -> Vec<&str> {
    value.pointer("/information/category").and_then(Value::as_object)
        .into_iter().flat_map(|categories| categories.iter())
        .filter(|(name, _)| name.as_str() != "meta")
        .filter_map(|(_, stream)| stream.get("Type").and_then(Value::as_str)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_real_stream_types_and_state() {
        let video: Value = serde_json::from_str(r#"{"state":"playing","information":{"category":{"Stream 0":{"Type":"Video"},"Stream 1":{"Type":"Audio"}}}}"#).unwrap();
        let music: Value = serde_json::from_str(r#"{"state":"paused","information":{"category":{"Stream 0":{"Type":"Audio"}}}}"#).unwrap();
        assert_eq!((parse_status(&video).unwrap().playing, parse_status(&video).unwrap().kind), (true, "movie"));
        assert_eq!((parse_status(&music).unwrap().playing, parse_status(&music).unwrap().kind), (false, "music"));
        let stopped: Value = serde_json::from_str(r#"{"state":"stopped"}"#).unwrap();
        assert_eq!((parse_status(&stopped).unwrap().playing, parse_status(&stopped).unwrap().kind), (false, "other"));
        let tagged_audio = serde_json::json!({"state":"playing", "information":{"category":{"meta":{"Type":"Video"},"Stream 0":{"Type":"Audio"}}}});
        assert_eq!(parse_status(&tagged_audio).unwrap().kind, "music");
    }

    #[test]
    fn rejects_unknown_state_and_http_auth_errors() {
        let unknown: Value = serde_json::from_str(r#"{"state":"mysterious"}"#).unwrap();
        assert!(parse_status(&unknown).is_err());
        assert!(parse_response(b"HTTP/1.0 401 Unauthorized\r\n\r\nno").unwrap_err().contains("password"));
    }

    #[test]
    fn handles_chunked_and_rejects_oversized_body() {
        let bytes = b"HTTP/1.0 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n13\r\n{\"state\":\"playing\"}\r\n0\r\n\r\n";
        assert!(parse_response(bytes).is_ok());
        let mut large = b"HTTP/1.0 200 OK\r\n\r\n".to_vec();
        large.resize(MAX_RESPONSE + 5, b'x');
        assert!(parse_response(&large).is_err());
    }
}
