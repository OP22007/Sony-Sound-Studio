//! Signed release updates. The webview never supplies URLs, keys or executable paths.
use serde::Serialize;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    current_version: String,
    status: String,
    version: Option<String>,
    notes: Option<String>,
    downloaded: u64,
    total: Option<u64>,
    error: Option<String>,
}

pub struct AppUpdates {
    snapshot: Mutex<Snapshot>,
    update: Mutex<Option<Update>>,
    busy: AtomicBool,
}

struct Operation<'a>(&'a AtomicBool);
impl Drop for Operation<'_> {
    fn drop(&mut self) { self.0.store(false, Ordering::Release); }
}

fn unsupported() -> Option<&'static str> {
    if cfg!(debug_assertions) {
        return Some("In-app installation is available in packaged release builds. Development builds must be updated from source.");
    }
    if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_none() {
        return Some("In-app updates require the AppImage edition on Linux. Update DEB/RPM installations using a new package from Releases.");
    }
    None
}

impl AppUpdates {
    pub fn new() -> Self {
        let reason = unsupported();
        Self {
            snapshot: Mutex::new(Snapshot {
                current_version: env!("CARGO_PKG_VERSION").into(),
                status: if reason.is_some() { "unsupported" } else { "idle" }.into(),
                version: None, notes: None, downloaded: 0, total: None,
                error: reason.map(str::to_owned),
            }),
            update: Mutex::new(None), busy: AtomicBool::new(false),
        }
    }

    fn claim(&self) -> Result<Operation<'_>, String> {
        self.busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "An update operation is already running".to_string())?;
        Ok(Operation(&self.busy))
    }

    fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn publish(&self, app: &AppHandle, change: impl FnOnce(&mut Snapshot)) -> Snapshot {
        let snapshot = {
            let mut value = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
            change(&mut value);
            value.clone()
        };
        let _ = app.emit("app-update", &snapshot);
        snapshot
    }

    fn failed(&self, app: &AppHandle, error: impl std::fmt::Display) -> Snapshot {
        self.publish(app, |s| { s.status = "error".into(); s.error = Some(error.to_string()); })
    }
}

#[tauri::command]
pub fn get_app_update(state: State<'_, AppUpdates>) -> Snapshot { state.snapshot() }

#[tauri::command]
pub async fn check_app_update(app: AppHandle, state: State<'_, AppUpdates>) -> Result<Snapshot, String> {
    let _operation = state.claim()?;
    if unsupported().is_some() { return Ok(state.snapshot()); }
    *state.update.lock().unwrap_or_else(|e| e.into_inner()) = None;
    state.publish(&app, |s| {
        s.status = "checking".into(); s.error = None; s.version = None;
        s.notes = None; s.downloaded = 0; s.total = None;
    });
    let cleanup_app = app.clone();
    let updater = match app.updater_builder()
        .timeout(Duration::from_secs(300))
        .on_before_exit(move || {
            cleanup_app.state::<Arc<crate::Runtime>>().stop_all();
            cleanup_app.cleanup_before_exit();
        }).build() {
        Ok(updater) => updater,
        Err(error) => return Ok(state.failed(&app, error)),
    };
    match updater.check().await {
        Ok(Some(update)) => {
            // Artifacts must be served from this project's HTTPS release assets.
            if !trusted_artifact(update.download_url.as_str()) {
                return Ok(state.failed(&app, "The update download URL is outside the trusted release repository"));
            }
            let version = update.version.clone();
            let notes = update.body.clone();
            *state.update.lock().unwrap_or_else(|e| e.into_inner()) = Some(update);
            Ok(state.publish(&app, |s| {
                s.status = "available".into(); s.version = Some(version); s.notes = notes;
            }))
        }
        Ok(None) => Ok(state.publish(&app, |s| s.status = "upToDate".into())),
        Err(error) => Ok(state.failed(&app, error)),
    }
}

fn trusted_artifact(url: &str) -> bool {
    url.starts_with("https://github.com/OP22007/Sony-Sound-Studio/releases/download/v")
        && !url.contains(['?', '#', '\\'])
}

#[tauri::command]
pub async fn install_app_update(app: AppHandle, state: State<'_, AppUpdates>) -> Result<Snapshot, String> {
    let _operation = state.claim()?;
    if unsupported().is_some() { return Ok(state.snapshot()); }
    let update = state.update.lock().unwrap_or_else(|e| e.into_inner()).clone()
        .ok_or_else(|| "Check for an update before installing".to_string())?;
    state.publish(&app, |s| {
        s.status = "downloading".into(); s.error = None; s.downloaded = 0; s.total = None;
    });
    let mut downloaded = 0u64;
    let mut last_event = Instant::now();
    let bytes = match update.download(|chunk, total| {
        downloaded = downloaded.saturating_add(chunk as u64);
        // Keep IPC and React quiet while maintaining a fresh snapshot on window reopen.
        let mut snapshot = state.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        snapshot.downloaded = downloaded; snapshot.total = total;
        if last_event.elapsed() >= Duration::from_millis(150) {
            let _ = app.emit("app-update", snapshot.clone());
            last_event = Instant::now();
        }
    }, || {}).await {
        Ok(bytes) => bytes,
        Err(error) => return Ok(state.failed(&app, error)),
    };
    // download() verifies the signature before this point. Never interrupt headphones
    // on a network/signature failure. install() independently verifies it again.
    state.publish(&app, |s| { s.status = "installing".into(); s.downloaded = bytes.len() as u64; });
    let install = tauri::async_runtime::spawn_blocking(move || update.install(bytes)).await;
    match install {
        Ok(Ok(())) => {
            app.state::<Arc<crate::Runtime>>().stop_all();
            app.restart();
        }
        Ok(Err(error)) => Ok(state.failed(&app, error)),
        Err(error) => Ok(state.failed(&app, error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operations_are_exclusive_and_release_after_failure() {
        let updates = AppUpdates::new();
        let guard = updates.claim().unwrap();
        assert!(updates.claim().is_err());
        drop(guard);
        assert!(updates.claim().is_ok());
    }
    #[test]
    fn only_this_repositories_https_artifacts_are_accepted() {
        assert!(trusted_artifact("https://github.com/OP22007/Sony-Sound-Studio/releases/download/v0.1.2/windows.exe"));
        for url in ["http://github.com/OP22007/Sony-Sound-Studio/releases/download/v0.1.2/a.exe", "https://github.com.evil.test/OP22007/Sony-Sound-Studio/releases/download/v1/a.exe", "https://github.com/other/repo/releases/download/v1/a.exe", "https://github.com/OP22007/Sony-Sound-Studio/releases/download/v1/a.exe?redirect=evil"] {
            assert!(!trusted_artifact(url));
        }
    }
}
