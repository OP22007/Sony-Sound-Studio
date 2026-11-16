use super::{is_supported_headphone, model_from_name, Connection, PairedDevice};
use std::io;
use std::mem::{size_of, zeroed};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use windows_sys::core::GUID;
use windows_sys::Win32::Devices::Bluetooth::*;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_NO_MORE_ITEMS};
use windows_sys::Win32::Networking::WinSock::*;

pub fn activity_snapshot() -> Result<super::DeviceActivitySnapshot, String> {
    use super::{ActiveAudioDevice, AudioTransport, DeviceActivitySnapshot};
    let mut snapshot = DeviceActivitySnapshot::default();
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
    if !handle.is_null() {
        struct Guard(HBLUETOOTH_DEVICE_FIND);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe {
                    BluetoothFindDeviceClose(self.0);
                }
            }
        }
        let _guard = Guard(handle);
        loop {
            let end = info
                .szName
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(info.szName.len());
            let name = String::from_utf16_lossy(&info.szName[..end]);
            if info.fConnected != 0 && is_supported_headphone(&name) {
                let address = unsafe { info.Address.Anonymous.ullLong };
                snapshot.bluetooth_connected.push(ActiveAudioDevice {
                    identity: format!("{address:012X}"),
                    name,
                    transport: AudioTransport::Bluetooth,
                });
            }
            if snapshot.bluetooth_connected.len() >= 128
                || unsafe { BluetoothFindNextDevice(handle, &mut info) } == 0
            {
                break;
            }
        }
    }
    snapshot.active_output = default_output_endpoint();
    Ok(snapshot)
}

fn default_output_endpoint() -> Option<super::ActiveAudioDevice> {
    use super::{ActiveAudioDevice, AudioTransport};
    use windows::Win32::Devices::FunctionDiscovery::{
        PKEY_Device_FriendlyName, PKEY_Device_InstanceId,
    };
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IDeviceTopology, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED, STGM_READ,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return None;
        }
        struct ComGuard;
        impl Drop for ComGuard {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _guard = ComGuard;
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let endpoint = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .ok()?;
        let id_ptr = endpoint.GetId().ok()?;
        let id_result = id_ptr.to_string();
        CoTaskMemFree(Some(id_ptr.0 as *const _));
        let id = id_result.ok()?;
        let store = endpoint.OpenPropertyStore(STGM_READ).ok()?;
        let read_string = |key: &windows::Win32::Foundation::PROPERTYKEY| -> Option<String> {
            let mut value: PROPVARIANT = store.GetValue(key).ok()?;
            let raw = &*value.Anonymous.Anonymous;
            let result = if raw.vt == VT_LPWSTR {
                raw.Anonymous.pwszVal.to_string().ok()
            } else {
                None
            };
            let _ = PropVariantClear(&mut value);
            result
        };
        let name = read_string(&PKEY_Device_FriendlyName).unwrap_or_else(|| id.clone());
        let instance = read_string(&PKEY_Device_InstanceId).unwrap_or_default();
        let topology_device_id = endpoint
            .Activate::<IDeviceTopology>(CLSCTX_ALL, None)
            .ok()
            .and_then(|topology| topology.GetConnector(0).ok())
            .and_then(|connector| connector.GetDeviceIdConnectedTo().ok())
            .and_then(|ptr| {
                let value = ptr.to_string().ok();
                CoTaskMemFree(Some(ptr.0 as *const _));
                value
            })
            .unwrap_or_default();
        let native_id = if topology_device_id.is_empty() {
            instance.clone()
        } else {
            topology_device_id.clone()
        };
        let lower = format!("{id} {instance} {topology_device_id}").to_ascii_lowercase();
        let bluetooth = lower.contains("bth") || lower.contains("bluetooth");
        let has_physical_id =
            !native_id.is_empty() && !native_id.to_ascii_lowercase().contains("swd\\mmdevapi");
        let known_other = !bluetooth && has_physical_id;
        let identity = native_id
            .split("DEV_")
            .nth(1)
            .and_then(|v| v.get(..12))
            .filter(|v| v.chars().all(|c| c.is_ascii_hexdigit()))
            .unwrap_or(&id)
            .to_string();
        Some(ActiveAudioDevice {
            identity,
            name,
            transport: if bluetooth {
                AudioTransport::Bluetooth
            } else if known_other {
                AudioTransport::Other
            } else {
                AudioTransport::Unknown
            },
        })
    }
}

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
        if is_supported_headphone(&name) {
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

fn service_guid(uuid: &str) -> Result<GUID, String> {
    match uuid {
        crate::oneplus::CURRENT_SERVICE_UUID => {
            Ok(GUID::from_u128(0x0000079a_d102_11e1_9b23_00025b00a5a5))
        }
        crate::oneplus::SERVICE_UUID => Ok(GUID::from_u128(0x00001107_d102_11e1_9b23_00025b00a5a5)),
        "96cc203e-5068-46ad-b32d-e316f5e069ba" => {
            Ok(GUID::from_u128(0x96cc203e_5068_46ad_b32d_e316f5e069ba))
        }
        "956c7b26-d49a-4ba8-b03f-b17d393cb6e2" => {
            Ok(GUID::from_u128(0x956c7b26_d49a_4ba8_b03f_b17d393cb6e2))
        }
        _ => Err("Unrecognized control-service UUID.".into()),
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
    let services = super::service_uuids(device);
    for &service_uuid in services {
        let service = service_guid(service_uuid)?;
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
        // A completed polling deadline is a timeout, not the initial "in progress" error.
        last_code = WSAETIMEDOUT;
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
    let reason = match last_code {
        WSAEADDRNOTAVAIL => {
            "Windows could not resolve or use the headphone settings-service address/channel"
        }
        WSAETIMEDOUT => "The headphone settings-service connection timed out",
        _ => "The headphone settings service did not connect",
    };
    Err(format!("{reason} (Windows code {last_code}). Audio pairing can remain connected while this separate service is unavailable. Tried settings services: {}. No headphone setting command was sent.", services.join(", ")))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_service_ids_become_the_matching_winsock_guid() {
        for model in crate::oneplus_models::models() {
            let device = PairedDevice {
                address: "fixture".into(),
                name: model.name.clone(),
                model: model.name.clone(),
            };
            let uuids = super::super::service_uuids(&device);
            assert_eq!(uuids, [model.service_uuid.as_str()]);
            let guid = service_guid(uuids[0]).unwrap();
            assert_eq!(
                guid.data1,
                if model.service_uuid == crate::oneplus::CURRENT_SERVICE_UUID {
                    0x079a
                } else {
                    0x1107
                }
            );
            assert_eq!(guid.data2, 0xd102);
            assert_eq!(guid.data3, 0x11e1);
            assert_eq!(guid.data4, [0x9b, 0x23, 0, 2, 0x5b, 0, 0xa5, 0xa5]);
        }
        assert!(service_guid("00000000-0000-0000-0000-000000000000").is_err());
    }

    #[test]
    fn sony_settings_services_keep_their_original_guids() {
        let device = PairedDevice {
            address: "fixture".into(),
            name: "WH-1000XM6".into(),
            model: "WH-1000XM6".into(),
        };
        let guids: Vec<_> = super::super::service_uuids(&device)
            .iter()
            .map(|uuid| service_guid(uuid).unwrap().data1)
            .collect();
        assert_eq!(guids, [0x96cc203e, 0x956c7b26]);
    }
}
