use super::{is_sony_headphone, model_from_name, Connection, PairedDevice};
use std::io;
use std::mem::{size_of, zeroed};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use windows_sys::core::GUID;
use windows_sys::Win32::Devices::Bluetooth::*;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_NO_MORE_ITEMS};
use windows_sys::Win32::Networking::WinSock::*;

const SERVICES: [GUID; 2] = [
    GUID::from_u128(0x96cc203e_5068_46ad_b32d_e316f5e069ba),
    GUID::from_u128(0x956c7b26_d49a_4ba8_b03f_b17d393cb6e2),
];

pub fn enumerate() -> Result<Vec<PairedDevice>, String> {
    // The documented API initializes these POD structures with their explicit sizes.
    let search = BLUETOOTH_DEVICE_SEARCH_PARAMS {
        dwSize: size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
        fReturnAuthenticated: 1,
        fReturnRemembered: 1,
        fReturnConnected: 1,
        fIssueInquiry: 0,
        ..Default::default()
    };
    let mut info = BLUETOOTH_DEVICE_INFO {
        dwSize: size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
        ..Default::default()
    };
    let handle = unsafe { BluetoothFindFirstDevice(&search, &mut info) };
    if handle.is_null() {
        let code = unsafe { GetLastError() };
        return if code == ERROR_NO_MORE_ITEMS {
            Ok(vec![])
        } else {
            Err(format!("Windows Bluetooth enumeration failed (code {code}). Turn on Bluetooth and pair the headphones in Windows Settings."))
        };
    }
    struct FindGuard(HBLUETOOTH_DEVICE_FIND);
    impl Drop for FindGuard {
        fn drop(&mut self) {
            unsafe {
                BluetoothFindDeviceClose(self.0);
            }
        }
    }
    let _guard = FindGuard(handle);
    let mut devices = Vec::new();
    loop {
        let end = info
            .szName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.szName.len());
        let name = String::from_utf16_lossy(&info.szName[..end]);
        if is_sony_headphone(&name) {
            let address = unsafe { info.Address.Anonymous.ullLong };
            devices.push(PairedDevice {
                address: format!("{address:012X}"),
                model: model_from_name(&name),
                name: name.chars().filter(|c| !c.is_control()).take(128).collect(),
            });
        }
        if devices.len() >= 128 || unsafe { BluetoothFindNextDevice(handle, &mut info) } == 0 {
            break;
        }
    }
    devices.sort_by(|a, b| a.address.cmp(&b.address));
    devices.dedup_by(|a, b| a.address == b.address);
    Ok(devices)
}

fn initialize_winsock() -> Result<(), String> {
    static INITIALIZED: OnceLock<Result<(), i32>> = OnceLock::new();
    INITIALIZED
        .get_or_init(|| {
            let mut data: WSADATA = unsafe { zeroed() };
            let result = unsafe { WSAStartup(0x0202, &mut data) };
            if result == 0 {
                Ok(())
            } else {
                Err(result)
            }
        })
        .as_ref()
        .map(|_| ())
        .map_err(|code| format!("Windows socket initialization failed (code {code})."))
}

struct BluetoothSocket(SOCKET, &'static str);

impl Drop for BluetoothSocket {
    fn drop(&mut self) {
        unsafe {
            shutdown(self.0, SD_BOTH);
            closesocket(self.0);
        }
    }
}

pub fn connect(
    device: &PairedDevice,
    cancelled: Arc<AtomicBool>,
) -> Result<Box<dyn Connection>, String> {
    initialize_winsock()?;
    let address = u64::from_str_radix(&device.address, 16)
        .map_err(|_| "Invalid cached Bluetooth address; discover devices again.".to_string())?;
    let mut last_code = 0;
    for (service, service_uuid) in SERVICES.into_iter().zip([
        "96cc203e-5068-46ad-b32d-e316f5e069ba",
        "956c7b26-d49a-4ba8-b03f-b17d393cb6e2",
    ]) {
        if cancelled.load(Ordering::Acquire) {
            return Err("Connection was cancelled.".into());
        }
        let raw = unsafe { socket(AF_BTH as i32, SOCK_STREAM, BTHPROTO_RFCOMM as i32) };
        if raw == INVALID_SOCKET {
            return Err(format!(
                "Cannot open a Bluetooth socket (code {}).",
                unsafe { WSAGetLastError() }
            ));
        }
        let socket = BluetoothSocket(raw, service_uuid);
        let mut nonblocking = 1;
        if unsafe { ioctlsocket(raw, FIONBIO, &mut nonblocking) } != 0 {
            return Err("Windows could not configure a cancellable Bluetooth connection.".into());
        }
        let remote = SOCKADDR_BTH {
            addressFamily: AF_BTH,
            btAddr: address,
            serviceClassId: service,
            port: 0,
        };
        let result = unsafe {
            windows_sys::Win32::Networking::WinSock::connect(
                raw,
                &remote as *const SOCKADDR_BTH as *const SOCKADDR,
                size_of::<SOCKADDR_BTH>() as i32,
            )
        };
        if result == 0 {
            return Ok(Box::new(socket));
        }
        last_code = unsafe { WSAGetLastError() };
        if ![WSAEWOULDBLOCK, WSAEINPROGRESS, WSAEALREADY].contains(&last_code) {
            continue;
        }
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            if cancelled.load(Ordering::Acquire) {
                return Err("Connection was cancelled.".into());
            }
            let mut poll = WSAPOLLFD {
                fd: raw,
                events: POLLWRNORM,
                revents: 0,
            };
            let ready = unsafe { WSAPoll(&mut poll, 1, 100) };
            if ready < 0 {
                last_code = unsafe { WSAGetLastError() };
                break;
            }
            if ready > 0 {
                let mut error = 0i32;
                let mut length = size_of::<i32>() as i32;
                if unsafe {
                    getsockopt(
                        raw,
                        SOL_SOCKET,
                        SO_ERROR,
                        &mut error as *mut i32 as *mut u8,
                        &mut length,
                    )
                } != 0
                {
                    last_code = unsafe { WSAGetLastError() };
                    break;
                }
                if error == 0 && poll.revents & POLLWRNORM != 0 {
                    return Ok(Box::new(socket));
                }
                last_code = error;
                break;
            }
        }
    }
    Err(format!("Sony's control service did not connect (Windows code {last_code}). Wake the headphones, pair them in Windows Settings, and close other headphone-control apps before retrying."))
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
            let mut poll = WSAPOLLFD {
                fd: self.0,
                events: POLLWRNORM,
                revents: 0,
            };
            let ready = unsafe { WSAPoll(&mut poll, 1, 100) };
            if ready == 0 {
                continue;
            }
            if ready < 0 {
                return Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }));
            }
            let count = unsafe {
                send(
                    self.0,
                    bytes[offset..].as_ptr(),
                    (bytes.len() - offset).min(1024) as i32,
                    0,
                )
            };
            if count <= 0 {
                return Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }));
            }
            offset += count as usize;
        }
        Ok(())
    }

    fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> io::Result<usize> {
        let mut poll = WSAPOLLFD {
            fd: self.0,
            events: POLLRDNORM,
            revents: 0,
        };
        let ready = unsafe {
            WSAPoll(
                &mut poll,
                1,
                timeout.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if ready == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        if ready < 0 {
            return Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }));
        }
        let read = unsafe {
            recv(
                self.0,
                buffer.as_mut_ptr(),
                buffer.len().min(i32::MAX as usize) as i32,
                0,
            )
        };
        if read == SOCKET_ERROR {
            let code = unsafe { WSAGetLastError() };
            return if code == WSAEWOULDBLOCK {
                Err(io::ErrorKind::WouldBlock.into())
            } else {
                Err(io::Error::from_raw_os_error(code))
            };
        }
        Ok(read as usize)
    }
}
