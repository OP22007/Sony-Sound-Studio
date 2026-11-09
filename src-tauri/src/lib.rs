mod app_updates;
mod control_session;
mod device_controls;
mod device_monitor;
mod device_observer;
mod discovery;
mod media_automation;
mod media_vlc;
#[cfg(windows)]
mod media_windows;
mod oneplus;
mod oneplus_eq;
mod oneplus_models;
mod output_levels;
mod preferences;
pub mod protocol;
mod sony_listening;
mod transport;
mod vlc_connection;

use preferences::{AppPreferences, PreferencesStore};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;

const MAX_DIAGNOSTICS: usize = 64;
const MAX_SESSIONS: usize = 8;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    id: String,
    name: String,
    model: String,
    connection: ConnectionState,
    battery: Option<u8>,
    firmware: Option<String>,
    capabilities: Vec<String>,
    settings: BTreeMap<String, Value>,
}

fn apply_battery_level(device: &mut Device, level: u8) {
    device.battery = Some(level);
    // Receipt time of a real device reply, never an unrelated UI refresh.
    if let Ok(elapsed) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        device.settings.insert(
            "batteryReadAtMs".into(),
            Value::from(elapsed.as_millis() as u64),
        );
    } else {
        device.settings.remove("batteryReadAtMs");
    }
}

fn apply_noise_readout(device: &mut Device, noise: &discovery::NoiseInfo) {
    device
        .settings
        .insert("noise".into(), Value::from(noise.mode));
    device
        .settings
        .insert("ambient".into(), Value::from(noise.ambient_level));
    device
        .settings
        .insert("noiseInquiry".into(), Value::from(noise.inquiry));
    device.settings.insert(
        "noiseConfirmation".into(),
        Value::from(if noise.value_change_status == 1 {
            "confirmed"
        } else {
            "unconfirmed"
        }),
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    revision: u64,
    platform: String,
    transport_status: String,
    devices: Vec<Device>,
    active_output_device_id: Option<String>,
    audio_output_observed: bool,
    diagnostics: Vec<String>,
}

struct Session {
    generation: u64,
    cancelled: Arc<AtomicBool>,
    commands: SyncSender<ControlCommand>,
}

struct ControlCommand {
    change: discovery::SettingChange,
    reply: SyncSender<Result<discovery::SettingConfirmation, String>>,
    automation: Option<(Arc<media_automation::Automation>, u64)>,
}

struct Inner {
    revision: u64,
    devices: BTreeMap<String, Device>,
    active_output_device_id: Option<String>,
    audio_output_observed: bool,
    catalogue: BTreeMap<String, transport::PairedDevice>,
    sessions: BTreeMap<String, Session>,
    diagnostics: VecDeque<String>,
    next_device_id: u64,
    next_generation: u64,
    discovering: bool,
    reconnect_jobs: BTreeMap<String, Arc<AtomicBool>>,
    reconnect_suppressed: BTreeSet<String>,
}

pub struct Runtime {
    inner: Mutex<Inner>,
    stopping: AtomicBool,
}

struct PreferencesState {
    store: PreferencesStore,
    current: Mutex<AppPreferences>,
}

impl Runtime {
    fn new() -> Self {
        let mut inner = Inner {
            revision: 0,
            devices: BTreeMap::new(),
            active_output_device_id: None,
            audio_output_observed: false,
            catalogue: BTreeMap::new(),
            sessions: BTreeMap::new(),
            diagnostics: VecDeque::new(),
            next_device_id: 1,
            next_generation: 1,
            discovering: false,
            reconnect_jobs: BTreeMap::new(),
            reconnect_suppressed: BTreeSet::new(),
        };
        inner.log("Native companion started. Device settings remain unavailable until protocol capabilities are validated.");
        Self {
            inner: Mutex::new(inner),
            stopping: AtomicBool::new(false),
        }
    }

    fn snapshot(&self) -> Result<AppState, String> {
        self.inner
            .lock()
            .map(|inner| inner.snapshot())
            .map_err(|_| "Native state is unavailable; restart the application.".into())
    }

    fn stop_all(&self) {
        self.stopping.store(true, Ordering::Release);
        if let Ok(mut inner) = self.inner.lock() {
            for cancelled in inner.reconnect_jobs.values() {
                cancelled.store(true, Ordering::Release);
            }
            inner.reconnect_jobs.clear();
            for session in inner.sessions.values() {
                session.cancelled.store(true, Ordering::Release);
            }
            inner.sessions.clear();
            for device in inner.devices.values_mut() {
                device.connection = ConnectionState::Disconnected;
                device.battery = None;
                device.capabilities.clear();
                device.settings.clear();
            }
            inner.log(
                "All control-service sessions were cancelled and live device readouts invalidated.",
            );
        }
    }

    fn cancel_reconnect(&self, device_id: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(cancelled) = inner.reconnect_jobs.remove(device_id) {
                cancelled.store(true, Ordering::Release);
            }
        }
    }

    fn suppress_reconnect(&self, device_id: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.reconnect_suppressed.insert(device_id.to_owned());
            if let Some(cancelled) = inner.reconnect_jobs.remove(device_id) {
                cancelled.store(true, Ordering::Release);
            }
        }
    }

    fn claim_reconnect(&self, device_id: &str) -> Option<Arc<AtomicBool>> {
        let mut inner = self.inner.lock().ok()?;
        if self.stopping.load(Ordering::Acquire)
            || inner.reconnect_suppressed.contains(device_id)
            || inner.sessions.contains_key(device_id)
            || inner.reconnect_jobs.contains_key(device_id)
        {
            return None;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        inner
            .reconnect_jobs
            .insert(device_id.to_owned(), Arc::clone(&cancelled));
        inner.log("Automatic control connection scheduled with bounded retries.");
        Some(cancelled)
    }
}

impl Inner {
    fn snapshot(&self) -> AppState {
        AppState {
            revision: self.revision,
            platform: std::env::consts::OS.into(),
            transport_status: transport::status().into(),
            devices: self.devices.values().cloned().collect(),
            active_output_device_id: self.active_output_device_id.clone(),
            audio_output_observed: self.audio_output_observed,
            diagnostics: self.diagnostics.iter().cloned().collect(),
        }
    }

    fn log(&mut self, message: &str) {
        // Every observable state mutation commits through this method while holding the same lock.
        // Event and invoke consumers use this revision to reject snapshots delivered out of order.
        self.revision = self
            .revision
            .checked_add(1)
            .expect("state revision exhausted");
        if self.diagnostics.len() == MAX_DIAGNOSTICS {
            self.diagnostics.pop_front();
        }
        // Logs exclude device addresses and identifiers. The explicit read-only NC/ASM probe may
        // add bounded protocol response bytes so the user can review their device's schema.
        self.diagnostics
            .push_back(message.chars().take(400).collect());
    }

    fn is_current(&self, id: &str, generation: u64) -> bool {
        self.sessions
            .get(id)
            .is_some_and(|s| s.generation == generation && !s.cancelled.load(Ordering::Acquire))
    }
}

fn emit_state(app: &AppHandle, runtime: &Runtime) {
    if let Ok(snapshot) = runtime.snapshot() {
        let _ = app.emit("app-state", snapshot);
    }
}

#[tauri::command]
fn get_app_state(runtime: State<'_, Arc<Runtime>>) -> Result<AppState, String> {
    runtime.snapshot()
}

#[tauri::command]
async fn get_output_levels(
    device_id: String,
    runtime: State<'_, Arc<Runtime>>,
) -> Result<Option<output_levels::OutputLevels>, String> {
    let runtime = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let expected_name = {
            let inner = runtime
                .inner
                .lock()
                .map_err(|_| "Device state unavailable.")?;
            if inner.active_output_device_id.as_deref() != Some(device_id.as_str())
                || !inner.audio_output_observed
            {
                return Ok(None);
            }
            let Some(device) = inner
                .devices
                .get(&device_id)
                .filter(|d| d.connection == ConnectionState::Connected)
            else {
                return Ok(None);
            };
            // Same-name devices cannot safely be distinguished by the meter endpoint name.
            let name = transport::normalize_endpoint_name(&device.name);
            if inner
                .devices
                .values()
                .filter(|d| transport::normalize_endpoint_name(&d.name) == name)
                .count()
                != 1
            {
                return Ok(None);
            }
            device.name.clone()
        };
        let levels = output_levels::read(&expected_name);
        let inner = runtime
            .inner
            .lock()
            .map_err(|_| "Device state unavailable.")?;
        if inner.active_output_device_id.as_deref() != Some(device_id.as_str())
            || !inner.audio_output_observed
            || !inner
                .devices
                .get(&device_id)
                .is_some_and(|d| d.connection == ConnectionState::Connected)
        {
            return Ok(None);
        }
        Ok(levels)
    })
    .await
    .map_err(|_| "Output meter unavailable.".to_string())?
}

#[tauri::command]
fn get_app_preferences(app: AppHandle) -> Result<AppPreferences, String> {
    let state = app.state::<PreferencesState>();
    let mut preferences = state
        .current
        .lock()
        .map_err(|_| "Preferences are unavailable.")?
        .clone();
    preferences.launch_at_login = app
        .autolaunch()
        .is_enabled()
        .map_err(|_| "Could not read the system launch-at-login setting.")?;
    Ok(preferences)
}

#[tauri::command]
async fn update_app_preferences(
    app: AppHandle,
    preferences: AppPreferences,
) -> Result<AppPreferences, String> {
    let worker_app = app.clone();
    let saved = tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<PreferencesState>();
        let mut current = state
            .current
            .lock()
            .map_err(|_| "Preferences are unavailable.")?;
        let launcher = worker_app.autolaunch();
        let previous_autostart = launcher
            .is_enabled()
            .map_err(|_| "Could not read the system launch-at-login setting.")?;
        preferences::save_with_autostart(
            &state.store,
            &preferences,
            previous_autostart,
            |enabled| {
                if enabled {
                    launcher.enable()
                } else {
                    launcher.disable()
                }
                .map_err(|_| "The operating system could not change launch at login.".to_string())
            },
        )?;
        *current = preferences.clone();
        drop(current);
        if !preferences.auto_reconnect {
            let runtime = worker_app.state::<Arc<Runtime>>();
            if let Ok(mut inner) = runtime.inner.lock() {
                for cancelled in inner.reconnect_jobs.values() {
                    cancelled.store(true, Ordering::Release);
                }
                inner.reconnect_jobs.clear();
            };
        }
        Ok::<_, String>(preferences)
    })
    .await
    .map_err(|_| "The preference save worker stopped unexpectedly.")??;
    let _ = app.emit("app-preferences", &saved);
    Ok(saved)
}

#[tauri::command]
async fn discover_devices(
    app: AppHandle,
    runtime: State<'_, Arc<Runtime>>,
) -> Result<AppState, String> {
    discover_devices_internal(app, Arc::clone(runtime.inner())).await
}

async fn discover_devices_internal(
    app: AppHandle,
    runtime: Arc<Runtime>,
) -> Result<AppState, String> {
    {
        let mut inner = runtime
            .inner
            .lock()
            .map_err(|_| "Native state unavailable.")?;
        if inner.discovering {
            return Err("Device discovery is already running.".into());
        }
        inner.discovering = true;
    }
    let result = tauri::async_runtime::spawn_blocking(transport::enumerate).await;
    let mut inner = runtime
        .inner
        .lock()
        .map_err(|_| "Native state unavailable.")?;
    inner.discovering = false;
    let found = match result {
        Ok(Ok(devices)) => devices,
        Ok(Err(error)) => {
            inner.log("Discovery failed. Check the operating system's Bluetooth service and pairing settings.");
            drop(inner);
            emit_state(&app, &runtime);
            return Err(error);
        }
        Err(_) => {
            inner.log("Discovery worker stopped unexpectedly.");
            return Err("Device discovery stopped; retry the scan.".into());
        }
    };
    let mut refreshed = BTreeMap::new();
    let mut catalogue = BTreeMap::new();
    for paired in found.into_iter().take(128) {
        let id = inner
            .catalogue
            .iter()
            .find(|(_, old)| old.address == paired.address)
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| {
                let prefix = if oneplus_models::is_oneplus_name(&paired.model) {
                    "oneplus"
                } else {
                    "sony"
                };
                let id = format!("{prefix}-{:04}", inner.next_device_id);
                inner.next_device_id += 1;
                id
            });
        let existing = inner.devices.get(&id).cloned();
        let device = existing.unwrap_or_else(|| Device {
            id: id.clone(),
            name: paired.name.clone(),
            model: paired.model.clone(),
            connection: ConnectionState::Disconnected,
            battery: None,
            firmware: None,
            capabilities: vec![],
            settings: BTreeMap::new(),
        });
        refreshed.insert(id.clone(), device);
        catalogue.insert(id, paired);
    }
    // A cached enumeration can omit a currently active device. Never destroy its live session.
    for id in inner.sessions.keys() {
        if let Some(device) = inner.devices.get(id) {
            refreshed
                .entry(id.clone())
                .or_insert_with(|| device.clone());
        }
        if let Some(device) = inner.catalogue.get(id) {
            catalogue
                .entry(id.clone())
                .or_insert_with(|| device.clone());
        }
    }
    inner.devices = refreshed;
    inner.catalogue = catalogue;
    let count = inner.devices.len();
    inner.log(&format!("Discovery completed: {count} paired supported headphone entries. Pairing does not imply a live control connection."));
    let snapshot = inner.snapshot();
    drop(inner);
    let _ = app.emit("app-state", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
async fn connect_device(
    app: AppHandle,
    runtime: State<'_, Arc<Runtime>>,
    device_id: String,
) -> Result<AppState, String> {
    runtime.cancel_reconnect(&device_id);
    connect_device_internal(app, Arc::clone(runtime.inner()), device_id, false).await
}

async fn connect_device_internal(
    app: AppHandle,
    runtime: Arc<Runtime>,
    device_id: String,
    automatic: bool,
) -> Result<AppState, String> {
    validate_id(&device_id)?;
    if runtime.stopping.load(Ordering::Acquire) {
        return Err("The application is shutting down.".into());
    }
    if automatic && !automatic_reconnect_enabled(&app) {
        return Err("Automatic reconnect is disabled.".into());
    }
    let (paired, generation, cancelled, command_receiver) = {
        let mut inner = runtime
            .inner
            .lock()
            .map_err(|_| "Native state unavailable.")?;
        let paired = inner
            .catalogue
            .get(&device_id)
            .cloned()
            .ok_or("Device is not in the current paired-device list. Scan again.")?;
        if automatic && inner.reconnect_suppressed.contains(&device_id) {
            return Err("Automatic reconnect was cancelled by an explicit disconnect.".into());
        }
        if !automatic {
            inner.reconnect_suppressed.remove(&device_id);
        }
        if inner.sessions.contains_key(&device_id) {
            return Err("This device is already connected or connecting.".into());
        }
        if inner.sessions.len() >= MAX_SESSIONS {
            return Err("The active device session limit has been reached.".into());
        }
        let generation = inner.next_generation;
        inner.next_generation += 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        let (commands, command_receiver) = mpsc::sync_channel(8);
        inner.sessions.insert(
            device_id.clone(),
            Session {
                generation,
                cancelled: Arc::clone(&cancelled),
                commands,
            },
        );
        if let Some(device) = inner.devices.get_mut(&device_id) {
            device.connection = ConnectionState::Connecting;
            device.battery = None;
            device.capabilities.clear();
            device.settings.clear();
        }
        inner.log(
            "Connecting to the device's control service. No device setting command will be sent.",
        );
        (paired, generation, cancelled, command_receiver)
    };
    emit_state(&app, &runtime);
    let reconnect_identity = paired.address.clone();
    let connect_cancel = Arc::clone(&cancelled);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut connection = transport::connect(&paired, Arc::clone(&connect_cancel))?;
        let (basics, read_session) = control_session::read_basics(&paired, &mut *connection, &connect_cancel)?;
        if !read_session.is_synchronized() {
            return Err("Device capability discovery lost protocol synchronization; reconnect before retrying.".into());
        }
        Ok::<_, String>((connection, basics, read_session, command_receiver))
    })
    .await;
    let result = result
        .map_err(|_| "Bluetooth connection worker stopped unexpectedly.".to_string())
        .and_then(|result| result);
    let mut inner = runtime
        .inner
        .lock()
        .map_err(|_| "Native state unavailable.")?;
    if !inner.is_current(&device_id, generation) {
        return Err("Connection was cancelled; stale connection results were discarded.".into());
    }
    if automatic && !automatic_reconnect_enabled(&app) {
        cancelled.store(true, Ordering::Release);
        inner.sessions.remove(&device_id);
        if let Some(device) = inner.devices.get_mut(&device_id) {
            device.connection = ConnectionState::Disconnected;
        }
        inner.log("An automatic connection attempt was cancelled because automatic reconnect was disabled.");
        drop(inner);
        emit_state(&app, &runtime);
        return Err("Automatic reconnect is disabled.".into());
    }
    let (connection, basics, read_session, command_receiver) = match result {
        Ok(result) => result,
        Err(error) => {
            inner.sessions.remove(&device_id);
            if let Some(device) = inner.devices.get_mut(&device_id) {
                device.connection = ConnectionState::Disconnected;
            }
            let detail: String = error
                .chars()
                .filter(|character| !character.is_control())
                .take(320)
                .collect();
            inner.log(&format!(
                "Control-service connection failed: {detail}. No device setting command was sent."
            ));
            drop(inner);
            emit_state(&app, &runtime);
            return Err(error);
        }
    };
    if let Some(device) = inner.devices.get_mut(&device_id) {
        device.connection = ConnectionState::Connected;
        device.settings.extend(read_session.metadata());
        device
            .settings
            .insert("serviceUuid".into(), Value::from(connection.service_uuid()));
        if let Some(model) = &basics.model {
            device.model = model.clone();
        }
        device.firmware = basics.firmware.clone();
        device.battery = basics.battery.as_ref().map(|battery| battery.level);
        device.capabilities = basics.controls.capabilities.clone();
        device.settings.insert(
            "supportReplyConfirmed".into(),
            Value::from(basics.controls.support_reply_confirmed),
        );
        match device_controls::settings_map(basics.controls.device_controls.clone()) {
            Ok(settings) => device.settings.extend(settings),
            Err(_) => {
                // A malformed descriptor must never expose a partially valid
                // editable control. Keep independently verified NC/EQ intact.
                device
                    .settings
                    .insert("controlDescriptors".into(), Value::Array(vec![]));
            }
        }
        for (key, value) in &basics.controls.read_only_values {
            device.settings.insert(key.clone(), value.as_json());
        }
        if let Some(noise) = &basics.controls.noise {
            apply_noise_readout(device, noise);
            device.settings.remove("noiseRequested");
        }
        if let Some(equalizer) = &basics.controls.equalizer {
            device
                .settings
                .insert("equalizer".into(), serde_json::json!(equalizer.bands));
            device.settings.insert(
                "equalizerPresetCode".into(),
                Value::from(equalizer.preset_code),
            );
        }
        if let Some(battery) = &basics.battery {
            apply_battery_level(device, battery.level);
            device
                .settings
                .insert("batteryCharging".into(), Value::from(battery.charging));
        }
        match &basics.protocol {
            Ok(info) => {
                device
                    .settings
                    .insert("protocolFormat".into(), Value::from(info.format));
                if info.format != "heymelody-opo-v1" {
                    device
                        .settings
                        .insert("protocolVersionRaw".into(), Value::from(info.version));
                } else {
                    // OPO v1 identifies our wrapper implementation, not a queried device version.
                    device.settings.remove("protocolVersionRaw");
                }
                device.settings.insert(
                    "protocolStatus".into(),
                    Value::from(if info.format == "heymelody-opo-v1" {
                        "HeyMelody product ID and capability replies confirmed; controls require matching device readback"
                    } else if basics.controls.support_reply_confirmed {
                        "Protocol and MDR V2 function support replies confirmed; active settings came from matched device GET responses"
                    } else {
                        "Read-only protocol reply confirmed; MDR V2 function support was unavailable"
                    }),
                );
            }
            Err(_) => {
                device.settings.insert(
                    "protocolStatus".into(),
                    Value::from("Socket connected; protocol reply not confirmed"),
                );
            }
        }
    }
    match basics.protocol {
        Ok(info) if info.format == "heymelody-opo-v1" => inner.log("HeyMelody OPO v1 adapter selected. Product ID and live capability replies confirmed; this does not establish hardware validation."),
        Ok(info) => inner.log(&format!("Read-only protocol-info confirmed: format {}, raw version {}, transport ACK observed {}. Feature capabilities remain unvalidated.", info.format, info.version, info.ack_observed)),
        Err(error) => inner.log(&error),
    }
    for warning in basics.warnings {
        inner.log(&warning);
    }
    let snapshot = inner.snapshot();
    drop(inner);
    if let Some(preferences) = app.try_state::<PreferencesState>() {
        if preferences
            .store
            .save_last_connected_device(&reconnect_identity)
            .is_err()
        {
            if let Ok(mut inner) = runtime.inner.lock() {
                inner.log(
                    "Connected, but the last device could not be saved for automatic reconnect.",
                );
            }
        }
    }
    let reader_runtime = Arc::clone(&runtime);
    let reader_app = app.clone();
    std::thread::Builder::new()
        .name("headphone-control-reader".into())
        .spawn(move || {
            let reconnect_id = device_id.clone();
            let unexpected_disconnect = observe_connection(
                reader_runtime,
                device_id,
                generation,
                cancelled,
                connection,
                read_session,
                command_receiver,
                |snapshot| {
                    let _ = reader_app.emit("app-state", snapshot);
                },
            );
            if unexpected_disconnect {
                schedule_reconnect(reader_app, reconnect_id);
            }
        })
        .map_err(|_| {
            runtime.stop_all();
            "Cannot start Bluetooth reader; connections were closed.".to_string()
        })?;
    let _ = app.emit("app-state", &snapshot);
    Ok(snapshot)
}

fn observe_connection(
    runtime: Arc<Runtime>,
    device_id: String,
    generation: u64,
    cancelled: Arc<AtomicBool>,
    mut connection: Box<dyn transport::Connection>,
    read_session: impl Into<control_session::Session>,
    command_receiver: Receiver<ControlCommand>,
    publish: impl Fn(AppState),
) -> bool {
    let mut read_session = read_session.into();
    let mut buffer = [0u8; 4096];
    let mut counts = (0usize, 0usize);
    let mut last_summary = Instant::now();
    while !cancelled.load(Ordering::Acquire) {
        match command_receiver.try_recv() {
            Ok(command) => {
                if command
                    .automation
                    .as_ref()
                    .is_some_and(|(automation, epoch)| !automation.authorizes(*epoch))
                {
                    let _ = command.reply.send(Err(
                        "Queued automatic mode request was cancelled before device I/O.".into(),
                    ));
                    continue;
                }
                let result =
                    read_session.apply_setting(&mut *connection, &cancelled, command.change);
                if !read_session.is_synchronized() {
                    cancelled.store(true, Ordering::Release);
                }
                let _ = command.reply.send(result);
                if cancelled.load(Ordering::Acquire) {
                    break;
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }
        match connection.read(&mut buffer, Duration::from_millis(200)) {
            Ok(0) => break,
            Ok(read) => {
                for frame in read_session.feed_frames(&buffer[..read]) {
                    match frame {
                        Ok(frame) => {
                            counts.0 += 1;
                            if discovery::acknowledge(&mut *connection, &frame).is_err() {
                                cancelled.store(true, Ordering::Release);
                                break;
                            }
                        }
                        Err(_) => counts.1 += 1,
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => break,
        }
        let nc_observations = read_session.drain_noise_observations();
        if !nc_observations.is_empty() {
            if let Ok(mut inner) = runtime.inner.lock() {
                if !inner.is_current(&device_id, generation) {
                    break;
                }
                for payload in nc_observations {
                    if let Some(noise) = read_session.parse_noise_notification(&payload) {
                        if let Some(device) = inner.devices.get_mut(&device_id) {
                            apply_noise_readout(device, &noise);
                            if noise.value_change_status == 1 {
                                device.settings.remove("noiseRequested");
                            }
                        }
                    }
                    let hex = payload
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    inner.log(&format!("Passive NC response bytes: {hex}"));
                }
            }
            if let Ok(snapshot) = runtime.snapshot() {
                publish(snapshot);
            }
        }
        let updates = read_session.drain_device_updates();
        if !updates.is_empty() {
            if let Ok(mut inner) = runtime.inner.lock() {
                if !inner.is_current(&device_id, generation) {
                    break;
                }
                if let Some(device) = inner.devices.get_mut(&device_id) {
                    for update in updates {
                        if device.capabilities.contains(&update.key) {
                            device
                                .settings
                                .insert(update.key, update.confirmed.as_json());
                        }
                    }
                }
            }
            if let Ok(snapshot) = runtime.snapshot() {
                publish(snapshot);
            }
        }
        let metadata = read_session.drain_metadata_updates();
        if !metadata.is_empty() {
            if let Ok(mut inner) = runtime.inner.lock() {
                if !inner.is_current(&device_id, generation) {
                    break;
                }
                if let Some(device) = inner.devices.get_mut(&device_id) {
                    if let Some(level) = metadata
                        .get("oneplusBatteryLevel")
                        .and_then(Value::as_u64)
                        .filter(|level| *level <= 100)
                    {
                        apply_battery_level(device, level as u8);
                    }
                    device.settings.extend(metadata);
                }
            }
            if let Ok(snapshot) = runtime.snapshot() {
                publish(snapshot);
            }
        }
        if last_summary.elapsed() >= Duration::from_secs(5) && counts != (0, 0) {
            if let Ok(mut inner) = runtime.inner.lock() {
                if !inner.is_current(&device_id, generation) {
                    break;
                }
                inner.log(&format!("Passive control stream: {} valid frames, {} rejected frames. Only bounded NC response bytes are recorded.", counts.0, counts.1));
            }
            counts = (0, 0);
            last_summary = Instant::now();
            if let Ok(snapshot) = runtime.snapshot() {
                publish(snapshot);
            }
        }
    }
    drop(connection);
    let mut unexpected_disconnect = false;
    if let Ok(mut inner) = runtime.inner.lock() {
        if inner
            .sessions
            .get(&device_id)
            .is_some_and(|session| session.generation == generation)
        {
            unexpected_disconnect = !runtime.stopping.load(Ordering::Acquire);
            inner.sessions.remove(&device_id);
            if let Some(device) = inner.devices.get_mut(&device_id) {
                device.connection = ConnectionState::Disconnected;
                device.battery = None;
                device.capabilities.clear();
                device.settings.clear();
            }
            inner.log("Control-service connection ended. Pending and confirmed capability state was invalidated.");
        }
    }
    if let Ok(snapshot) = runtime.snapshot() {
        publish(snapshot);
    }
    unexpected_disconnect
}

fn automatic_reconnect_enabled(app: &AppHandle) -> bool {
    app.try_state::<PreferencesState>()
        .and_then(|state| {
            state
                .current
                .lock()
                .ok()
                .map(|preferences| preferences.auto_reconnect)
        })
        .unwrap_or(false)
}

fn schedule_reconnect(app: AppHandle, device_id: String) {
    if !automatic_reconnect_enabled(&app) {
        return;
    }
    let runtime = Arc::clone(app.state::<Arc<Runtime>>().inner());
    let Some(cancelled) = runtime.claim_reconnect(&device_id) else {
        return;
    };
    emit_state(&app, &runtime);
    let worker_runtime = Arc::clone(&runtime);
    let worker_id = device_id.clone();
    let worker_cancelled = Arc::clone(&cancelled);
    let worker_app = app.clone();
    let spawned = std::thread::Builder::new().name("sony-reconnect".into()).spawn(move || {
        let mut connected = false;
        'attempts: for delay in [2, 5, 15, 30, 60] {
            for _ in 0..delay {
                if worker_cancelled.load(Ordering::Acquire) || worker_runtime.stopping.load(Ordering::Acquire) || !automatic_reconnect_enabled(&worker_app) { break 'attempts; }
                std::thread::sleep(Duration::from_secs(1));
            }
            if worker_cancelled.load(Ordering::Acquire) || worker_runtime.stopping.load(Ordering::Acquire) || !automatic_reconnect_enabled(&worker_app) { break; }
            if tauri::async_runtime::block_on(connect_device_internal(worker_app.clone(), Arc::clone(&worker_runtime), worker_id.clone(), true)).is_ok() {
                connected = true;
                break;
            }
        }
        if let Ok(mut inner) = worker_runtime.inner.lock() {
            if inner.reconnect_jobs.get(&worker_id).is_some_and(|active| Arc::ptr_eq(active, &worker_cancelled)) {
                inner.reconnect_jobs.remove(&worker_id);
                if !connected && !worker_cancelled.load(Ordering::Acquire) && !worker_runtime.stopping.load(Ordering::Acquire) {
                    inner.log("Automatic reconnect stopped after bounded retries. Wake the headphone and connect again from the studio.");
                }
            }
        }
        emit_state(&worker_app, &worker_runtime);
    });
    if spawned.is_err() {
        runtime.cancel_reconnect(&device_id);
        if let Ok(mut inner) = runtime.inner.lock() {
            inner.log("Could not start the reconnect worker. Connect manually from the studio.");
        }
        emit_state(&app, &runtime);
    }
}

#[tauri::command]
fn disconnect_device(
    app: AppHandle,
    runtime: State<'_, Arc<Runtime>>,
    device_id: String,
) -> Result<AppState, String> {
    validate_id(&device_id)?;
    runtime.suppress_reconnect(&device_id);
    let mut inner = runtime
        .inner
        .lock()
        .map_err(|_| "Native state unavailable.")?;
    if !inner.devices.contains_key(&device_id) {
        return Err("Unknown device; scan again.".into());
    }
    if let Some(session) = inner.sessions.remove(&device_id) {
        session.cancelled.store(true, Ordering::Release);
    }
    if let Some(device) = inner.devices.get_mut(&device_id) {
        device.connection = ConnectionState::Disconnected;
        device.battery = None;
        device.capabilities.clear();
        device.settings.clear();
    }
    inner.log(
        "Control-service session cancelled. The operating system's audio pairing is unchanged.",
    );
    let snapshot = inner.snapshot();
    drop(inner);
    let _ = app.emit("app-state", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
async fn apply_setting(
    app: AppHandle,
    runtime: State<'_, Arc<Runtime>>,
    device_id: String,
    setting: String,
    value: Value,
) -> Result<AppState, String> {
    apply_setting_internal(
        app,
        Arc::clone(runtime.inner()),
        device_id,
        setting,
        value,
        None,
    )
    .await
}

fn supports_listening_mode(device: &Device, mode: &str) -> bool {
    device
        .capabilities
        .iter()
        .any(|capability| capability == "listeningMode")
        && device
            .settings
            .get("controlDescriptors")
            .and_then(Value::as_array)
            .is_some_and(|descriptors| {
                descriptors.iter().any(|descriptor| {
                    descriptor.get("key").and_then(Value::as_str) == Some("listeningMode")
                        && descriptor
                            .get("options")
                            .and_then(Value::as_array)
                            .is_some_and(|options| {
                                options.iter().any(|option| {
                                    option.get("value").and_then(Value::as_str) == Some(mode)
                                })
                            })
                })
            })
}

async fn apply_setting_internal(
    app: AppHandle,
    runtime: Arc<Runtime>,
    device_id: String,
    setting: String,
    value: Value,
    automation_epoch: Option<u64>,
) -> Result<AppState, String> {
    validate_id(&device_id)?;
    if setting.is_empty()
        || setting.len() > 80
        || !setting
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("Invalid setting name.".into());
    }
    // Re-selecting the confirmed mode is not a manual override.
    let manual_mode_change = automation_epoch.is_none()
        && setting == "listeningMode"
        && runtime
            .inner
            .lock()
            .map_err(|_| "Native state unavailable.")?
            .devices
            .get(&device_id)
            .and_then(|device| device.settings.get("listeningMode"))
            != Some(&value);
    let change = match setting.as_str() {
        "probe-noise-schema" => discovery::SettingChange::ProbeNoiseSchema,
        "noise" => {
            let mode = value.as_str().ok_or("Noise-control mode must be text.")?;
            if !["cancelling", "ambient", "off"].contains(&mode) {
                return Err("Noise-control mode is not supported.".into());
            }
            discovery::SettingChange::NoiseMode(mode.into())
        }
        "equalizer" => {
            let bands = value
                .as_array()
                .ok_or("Equalizer setting must be an array.")?;
            if bands.len() != 10 {
                return Err("The WH-1000XM6 equalizer requires ten bands.".into());
            }
            let bands = bands
                .iter()
                .map(|value| {
                    value
                        .as_u64()
                        .filter(|v| *v <= 12)
                        .map(|v| v as u8)
                        .ok_or("Equalizer values must be integer device steps from 0 to 12.")
                })
                .collect::<Result<Vec<_>, _>>()?;
            discovery::SettingChange::Equalizer(bands)
        }
        "oneplusEqProfile" => {
            if serde_json::to_vec(&value)
                .map_err(|_| "OnePlus EQ profile action could not be serialized.")?
                .len()
                > 1024
            {
                return Err("OnePlus EQ profile action exceeds the size limit.".into());
            }
            let change: oneplus_eq::EqProfileChange = serde_json::from_value(value)
                .map_err(|_| "OnePlus EQ profile action has an invalid shape.")?;
            change.validate()?;
            {
                let inner = runtime
                    .inner
                    .lock()
                    .map_err(|_| "Native state unavailable.")?;
                let device = inner
                    .devices
                    .get(&device_id)
                    .ok_or("Unknown device; scan again.")?;
                if device
                    .settings
                    .get("oneplusEqProfile")
                    .and_then(Value::as_bool)
                    != Some(true)
                {
                    return Err("This device has no confirmed writable OnePlus EQ profile.".into());
                }
            }
            discovery::SettingChange::OnePlusEqProfile(change)
        }
        _ => {
            let inner = runtime
                .inner
                .lock()
                .map_err(|_| "Native state unavailable.")?;
            let device = inner
                .devices
                .get(&device_id)
                .ok_or("Unknown device; scan again.")?;
            let descriptor = device
                .settings
                .get("controlDescriptors")
                .and_then(Value::as_array)
                .and_then(|descriptors| {
                    descriptors.iter().find(|descriptor| {
                        descriptor.get("key").and_then(Value::as_str) == Some(setting.as_str())
                    })
                })
                .ok_or_else(|| {
                    format!("The {setting} setting has no validated desktop command.")
                })?;
            let descriptor = device_controls::descriptor_from_json(descriptor)?;
            let value = match value {
                Value::Bool(value) => device_controls::ControlValue::Boolean(value),
                Value::String(value) if value.len() <= 128 => {
                    device_controls::ControlValue::Text(value)
                }
                Value::Number(value) => device_controls::ControlValue::Integer(
                    value.as_i64().ok_or("Device setting must be an integer.")?,
                ),
                _ => {
                    return Err(
                        "Device setting requires a boolean, integer, or short text value.".into(),
                    )
                }
            };
            discovery::SettingChange::DeviceControl(device_controls::validate_change(
                &descriptor,
                value,
            )?)
        }
    };
    let (sender, generation) = {
        let inner = runtime
            .inner
            .lock()
            .map_err(|_| "Native state unavailable.")?;
        let device = inner
            .devices
            .get(&device_id)
            .ok_or("Unknown device; scan again.")?;
        if device.connection != ConnectionState::Connected {
            return Err("Connect the headphones before changing a setting.".into());
        }
        let capability = match &change {
            discovery::SettingChange::ProbeNoiseSchema => "noise",
            discovery::SettingChange::NoiseMode(_) => "noise",
            discovery::SettingChange::Equalizer(_) => "equalizer",
            discovery::SettingChange::DeviceControl(change) => &change.key,
            discovery::SettingChange::OnePlusEqProfile(_) => "oneplusEqProfile",
        };
        if !device
            .capabilities
            .iter()
            .any(|available| available == capability)
        {
            return Err(format!(
                "The device has not confirmed the {capability} capability."
            ));
        }
        if matches!(&change, discovery::SettingChange::ProbeNoiseSchema)
            && (device.model != "WH-1000XM6" || device.firmware.as_deref() != Some("3.1.5"))
        {
            return Err("The read-only NC/ASM probe is scoped to the connected WH-1000XM6 on firmware 3.1.5.".into());
        }
        inner
            .sessions
            .get(&device_id)
            .map(|session| (session.commands.clone(), session.generation))
            .ok_or("The device control session is no longer active.")?
    };
    let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
    {
        let automation = app.state::<Arc<media_automation::Automation>>();
        // A room-distance adjustment, NC change or repeated selection does not opt
        // out of following playback. Only a deliberately different listening mode does.
        let mut guard = automation.enqueue_guard(automation_epoch, false)?;
        sender
            .try_send(ControlCommand {
                change,
                reply: reply_sender,
                automation: automation_epoch.map(|epoch| (Arc::clone(automation.inner()), epoch)),
            })
            .map_err(|_| {
                "The device control queue is busy or closed; try again shortly.".to_string()
            })?;
        if manual_mode_change {
            automation.mark_manual_hold(&mut guard);
        }
    }
    if automation_epoch.is_none() {
        if let Ok(snapshot) = app.state::<Arc<media_automation::Automation>>().snapshot() {
            let _ = app.emit("media-automation", &snapshot);
        }
    }
    let confirmation = tauri::async_runtime::spawn_blocking(move || {
        reply_receiver.recv_timeout(Duration::from_secs(30))
    })
    .await
    .map_err(|_| "The device control worker stopped unexpectedly.".to_string())?
    .map_err(|_| {
        "The device did not confirm the setting before the request deadline.".to_string()
    })??;
    // Keep authorization through the state commit, so a newer manual command
    // cannot be published before this older automatic confirmation.
    let confirmation_authority = app.state::<Arc<media_automation::Automation>>();
    let confirmation_guard = automation_epoch
        .map(|epoch| confirmation_authority.enqueue_guard(Some(epoch), false))
        .transpose()?;
    let mut inner = runtime
        .inner
        .lock()
        .map_err(|_| "Native state unavailable.")?;
    if !inner.is_current(&device_id, generation) {
        return Err("The device connection changed while applying the setting; stale readback was discarded.".into());
    }
    let is_noise_probe = matches!(
        &confirmation,
        discovery::SettingConfirmation::NoiseSchemaProbe { .. }
    );
    let mut probe_messages = None;
    let mut noise_message = None;
    let mut noise_observation_messages = Vec::new();
    let mut setting_confirmed = true;
    let device = inner
        .devices
        .get_mut(&device_id)
        .ok_or("Unknown device; scan again.")?;
    match confirmation {
        discovery::SettingConfirmation::DeviceControl(confirmation) => {
            device
                .settings
                .insert(confirmation.key, confirmation.confirmed.as_json());
        }
        discovery::SettingConfirmation::OnePlusEqProfiles(profiles) => {
            device.settings.insert(
                "oneplusEqualizerProfiles".into(),
                serde_json::json!(profiles),
            );
        }
        discovery::SettingConfirmation::Noise {
            before: baseline,
            noise,
            requested_mode,
            confirmed,
            observations,
        } => {
            apply_noise_readout(device, &noise);
            device
                .settings
                .insert("noiseRequested".into(), Value::from(requested_mode.clone()));
            device.settings.insert(
                "noiseConfirmation".into(),
                Value::from(if confirmed {
                    "confirmed"
                } else {
                    "unconfirmed"
                }),
            );
            let to_hex = |bytes: &[u8]| {
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            noise_message = Some(format!(
                "NC/ASM requested {requested_mode}; baseline inquiry 0x{:02x} {} [{}]; latest inquiry 0x{:02x} {} [{}]; readback confirmed {confirmed}. No automatic restoration command was sent.",
                baseline.inquiry,
                baseline.mode,
                to_hex(&baseline.raw_payload),
                noise.inquiry,
                noise.mode,
                to_hex(&noise.raw_payload),
            ));
            noise_observation_messages = observations
                .iter()
                .map(|payload| format!("Observed NC response bytes: {}", to_hex(payload)))
                .collect();
            setting_confirmed = confirmed;
        }
        discovery::SettingConfirmation::Equalizer(equalizer) => {
            device
                .settings
                .insert("equalizer".into(), serde_json::json!(equalizer.bands));
            device.settings.insert(
                "equalizerPresetCode".into(),
                Value::from(equalizer.preset_code),
            );
        }
        discovery::SettingConfirmation::NoiseSchemaProbe {
            capability,
            status,
            parameter,
            parameter19,
        } => {
            let to_hex = |bytes: &[u8]| {
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            probe_messages = Some((
                to_hex(&capability),
                to_hex(&status),
                to_hex(&parameter),
                to_hex(&parameter19),
            ));
        }
    }
    let _ = device;
    if let Some(message) = noise_message {
        inner.log(&message);
    }
    for message in noise_observation_messages {
        inner.log(&message);
    }
    if let Some((capability_hex, status_hex, parameter_hex, parameter19_hex)) = probe_messages {
        inner.log(&format!(
            "NC/ASM 0x17 capability response bytes: {capability_hex}"
        ));
        inner.log(&format!("NC/ASM 0x17 status response bytes: {status_hex}"));
        inner.log(&format!(
            "NC/ASM 0x17 parameter response bytes: {parameter_hex}"
        ));
        inner.log(&format!(
            "NC/ASM 0x19 parameter response bytes (raw): {parameter19_hex}"
        ));
    }
    if !is_noise_probe && setting_confirmed {
        inner.log("A device setting was updated after matched readback confirmation.");
    }
    let snapshot = inner.snapshot();
    drop(inner);
    drop(confirmation_guard);
    drop(confirmation_authority);
    let _ = app.emit("app-state", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn get_diagnostics(runtime: State<'_, Arc<Runtime>>) -> Result<Vec<String>, String> {
    runtime.snapshot().map(|state| state.diagnostics)
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.len() > 64
        || !(id.starts_with("sony-") || id.starts_with("oneplus-"))
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("Invalid device identifier.".into());
    }
    Ok(())
}

fn open_main(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }
    if let Some(config) = app.config().app.windows.first() {
        let window = WebviewWindowBuilder::from_config(app, config)?.build()?;
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

fn tray_image() -> tauri::image::Image<'static> {
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 0i32..32 {
        for x in 0i32..32 {
            let distance = (x - 16).pow(2) + (y - 16).pow(2);
            let arc = (90..=150).contains(&distance) && y <= 18;
            let cups = ((5..=10).contains(&x) || (22..=27).contains(&x)) && (16..=26).contains(&y);
            if arc || cups {
                let offset = ((y * 32 + x) * 4) as usize;
                rgba[offset..offset + 4].copy_from_slice(&[232, 174, 91, 255]);
            }
        }
    }
    tauri::image::Image::new_owned(rgba, 32, 32)
}

pub fn run() {
    let runtime = Arc::new(Runtime::new());
    let shutdown_runtime = Arc::clone(&runtime);
    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--autostart"])
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_updates::AppUpdates::new())
        .manage(runtime)
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            get_output_levels,
            app_updates::get_app_update,
            app_updates::check_app_update,
            app_updates::install_app_update,
            discover_devices,
            connect_device,
            disconnect_device,
            apply_setting,
            get_diagnostics,
            get_app_preferences,
            update_app_preferences,
            media_automation::get_media_automation,
            media_automation::set_media_automation,
            media_automation::resume_media_automation,
            media_automation::prepare_media_companion,
            vlc_connection::get_vlc_connection,
            vlc_connection::configure_vlc_connection,
            vlc_connection::configure_vlc_startup,
            vlc_connection::test_vlc_connection
        ])
        .setup(|app| {
            app.manage(Arc::new(vlc_connection::VlcConnection::new(
                app.path().app_config_dir()?,
            )));
            let (automation, receiver) =
                media_automation::Automation::new(app.path().app_config_dir()?)
                    .map_err(std::io::Error::other)?;
            let automation = Arc::new(automation);
            app.manage(automation.clone());
            media_automation::start(app.handle().clone(), automation, receiver);
            let store = PreferencesStore::new(app.path().app_config_dir()?);
            let loaded = store.load().map_err(std::io::Error::other)?;
            if let Some(diagnostic) = loaded.diagnostic {
                if let Ok(mut inner) = app.state::<Arc<Runtime>>().inner.lock() {
                    inner.log(diagnostic);
                }
            }
            let explicitly_minimized = std::env::args().any(|arg| arg == "--start-minimized");
            let start_minimized = loaded
                .preferences
                .should_start_minimized(explicitly_minimized);
            app.manage(PreferencesState {
                store,
                current: Mutex::new(loaded.preferences),
            });
            let open = MenuItem::with_id(app, "open", "Open Sound Connect", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit and disconnect", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            TrayIconBuilder::with_id("sound-connect-tray")
                .icon(tray_image())
                .tooltip("Sound Connect Desktop")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        let _ = open_main(app);
                    }
                    "quit" => {
                        app.state::<Arc<Runtime>>().stop_all();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        let _ = open_main(tray.app_handle());
                    }
                })
                .build(app)?;
            if !start_minimized {
                open_main(app.handle())?;
            }
            device_observer::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let close_to_tray = window
                        .app_handle()
                        .try_state::<PreferencesState>()
                        .and_then(|state| {
                            state
                                .current
                                .lock()
                                .ok()
                                .map(|preferences| preferences.close_to_tray)
                        })
                        .unwrap_or(true);
                    if close_to_tray {
                        // Destroy releases the webview; tray sessions belong to the application state.
                        let _ = window.destroy();
                    } else {
                        window.app_handle().state::<Arc<Runtime>>().stop_all();
                        window.app_handle().exit(0);
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("could not initialize Sound Connect Desktop");
    app.run(move |_app, event| match event {
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        RunEvent::Exit => shutdown_runtime.stop_all(),
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => {
            let _ = open_main(_app);
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_disconnect_cancels_retries_and_prevents_a_late_reader_retry() {
        let runtime = Runtime::new();
        let pending = runtime.claim_reconnect("sony-0001").unwrap();
        runtime.suppress_reconnect("sony-0001");
        assert!(pending.load(Ordering::Acquire));
        assert!(runtime.claim_reconnect("sony-0001").is_none());
        assert!(runtime.inner.lock().unwrap().reconnect_jobs.is_empty());
    }

    #[test]
    fn concurrent_disconnect_observers_cannot_create_duplicate_retry_jobs() {
        let runtime = Arc::new(Runtime::new());
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let runtime = Arc::clone(&runtime);
                std::thread::spawn(move || runtime.claim_reconnect("sony-0001").is_some())
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| usize::from(worker.join().unwrap()))
                .sum::<usize>(),
            1
        );
        let active = runtime.inner.lock().unwrap().reconnect_jobs["sony-0001"].clone();
        runtime.stop_all();
        assert!(active.load(Ordering::Acquire));
        assert!(runtime.claim_reconnect("sony-0001").is_none());
    }

    #[test]
    fn diagnostics_are_bounded_and_do_not_expose_device_addresses() {
        let runtime = Runtime::new();
        let mut inner = runtime.inner.lock().unwrap();
        for _ in 0..1000 {
            inner.log("A bounded diagnostic entry");
        }
        assert_eq!(inner.diagnostics.len(), MAX_DIAGNOSTICS);
        assert!(inner.snapshot().devices.is_empty());
    }

    #[test]
    fn generation_prevents_stale_results_after_disconnect_and_reconnect() {
        let runtime = Runtime::new();
        let mut inner = runtime.inner.lock().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (commands, _receiver) = mpsc::sync_channel(1);
        inner.sessions.insert(
            "sony-0001".into(),
            Session {
                generation: 1,
                cancelled: Arc::clone(&cancel),
                commands,
            },
        );
        assert!(inner.is_current("sony-0001", 1));
        cancel.store(true, Ordering::Release);
        assert!(!inner.is_current("sony-0001", 1));
        let (commands, _receiver) = mpsc::sync_channel(1);
        inner.sessions.insert(
            "sony-0001".into(),
            Session {
                generation: 2,
                cancelled: Arc::new(AtomicBool::new(false)),
                commands,
            },
        );
        assert!(!inner.is_current("sony-0001", 1));
        assert!(inner.is_current("sony-0001", 2));
    }

    #[test]
    fn untrusted_device_identifiers_are_rejected() {
        assert!(validate_id("sony-0001").is_ok());
        assert!(validate_id("oneplus-0001").is_ok());
        for id in [
            "",
            "../../file",
            "sony-hello;rm",
            "AA:BB:CC:DD:EE:FF",
            "sony-\n",
        ] {
            assert!(validate_id(id).is_err());
        }
    }

    #[test]
    fn ipc_serialization_matches_frontend_contract() {
        let snapshot = Runtime::new().snapshot().unwrap();
        let json = serde_json::to_value(snapshot).unwrap();
        assert!(json.get("transportStatus").is_some());
        assert!(json.get("revision").and_then(Value::as_u64).is_some());
        assert!(json.get("transport_status").is_none());
        assert_eq!(
            serde_json::to_value(ConnectionState::Connecting).unwrap(),
            "connecting"
        );
    }

    #[test]
    fn immediate_eof_publishes_newer_revision_than_the_pending_connect_result() {
        struct ImmediateEof;
        impl transport::Connection for ImmediateEof {
            fn service_uuid(&self) -> &'static str {
                "956c7b26-d49a-4ba8-b03f-b17d393cb6e2"
            }
            fn read(&mut self, _: &mut [u8], _: Duration) -> std::io::Result<usize> {
                Ok(0)
            }
            fn write_all(&mut self, _: &[u8], _: Duration) -> std::io::Result<()> {
                Ok(())
            }
        }
        let runtime = Arc::new(Runtime::new());
        let cancel = Arc::new(AtomicBool::new(false));
        let (commands, command_receiver) = mpsc::sync_channel(1);
        let mut connection = Box::new(ImmediateEof);
        let (_, read_session) = discovery::read_basics(&mut *connection, &cancel);
        {
            let mut inner = runtime.inner.lock().unwrap();
            inner.devices.insert(
                "sony-0001".into(),
                Device {
                    id: "sony-0001".into(),
                    name: "Fixture device".into(),
                    model: "Fixture".into(),
                    connection: ConnectionState::Connected,
                    battery: Some(43),
                    firmware: None,
                    capabilities: vec!["fixture".into()],
                    settings: BTreeMap::new(),
                },
            );
            inner.sessions.insert(
                "sony-0001".into(),
                Session {
                    generation: 1,
                    cancelled: Arc::clone(&cancel),
                    commands,
                },
            );
            inner.log("Fixture connected");
        }
        // The invoke captured this result, but has not delivered it to the UI yet.
        let pending_connect_result = runtime.snapshot().unwrap();
        let publications = Mutex::new(Vec::new());
        observe_connection(
            Arc::clone(&runtime),
            "sony-0001".into(),
            1,
            cancel,
            connection,
            read_session,
            command_receiver,
            |snapshot| publications.lock().unwrap().push(snapshot),
        );
        let publications = publications.into_inner().unwrap();
        let disconnected_event = publications.last().unwrap();
        assert!(disconnected_event.revision > pending_connect_result.revision);
        assert_eq!(
            disconnected_event.devices[0].connection,
            ConnectionState::Disconnected
        );
        assert_eq!(disconnected_event.devices[0].battery, None);
        assert!(disconnected_event.devices[0].capabilities.is_empty());
        assert_eq!(
            runtime.snapshot().unwrap().revision,
            disconnected_event.revision
        );
    }

    #[test]
    fn disabled_automatic_command_is_rejected_from_the_queue_before_device_io() {
        struct NoWrites(Arc<std::sync::atomic::AtomicUsize>);
        impl transport::Connection for NoWrites {
            fn service_uuid(&self) -> &'static str {
                "956c7b26-d49a-4ba8-b03f-b17d393cb6e2"
            }
            fn read(&mut self, _: &mut [u8], _: Duration) -> std::io::Result<usize> {
                Ok(0)
            }
            fn write_all(&mut self, _: &[u8], _: Duration) -> std::io::Result<()> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
        let mut nonce = [0u8; 8];
        getrandom::fill(&mut nonce).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sound-connect-queue-test-{}",
            u64::from_le_bytes(nonce)
        ));
        let (automation, _) = media_automation::Automation::new(path.clone()).unwrap();
        let automation = Arc::new(automation);
        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut connection = Box::new(NoWrites(writes.clone()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (_, read_session) = discovery::read_basics(&mut *connection, &cancel);
        let baseline = writes.load(Ordering::SeqCst);
        let (commands, receiver) = mpsc::sync_channel(1);
        let (reply, response) = mpsc::sync_channel(1);
        commands
            .send(ControlCommand {
                change: discovery::SettingChange::NoiseMode("cancelling".into()),
                reply,
                automation: Some((automation, 0)),
            })
            .unwrap();
        observe_connection(
            Arc::new(Runtime::new()),
            "sony-fixture".into(),
            1,
            cancel,
            connection,
            read_session,
            receiver,
            |_| {},
        );
        assert!(response
            .recv()
            .unwrap()
            .unwrap_err()
            .contains("cancelled before device I/O"));
        assert_eq!(writes.load(Ordering::SeqCst), baseline);
        std::fs::remove_dir_all(path).unwrap();
    }
}
