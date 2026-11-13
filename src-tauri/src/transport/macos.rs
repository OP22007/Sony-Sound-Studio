//! Experimental Classic Bluetooth control transport through Apple's IOBluetooth.
//! Objective-C objects and callbacks stay on an owner thread. Caller deadlines
//! do not imply native cancellation: an unresolved write poisons the connection
//! and keeps the device lease until the native call returns and the channel closes.

use super::{
    is_supported_headphone, model_from_name, ActiveAudioDevice, AudioTransport,
    DeviceActivitySnapshot, PairedDevice,
};
use super::macos_state::{uuid_bytes, ChannelState, MAX_RECEIVE};
use super::worker::{self, Driver, Signals};
use objc2::rc::{autoreleasepool, Retained};
use objc2::{define_class, msg_send, AnyThread, DefinedClass};
use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSObject, NSObjectProtocol, NSRunLoop, NSString};
use objc2_io_bluetooth::{IOBluetoothDevice, IOBluetoothRFCOMMChannel, IOBluetoothRFCOMMChannelDelegate, IOBluetoothSDPUUID};
use std::cell::RefCell;
use std::ffi::c_void;
use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn activity_snapshot() -> Result<super::DeviceActivitySnapshot, String> {
    autoreleasepool(|_| {
        let Some(paired) = (unsafe { IOBluetoothDevice::pairedDevices() }) else {
            return Ok(DeviceActivitySnapshot::default());
        };
        let mut snapshot = DeviceActivitySnapshot::default();
        for index in 0..paired.count().min(256) {
            let item = paired.objectAtIndex(index);
            let Some(device) = item.downcast_ref::<IOBluetoothDevice>() else {
                continue;
            };
            if !unsafe { device.isConnected() } {
                continue;
            }
            let name = unsafe { device.name() }.to_string();
            if !is_supported_headphone(&name) {
                continue;
            }
            if let Some(address) = unsafe { device.addressString() } {
                snapshot.bluetooth_connected.push(ActiveAudioDevice {
                    identity: address.to_string(),
                    name: name.chars().filter(|c| !c.is_control()).take(128).collect(),
                    transport: AudioTransport::Bluetooth,
                });
            }
        }
        snapshot
            .bluetooth_connected
            .sort_by(|a, b| a.identity.cmp(&b.identity));
        snapshot
            .bluetooth_connected
            .dedup_by(|a, b| a.identity == b.identity);
        Ok(snapshot)
    })
}

pub fn enumerate() -> Result<Vec<PairedDevice>, String> {
    autoreleasepool(|_| {
        let Some(paired) = (unsafe { IOBluetoothDevice::pairedDevices() }) else {
            return Ok(vec![]);
        };
        let mut devices = Vec::new();
        for index in 0..paired.count().min(256) {
            let item = paired.objectAtIndex(index);
            let Some(device) = item.downcast_ref::<IOBluetoothDevice>() else {
                continue;
            };
            let name = unsafe { device.name() }.to_string();
            if !is_supported_headphone(&name) {
                continue;
            }
            if let Some(address) = unsafe { device.addressString() } {
                devices.push(PairedDevice {
                    address: address.to_string(),
                    model: model_from_name(&name),
                    name: name.chars().filter(|c| !c.is_control()).take(128).collect(),
                });
            }
        }
        devices.sort_by(|a, b| a.address.cmp(&b.address));
        devices.dedup_by(|a, b| a.address == b.address);
        devices.truncate(128);
        Ok(devices)
    })
}

// NSObject has no subclass requirements. The delegate, its RefCell, the channel
// and the run loop are never sent to another thread or accessed concurrently.
define_class!(
    #[unsafe(super = NSObject)]
    #[name = "SoundStudioRFCOMMDelegate"]
    #[ivars = RefCell<ChannelState>]
    struct ChannelDelegate;

    unsafe impl NSObjectProtocol for ChannelDelegate {}

    unsafe impl IOBluetoothRFCOMMChannelDelegate for ChannelDelegate {
        #[unsafe(method(rfcommChannelOpenComplete:status:))]
        unsafe fn opened(&self, _channel: Option<&IOBluetoothRFCOMMChannel>, status: i32) {
            self.ivars().borrow_mut().opened = Some(status);
        }

        #[unsafe(method(rfcommChannelClosed:))]
        unsafe fn closed(&self, _channel: Option<&IOBluetoothRFCOMMChannel>) {
            self.ivars().borrow_mut().closed = true;
        }

        #[unsafe(method(rfcommChannelData:data:length:))]
        unsafe fn data(&self, _channel: Option<&IOBluetoothRFCOMMChannel>, bytes: *mut c_void, length: usize) {
            let mut state = self.ivars().borrow_mut();
            if length == 0 { return; }
            if bytes.is_null() || length > MAX_RECEIVE {
                state.closed = true;
                state.overflow = true;
                return;
            }
            // The framework owns this memory for the duration of this callback.
            // Copy immediately; no pointer or borrowed slice escapes the callback.
            state.receive(unsafe { std::slice::from_raw_parts(bytes.cast::<u8>(), length) });
        }
    }
);

impl ChannelDelegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(RefCell::new(ChannelState::default()));
        unsafe { msg_send![super(this), init] }
    }
}

fn pump(run_loop: &NSRunLoop, deadline: Instant) {
    let duration = deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(10));
    if duration.is_zero() { return; }
    let started = Instant::now();
    autoreleasepool(|_| {
        let until = NSDate::dateWithTimeIntervalSinceNow(duration.as_secs_f64());
        // No RefCell borrow may be held here: callbacks can re-enter the delegate.
        run_loop.runMode_beforeDate(unsafe { NSDefaultRunLoopMode }, &until);
    });
    // A loop without sources can return immediately. Avoid a busy spin during SDP.
    if started.elapsed() < Duration::from_millis(1) {
        std::thread::sleep(duration.min(Duration::from_millis(2)));
    }
}

fn native_error(operation: &str, code: i32) -> String {
    format!("macOS Bluetooth {operation} failed (0x{:08x}). Check Bluetooth permission in System Settings > Privacy & Security, wake the headphones and close other headphone-control apps.", code as u32)
}

struct NativeChannel {
    channel: Retained<IOBluetoothRFCOMMChannel>,
    delegate: Retained<ChannelDelegate>,
    run_loop: Retained<NSRunLoop>,
    _device: Retained<IOBluetoothDevice>,
}

impl Drop for NativeChannel {
    fn drop(&mut self) {
        // Close only this control channel; never close the device's audio link.
        // No asynchronous write buffer exists: writeSync's owned bytes stay alive
        // until it returns. Keep the delegate alive throughout close, then detach.
        unsafe {
            self.channel.closeChannel();
            self.channel.setDelegate(None);
        }
        // IOBluetooth may already have queued a callback for the delegate when it
        // is detached (the Chromium FB13705522 hazard that sony-device-center also
        // works around). Keep the delegate and channel alive while the run loop
        // delivers or discards them, instead of freeing them under a queued call.
        let drain_until = Instant::now() + Duration::from_millis(40);
        while Instant::now() < drain_until {
            pump(&self.run_loop, drain_until);
        }
    }
}

impl Driver for NativeChannel {
    fn read(&mut self, buffer: &mut [u8], deadline: Instant, signals: &Signals) -> io::Result<usize> {
        if buffer.is_empty() { return Ok(0); }
        loop {
            if signals.stopped() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "Bluetooth control connection was cancelled."));
            }
            {
                let mut state = self.delegate.ivars().borrow_mut();
                if state.overflow {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Bluetooth receive buffer overflowed; reconnect before using controls."));
                }
                let count = state.read(buffer);
                if count != 0 { return Ok(count); }
                if state.closed { return Ok(0); }
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "No Bluetooth data before the read deadline."));
            }
            pump(&self.run_loop, deadline);
        }
    }

    fn write(&mut self, bytes: &[u8], deadline: Instant, signals: &Signals) -> io::Result<()> {
        if self.delegate.ivars().borrow().closed {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "Bluetooth control channel closed."));
        }
        let mtu = usize::from(unsafe { self.channel.getMTU() });
        if mtu == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bluetooth returned an invalid RFCOMM MTU."));
        }
        let mut owned = bytes.to_vec();
        for chunk in owned.chunks_mut(mtu) {
            if signals.stopped() || Instant::now() >= deadline {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "Bluetooth write outcome is unknown; reconnect and read the device state before another change."));
            }
            // This call may block. Only the owner thread can block: the proxy
            // enforces the caller's deadline and poisons the lease on timeout.
            // The owned Vec remains valid even after that caller has returned.
            let status = autoreleasepool(|_| unsafe {
                self.channel.writeSync_length(chunk.as_mut_ptr().cast(), chunk.len() as u16)
            });
            if status != 0 {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, native_error("write", status)));
            }
        }
        if signals.stopped() || Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "Bluetooth write outcome is unknown; no retry or restoration command was sent."));
        }
        Ok(()) // This is transport completion, never device-setting confirmation.
    }
}

fn open(device: PairedDevice, signals: Arc<Signals>, deadline: Instant) -> Result<(Box<dyn Driver>, &'static str), String> {
    autoreleasepool(|_| {
        if signals.stopped() { return Err("Bluetooth connection was cancelled.".into()); }
        // Construct every Objective-C object here, on its eventual owner thread.
        let address = NSString::from_str(&device.address);
        let native = unsafe { IOBluetoothDevice::deviceWithAddressString(Some(&address)) }
            .ok_or("macOS could not resolve the paired Bluetooth device.")?;
        if !unsafe { native.isPaired() } {
            return Err("Pair the headphones in macOS Bluetooth settings before connecting.".into());
        }
        let run_loop = NSRunLoop::currentRunLoop();
        let previous_update = unsafe { native.getLastServicesUpdate() }
            .map(|date| date.timeIntervalSinceReferenceDate());
        // nil target deliberately avoids an uncancellable SDP completion target
        // outliving its Rust owner. Poll the native service-update stamp instead.
        let status = unsafe { native.performSDPQuery(None) };
        if status != 0 { return Err(native_error("service discovery", status)); }
        let sdp_deadline = deadline.min(Instant::now() + Duration::from_secs(6));
        loop {
            if signals.stopped() { return Err("Bluetooth service discovery was cancelled.".into()); }
            let updated = unsafe { native.getLastServicesUpdate() }
                .map(|date| date.timeIntervalSinceReferenceDate());
            if updated.is_some() && updated != previous_update { break; }
            if Instant::now() >= sdp_deadline {
                return Err("macOS Bluetooth service discovery timed out. Wake the headphones and check Bluetooth permission; cached service records were not used.".into());
            }
            pump(&run_loop, sdp_deadline);
        }
        let mut last_error = None;
        for &service in super::service_uuids(&device) {
            if signals.stopped() || Instant::now() >= deadline { break; }
            let bytes = uuid_bytes(service).ok_or("Invalid control service UUID in the app registry.")?;
            let uuid = unsafe { IOBluetoothSDPUUID::uuidWithBytes_length(bytes.as_ptr().cast(), 16) }
                .ok_or("macOS could not construct the control service UUID.")?;
            let Some(record) = (unsafe { native.getServiceRecordForUUID(Some(&uuid)) }) else { continue; };
            let mut channel_id = 0;
            let status = unsafe { record.getRFCOMMChannelID(&mut channel_id) };
            if status != 0 || !(1..=30).contains(&channel_id) {
                last_error = Some("The advertised control service has no valid RFCOMM channel.".into());
                continue;
            }
            let delegate = ChannelDelegate::new();
            let mut channel = None;
            let status = unsafe {
                native.openRFCOMMChannelAsync_withChannelID_delegate(Some(&mut channel), channel_id, Some(&delegate))
            };
            if status != 0 {
                last_error = Some(native_error("control connection", status));
                continue;
            }
            let channel = channel.ok_or("macOS accepted the RFCOMM open without returning a channel.")?;
            // RAII closes/detaches on every success, error and cancellation path.
            let owner = NativeChannel { channel, delegate, run_loop: run_loop.clone(), _device: native.clone() };
            let open_deadline = deadline.min(Instant::now() + Duration::from_secs(3));
            loop {
                if signals.stopped() || Instant::now() >= open_deadline {
                    last_error = Some("macOS control connection timed out or was cancelled. No device setting command was sent.".into());
                    break;
                }
                let (opened, closed) = { let state = owner.delegate.ivars().borrow(); (state.opened, state.closed) };
                if let Some(status) = opened {
                    if status == 0 && !closed {
                        return Ok((Box::new(owner) as Box<dyn Driver>, service));
                    }
                    last_error = Some(native_error("control channel open", status));
                    break;
                }
                if closed { last_error = Some("The headphones closed the control channel while connecting.".into()); break; }
                pump(&run_loop, open_deadline);
            }
        }
        Err(last_error.unwrap_or_else(|| "No compatible Sony or OnePlus control service was found in the headphones' fresh service records.".into()))
    })
}

pub fn connect(device: &PairedDevice, cancelled: Arc<AtomicBool>) -> Result<Box<dyn super::Connection>, String> {
    let paired = device.clone();
    worker::connect(device.address.clone(), cancelled, move |signals, deadline| open(paired, signals, deadline))
}
