//! Private opt-in configuration for VLC's documented local status endpoint.
use crate::{
    media_vlc::{self, Playback},
    preferences::PreferencesStore,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, Manager};

const CONFIG: &str = "vlc-connection.json";
#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct Config {
    enabled: bool,
    password: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    enabled: bool,
    password_configured: bool,
    status: String,
    playing: bool,
    kind: Option<String>,
    startup_configured: bool,
}
struct Inner {
    config: Config,
    snapshot: Snapshot,
}
pub struct VlcConnection {
    inner: Mutex<Inner>,
    store: PreferencesStore,
}

impl VlcConnection {
    pub fn new(path: PathBuf) -> Self {
        let config = std::fs::read(path.join(CONFIG))
            .ok()
            .filter(|bytes| bytes.len() <= 4096)
            .and_then(|bytes| serde_json::from_slice::<Config>(&bytes).ok())
            .filter(|config| {
                valid_password(&config.password) || (!config.enabled && config.password.is_empty())
            })
            .unwrap_or_default();
        let snapshot = Snapshot {
            enabled: config.enabled,
            password_configured: !config.password.is_empty(),
            status: if config.enabled {
                "Waiting for a local VLC status check."
            } else {
                "VLC connection is off."
            }
            .into(),
            playing: false,
            kind: None,
            startup_configured: vlcrc_path()
                .is_some_and(|path| vlcrc_is_configured(&path, &config.password)),
        };
        Self {
            inner: Mutex::new(Inner { config, snapshot }),
            store: PreferencesStore::new(path),
        }
    }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        self.inner
            .lock()
            .map(|inner| inner.snapshot.clone())
            .map_err(|_| "VLC connection unavailable.".into())
    }

    pub fn enabled(&self) -> bool {
        self.inner.lock().is_ok_and(|inner| inner.config.enabled)
    }

    pub fn poll(&self, app: &AppHandle) -> Result<Option<Playback>, String> {
        let (enabled, password) = {
            let inner = self
                .inner
                .lock()
                .map_err(|_| "VLC connection unavailable.")?;
            (inner.config.enabled, inner.config.password.clone())
        };
        if !enabled {
            return Ok(None);
        }
        let result = media_vlc::read(&password);
        if !self
            .inner
            .lock()
            .is_ok_and(|inner| inner.config.enabled && inner.config.password == password)
        {
            return Ok(None);
        }
        self.publish_result(app, &password, &result);
        result.map(Some)
    }

    fn publish_result(&self, app: &AppHandle, password: &str, result: &Result<Playback, String>) {
        if let Ok(mut inner) = self.inner.lock() {
            // Ignore a reply issued with a password the user has since replaced.
            if !inner.config.enabled || password != inner.config.password {
                return;
            }
            let mut next = inner.snapshot.clone();
            next.playing = result.as_ref().is_ok_and(|playback| playback.playing);
            next.kind = result.as_ref().ok().map(|playback| playback.kind.into());
            next.status = match result {
                Ok(playback) if playback.playing => match playback.kind {
                    "movie" => "Connected to VLC · video playback detected.",
                    "music" => "Connected to VLC · audio playback detected.",
                    _ => "Connected to VLC · playback type is unknown; Standard is used.",
                }
                .into(),
                Ok(_) => "Connected to VLC · paused or stopped.".into(),
                Err(error) => error.clone(),
            };
            if next != inner.snapshot {
                inner.snapshot = next.clone();
                let _ = app.emit("vlc-connection", &next);
            }
        }
    }

    fn save(&self, enabled: bool, password: Option<String>) -> Result<Snapshot, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "VLC connection unavailable.")?;
        let password = password
            .filter(|password| !password.is_empty())
            .unwrap_or_else(|| inner.config.password.clone());
        if enabled && !valid_password(&password) {
            return Err("Enter the dedicated password set in VLC's Lua HTTP settings (1–512 characters, without control characters).".into());
        }
        let config = Config { enabled, password };
        self.store
            .atomic_write(
                CONFIG,
                &serde_json::to_vec(&config).map_err(|_| "Could not encode VLC preferences.")?,
            )
            .map_err(|_| "Could not save VLC connection preferences.")?;
        inner.config = config;
        inner.snapshot.enabled = enabled;
        inner.snapshot.password_configured = !inner.config.password.is_empty();
        inner.snapshot.startup_configured =
            vlcrc_path().is_some_and(|path| vlcrc_is_configured(&path, &inner.config.password));
        inner.snapshot.playing = false;
        inner.snapshot.kind = None;
        inner.snapshot.status = if enabled {
            "Waiting for a local VLC status check."
        } else {
            "VLC connection is off."
        }
        .into();
        Ok(inner.snapshot.clone())
    }
}

fn vlcrc_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("vlc").join("vlcrc"))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn vlc_is_running() -> Result<bool, String> {
    #[cfg(windows)]
    {
        let system_root =
            std::env::var_os("SystemRoot").ok_or("Could not locate Windows process tools.")?;
        let output = std::process::Command::new(
            PathBuf::from(system_root)
                .join("System32")
                .join("tasklist.exe"),
        )
        .args(["/FI", "IMAGENAME eq vlc.exe", "/FO", "CSV", "/NH"])
        .output()
        .map_err(|_| "Could not check whether VLC is running.")?;
        if !output.status.success() {
            return Err("Could not check whether VLC is running.".into());
        }
        let output = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        Ok(output.lines().any(|line| line.starts_with("\"vlc.exe\"")))
    }
    #[cfg(not(windows))]
    {
        Err("Persistent VLC setup is currently supported on Windows only.".into())
    }
}

fn vlcrc_is_configured(path: &std::path::Path, expected_password: &str) -> bool {
    std::fs::read_to_string(path).ok().is_some_and(|text| {
        let values = vlcrc_values(&text);
        values
            .get("extraintf")
            .is_some_and(|value| value.split(':').any(|item| item == "http"))
            && values
                .get("http-host")
                .is_some_and(|value| *value == "127.0.0.1")
            && values
                .get("http-port")
                .is_some_and(|value| *value == "8080")
            && values
                .get("http-password")
                .is_some_and(|value| !expected_password.is_empty() && *value == expected_password)
    })
}

fn vlcrc_values(text: &str) -> std::collections::BTreeMap<&str, &str> {
    let mut values = std::collections::BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.trim(), value.trim());
        }
    }
    values
}

fn configure_vlcrc(
    path: &std::path::Path,
    password: &str,
    vlc_running: bool,
) -> Result<PathBuf, String> {
    if vlc_running {
        return Err("Close VLC before saving its startup settings, then try again.".into());
    }
    if !valid_password(password)
        || password
            .chars()
            .any(|character| matches!(character, '#' | '=' | '\\') || character.is_whitespace())
    {
        return Err("The saved VLC password contains characters that cannot be safely written to VLC preferences.".into());
    }
    let old = match std::fs::read(path) {
        Ok(bytes) if bytes.len() <= 1024 * 1024 => bytes,
        Ok(_) => return Err("VLC preferences file is unexpectedly large.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_) => return Err("Could not read VLC preferences.".into()),
    };
    let text = std::str::from_utf8(&old).map_err(|_| "VLC preferences are not valid UTF-8.")?;
    let values = vlcrc_values(text);
    let existing = values.get("extraintf").copied().unwrap_or("");
    let mut interfaces: Vec<&str> = existing
        .split(':')
        .filter(|item| !item.is_empty() && *item != "http")
        .collect();
    interfaces.push("http");
    let replacements = [
        ("extraintf", interfaces.join(":")),
        ("http-host", "127.0.0.1".into()),
        ("http-port", "8080".into()),
        ("http-password", password.into()),
    ];
    let keys = ["extraintf", "http-host", "http-port", "http-password"];
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut output = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#')
            && trimmed
                .split_once('=')
                .is_some_and(|(key, _)| keys.contains(&key.trim()))
        {
            continue;
        }
        output.push_str(line);
        output.push_str(newline);
    }
    if !output.is_empty() && !output.ends_with(newline) {
        output.push_str(newline);
    }
    for (key, value) in replacements {
        output.push_str(key);
        output.push('=');
        output.push_str(&value);
        output.push_str(newline);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "Could not access VLC preferences folder.")?;
    }
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce).map_err(|_| "Could not prepare VLC preferences backup.")?;
    let backup = path.with_extension(format!("vlcrc-{:016x}.bak", u64::from_le_bytes(nonce)));
    if path.exists() {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
            .map_err(|_| "Could not create a backup of VLC preferences.")?;
        if let Err(_) = file.write_all(&old).and_then(|_| file.sync_all()) {
            let _ = std::fs::remove_file(&backup);
            return Err("Could not save the VLC preferences backup.".into());
        }
    }
    let temp = path.with_extension(format!("vlcrc-{:016x}.tmp", u64::from_le_bytes(nonce)));
    let write_result = (|| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(output.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, path)?;
        Ok::<(), std::io::Error>(())
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temp);
        if backup.exists() {
            let _ = std::fs::remove_file(&backup);
        }
        return Err("Could not atomically save VLC preferences.".into());
    }
    Ok(backup)
}

fn valid_password(password: &str) -> bool {
    !password.is_empty() && password.len() <= 512 && !password.chars().any(char::is_control)
}

#[tauri::command]
pub fn get_vlc_connection(app: AppHandle) -> Result<Snapshot, String> {
    app.state::<Arc<VlcConnection>>().snapshot()
}

#[tauri::command]
pub fn configure_vlc_connection(
    app: AppHandle,
    enabled: bool,
    password: Option<String>,
) -> Result<Snapshot, String> {
    let snapshot = app.state::<Arc<VlcConnection>>().save(enabled, password)?;
    if let Some(automation) = app.try_state::<Arc<crate::media_automation::Automation>>() {
        automation.source_configuration_changed();
    }
    let _ = app.emit("vlc-connection", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn configure_vlc_startup(app: AppHandle) -> Result<Snapshot, String> {
    if !cfg!(windows) {
        return Err("Persistent VLC setup is currently supported on Windows only.".into());
    }
    let state = app.state::<Arc<VlcConnection>>();
    let password = {
        let inner = state
            .inner
            .lock()
            .map_err(|_| "VLC connection unavailable.")?;
        if !inner.config.enabled || !valid_password(&inner.config.password) {
            return Err(
                "Save and enable the VLC connection with its dedicated password first.".into(),
            );
        }
        inner.config.password.clone()
    };
    let path = vlcrc_path().ok_or("Could not locate VLC preferences.")?;
    tauri::async_runtime::spawn_blocking(move || {
        if vlc_is_running()? {
            return Err("Close VLC before saving its startup settings, then try again.".into());
        }
        configure_vlcrc(&path, &password, false)
    })
    .await
    .map_err(|_| "The VLC startup setup stopped unexpectedly.")??;
    let mut inner = state
        .inner
        .lock()
        .map_err(|_| "VLC connection unavailable.")?;
    inner.snapshot.startup_configured = true;
    let snapshot = inner.snapshot.clone();
    drop(inner);
    let _ = app.emit("vlc-connection", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn test_vlc_connection(
    app: AppHandle,
    password: Option<String>,
) -> Result<Snapshot, String> {
    let state = app.state::<Arc<VlcConnection>>().inner().clone();
    let password = password
        .filter(|password| !password.is_empty())
        .unwrap_or_else(|| {
            state
                .inner
                .lock()
                .map(|inner| inner.config.password.clone())
                .unwrap_or_default()
        });
    if !valid_password(&password) {
        return Err("Enter the password configured in VLC's Lua HTTP settings first.".into());
    }
    let result = tauri::async_runtime::spawn_blocking(move || media_vlc::read(&password))
        .await
        .map_err(|_| "The VLC status check stopped unexpectedly.")??;
    let mut snapshot = state.snapshot()?;
    snapshot.playing = result.playing;
    snapshot.kind = Some(result.kind.into());
    snapshot.status = if result.playing {
        "VLC connection test succeeded; playback detected."
    } else {
        "VLC connection test succeeded; playback is paused or stopped."
    }
    .into();
    // A test response is informational; it never enables automation or changes a headphone.
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vlc_requires_opt_in_and_does_not_expose_password_in_snapshots() {
        let mut nonce = [0u8; 8];
        getrandom::fill(&mut nonce).unwrap();
        let path =
            std::env::temp_dir().join(format!("sound-connect-vlc-{}", u64::from_le_bytes(nonce)));
        let state = VlcConnection::new(path.clone());
        assert!(!state.enabled());
        assert!(state.save(true, None).is_err());
        assert!(state.save(true, Some("bad\r\npassword".into())).is_err());
        state
            .save(true, Some("dedicated-test-secret".into()))
            .unwrap();
        let json = serde_json::to_string(&state.snapshot().unwrap()).unwrap();
        assert!(!json.contains("dedicated-test-secret"));
        let restored = VlcConnection::new(path.clone());
        assert!(restored.enabled());
        restored.save(false, None).unwrap();
        assert!(!restored.enabled());
        assert!(restored.snapshot().unwrap().password_configured);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn startup_configuration_preserves_preferences_and_backups_before_atomic_write() {
        let path = std::env::temp_dir().join(format!("sound-connect-vlcrc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let vlcrc = path.join("vlcrc");
        std::fs::write(
            &vlcrc,
            "# keep comment\nextraintf=rc:lua\nvolume=256\nhttp-port=1234\n",
        )
        .unwrap();

        assert!(configure_vlcrc(&vlcrc, "local-pass-42", true).is_err());
        assert_eq!(
            std::fs::read_to_string(&vlcrc).unwrap(),
            "# keep comment\nextraintf=rc:lua\nvolume=256\nhttp-port=1234\n"
        );
        let backup = configure_vlcrc(&vlcrc, "local-pass-42", false).unwrap();
        let saved = std::fs::read_to_string(&vlcrc).unwrap();
        assert!(saved.contains("# keep comment"));
        assert!(saved.contains("volume=256"));
        assert!(saved.contains("extraintf=rc:lua:http"));
        assert!(saved.contains("http-host=127.0.0.1"));
        assert!(saved.contains("http-port=8080"));
        assert!(saved.contains("http-password=local-pass-42"));
        assert_eq!(
            std::fs::read_to_string(backup).unwrap(),
            "# keep comment\nextraintf=rc:lua\nvolume=256\nhttp-port=1234\n"
        );
        assert!(vlcrc_is_configured(&vlcrc, "local-pass-42"));
        assert!(!vlcrc_is_configured(&vlcrc, "another-password"));
        assert!(configure_vlcrc(&vlcrc, "unsafe pass", false).is_err());
        std::fs::remove_dir_all(path).unwrap();
    }
}
