//! Local listening history. Everything here stays on this computer: segments are appended to a
//! file in the app configuration folder and no code in this module touches the network.
//!
//! The recorder is a small state machine fed by a sampler thread. It is pure (time and samples
//! are passed in) so the rules for opening, closing and splitting segments are unit tested.

use crate::preferences::PreferencesStore;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const HISTORY_FILE: &str = "history.jsonl";
const OPEN_FILE: &str = "history-open.json";
const SETTINGS_FILE: &str = "history-settings.json";
const TICK: Duration = Duration::from_secs(5);
const CHECKPOINT_MS: u64 = 60_000;
/// Segments shorter than this are mode flicker or reconnect noise.
const MIN_SEGMENT_MS: u64 = 5_000;
/// A segment is never allowed to outlive a missed disconnect for long.
const MAX_SEGMENT_MS: u64 = 12 * 60 * 60 * 1000;
/// A gap between samples this large means the computer slept.
const SLEEP_GAP_MS: u64 = 30_000;
const DAY_MS: u64 = 24 * 60 * 60 * 1000;
const RECENT: usize = 20;
const CHART_DAYS: u64 = 90;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub device: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Song, video, page or game window title. Only with exact details on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// Full page address, including any query. Only with exact details on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Segment {
    fn same_state(&self, other: &Segment) -> bool {
        self.device == other.device
            && self.mode == other.mode
            && self.noise == other.noise
            && self.category == other.category
            && self.source == other.source
            && self.title == other.title
            && self.artist == other.artist
            && self.url == other.url
    }
    fn duration(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSample {
    pub device: String,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub noise: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaSample {
    pub category: String,
    pub source: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub enabled: bool,
    /// Playback type plus app or site name.
    pub details: bool,
    /// Exact titles and addresses. Only takes effect while `enabled` and `details` are on.
    pub exact: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: true, details: false, exact: false }
    }
}

impl Settings {
    pub fn exact_effective(&self) -> bool {
        self.enabled && self.details && self.exact
    }
}

#[derive(Default)]
pub struct Recorder {
    open: BTreeMap<String, Segment>,
    last_tick: Option<u64>,
}

impl Recorder {
    /// Feed one sample of the world. Returns segments that closed and are ready to persist.
    pub fn observe(
        &mut self,
        now: u64,
        devices: &[DeviceSample],
        media: Option<&MediaSample>,
        settings: Settings,
    ) -> Vec<Segment> {
        let mut closed = Vec::new();
        if let Some(previous) = self.last_tick {
            if now.saturating_sub(previous) > SLEEP_GAP_MS {
                // The computer slept: end everything where we last saw it alive.
                self.close_all_at(previous, &mut closed);
            }
        }
        self.last_tick = Some(now);
        if !settings.enabled {
            self.close_all_at(now, &mut closed);
            return keep_long_enough(closed);
        }
        let gone: Vec<String> = self
            .open
            .keys()
            .filter(|name| !devices.iter().any(|device| &device.device == *name))
            .cloned()
            .collect();
        for name in gone {
            if let Some(mut segment) = self.open.remove(&name) {
                segment.end = now;
                closed.push(segment);
            }
        }
        for device in devices {
            let (category, source) = match (settings.details, media) {
                (true, Some(media)) => (Some(media.category.clone()), Some(media.source.clone())),
                _ => (None, None),
            };
            let (title, artist, url) = match (settings.exact_effective(), media) {
                (true, Some(media)) => (media.title.clone(), media.artist.clone(), media.url.clone()),
                _ => (None, None, None),
            };
            let candidate = Segment {
                start: now,
                end: now,
                device: device.device.clone(),
                model: device.model.clone(),
                mode: device.mode.clone(),
                noise: device.noise.clone(),
                category,
                source,
                title,
                artist,
                url,
            };
            match self.open.get_mut(&device.device) {
                Some(open) if open.same_state(&candidate) && now - open.start < MAX_SEGMENT_MS => {
                    open.end = now;
                }
                Some(_) => {
                    let mut finished = self.open.remove(&device.device).expect("present");
                    finished.end = now;
                    closed.push(finished);
                    self.open.insert(device.device.clone(), candidate);
                }
                None => {
                    self.open.insert(device.device.clone(), candidate);
                }
            }
        }
        keep_long_enough(closed)
    }

    pub fn close_all(&mut self, now: u64) -> Vec<Segment> {
        let mut closed = Vec::new();
        self.close_all_at(now, &mut closed);
        self.last_tick = None;
        keep_long_enough(closed)
    }

    /// Forget open segments without recording them (used by Clear history).
    pub fn discard(&mut self) {
        self.open.clear();
        self.last_tick = None;
    }

    pub fn open_segments(&self) -> Vec<Segment> {
        self.open.values().cloned().collect()
    }

    fn close_all_at(&mut self, at: u64, closed: &mut Vec<Segment>) {
        for (_, mut segment) in std::mem::take(&mut self.open) {
            segment.end = at.max(segment.start);
            closed.push(segment);
        }
    }
}

fn keep_long_enough(segments: Vec<Segment>) -> Vec<Segment> {
    segments.into_iter().filter(|segment| segment.duration() >= MIN_SEGMENT_MS).collect()
}

pub struct HistoryStore {
    dir: PathBuf,
    files: PreferencesStore,
    io: Mutex<()>,
}

impl HistoryStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { files: PreferencesStore::new(dir.clone()), dir, io: Mutex::new(()) }
    }

    pub fn append(&self, segments: &[Segment]) -> io::Result<()> {
        if segments.is_empty() {
            return Ok(());
        }
        let _guard = self.io.lock().map_err(|_| io::Error::other("history lock"))?;
        fs::create_dir_all(&self.dir)?;
        let mut file = OpenOptions::new().create(true).append(true).open(self.dir.join(HISTORY_FILE))?;
        let mut buffer = Vec::new();
        for segment in segments {
            serde_json::to_writer(&mut buffer, segment).map_err(io::Error::other)?;
            buffer.push(b'\n');
        }
        file.write_all(&buffer)?;
        file.sync_data()
    }

    pub fn write_open(&self, segments: &[Segment]) -> io::Result<()> {
        let _guard = self.io.lock().map_err(|_| io::Error::other("history lock"))?;
        if segments.is_empty() {
            return match fs::remove_file(self.dir.join(OPEN_FILE)) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            };
        }
        let bytes = serde_json::to_vec(segments).map_err(io::Error::other)?;
        self.files.atomic_write(OPEN_FILE, &bytes)
    }

    /// Segments that were open when the app last stopped uncleanly end at their last checkpoint.
    pub fn recover(&self) -> io::Result<()> {
        let bytes = {
            let _guard = self.io.lock().map_err(|_| io::Error::other("history lock"))?;
            match fs::read(self.dir.join(OPEN_FILE)) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error),
            }
        };
        let segments: Vec<Segment> = serde_json::from_slice(&bytes).unwrap_or_default();
        self.append(&keep_long_enough(segments))?;
        self.write_open(&[])
    }

    pub fn read_all(&self) -> (Vec<Segment>, usize) {
        let Ok(_guard) = self.io.lock() else { return (Vec::new(), 0) };
        let Ok(file) = fs::File::open(self.dir.join(HISTORY_FILE)) else { return (Vec::new(), 0) };
        let (mut segments, mut skipped) = (Vec::new(), 0);
        for line in io::BufReader::new(file).lines() {
            let Ok(line) = line else { skipped += 1; continue };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Segment>(&line) {
                Ok(segment) if segment.end >= segment.start => segments.push(segment),
                _ => skipped += 1,
            }
        }
        (segments, skipped)
    }

    pub fn clear(&self) -> io::Result<()> {
        let _guard = self.io.lock().map_err(|_| io::Error::other("history lock"))?;
        for name in [HISTORY_FILE, OPEN_FILE] {
            match fs::remove_file(self.dir.join(name)) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        Ok(())
    }

    fn load_settings(&self) -> Settings {
        fs::read(self.dir.join(SETTINGS_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save_settings(&self, settings: &Settings) -> io::Result<()> {
        self.files.atomic_write(SETTINGS_FILE, &serde_json::to_vec(settings).map_err(io::Error::other)?)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub key: String,
    pub ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub start_ms: u64,
    pub ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub range: String,
    pub enabled: bool,
    pub details: bool,
    pub total_ms: u64,
    pub first_ms: Option<u64>,
    pub days: Vec<Day>,
    pub modes: Vec<Bucket>,
    pub noise: Vec<Bucket>,
    pub devices: Vec<Bucket>,
    /// Every headphone that has any history, ignoring the device filter, for the picker.
    pub known_devices: Vec<String>,
    pub categories: Vec<Bucket>,
    pub sources: Vec<Bucket>,
    pub titles: Vec<Bucket>,
    pub urls: Vec<Bucket>,
    pub exact: bool,
    pub recent: Vec<Segment>,
    pub skipped: usize,
    pub write_error: Option<String>,
    pub content_supported: bool,
}

fn rank(map: BTreeMap<String, u64>) -> Vec<Bucket> {
    let mut buckets: Vec<Bucket> = map.into_iter().filter(|(_, ms)| *ms > 0).map(|(key, ms)| Bucket { key, ms }).collect();
    buckets.sort_by(|a, b| b.ms.cmp(&a.ms).then_with(|| a.key.cmp(&b.key)));
    buckets
}

/// Aggregate segments for a range. `tz_offset_minutes` follows `Date.getTimezoneOffset` (UTC minus
/// local) so days split at the viewer's local midnight. Pure, so it is easy to test.
pub fn summarize(
    segments: &[Segment],
    range: &str,
    now: u64,
    tz_offset_minutes: i32,
    settings: Settings,
) -> Summary {
    let offset = i64::from(tz_offset_minutes) * 60_000;
    let local = |ms: u64| ms as i64 - offset;
    let day_start = |index: i64| (index * DAY_MS as i64 + offset).max(0) as u64;
    let today = local(now).div_euclid(DAY_MS as i64);
    let span: Option<i64> = match range {
        "today" => Some(1),
        "30d" => Some(30),
        "all" => None,
        _ => Some(7),
    };
    let range_start = span.map_or(0, |days| day_start(today - (days - 1)));
    let first_ms = segments.iter().map(|segment| segment.start).min();
    let chart_first = match span {
        // The chart always shows at least a week so "Today" has something to compare against.
        Some(days) => today - (days.max(7) - 1),
        None => first_ms.map_or(today, |first| local(first).div_euclid(DAY_MS as i64)).max(today - (CHART_DAYS as i64 - 1)),
    };
    let mut days: Vec<Day> = (chart_first..=today).map(|index| Day { start_ms: day_start(index), ms: 0 }).collect();
    let (mut modes, mut noise, mut devices, mut categories, mut sources) =
        (BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let (mut titles, mut urls) = (BTreeMap::new(), BTreeMap::new());
    let mut total = 0u64;
    for segment in segments {
        let (start, end) = (segment.start.max(range_start), segment.end.min(now));
        if end <= start {
            continue;
        }
        total += end - start;
        let share = end - start;
        *devices.entry(segment.device.clone()).or_insert(0u64) += share;
        *modes.entry(segment.mode.clone().unwrap_or_else(|| "unknown".into())).or_insert(0u64) += share;
        if let Some(value) = &segment.noise {
            *noise.entry(value.clone()).or_insert(0u64) += share;
        }
        if let Some(value) = &segment.category {
            *categories.entry(value.clone()).or_insert(0u64) += share;
        }
        if let Some(value) = &segment.source {
            *sources.entry(value.clone()).or_insert(0u64) += share;
        }
        if let Some(title) = &segment.title {
            let label = match &segment.artist {
                Some(artist) => format!("{artist} \u{2014} {title}"),
                None => title.clone(),
            };
            *titles.entry(label).or_insert(0u64) += share;
        }
        if let Some(value) = &segment.url {
            *urls.entry(value.clone()).or_insert(0u64) += share;
        }
        // Split across local midnights for the daily chart.
        let mut cursor = start;
        while cursor < end {
            let index = local(cursor).div_euclid(DAY_MS as i64);
            let next = day_start(index + 1).min(end).max(cursor + 1);
            if let Some(day) = days.iter_mut().find(|day| day.start_ms == day_start(index)) {
                day.ms += next - cursor;
            }
            cursor = next;
        }
    }
    let mut recent: Vec<Segment> = segments.to_vec();
    recent.sort_by_key(|segment| std::cmp::Reverse(segment.end));
    recent.truncate(RECENT);
    Summary {
        range: range.into(),
        enabled: settings.enabled,
        details: settings.details,
        total_ms: total,
        first_ms,
        days,
        modes: rank(modes),
        noise: rank(noise),
        devices: rank(devices),
        known_devices: Vec::new(),
        categories: rank(categories),
        sources: rank(sources),
        titles: rank(titles),
        urls: rank(urls),
        exact: settings.exact,
        recent,
        skipped: 0,
        write_error: None,
        content_supported: cfg!(windows),
    }
}

pub struct History {
    store: HistoryStore,
    settings: Mutex<Settings>,
    recorder: Mutex<Recorder>,
    last_checkpoint: Mutex<u64>,
    write_error: Mutex<Option<String>>,
}

impl History {
    pub fn new(dir: PathBuf) -> Self {
        let store = HistoryStore::new(dir);
        let write_error = store.recover().err().map(|_| "History from the last session could not be recovered.".to_string());
        let settings = store.load_settings();
        Self {
            store,
            settings: Mutex::new(settings),
            recorder: Mutex::new(Recorder::default()),
            last_checkpoint: Mutex::new(0),
            write_error: Mutex::new(write_error),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().map(|settings| *settings).unwrap_or_default()
    }

    pub fn set_settings(&self, next: Settings) -> Result<Settings, String> {
        self.store.save_settings(&next).map_err(|_| "History settings could not be saved.".to_string())?;
        if let Ok(mut settings) = self.settings.lock() {
            *settings = next;
        }
        Ok(next)
    }

    fn fail(&self, error: Option<io::Error>) {
        if let Ok(mut slot) = self.write_error.lock() {
            *slot = error.map(|_| "History couldn't be saved. Check free disk space and folder permissions.".to_string());
        }
    }

    pub fn tick(&self, now: u64, devices: &[DeviceSample], media: Option<&MediaSample>) {
        let settings = self.settings();
        let (closed, open) = match self.recorder.lock() {
            Ok(mut recorder) => (recorder.observe(now, devices, media, settings), recorder.open_segments()),
            Err(_) => return,
        };
        let mut failure = self.store.append(&closed).err();
        let due = self.last_checkpoint.lock().map(|last| now.saturating_sub(*last) >= CHECKPOINT_MS).unwrap_or(false);
        if due || !closed.is_empty() {
            if let Err(error) = self.store.write_open(&open) {
                failure.get_or_insert(error);
            }
            if let Ok(mut last) = self.last_checkpoint.lock() {
                *last = now;
            }
        }
        self.fail(failure);
    }

    /// Close everything (app exit). Safe to call more than once.
    pub fn finish(&self, now: u64) {
        let closed = self.recorder.lock().map(|mut recorder| recorder.close_all(now)).unwrap_or_default();
        let failure = self.store.append(&closed).err().or_else(|| self.store.write_open(&[]).err());
        self.fail(failure);
    }

    pub fn clear(&self) -> Result<(), String> {
        if let Ok(mut recorder) = self.recorder.lock() {
            recorder.discard();
        }
        self.store.clear().map_err(|_| "History could not be cleared.".to_string())?;
        self.fail(None);
        Ok(())
    }

    /// `device` limits the summary to one headphone by name; `None` means every device.
    pub fn summary(&self, range: &str, now: u64, tz_offset_minutes: i32, device: Option<&str>) -> Summary {
        let settings = self.settings();
        let (mut segments, skipped) = self.store.read_all();
        if let Ok(recorder) = self.recorder.lock() {
            segments.extend(recorder.open_segments());
        }
        let mut known: Vec<String> = segments.iter().map(|segment| segment.device.clone()).collect();
        known.sort();
        known.dedup();
        if let Some(device) = device {
            segments.retain(|segment| segment.device == device);
        }
        let mut summary = summarize(&segments, range, now, tz_offset_minutes, settings);
        summary.known_devices = known;
        summary.skipped = skipped;
        summary.write_error = self.write_error.lock().ok().and_then(|slot| slot.clone());
        summary
    }
}

fn normalize_address(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Normalized addresses of every device the OS currently reports as Bluetooth-connected.
pub fn linked_addresses(connected: &[crate::transport::ActiveAudioDevice]) -> std::collections::BTreeSet<String> {
    connected
        .iter()
        .filter(|device| device.transport == crate::transport::AudioTransport::Bluetooth)
        .map(|device| normalize_address(&device.identity))
        .collect()
}

/// Is this device's Bluetooth link up? When the OS could not be asked, or the device's address is
/// unknown, there is no evidence against the control session, so it is trusted.
pub fn is_linked(linked: Option<&std::collections::BTreeSet<String>>, address: Option<&str>) -> bool {
    match (linked, address) {
        (Some(linked), Some(address)) => linked.contains(&normalize_address(address)),
        _ => true,
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis() as u64).unwrap_or(0)
}

fn text(value: Option<&serde_json::Value>) -> Option<String> {
    value.and_then(|value| value.as_str()).filter(|value| !value.is_empty()).map(str::to_owned)
}

fn truthy(value: Option<&serde_json::Value>) -> bool {
    match value {
        Some(serde_json::Value::Bool(flag)) => *flag,
        Some(serde_json::Value::String(text)) => !matches!(text.as_str(), "" | "off" | "false" | "none"),
        _ => false,
    }
}

/// Sony reports a listening mode; OnePlus reports spatial audio and a game profile instead.
pub fn sample_device(device: &crate::Device) -> DeviceSample {
    let settings = &device.settings;
    let mode = text(settings.get("listeningMode")).or_else(|| {
        if truthy(settings.get("gameSoundProfile")) {
            Some("game".into())
        } else if truthy(settings.get("spatialAudio")) {
            Some("spatial".into())
        } else {
            None
        }
    });
    let noise = text(settings.get("noise")).or_else(|| text(settings.get("noiseProfile")));
    DeviceSample { device: device.name.clone(), model: Some(device.model.clone()).filter(|model| !model.is_empty()), mode, noise }
}

pub fn start(app: AppHandle, history: Arc<History>) {
    std::thread::spawn(move || {
        let runtime = app.state::<Arc<crate::Runtime>>().inner().clone();
        let automation = app.state::<Arc<crate::media_automation::Automation>>().inner().clone();
        loop {
            // Collection of titles and addresses follows the saved setting, so a changed switch
            // takes effect immediately and never depends on the page being open.
            automation.set_exact_details(history.settings().exact_effective());
            std::thread::sleep(TICK);
            if runtime.stopping.load(std::sync::atomic::Ordering::Acquire) {
                history.finish(now_ms());
                break;
            }
            let Ok(state) = runtime.snapshot() else { continue };
            let devices: Vec<DeviceSample> = state
                .devices
                .iter()
                // A control session can outlive the Bluetooth link (headphones switched off or out
                // of range), so the OS must agree before time is counted.
                .filter(|device| device.connection == crate::ConnectionState::Connected && runtime.link_is_up(&device.id))
                .map(sample_device)
                .collect();
            let detail = automation.playback_detail().unwrap_or_default();
            let media = automation.snapshot().ok().and_then(|snapshot| {
                match (snapshot.enabled, snapshot.desired_mode, snapshot.source) {
                    (true, Some(category), Some(source)) => Some(MediaSample {
                        category,
                        source,
                        title: detail.title,
                        artist: detail.artist,
                        url: detail.url,
                    }),
                    _ => None,
                }
            });
            history.tick(now_ms(), &devices, media.as_ref());
        }
    });
}

#[tauri::command]
pub fn get_history(app: AppHandle, range: String, tz_offset_minutes: i32, device: Option<String>) -> Summary {
    app.state::<Arc<History>>().summary(&range, now_ms(), tz_offset_minutes, device.as_deref())
}

#[tauri::command]
pub fn get_history_settings(app: AppHandle) -> Settings {
    app.state::<Arc<History>>().settings()
}

#[tauri::command]
pub fn set_history_settings(app: AppHandle, enabled: bool, details: bool, exact: bool) -> Result<Settings, String> {
    let saved = app.state::<Arc<History>>().set_settings(Settings { enabled, details, exact })?;
    app.state::<Arc<crate::media_automation::Automation>>().set_exact_details(saved.exact_effective());
    Ok(saved)
}

#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<(), String> {
    app.state::<Arc<History>>().clear()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: Settings = Settings { enabled: true, details: false, exact: false };
    const DETAILS: Settings = Settings { enabled: true, details: true, exact: false };
    const EXACT: Settings = Settings { enabled: true, details: true, exact: true };

    fn device(name: &str, mode: &str) -> DeviceSample {
        DeviceSample { device: name.into(), model: None, mode: Some(mode.into()), noise: Some("off".into()) }
    }
    fn media(category: &str, source: &str) -> MediaSample {
        MediaSample { category: category.into(), source: source.into(), title: None, artist: None, url: None }
    }
    fn exact_media() -> MediaSample {
        MediaSample {
            category: "music".into(), source: "Spotify".into(),
            title: Some("Karma Police".into()), artist: Some("Radiohead".into()),
            url: Some("https://open.spotify.com/track/abc?si=xyz".into()),
        }
    }
    fn seg(start: u64, end: u64, device: &str, mode: &str) -> Segment {
        Segment { start, end, device: device.into(), mode: Some(mode.into()), ..Segment::default() }
    }
    const MINUTE: u64 = 60_000;
    const HOUR: u64 = 60 * MINUTE;

    #[test]
    fn a_mode_change_closes_one_segment_and_opens_the_next() {
        let mut recorder = Recorder::default();
        assert!(recorder.observe(0, &[device("XM6", "standard")], None, ON).is_empty());
        assert!(recorder.observe(10_000, &[device("XM6", "standard")], None, ON).is_empty());
        let closed = recorder.observe(20_000, &[device("XM6", "cinema")], None, ON);
        let mut expected = seg(0, 20_000, "XM6", "standard");
        expected.noise = Some("off".into());
        assert_eq!(closed, vec![expected]);
        assert_eq!(recorder.open_segments()[0].mode.as_deref(), Some("cinema"));
    }

    #[test]
    fn a_disconnect_closes_the_segment_and_two_devices_record_independently() {
        let mut recorder = Recorder::default();
        recorder.observe(0, &[device("A", "standard"), device("B", "music")], None, ON);
        let closed = recorder.observe(30_000, &[device("A", "standard")], None, ON);
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].device, "B");
        assert_eq!(closed[0].duration(), 30_000);
        assert_eq!(recorder.open_segments().len(), 1);
    }

    #[test]
    fn blips_shorter_than_five_seconds_are_dropped() {
        let mut recorder = Recorder::default();
        recorder.observe(0, &[device("A", "standard")], None, ON);
        assert!(recorder.observe(3_000, &[device("A", "game")], None, ON).is_empty());
        assert!(recorder.observe(60_000, &[], None, ON).iter().all(|s| s.duration() >= MIN_SEGMENT_MS));
    }

    #[test]
    fn sleep_ends_the_segment_at_the_last_sample_not_at_wake() {
        let mut recorder = Recorder::default();
        recorder.observe(0, &[device("A", "standard")], None, ON);
        recorder.observe(10_000, &[device("A", "standard")], None, ON);
        let closed = recorder.observe(10 * HOUR, &[device("A", "standard")], None, ON);
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].end, 10_000, "the sleep is not counted as listening");
        assert_eq!(recorder.open_segments()[0].start, 10 * HOUR);
    }

    #[test]
    fn a_segment_is_capped_at_twelve_hours() {
        let mut recorder = Recorder::default();
        let mut now = 0;
        let mut closed = Vec::new();
        while now <= 13 * HOUR {
            closed.extend(recorder.observe(now, &[device("A", "standard")], None, ON));
            now += 10_000;
        }
        assert_eq!(closed.len(), 1);
        assert!(closed[0].duration() <= MAX_SEGMENT_MS + 10_000);
    }

    #[test]
    fn details_are_recorded_only_while_the_toggle_is_on_and_only_from_then_on() {
        let mut recorder = Recorder::default();
        recorder.observe(0, &[device("A", "music")], Some(&media("music", "Spotify")), ON);
        assert_eq!(recorder.open_segments()[0].source, None, "toggle off: nothing detailed is stored");
        let closed = recorder.observe(20_000, &[device("A", "music")], Some(&media("music", "Spotify")), DETAILS);
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].source, None, "earlier segment keeps no details");
        assert_eq!(recorder.open_segments()[0].source.as_deref(), Some("Spotify"));
        assert_eq!(recorder.open_segments()[0].category.as_deref(), Some("music"));
    }

    #[test]
    fn disabling_history_closes_everything_and_records_nothing_new() {
        let mut recorder = Recorder::default();
        recorder.observe(0, &[device("A", "standard")], None, ON);
        let closed = recorder.observe(20_000, &[device("A", "standard")], None, Settings { enabled: false, details: false, exact: false });
        assert_eq!(closed.len(), 1);
        assert!(recorder.open_segments().is_empty());
    }

    #[test]
    fn summary_splits_a_segment_across_local_midnight() {
        // 23:00 to 01:00 local on a UTC+2 machine (JS offset is -120).
        let offset = -120;
        let day = 20 * DAY_MS;
        let midnight_local = day - 2 * HOUR; // local midnight expressed in UTC
        let segment = seg(midnight_local - HOUR, midnight_local + HOUR, "A", "standard");
        let summary = summarize(&[segment], "7d", midnight_local + 2 * HOUR, offset, ON);
        assert_eq!(summary.total_ms, 2 * HOUR);
        let worked: Vec<u64> = summary.days.iter().map(|d| d.ms).filter(|ms| *ms > 0).collect();
        assert_eq!(worked, vec![HOUR, HOUR]);
    }

    #[test]
    fn summary_ranges_clip_and_group_by_mode_device_and_category() {
        let now = 40 * DAY_MS + 12 * HOUR;
        let old = seg(10 * DAY_MS, 10 * DAY_MS + HOUR, "A", "standard");
        let mut today = seg(now - 2 * HOUR, now - HOUR, "A", "music");
        today.category = Some("music".into());
        today.source = Some("Spotify".into());
        let other = seg(now - 30 * MINUTE, now - 10 * MINUTE, "B", "game");
        let all = [old, today, other];
        let day = summarize(&all, "today", now, 0, DETAILS);
        assert_eq!(day.total_ms, HOUR + 20 * MINUTE);
        assert_eq!(day.modes[0], Bucket { key: "music".into(), ms: HOUR });
        assert_eq!(day.devices.len(), 2);
        assert_eq!(day.categories, vec![Bucket { key: "music".into(), ms: HOUR }]);
        assert_eq!(day.sources[0].key, "Spotify");
        assert_eq!(summarize(&all, "30d", now, 0, ON).total_ms, HOUR + 20 * MINUTE, "old segment is outside 30 days");
        assert_eq!(summarize(&all, "all", now, 0, ON).total_ms, 2 * HOUR + 20 * MINUTE);
        assert_eq!(day.recent[0].device, "B");
    }

    #[test]
    fn device_sampling_reads_sony_and_oneplus_shapes() {
        let mut settings = BTreeMap::new();
        settings.insert("listeningMode".to_string(), serde_json::json!("cinema"));
        settings.insert("noise".to_string(), serde_json::json!("ambient"));
        let sony = crate::Device { id: "1".into(), name: "XM6".into(), model: "WH-1000XM6".into(), connection: crate::ConnectionState::Connected, battery: None, firmware: None, capabilities: vec![], settings };
        let sample = sample_device(&sony);
        assert_eq!((sample.mode.as_deref(), sample.noise.as_deref()), (Some("cinema"), Some("ambient")));
        let mut settings = BTreeMap::new();
        settings.insert("gameSoundProfile".to_string(), serde_json::json!(true));
        settings.insert("spatialAudio".to_string(), serde_json::json!(true));
        let earbuds = crate::Device { settings, ..sony };
        assert_eq!(sample_device(&earbuds).mode.as_deref(), Some("game"), "game wins over spatial");
    }

    #[test]
    fn time_is_counted_only_while_the_os_reports_the_bluetooth_link_up() {
        use crate::transport::{ActiveAudioDevice, AudioTransport};
        let active = |identity: &str, transport| ActiveAudioDevice { identity: identity.into(), name: "x".into(), transport };
        let linked = linked_addresses(&[active("AA:BB:CC:00:11:22", AudioTransport::Bluetooth), active("wired", AudioTransport::Other)]);
        assert_eq!(linked.len(), 1, "only Bluetooth links count");
        assert!(is_linked(Some(&linked), Some("aa-bb-cc-00-11-22")), "address formats are normalized");
        assert!(!is_linked(Some(&linked), Some("11:22:33:44:55:66")), "a headphone the OS no longer lists as connected");
        assert!(!is_linked(Some(&std::collections::BTreeSet::new()), Some("aa:bb:cc:00:11:22")), "no Bluetooth device connected at all");
        assert!(is_linked(None, Some("aa:bb:cc:00:11:22")), "OS could not be asked: trust the control session");
        assert!(is_linked(Some(&linked), None), "unknown address: trust the control session");
    }

    #[test]
    fn a_headphone_that_drops_off_bluetooth_stops_counting_within_one_sample() {
        let mut recorder = Recorder::default();
        let mut closed = Vec::new();
        for tick in 0..=6 {
            closed.extend(recorder.observe(tick * 5_000, &[device("A", "standard")], None, ON));
        }
        // The sampler stops passing the device once the link is gone.
        closed.extend(recorder.observe(35_000, &[], None, ON));
        for tick in 8..=60 {
            closed.extend(recorder.observe(tick * 5_000, &[], None, ON));
        }
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].end, 35_000, "time stops at the first sample without the link");
        assert!(recorder.open_segments().is_empty());
    }

    #[test]
    fn exact_titles_and_urls_need_all_three_switches_and_never_leak_when_off() {
        let url_only = Settings { enabled: true, details: false, exact: true };
        for (settings, recorded) in [(ON, false), (DETAILS, false), (url_only, false), (EXACT, true)] {
            let mut recorder = Recorder::default();
            recorder.observe(0, &[device("A", "music")], Some(&exact_media()), settings);
            let open = &recorder.open_segments()[0];
            assert_eq!(open.title.is_some() && open.artist.is_some() && open.url.is_some(), recorded, "{settings:?}");
            if !recorded {
                assert!(open.title.is_none() && open.artist.is_none() && open.url.is_none());
            }
        }
    }

    #[test]
    fn a_new_song_or_page_starts_a_new_segment_and_the_summary_ranks_titles_and_urls() {
        let mut recorder = Recorder::default();
        let mut closed = Vec::new();
        let first = exact_media();
        let mut second = exact_media();
        second.title = Some("Creep".into());
        second.url = Some("https://open.spotify.com/track/def?si=uvw".into());
        for tick in 0..=6 {
            closed.extend(recorder.observe(tick * 5_000, &[device("A", "music")], Some(&first), EXACT));
        }
        for tick in 7..=9 {
            closed.extend(recorder.observe(tick * 5_000, &[device("A", "music")], Some(&second), EXACT));
        }
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].title.as_deref(), Some("Karma Police"));
        let mut all = closed;
        all.extend(recorder.open_segments());
        let summary = summarize(&all, "all", 10 * 5_000, 0, EXACT);
        assert_eq!(summary.titles[0].key, "Radiohead \u{2014} Karma Police");
        assert_eq!(summary.urls.len(), 2);
        assert!(summary.urls[0].key.contains("?si=xyz"), "the query is kept as the user asked");
        assert!(summary.exact);
    }

    #[test]
    fn old_history_files_without_the_new_fields_still_load() {
        let old = r#"{"start":0,"end":60000,"device":"A","mode":"music","category":"music","source":"Spotify"}"#;
        let segment: Segment = serde_json::from_str(old).unwrap();
        assert_eq!((segment.title, segment.url), (None, None));
        let settings: Settings = serde_json::from_str(r#"{"enabled":true,"details":true}"#).unwrap();
        assert!(!settings.exact, "an existing install must not start recording exact titles");
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sound-connect-history-{name}-{}", now_ms()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn store_round_trips_skips_corrupt_lines_recovers_open_segments_and_clears() {
        let dir = scratch("store");
        let store = HistoryStore::new(dir.clone());
        store.append(&[seg(0, 60_000, "A", "standard")]).unwrap();
        fs::write(dir.join(HISTORY_FILE), [fs::read(dir.join(HISTORY_FILE)).unwrap(), b"{not json\n".to_vec()].concat()).unwrap();
        store.write_open(&[seg(100_000, 200_000, "A", "music")]).unwrap();
        let (segments, skipped) = store.read_all();
        assert_eq!((segments.len(), skipped), (1, 1));
        store.recover().unwrap();
        assert_eq!(store.read_all().0.len(), 2, "the open segment is recovered at its checkpoint");
        assert!(!dir.join(OPEN_FILE).exists());
        store.clear().unwrap();
        assert_eq!(store.read_all(), (vec![], 0));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn history_flushes_on_finish_and_clear_forgets_open_segments() {
        let dir = scratch("history");
        let history = History::new(dir.clone());
        for now in (1_000..=61_000).step_by(5_000) {
            history.tick(now, &[device("A", "standard")], None);
        }
        assert_eq!(history.summary("today", 61_000, 0, None).total_ms, 60_000, "the live segment is included");
        history.finish(61_000);
        assert_eq!(history.store.read_all().0.len(), 1);
        history.tick(70_000, &[device("A", "standard")], None);
        history.clear().unwrap();
        assert_eq!(history.summary("all", 80_000, 0, None).total_ms, 0);
        assert!(!dir.join(HISTORY_FILE).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn history_can_be_limited_to_one_device_while_the_picker_still_lists_all_of_them() {
        let dir = scratch("device-filter");
        let history = History::new(dir.clone());
        history.store.append(&[seg(0, 600_000, "XM6", "standard"), seg(0, 300_000, "Buds", "game")]).unwrap();
        let both = history.summary("all", 700_000, 0, None);
        assert_eq!((both.total_ms, both.devices.len()), (900_000, 2));
        let xm6 = history.summary("all", 700_000, 0, Some("XM6"));
        assert_eq!(xm6.total_ms, 600_000, "only the chosen headphone is counted");
        assert_eq!(xm6.modes.len(), 1);
        assert_eq!(xm6.known_devices, vec!["Buds".to_string(), "XM6".to_string()], "the picker still lists every device");
        assert_eq!(history.summary("all", 700_000, 0, Some("Unknown")).total_ms, 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_default_to_basic_on_details_off_and_persist() {
        let dir = scratch("settings");
        let history = History::new(dir.clone());
        assert_eq!(history.settings(), Settings { enabled: true, details: false, exact: false });
        history.set_settings(Settings { enabled: true, details: true, exact: true }).unwrap();
        assert!(History::new(dir.clone()).settings().exact_effective());
        let _ = fs::remove_dir_all(dir);
    }
}
