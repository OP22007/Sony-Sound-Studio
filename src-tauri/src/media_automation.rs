//! Opt-in, local playback rules. No titles, audio, artwork or listening history.
use crate::{
    listening_policy::{self, Category},
    preferences::PreferencesStore,
    ConnectionState, Runtime,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

const PORT: u16 = 47653;
const TTL: Duration = Duration::from_secs(16);
const CONFIG: &str = "media-automation.json";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub enabled: bool,
    pub status: String,
    pub source: Option<String>,
    pub desired_mode: Option<String>,
    pub planned_settings: Vec<String>,
    pub reason: String,
    pub manual_override: bool,
    pub extension_token: String,
    pub browser_connected: bool,
    pub browser_receiver_error: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    enabled: bool,
    token: String,
    #[serde(default)]
    manual_override: bool,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BrowserReport {
    token: String,
    session_id: String,
    source: String,
    site: String,
    kind: String,
    playing: bool,
    spatial: bool,
}
struct BrowserEntry {
    report: BrowserReport,
    updated: Instant,
}
pub(crate) struct Inner {
    snapshot: Snapshot,
    epoch: u64,
    browser: BTreeMap<String, BrowserEntry>,
    browser_seen: Option<Instant>,
}
pub struct Automation {
    inner: Mutex<Inner>,
    store: PreferencesStore,
    wake: SyncSender<()>,
    http_wake: SyncSender<()>,
    http_receiver: Mutex<Option<mpsc::Receiver<()>>>,
}

impl Automation {
    pub fn new(path: PathBuf) -> Result<(Self, mpsc::Receiver<()>), String> {
        let config = match std::fs::read(path.join(CONFIG)) {
            Ok(bytes) if bytes.len() <= 4096 => serde_json::from_slice::<Config>(&bytes).ok(),
            _ => None,
        }
        .filter(|config| valid_token(&config.token));
        let mut config = match config {
            Some(config) => config,
            None => {
                let mut bytes = [0u8; 32];
                getrandom::fill(&mut bytes)
                    .map_err(|_| "Could not generate a browser pairing key.")?;
                Config {
                    enabled: false,
                    token: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
                    manual_override: false,
                }
            }
        };
        // Manual holds are session-scoped. A stale persisted value must not
        // make startup appear to have paused automation without a user action.
        config.manual_override = false;
        let (wake, receiver) = mpsc::sync_channel(1);
        let (http_wake, http_receiver) = mpsc::sync_channel(1);
        let store = PreferencesStore::new(path);
        store
            .atomic_write(
                CONFIG,
                &serde_json::to_vec(&config).map_err(|_| "Could not encode media preferences.")?,
            )
            .map_err(|_| "Could not save media preferences.")?;
        Ok((Self { inner: Mutex::new(Inner {
            snapshot: Snapshot { enabled: config.enabled, status: if !cfg!(windows) { "unsupported" } else if config.manual_override { "manual" } else if config.enabled { "waiting" } else { "disabled" }.into(),
                source: None, desired_mode: None, planned_settings: Vec::new(), reason: if config.manual_override { "Your manual listening mode is held until you resume Follow media." } else { "Enable Follow media to use local playback rules. Windows playback detection is required." }.into(),
                manual_override: config.manual_override, extension_token: config.token, browser_connected: false, browser_receiver_error: None },
            epoch: 0, browser: BTreeMap::new(), browser_seen: None,
        }), store, wake, http_wake, http_receiver: Mutex::new(Some(http_receiver)) }, receiver))
    }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        self.inner
            .lock()
            .map(|inner| inner.snapshot.clone())
            .map_err(|_| "Media rules unavailable.".into())
    }

    pub(crate) fn authorizes(&self, epoch: u64) -> bool {
        self.inner.lock().is_ok_and(|inner| {
            inner.epoch == epoch && inner.snapshot.enabled && !inner.snapshot.manual_override
        })
    }

    pub(crate) fn source_configuration_changed(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.epoch = inner.epoch.wrapping_add(1);
        }
        let _ = self.wake.try_send(());
    }

    // Called at the same queue boundary as a manual mode command. It cannot race
    // a later automatic enqueue: both take this lock before sending commands.
    pub fn enqueue_guard(
        &self,
        epoch: Option<u64>,
        manual: bool,
    ) -> Result<std::sync::MutexGuard<'_, Inner>, String> {
        let inner = self.inner.lock().map_err(|_| "Media rules unavailable.")?;
        if let Some(epoch) = epoch {
            if !inner.snapshot.enabled || inner.snapshot.manual_override || inner.epoch != epoch {
                return Err(
                    "Automatic mode request was cancelled by a newer preference or manual choice."
                        .into(),
                );
            }
        }
        // Hold state is committed after successful queueing via mark_manual_hold.
        let _ = manual;
        Ok(inner)
    }

    pub fn mark_manual_hold(&self, inner: &mut std::sync::MutexGuard<'_, Inner>) {
        if !inner.snapshot.enabled {
            return;
        }
        inner.epoch = inner.epoch.wrapping_add(1);
        inner.snapshot.manual_override = true;
        inner.snapshot.status = "manual".into();
        inner.snapshot.reason =
            "Your manual listening mode is held until you resume Follow media.".into();
        let _ = self.wake.try_send(());
    }
}

fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[tauri::command]
pub fn get_media_automation(app: AppHandle) -> Result<Snapshot, String> {
    app.state::<Arc<Automation>>().snapshot()
}

#[tauri::command]
pub fn prepare_media_companion(app: AppHandle, browser: String) -> Result<String, String> {
    let manifest = match browser.as_str() {
        "chromium" => include_bytes!("../../extensions/media-mode/manifest.json").as_slice(),
        "firefox" => include_bytes!("../../extensions/media-mode/manifest.firefox.json").as_slice(),
        _ => return Err("Choose Chromium or Firefox.".into()),
    };
    let path = app
        .path()
        .app_config_dir()
        .map_err(|_| "Could not locate browser companion files.")?
        .join("browser-companion")
        .join(browser);
    let store = PreferencesStore::new(path.clone());
    let files: &[(&str, &[u8])] = &[
        ("manifest.json", manifest),
        (
            "background.js",
            include_bytes!("../../extensions/media-mode/background.js"),
        ),
        (
            "content.js",
            include_bytes!("../../extensions/media-mode/content.js"),
        ),
        (
            "popup.html",
            include_bytes!("../../extensions/media-mode/popup.html"),
        ),
        (
            "popup.js",
            include_bytes!("../../extensions/media-mode/popup.js"),
        ),
        (
            "README.md",
            include_bytes!("../../extensions/media-mode/README.md"),
        ),
    ];
    for (name, bytes) in files {
        store
            .atomic_write(name, bytes)
            .map_err(|_| "Could not prepare browser companion files.")?;
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer.exe")
            .arg(&path)
            .spawn();
    }
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn set_media_automation(app: AppHandle, enabled: bool) -> Result<Snapshot, String> {
    if enabled && !cfg!(windows) {
        return Err("Follow media currently requires Windows.".into());
    }
    let state = app.state::<Arc<Automation>>();
    let mut inner = state.inner.lock().map_err(|_| "Media rules unavailable.")?;
    let config = Config {
        enabled,
        token: inner.snapshot.extension_token.clone(),
        manual_override: false,
    };
    state
        .store
        .atomic_write(
            CONFIG,
            &serde_json::to_vec(&config).map_err(|_| "Could not encode media preferences.")?,
        )
        .map_err(|_| "Could not save media preferences.")?;
    inner.epoch = inner.epoch.wrapping_add(1);
    inner.snapshot.enabled = enabled;
    inner.snapshot.manual_override = false;
    inner.snapshot.status = if enabled { "waiting" } else { "disabled" }.into();
    inner.snapshot.reason = if enabled {
        "Waiting for stable playback."
    } else {
        "Follow media is off. Your listening mode is unchanged."
    }
    .into();
    inner.snapshot.source = None;
    inner.snapshot.desired_mode = None;
    inner.snapshot.planned_settings.clear();
    inner.snapshot.browser_connected = false;
    inner.snapshot.browser_receiver_error = None;
    inner.browser_seen = None;
    inner.browser.clear();
    let snapshot = inner.snapshot.clone();
    drop(inner);
    let _ = state.wake.try_send(());
    let _ = state.http_wake.try_send(());
    let _ = app.emit("media-automation", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn resume_media_automation(app: AppHandle) -> Result<Snapshot, String> {
    let state = app.state::<Arc<Automation>>();
    let mut inner = state.inner.lock().map_err(|_| "Media rules unavailable.")?;
    let config = Config {
        enabled: inner.snapshot.enabled,
        token: inner.snapshot.extension_token.clone(),
        manual_override: false,
    };
    state
        .store
        .atomic_write(
            CONFIG,
            &serde_json::to_vec(&config).map_err(|_| "Could not encode media preferences.")?,
        )
        .map_err(|_| "Could not save media preferences.")?;
    inner.epoch = inner.epoch.wrapping_add(1);
    inner.snapshot.manual_override = false;
    inner.snapshot.status = "waiting".into();
    inner.snapshot.reason = "Waiting for stable playback.".into();
    let snapshot = inner.snapshot.clone();
    drop(inner);
    let _ = state.wake.try_send(());
    let _ = app.emit("media-automation", &snapshot);
    Ok(snapshot)
}

// Browser reports carry only a host, coarse category and opaque tab ID. The
// desktop independently classifies hosts; it never trusts a claimed Atmos flag.
fn host_kind(site: &str) -> &'static str {
    let matches = |base: &str| {
        site == base
            || site
                .strip_suffix(base)
                .is_some_and(|prefix| prefix.ends_with('.'))
    };
    if [
        "netflix.com",
        "primevideo.com",
        "disneyplus.com",
        "hulu.com",
        "max.com",
        "hbomax.com",
        "tv.apple.com",
        "peacocktv.com",
        "paramountplus.com",
        "jiohotstar.com",
        "hotstar.com",
    ]
    .iter()
    .any(|host| matches(host))
    {
        "movie"
    } else if [
        "music.youtube.com",
        "music.apple.com",
        "tidal.com",
        "listen.tidal.com",
        "open.spotify.com",
        "soundcloud.com",
        "deezer.com",
        "music.amazon.com",
    ]
    .iter()
    .any(|host| matches(host))
    {
        "music"
    } else {
        "other"
    }
}

fn accept_report(state: &Automation, report: BrowserReport, now: Instant) -> Result<(), ()> {
    if report.session_id.is_empty()
        || report.session_id.len() > 96
        || report.source.len() > 80
        || report.site.is_empty()
        || report.site.len() > 253
        || report.spatial
        || !["movie", "music", "other"].contains(&report.kind.as_str())
        || !report
            .site
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
    {
        return Err(());
    }
    let mut inner = state.inner.lock().map_err(|_| ())?;
    if !inner.snapshot.enabled || report.token != inner.snapshot.extension_token {
        return Err(());
    }
    inner
        .browser
        .retain(|_, entry| now.duration_since(entry.updated) < TTL);
    if !report.playing {
        inner.browser.remove(&report.session_id);
    } else {
        if inner.browser.len() >= 32 && !inner.browser.contains_key(&report.session_id) {
            return Err(());
        }
        inner.browser.insert(
            report.session_id.clone(),
            BrowserEntry {
                report,
                updated: now,
            },
        );
    }
    inner.snapshot.browser_connected = true;
    inner.browser_seen = Some(now);
    let _ = state.wake.try_send(());
    Ok(())
}

// Fixed-size, loopback-only HTTP receiver. No arbitrary URLs, commands, files or
// device IDs are accepted. A cryptographic pairing key is required on each POST.
fn receive(stream: &mut TcpStream, state: &Automation) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
    let deadline = Instant::now() + Duration::from_millis(400);
    let mut bytes = Vec::new();
    let mut expected = None;
    while bytes.len() <= 4096 && Instant::now() < deadline {
        let mut buffer = [0u8; 1024];
        let Ok(count) = stream.read(&mut buffer) else {
            break;
        };
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let Ok(header) = std::str::from_utf8(&bytes[..end]) else {
                break;
            };
            let lines: Vec<_> = header.split("\r\n").collect();
            let get = |name: &str| {
                lines
                    .iter()
                    .skip(1)
                    .filter_map(|line| line.split_once(':'))
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(_, value)| value.trim())
            };
            let origin = get("origin").unwrap_or("");
            // Privileged extension requests may omit Origin. Such requests still
            // require the secret token; ordinary web origins remain forbidden.
            let allowed_origin = origin.is_empty()
                || ["chrome-extension://", "moz-extension://"]
                    .iter()
                    .any(|prefix| {
                        origin.strip_prefix(prefix).is_some_and(|id| {
                            !id.is_empty()
                                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                        })
                    });
            let cors = if origin.is_empty() {
                String::new()
            } else {
                format!("Access-Control-Allow-Origin: {origin}\r\n")
            };
            if get("host") != Some("127.0.0.1:47653")
                || !allowed_origin
                || get("transfer-encoding").is_some()
            {
                break;
            }
            if ["OPTIONS /media HTTP/1.1", "OPTIONS /pair HTTP/1.1"].contains(&lines[0]) {
                let _ = write!(stream, "HTTP/1.1 204 No Content\r\n{cors}Access-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Private-Network: true\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                return;
            }
            if !["POST /media HTTP/1.1", "POST /pair HTTP/1.1"].contains(&lines[0])
                || get("content-type").is_none_or(|value| !value.starts_with("application/json"))
            {
                break;
            }
            let Ok(length) = get("content-length").unwrap_or("").parse::<usize>() else {
                break;
            };
            if length > 2048
                || end + 4 + length > 4096
                || lines
                    .iter()
                    .filter(|line| line.to_ascii_lowercase().starts_with("content-length:"))
                    .count()
                    != 1
            {
                break;
            }
            expected = Some((end + 4, length, cors, lines[0] == "POST /pair HTTP/1.1"));
        }
        if let Some((start, length, ref cors, pair)) = expected {
            if bytes.len() >= start + length {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Pair {
                    token: String,
                }
                let accepted = if pair {
                    serde_json::from_slice::<Pair>(&bytes[start..start + length])
                        .ok()
                        .is_some_and(|request| {
                            state.inner.lock().is_ok_and(|mut inner| {
                                if !inner.snapshot.enabled
                                    || request.token != inner.snapshot.extension_token
                                {
                                    return false;
                                }
                                inner.browser_seen = Some(Instant::now());
                                inner.snapshot.browser_connected = true;
                                true
                            })
                        })
                } else {
                    serde_json::from_slice::<BrowserReport>(&bytes[start..start + length])
                        .ok()
                        .is_some_and(|report| accept_report(state, report, Instant::now()).is_ok())
                };
                if accepted && pair {
                    let _ = write!(stream, "HTTP/1.1 200 OK\r\n{cors}Content-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{{\"ok\":true}}");
                    return;
                }
                let status = if accepted {
                    "204 No Content"
                } else {
                    "403 Forbidden"
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\n{cors}Content-Length: 0\r\nConnection: close\r\n\r\n"
                );
                return;
            }
        }
    }
    let _ = stream
        .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
}

#[cfg(windows)]
fn start_http(app: AppHandle, state: Arc<Automation>, runtime: Arc<Runtime>) {
    let Some(receiver) = state
        .http_receiver
        .lock()
        .ok()
        .and_then(|mut receiver| receiver.take())
    else {
        return;
    };
    std::thread::spawn(move || {
        let mut listener = None;
        while !runtime.stopping.load(std::sync::atomic::Ordering::Acquire) {
            if !state.snapshot().is_ok_and(|snapshot| snapshot.enabled) {
                listener = None;
                let _ = receiver.recv_timeout(Duration::from_secs(5));
                continue;
            }
            if listener.is_none() {
                match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, PORT)) {
                    Ok(socket) => {
                        let _ = socket.set_nonblocking(true);
                        listener = Some(socket);
                        if let Ok(mut inner) = state.inner.lock() {
                            inner.snapshot.browser_receiver_error = None;
                        }
                    }
                    Err(_) => {
                        if let Ok(mut inner) = state.inner.lock() {
                            inner.snapshot.browser_receiver_error =
                                Some("Browser receiver unavailable: port 47653 is in use.".into());
                        }
                        if let Ok(snapshot) = state.snapshot() {
                            let _ = app.emit("media-automation", &snapshot);
                        }
                        let _ = receiver.recv_timeout(Duration::from_secs(5));
                        continue;
                    }
                }
            }
            if let Some(listener) = listener.as_ref() {
                for _ in 0..8 {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            receive(&mut stream, &state);
                            if let Ok(snapshot) = state.snapshot() {
                                let _ = app.emit("media-automation", &snapshot);
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
            let _ = receiver.recv_timeout(Duration::from_millis(100));
        }
    });
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Choice {
    key: String,
    source: String,
    category: Category,
    reason: &'static str,
}

#[derive(Default)]
struct StableChoice {
    candidate: Option<Choice>,
    since: Option<Instant>,
}
impl StableChoice {
    fn update(&mut self, choice: Option<Choice>, now: Instant) -> Option<Choice> {
        if self.candidate != choice {
            self.candidate = choice;
            self.since = Some(now);
        }
        if self
            .since
            .is_some_and(|since| now.duration_since(since) >= Duration::from_secs(3))
        {
            self.candidate.clone()
        } else {
            None
        }
    }
}

pub fn start(app: AppHandle, state: Arc<Automation>, receiver: mpsc::Receiver<()>) {
    #[cfg(not(windows))]
    {
        let _ = (app, state, receiver);
    }
    #[cfg(windows)]
    std::thread::spawn(move || {
        let runtime = app.state::<Arc<Runtime>>().inner().clone();
        start_http(app.clone(), state.clone(), runtime.clone());
        let mut observer: Option<crate::media_windows::Observer> = None;
        let mut stable = StableChoice::default();
        let mut native = None;
        let mut refresh = Instant::now() - Duration::from_secs(60);
        let mut last_published = None;
        let mut last_attempt: Option<(String, Instant)> = None;
        let mut observed_epoch = 0;
        let vlc = app
            .state::<Arc<crate::vlc_connection::VlcConnection>>()
            .inner()
            .clone();
        let mut vlc_refresh = Instant::now() - Duration::from_secs(60);
        let mut vlc_playback = None;
        let mut game_exe: Option<String> = None;
        let mut game_refresh = Instant::now() - Duration::from_secs(60);
        loop {
            if runtime.stopping.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
            let wait = if state.snapshot().is_ok_and(|snapshot| snapshot.enabled) {
                1
            } else {
                30
            };
            let signalled = receiver.recv_timeout(Duration::from_secs(wait)).is_ok();
            let enabled = state.snapshot().is_ok_and(|snapshot| snapshot.enabled);
            if !enabled {
                observer = None;
                native = None;
                stable = StableChoice::default();
                last_attempt = None;
                vlc_playback = None;
                game_exe = None;
            } else {
                let now = Instant::now();
                if signalled || now.duration_since(game_refresh) >= Duration::from_secs(2) {
                    game_exe = crate::game_detector::detect(&crate::game_detector::SystemProbe);
                    game_refresh = now;
                }
                if vlc.enabled()
                    && (signalled || now.duration_since(vlc_refresh) >= Duration::from_secs(2))
                {
                    vlc_playback = vlc.poll(&app).ok().flatten();
                    vlc_refresh = now;
                } else if !vlc.enabled() {
                    vlc_playback = None;
                }
                let mut detection_error = None;
                if observer.is_none() && now.duration_since(refresh) >= Duration::from_secs(15) {
                    match crate::media_windows::Observer::new(state.wake.clone()) {
                        Ok(value) => observer = Some(value),
                        Err(error) => detection_error = Some(error),
                    }
                    refresh = now;
                    if let Some(observer) = observer.as_mut() {
                        native = observer.current().ok().flatten();
                    }
                } else if signalled || now.duration_since(refresh) >= Duration::from_secs(30) {
                    if let Some(observer) = observer.as_mut() {
                        match observer.current() {
                            Ok(value) => native = value,
                            Err(error) => {
                                native = None;
                                detection_error = Some(error);
                            }
                        }
                        refresh = now;
                    }
                }
                let (choice, epoch, manual) = {
                    let Ok(mut inner) = state.inner.lock() else {
                        break;
                    };
                    inner
                        .browser
                        .retain(|_, entry| now.saturating_duration_since(entry.updated) < TTL);
                    inner.snapshot.browser_connected = inner.browser_seen.is_some_and(|seen| {
                        now.saturating_duration_since(seen) < Duration::from_secs(30)
                    });
                    let browser = if inner.browser.len() == 1 {
                        inner.browser.values().next()
                    } else {
                        None
                    };
                    let conflicting_vlc = game_exe.is_none() && vlc_playback
                        .as_ref()
                        .is_some_and(|playback| playback.playing)
                        && (!inner.browser.is_empty()
                            || native.as_ref().is_some_and(|observation| {
                                observation.playing
                                    && !observation.browser
                                    && observation.source != "VLC"
                            }));
                    let choice = if conflicting_vlc {
                        None
                    } else if let Some(playback) =
                        vlc_playback.as_ref().filter(|playback| playback.playing)
                    {
                        Some(Choice {
                            key: "vlc-local-status".into(),
                            source: "VLC".into(),
                            category: Category::from_kind(playback.kind),
                            reason: match Category::from_kind(playback.kind) {
                                Category::Movie => "VLC reports a video stream",
                                Category::Music => "VLC reports an audio stream",
                                _ => "VLC unknown stream",
                            },
                        })
                    } else if let Some(observation) = native.as_ref().filter(|observation| {
                        observation.playing
                            && !observation.browser
                            && !(observation.source == "VLC" && vlc_playback.is_some())
                    }) {
                        Some(Choice {
                            key: observation.key.clone(),
                            source: observation.source.clone(),
                            category: Category::from_kind(&observation.kind),
                            reason: match Category::from_kind(&observation.kind) {
                                Category::Movie => "Movie/video player rule",
                                Category::Music => "Music player rule",
                                _ => "Other player",
                            },
                        })
                    } else if native
                        .as_ref()
                        .is_none_or(|observation| observation.browser || !observation.playing)
                    {
                        browser.map(|entry| Choice {
                            key: format!(
                                "browser:{}:{}",
                                entry.report.session_id, entry.report.site
                            ),
                            source: entry.report.site.clone(),
                            category: Category::from_kind(host_kind(&entry.report.site)),
                            reason: match Category::from_kind(host_kind(&entry.report.site)) {
                                Category::Movie => "Movie/show streaming site rule",
                                Category::Music => "Music streaming site rule",
                                _ => "Ordinary browser content",
                            },
                        })
                    } else {
                        None
                    };
                    // A running game wins over background music or video.
                    let choice = match game_exe.as_ref() {
                        Some(exe) => Some(Choice {
                            key: format!("game:{exe}"),
                            source: exe.clone(),
                            category: Category::Game,
                            reason: "Game detected",
                        }),
                        None => choice,
                    };
                    inner.snapshot.source = choice.as_ref().map(|choice| choice.source.clone());
                    inner.snapshot.desired_mode = choice.as_ref().map(|choice| choice.category.key().into());
                    if !inner.snapshot.manual_override {
                        inner.snapshot.status = "waiting".into();
                        inner.snapshot.reason = detection_error.or_else(|| inner.snapshot.browser_receiver_error.clone()).unwrap_or_else(|| {
                            if conflicting_vlc { "VLC and another player are active. Pause one before automatic switching.".into() }
                            else if inner.browser.len() > 1 && game_exe.is_none() { "Multiple browser tabs are playing. Pause one before automatic switching.".into() }
                            else { choice.as_ref().map(|choice| choice.reason.into()).unwrap_or_else(|| "Waiting for playback. Pauses keep the current mode; browser sites need the companion extension.".into()) }
                        });
                    }
                    (choice, inner.epoch, inner.snapshot.manual_override)
                };
                if epoch != observed_epoch {
                    stable = StableChoice::default();
                    last_attempt = None;
                    observed_epoch = epoch;
                }
                let ready = stable.update(if manual { None } else { choice }, now);
                if let Some(choice) = ready {
                    let (target, unavailable_reason) = runtime.snapshot().ok().map(|snapshot| {
                        let connected: Vec<_> = snapshot.devices.into_iter().filter(|device| device.connection == ConnectionState::Connected).collect();
                        if let Some(active_id) = snapshot.active_output_device_id {
                            let active = connected.into_iter().find(|device| device.id == active_id);
                            (active, Some("The active Windows output is not a connected Sound Connect headphone."))
                        } else if snapshot.audio_output_observed {
                            (None, Some("The active Windows output is not a connected Sound Connect headphone."))
                        } else if connected.len() == 1 {
                            (connected.into_iter().next(), None)
                        } else {
                            (None, Some("Choose a Windows output device or connect exactly one headphone to follow media."))
                        }
                    }).unwrap_or((None, Some("Device status is unavailable.")));
                    if let Some(device) = target {
                        // Readback can be stale during discovery or an automatic
                        // transition; it cannot establish user intent. Reconcile
                        // against the stable playback choice through normal apply.
                        let mut status = "device-unavailable";
                        let mut reason =
                            "The connected headphone offers no confirmed setting for this content."
                                .to_string();
                        let desired = listening_policy::desired_settings(&device, choice.category);
                        let planned: Vec<String> = desired.iter().map(listening_policy::describe).collect();
                        if !desired.is_empty() {
                            let pending = listening_policy::pending_settings(&device, choice.category);
                            if pending.is_empty() {
                                status = "following";
                                reason = format!("{} · confirmed by headphones.", choice.reason);
                            } else {
                                let attempt_key = format!(
                                    "{epoch}:{}:{}:{}",
                                    device.id,
                                    choice.key,
                                    choice.category.key()
                                );
                                if last_attempt.as_ref().is_none_or(|(key, time)| {
                                    key != &attempt_key
                                        || now.duration_since(*time) >= Duration::from_secs(30)
                                }) {
                                    last_attempt = Some((attempt_key, now));
                                    status = "following";
                                    reason = format!("{} · confirmed by headphones.", choice.reason);
                                    for setting in pending {
                                        if let Err(error) = tauri::async_runtime::block_on(
                                            crate::apply_setting_internal(
                                                app.clone(),
                                                runtime.clone(),
                                                device.id.clone(),
                                                setting.key.into(),
                                                setting.value.clone(),
                                                Some(epoch),
                                            ),
                                        ) {
                                            status = "error";
                                            reason = format!(
                                                "{} could not be confirmed: {error}",
                                                listening_policy::describe(&setting)
                                            );
                                            break;
                                        }
                                    }
                                } else {
                                    status = "retrying";
                                    reason =
                                        "Waiting before retrying the unconfirmed setting request."
                                            .into();
                                }
                            }
                        }
                        if let Ok(mut inner) = state.inner.lock() {
                            if inner.epoch == epoch && !inner.snapshot.manual_override {
                                inner.snapshot.status = status.into();
                                inner.snapshot.planned_settings = planned;
                                inner.snapshot.reason = reason;
                            }
                        }
                    } else if let Ok(mut inner) = state.inner.lock() {
                        if inner.epoch == epoch && !inner.snapshot.manual_override {
                            inner.snapshot.status = "device-unavailable".into();
                            inner.snapshot.reason = unavailable_reason.unwrap_or("Connect exactly one headphone with confirmed Cinema support to follow media.").into();
                        }
                    }
                }
            }
            if let Ok(snapshot) = state.snapshot() {
                if last_published.as_ref() != Some(&snapshot) {
                    let _ = app.emit("media-automation", &snapshot);
                    last_published = Some(snapshot);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Automation, PathBuf) {
        let mut nonce = [0u8; 8];
        getrandom::fill(&mut nonce).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sound-connect-media-test-{}",
            u64::from_le_bytes(nonce)
        ));
        let (state, _) = Automation::new(path.clone()).unwrap();
        (state, path)
    }
    fn report(state: &Automation) -> BrowserReport {
        BrowserReport {
            token: state.snapshot().unwrap().extension_token,
            session_id: "test-tab".into(),
            source: "browser".into(),
            site: "www.netflix.com".into(),
            kind: "movie".into(),
            playing: true,
            spatial: false,
        }
    }

    #[test]
    fn browser_reports_require_opt_in_and_the_pairing_token_and_reject_spatial_claims() {
        let (state, path) = fixture();
        let now = Instant::now();
        assert!(accept_report(&state, report(&state), now).is_err());
        state.inner.lock().unwrap().snapshot.enabled = true;
        let mut forged = report(&state);
        forged.token = "wrong".into();
        assert!(accept_report(&state, forged, now).is_err());
        let mut forged = report(&state);
        forged.spatial = true;
        assert!(accept_report(&state, forged, now).is_err());
        assert!(accept_report(&state, report(&state), now).is_ok());
        assert_eq!(state.inner.lock().unwrap().browser.len(), 1);
        let mut stopped = report(&state);
        stopped.playing = false;
        accept_report(&state, stopped, now).unwrap();
        assert!(state.inner.lock().unwrap().browser.is_empty());
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn stale_browser_tabs_expire_and_manual_choice_cancels_queued_automation_for_this_session() {
        let (state, path) = fixture();
        state.inner.lock().unwrap().snapshot.enabled = true;
        let now = Instant::now();
        accept_report(&state, report(&state), now).unwrap();
        let mut next = report(&state);
        next.session_id = "new-tab".into();
        accept_report(&state, next, now + TTL).unwrap();
        assert_eq!(state.inner.lock().unwrap().browser.len(), 1);
        let epoch = state.inner.lock().unwrap().epoch;
        drop(state.enqueue_guard(Some(epoch), false).unwrap());
        let mut guard = state.enqueue_guard(None, true).unwrap();
        assert!(!guard.snapshot.manual_override);
        state.mark_manual_hold(&mut guard);
        drop(guard);
        assert!(state.snapshot().unwrap().manual_override);
        assert!(state.enqueue_guard(Some(epoch), false).is_err());
        assert!(!state.authorizes(epoch));
        let (restored, _) = Automation::new(path.clone()).unwrap();
        assert!(!restored.snapshot().unwrap().manual_override);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn token_is_persistent_and_automation_defaults_to_off() {
        let (state, path) = fixture();
        let before = state.snapshot().unwrap();
        assert!(!before.enabled);
        assert!(valid_token(&before.extension_token));
        let (restored, _) = Automation::new(path.clone()).unwrap();
        assert_eq!(
            restored.snapshot().unwrap().extension_token,
            before.extension_token
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    fn request(state: Automation, body: &str, origin: &str) -> String {
        request_endpoint(state, body, origin, "/media", "POST")
    }

    fn request_endpoint(
        state: Automation,
        body: &str,
        origin: &str,
        path: &str,
        method: &str,
    ) -> String {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let task = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            receive(&mut stream, &state);
        });
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write!(client, "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:47653\r\nOrigin: {origin}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut response = String::new();
        if let Err(error) = client.read_to_string(&mut response) {
            // Windows may reset after an intentionally rejected request whose
            // body arrived after the rejection. The HTTP rejection must exist.
            assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
            assert!(response.starts_with("HTTP/1.1 400"));
        }
        task.join().unwrap();
        response
    }

    #[test]
    fn browser_pairing_is_authenticated_with_and_without_privileged_origin() {
        for origin in [
            "",
            "chrome-extension://abcdefghijklmnop",
            "moz-extension://abc-def-123",
        ] {
            let (state, path) = fixture();
            state.inner.lock().unwrap().snapshot.enabled = true;
            let body =
                serde_json::json!({"token": state.snapshot().unwrap().extension_token}).to_string();
            let response = request_endpoint(state, &body, origin, "/pair", "POST");
            assert!(response.starts_with("HTTP/1.1 200"));
            assert!(response.ends_with("{\"ok\":true}"));
            std::fs::remove_dir_all(path).unwrap();
        }
        let (state, path) = fixture();
        state.inner.lock().unwrap().snapshot.enabled = true;
        assert!(
            request_endpoint(state, "{\"token\":\"wrong\"}", "", "/pair", "POST")
                .starts_with("HTTP/1.1 403")
        );
        std::fs::remove_dir_all(path).unwrap();
        let (state, path) = fixture();
        let response = request_endpoint(
            state,
            "",
            "chrome-extension://abcdefghijklmnop",
            "/pair",
            "OPTIONS",
        );
        assert!(response.starts_with("HTTP/1.1 204"));
        assert!(
            response.contains("Access-Control-Allow-Origin: chrome-extension://abcdefghijklmnop")
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn loopback_http_accepts_paired_extension_and_rejects_web_pages() {
        let (state, path) = fixture();
        state.inner.lock().unwrap().snapshot.enabled = true;
        let body = serde_json::json!({ "token": state.snapshot().unwrap().extension_token,
            "sessionId":"tab", "source":"browser", "site":"netflix.com", "kind":"movie", "playing":true, "spatial":false }).to_string();
        assert!(request(state, &body, "chrome-extension://abcdefghijklmnop")
            .starts_with("HTTP/1.1 204"));
        let (state, _) = Automation::new(path.clone()).unwrap();
        assert!(request(state, &body, "https://netflix.com").starts_with("HTTP/1.1 400"));
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn movie_hosts_do_not_match_lookalikes_or_ordinary_youtube() {
        assert_eq!(host_kind("www.netflix.com"), "movie");
        assert_eq!(host_kind("netflix.com.evil.example"), "other");
        assert_eq!(host_kind("notnetflix.com"), "other");
        assert_eq!(host_kind("youtube.com"), "other");
        assert_eq!(host_kind("music.youtube.com"), "music");
        assert_eq!(host_kind("music.apple.com"), "music");
        assert_eq!(host_kind("listen.tidal.com"), "music");
    }
    #[test]
    fn rapid_changes_and_brief_pauses_do_not_produce_a_switch() {
        let now = Instant::now();
        let choice = Choice {
            key: "a".into(),
            source: "Netflix".into(),
            category: Category::Movie,
            reason: "site",
        };
        let mut stable = StableChoice::default();
        assert!(stable.update(Some(choice.clone()), now).is_none());
        assert!(stable
            .update(Some(choice.clone()), now + Duration::from_secs(2))
            .is_none());
        assert_eq!(
            stable
                .update(Some(choice), now + Duration::from_secs(3))
                .unwrap()
                .category,
            Category::Movie
        );
        assert!(stable.update(None, now + Duration::from_secs(4)).is_none());
    }

    #[test]
    fn music_sites_are_classified_without_matching_lookalikes() {
        for site in ["open.spotify.com", "music.youtube.com", "listen.tidal.com", "soundcloud.com"] {
            assert_eq!(host_kind(site), "music", "{site}");
        }
        assert_eq!(host_kind("open.spotify.com.evil.example"), "other");
        assert_eq!(host_kind("www.youtube.com"), "other");
        assert_eq!(Category::from_kind(host_kind("netflix.com")), Category::Movie);
    }
}
