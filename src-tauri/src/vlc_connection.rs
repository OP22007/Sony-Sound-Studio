//! Private opt-in configuration for VLC's documented local status endpoint.
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::{Arc, Mutex}};
use tauri::{AppHandle, Emitter, Manager};
use crate::{media_vlc::{self, Playback}, preferences::PreferencesStore};

const CONFIG: &str = "vlc-connection.json";
#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct Config { enabled: bool, password: String }

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    enabled: bool, password_configured: bool, status: String,
    playing: bool, kind: Option<String>,
}
struct Inner { config: Config, snapshot: Snapshot }
pub struct VlcConnection { inner: Mutex<Inner>, store: PreferencesStore }

impl VlcConnection {
    pub fn new(path: PathBuf) -> Self {
        let config = std::fs::read(path.join(CONFIG)).ok().filter(|bytes| bytes.len() <= 4096)
            .and_then(|bytes| serde_json::from_slice::<Config>(&bytes).ok())
            .filter(|config| valid_password(&config.password) || (!config.enabled && config.password.is_empty()))
            .unwrap_or_default();
        let snapshot = Snapshot { enabled: config.enabled, password_configured: !config.password.is_empty(),
            status: if config.enabled { "Waiting for a local VLC status check." } else { "VLC connection is off." }.into(), playing: false, kind: None };
        Self { inner: Mutex::new(Inner { config, snapshot }), store: PreferencesStore::new(path) }
    }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        self.inner.lock().map(|inner| inner.snapshot.clone()).map_err(|_| "VLC connection unavailable.".into())
    }

    pub fn enabled(&self) -> bool { self.inner.lock().is_ok_and(|inner| inner.config.enabled) }

    pub fn poll(&self, app: &AppHandle) -> Result<Option<Playback>, String> {
        let (enabled, password) = {
            let inner = self.inner.lock().map_err(|_| "VLC connection unavailable.")?;
            (inner.config.enabled, inner.config.password.clone())
        };
        if !enabled { return Ok(None); }
        let result = media_vlc::read(&password);
        if !self.inner.lock().is_ok_and(|inner| inner.config.enabled && inner.config.password == password) { return Ok(None); }
        self.publish_result(app, &password, &result);
        result.map(Some)
    }

    fn publish_result(&self, app: &AppHandle, password: &str, result: &Result<Playback, String>) {
        if let Ok(mut inner) = self.inner.lock() {
            // Ignore a reply issued with a password the user has since replaced.
            if !inner.config.enabled || password != inner.config.password { return; }
            let mut next = inner.snapshot.clone();
            next.playing = result.as_ref().is_ok_and(|playback| playback.playing);
            next.kind = result.as_ref().ok().map(|playback| playback.kind.into());
            next.status = match result {
                Ok(playback) if playback.playing => match playback.kind {
                    "movie" => "Connected to VLC · video playback detected.",
                    "music" => "Connected to VLC · audio playback detected.",
                    _ => "Connected to VLC · playback type is unknown; Standard is used.",
                }.into(),
                Ok(_) => "Connected to VLC · paused or stopped.".into(),
                Err(error) => error.clone(),
            };
            if next != inner.snapshot { inner.snapshot = next.clone(); let _ = app.emit("vlc-connection", &next); }
        }
    }

    fn save(&self, enabled: bool, password: Option<String>) -> Result<Snapshot, String> {
        let mut inner = self.inner.lock().map_err(|_| "VLC connection unavailable.")?;
        let password = password.filter(|password| !password.is_empty()).unwrap_or_else(|| inner.config.password.clone());
        if enabled && !valid_password(&password) { return Err("Enter the dedicated password set in VLC's Lua HTTP settings (1–512 characters, without control characters).".into()); }
        let config = Config { enabled, password };
        self.store.atomic_write(CONFIG, &serde_json::to_vec(&config).map_err(|_| "Could not encode VLC preferences.")?)
            .map_err(|_| "Could not save VLC connection preferences.")?;
        inner.config = config;
        inner.snapshot.enabled = enabled;
        inner.snapshot.password_configured = !inner.config.password.is_empty();
        inner.snapshot.playing = false; inner.snapshot.kind = None;
        inner.snapshot.status = if enabled { "Waiting for a local VLC status check." } else { "VLC connection is off." }.into();
        Ok(inner.snapshot.clone())
    }
}

fn valid_password(password: &str) -> bool {
    !password.is_empty() && password.len() <= 512 && !password.chars().any(char::is_control)
}

#[tauri::command]
pub fn get_vlc_connection(app: AppHandle) -> Result<Snapshot, String> { app.state::<Arc<VlcConnection>>().snapshot() }

#[tauri::command]
pub fn configure_vlc_connection(app: AppHandle, enabled: bool, password: Option<String>) -> Result<Snapshot, String> {
    let snapshot = app.state::<Arc<VlcConnection>>().save(enabled, password)?;
    if let Some(automation) = app.try_state::<Arc<crate::media_automation::Automation>>() { automation.source_configuration_changed(); }
    let _ = app.emit("vlc-connection", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn test_vlc_connection(app: AppHandle, password: Option<String>) -> Result<Snapshot, String> {
    let state = app.state::<Arc<VlcConnection>>().inner().clone();
    let password = password.filter(|password| !password.is_empty()).unwrap_or_else(|| state.inner.lock().map(|inner| inner.config.password.clone()).unwrap_or_default());
    if !valid_password(&password) { return Err("Enter the password configured in VLC's Lua HTTP settings first.".into()); }
    let result = tauri::async_runtime::spawn_blocking(move || media_vlc::read(&password)).await
        .map_err(|_| "The VLC status check stopped unexpectedly.")??;
    let mut snapshot = state.snapshot()?;
    snapshot.playing = result.playing;
    snapshot.kind = Some(result.kind.into());
    snapshot.status = if result.playing { "VLC connection test succeeded; playback detected." } else { "VLC connection test succeeded; playback is paused or stopped." }.into();
    // A test response is informational; it never enables automation or changes a headphone.
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vlc_requires_opt_in_and_does_not_expose_password_in_snapshots() {
        let mut nonce = [0u8; 8]; getrandom::fill(&mut nonce).unwrap();
        let path = std::env::temp_dir().join(format!("sound-connect-vlc-{}", u64::from_le_bytes(nonce)));
        let state = VlcConnection::new(path.clone());
        assert!(!state.enabled());
        assert!(state.save(true, None).is_err());
        assert!(state.save(true, Some("bad\r\npassword".into())).is_err());
        state.save(true, Some("dedicated-test-secret".into())).unwrap();
        let json = serde_json::to_string(&state.snapshot().unwrap()).unwrap();
        assert!(!json.contains("dedicated-test-secret"));
        let restored = VlcConnection::new(path.clone());
        assert!(restored.enabled());
        restored.save(false, None).unwrap();
        assert!(!restored.enabled());
        assert!(restored.snapshot().unwrap().password_configured);
        std::fs::remove_dir_all(path).unwrap();
    }
}
