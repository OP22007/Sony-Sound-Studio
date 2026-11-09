use super::{is_supported_headphone, model_from_name, Connection, PairedDevice};
use std::io::{self, Read};
use std::mem::size_of;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn run_tool(program: &str, args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut child = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(|_| format!("{program} is unavailable. Install BlueZ tools and enable Bluetooth before retrying."))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Cannot read Bluetooth tool output.")?;
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stdout.take(65536).read_to_end(&mut output);
        output
    });
    let deadline = Instant::now() + timeout;
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "{program} failed. Check the Bluetooth service and permissions."
                    ))
                }
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!(
                    "{program} timed out; check Bluetooth service availability."
                ));
            }
        }
    };
    let output = reader.join().unwrap_or_default();
    result?;
    Ok(String::from_utf8_lossy(&output).into_owned())
}

pub fn enumerate() -> Result<Vec<PairedDevice>, String> {
    // BlueZ 5.65+ accepts `devices Paired`; older releases only know `paired-devices`.
    let output = match run_tool("bluetoothctl", &["devices", "Paired"], Duration::from_secs(5)) {
        Ok(output) if output.lines().any(|line| line.starts_with("Device ")) => output,
        _ => run_tool("bluetoothctl", &["paired-devices"], Duration::from_secs(5)).unwrap_or_default(),
    };
    let mut devices = Vec::new();
    for line in output.lines().take(256) {
        let mut fields = line.splitn(3, ' ');
        if fields.next() != Some("Device") {
            continue;
        }
        let address = fields.next().unwrap_or_default();
        let name = fields.next().unwrap_or_default();
        if valid_address(address) && is_supported_headphone(name) {
            devices.push(PairedDevice {
                address: address.into(),
                name: name.chars().filter(|c| !c.is_control()).take(128).collect(),
                model: model_from_name(name),
            });
        }
    }
    devices.sort_by(|a, b| a.address.cmp(&b.address));
    devices.dedup_by(|a, b| a.address == b.address);
    devices.truncate(128);
    Ok(devices)
}

pub fn activity_snapshot() -> Result<super::DeviceActivitySnapshot, String> {
    use super::{ActiveAudioDevice, AudioTransport, DeviceActivitySnapshot};
    let output = run_tool(
        "bluetoothctl",
        &["devices", "Connected"],
        Duration::from_secs(5),
    )?;
    let mut snapshot = DeviceActivitySnapshot::default();
    for line in output.lines().take(256) {
        let mut fields = line.splitn(3, ' ');
        if fields.next() != Some("Device") {
            continue;
        }
        let address = fields.next().unwrap_or_default();
        let name = fields.next().unwrap_or_default();
        if valid_address(address) && is_supported_headphone(name) {
            snapshot.bluetooth_connected.push(ActiveAudioDevice {
                identity: address.into(),
                name: name.into(),
                transport: AudioTransport::Bluetooth,
            });
        }
    }
    // BlueZ does not reliably expose the system's active audio route through bluetoothctl.
    Ok(snapshot)
}

fn valid_address(address: &str) -> bool {
    address.len() == 17
        && address.split(':').count() == 6
        && address
            .split(':')
            .all(|part| part.len() == 2 && u8::from_str_radix(part, 16).is_ok())
}

const SOL_RFCOMM: libc::c_int = 18;
const RFCOMM_LM: libc::c_int = 3;
const RFCOMM_LM_AUTH: u32 = 0x0002;
const RFCOMM_LM_ENCRYPT: u32 = 0x0004;

/// Finds the RFCOMM channel of the record whose service class list contains `uuid`
/// in `sdptool browse` output.
fn rfcomm_channel_for_service(records: &str, uuid: &str) -> Option<u8> {
    records
        .split("Service RecHandle:")
        .filter(|record| record.to_ascii_lowercase().contains(uuid))
        .find_map(|record| {
            record.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("Channel:")
                    .and_then(|x| x.trim().parse::<u8>().ok())
            })
        })
}

#[repr(C)]
struct SockAddrRc {
    family: libc::sa_family_t,
    address: [u8; 6],
    channel: u8,
}

struct BluetoothSocket(libc::c_int, &'static str);
impl Drop for BluetoothSocket {
    fn drop(&mut self) {
        unsafe {
            libc::shutdown(self.0, libc::SHUT_RDWR);
            libc::close(self.0);
        }
    }
}

pub fn connect(
    device: &PairedDevice,
    cancelled: Arc<AtomicBool>,
) -> Result<Box<dyn Connection>, String> {
    if !valid_address(&device.address) {
        return Err("Invalid cached Bluetooth address; discover again.".into());
    }
    // `sdptool search` only understands 16-bit and named services, so 128-bit Sony UUIDs are
    // looked up from one `sdptool browse` listing instead.
    let records = run_tool("sdptool", &["browse", &device.address], Duration::from_secs(8)).map_err(|_| {
        "Could not read the headphones' Bluetooth service records. Make sure the headphones are on and connected, and that BlueZ's sdptool is installed.".to_string()
    })?;
    for &service in super::service_uuids(device) {
        if cancelled.load(Ordering::Acquire) {
            return Err("Connection was cancelled.".into());
        }
        let Some(channel) = rfcomm_channel_for_service(&records, service).filter(|c| (1..=30).contains(c)) else {
            continue;
        };
        let raw = unsafe {
            libc::socket(
                libc::AF_BLUETOOTH,
                libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                3,
            )
        };
        if raw < 0 {
            return Err("Cannot open a Linux RFCOMM socket; check Bluetooth permissions.".into());
        }
        let socket = BluetoothSocket(raw, service);
        // Match the security level the headset expects (authenticated, encrypted link).
        let link_mode: u32 = RFCOMM_LM_AUTH | RFCOMM_LM_ENCRYPT;
        unsafe {
            libc::setsockopt(
                raw,
                SOL_RFCOMM,
                RFCOMM_LM,
                &link_mode as *const u32 as *const libc::c_void,
                size_of::<u32>() as libc::socklen_t,
            );
        }
        let mut address = [0; 6];
        for (index, part) in device.address.split(':').rev().enumerate() {
            address[index] = u8::from_str_radix(part, 16).unwrap_or(0);
        }
        let remote = SockAddrRc {
            family: libc::AF_BLUETOOTH as libc::sa_family_t,
            address,
            channel,
        };
        let result = unsafe {
            libc::connect(
                raw,
                &remote as *const SockAddrRc as *const libc::sockaddr,
                size_of::<SockAddrRc>() as libc::socklen_t,
            )
        };
        if result == 0 {
            return Ok(Box::new(socket));
        }
        if io::Error::last_os_error().raw_os_error() != Some(libc::EINPROGRESS) {
            continue;
        }
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline && !cancelled.load(Ordering::Acquire) {
            let mut poll = libc::pollfd {
                fd: raw,
                events: libc::POLLOUT,
                revents: 0,
            };
            if unsafe { libc::poll(&mut poll, 1, 100) } > 0 {
                let mut error = 0i32;
                let mut length = size_of::<i32>() as libc::socklen_t;
                let success = unsafe {
                    libc::getsockopt(
                        raw,
                        libc::SOL_SOCKET,
                        libc::SO_ERROR,
                        &mut error as *mut i32 as *mut libc::c_void,
                        &mut length,
                    )
                };
                if success == 0 && error == 0 && poll.revents & libc::POLLOUT != 0 {
                    return Ok(Box::new(socket));
                }
                break;
            }
        }
    }
    Err("Sony's RFCOMM control service did not connect. Wake the paired headphones and close other headphone-control apps. Linux transport is not hardware-tested.".into())
}

impl Connection for BluetoothSocket {
    fn service_uuid(&self) -> &'static str {
        self.1
    }

    fn write_all(&mut self, bytes: &[u8], timeout: Duration) -> io::Result<()> {
        let deadline = Instant::now() + timeout;
        let mut offset = 0;
        while offset < bytes.len() {
            if Instant::now() >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let mut poll = libc::pollfd {
                fd: self.0,
                events: libc::POLLOUT,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut poll, 1, 100) };
            if ready == 0 {
                continue;
            }
            if ready < 0 {
                return Err(io::Error::last_os_error());
            }
            let count = unsafe {
                libc::send(
                    self.0,
                    bytes[offset..].as_ptr() as *const libc::c_void,
                    (bytes.len() - offset).min(1024),
                    libc::MSG_NOSIGNAL,
                )
            };
            if count <= 0 {
                return Err(io::Error::last_os_error());
            }
            offset += count as usize;
        }
        Ok(())
    }

    fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> io::Result<usize> {
        let mut poll = libc::pollfd {
            fd: self.0,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe {
            libc::poll(
                &mut poll,
                1,
                timeout.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if ready == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        if ready < 0 {
            return Err(io::Error::last_os_error());
        }
        let count = unsafe {
            libc::recv(
                self.0,
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
                0,
            )
        };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BROWSE: &str = "Browsing 58:18:62:A1:FE:E9 ...\nService RecHandle: 0x10001\nService Class ID List:\n  \"Headset\" (0x1108)\nProtocol Descriptor List:\n  \"RFCOMM\" (0x0003)\n    Channel: 1\n\nService RecHandle: 0x20000000\nService Class ID List:\n  UUID 128: 956c7b26-d49a-4ba8-b03f-b17d393cb6e2\nProtocol Descriptor List:\n  \"L2CAP\" (0x0100)\n  \"RFCOMM\" (0x0003)\n    Channel: 9\n";

    #[test]
    fn finds_the_channel_of_the_matching_128_bit_service_only() {
        assert_eq!(rfcomm_channel_for_service(BROWSE, "956c7b26-d49a-4ba8-b03f-b17d393cb6e2"), Some(9));
        assert_eq!(rfcomm_channel_for_service(BROWSE, "96cc203e-5068-46ad-b32d-e316f5e069ba"), None);
    }
}
