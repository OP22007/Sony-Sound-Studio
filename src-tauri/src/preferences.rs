//! Durable user preferences and the private identity used for opt-in reconnect.
//!
//! Callers should create the store with Tauri's `app.path().app_config_dir()` and
//! keep one store in managed state. Writes are serialized so concurrent preference
//! updates cannot overwrite one another halfway through a file replacement.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const PREFERENCES_FILE: &str = "preferences.json";
const LAST_DEVICE_FILE: &str = "last-connected-device";

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    #[default]
    Dark,
    System,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct AppPreferences {
    pub theme: Theme,
    pub reduce_motion: bool,
    pub close_to_tray: bool,
    pub launch_at_login: bool,
    pub auto_reconnect: bool,
    pub start_minimized: bool,
    pub low_battery_alerts: bool,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            reduce_motion: false,
            close_to_tray: true,
            launch_at_login: false,
            auto_reconnect: true,
            start_minimized: false,
            low_battery_alerts: true,
        }
    }
}

impl AppPreferences {
    /// Start hidden when the preference is set or the launcher explicitly requests it.
    pub fn should_start_minimized(&self, explicitly_requested: bool) -> bool {
        self.start_minimized || explicitly_requested
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedPreferences {
    pub preferences: AppPreferences,
    /// A user-safe, non-sensitive recovery message. Never includes file contents or paths.
    pub diagnostic: Option<&'static str>,
}

/// One instance is intended to be shared through Tauri managed state.
pub struct PreferencesStore {
    config_dir: PathBuf,
    io_lock: Mutex<()>,
}

/// Keep the OS launch entry and the durable preference file consistent. An OS
/// registration failure must not commit a preference that did not take effect.
pub fn save_with_autostart(
    store: &PreferencesStore,
    preferences: &AppPreferences,
    previous_autostart: bool,
    set_autostart: impl Fn(bool) -> Result<(), String>,
) -> Result<(), String> {
    let changed = preferences.launch_at_login != previous_autostart;
    if changed { set_autostart(preferences.launch_at_login)?; }
    if let Err(error) = store.save(preferences) {
        if changed && set_autostart(previous_autostart).is_err() {
            return Err(format!("{error} Launch at login also could not be restored; inspect its current system setting."));
        }
        return Err(error);
    }
    Ok(())
}

impl PreferencesStore {
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
            io_lock: Mutex::new(()),
        }
    }

    pub fn load(&self) -> Result<LoadedPreferences, String> {
        let _guard = self
            .io_lock
            .lock()
            .map_err(|_| "Preferences are unavailable; restart the application.".to_string())?;
        let path = self.config_dir.join(PREFERENCES_FILE);
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<AppPreferences>(&bytes) {
                Ok(preferences) => Ok(LoadedPreferences { preferences, diagnostic: None }),
                Err(_) => Ok(LoadedPreferences {
                    preferences: AppPreferences::default(),
                    diagnostic: Some("Saved preferences could not be read; defaults are active until valid settings are saved."),
                }),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(LoadedPreferences {
                preferences: AppPreferences::default(),
                diagnostic: None,
            }),
            Err(_) => Err("Could not read saved preferences.".into()),
        }
    }

    /// Saves only after the new file has been fully written and synced. On failure,
    /// the existing preferences file remains intact and the error is returned.
    pub fn save(&self, preferences: &AppPreferences) -> Result<(), String> {
        let _guard = self
            .io_lock
            .lock()
            .map_err(|_| "Preferences are unavailable; restart the application.".to_string())?;
        let bytes = serde_json::to_vec_pretty(preferences)
            .map_err(|_| "Could not encode preferences.".to_string())?;
        self.atomic_write(PREFERENCES_FILE, &bytes)
            .map_err(|_| "Could not save preferences. Check that the application can write to its configuration folder.".to_string())
    }

    /// Stores a stable catalog transport identity for opt-in reconnect. This must
    /// never be the app's ephemeral UI id. The value stays out of diagnostics.
    pub fn save_last_connected_device(&self, identity: &str) -> Result<(), String> {
        if identity.is_empty() || identity.len() > 256 || identity.chars().any(char::is_control) {
            return Err("The device identity is invalid.".into());
        }
        let _guard = self
            .io_lock
            .lock()
            .map_err(|_| "Preferences are unavailable; restart the application.".to_string())?;
        self.atomic_write(LAST_DEVICE_FILE, identity.as_bytes())
            .map_err(|_| "Could not save the reconnect device.".to_string())
    }

    pub fn load_last_connected_device(&self) -> Result<Option<String>, String> {
        let _guard = self
            .io_lock
            .lock()
            .map_err(|_| "Preferences are unavailable; restart the application.".to_string())?;
        match fs::read(self.config_dir.join(LAST_DEVICE_FILE)) {
            Ok(bytes) => {
                let identity = String::from_utf8(bytes)
                    .map_err(|_| "Saved reconnect device data is invalid.".to_string())?;
                if identity.is_empty()
                    || identity.len() > 256
                    || identity.chars().any(char::is_control)
                {
                    return Err("Saved reconnect device data is invalid.".into());
                }
                Ok(Some(identity))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err("Could not read the saved reconnect device.".into()),
        }
    }

    pub fn clear_last_connected_device(&self) -> Result<(), String> {
        let _guard = self
            .io_lock
            .lock()
            .map_err(|_| "Preferences are unavailable; restart the application.".to_string())?;
        match fs::remove_file(self.config_dir.join(LAST_DEVICE_FILE)) {
            Ok(()) => sync_directory(&self.config_dir)
                .map_err(|_| "Could not persist the reconnect preference.".to_string()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Could not clear the saved reconnect device.".into()),
        }
    }

    pub(crate) fn atomic_write(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        let target = self.config_dir.join(name);
        let temp = self
            .config_dir
            .join(format!(".{name}.{}.tmp", unique_suffix()));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            replace_file(&temp, &target)?;
            sync_directory(&self.config_dir)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

fn unique_suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

static TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    // MoveFileExW with REPLACE_EXISTING is the Windows equivalent of atomic rename-over.
    let ok = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "sound-connect-preferences-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn defaults_and_json_contract_are_stable() {
        let p = AppPreferences::default();
        assert_eq!(p.theme, Theme::Dark);
        assert!(!p.reduce_motion);
        assert!(p.close_to_tray);
        assert!(!p.launch_at_login);
        assert!(p.auto_reconnect);
        assert!(!p.start_minimized);
        assert_eq!(
            serde_json::from_str::<AppPreferences>(r#"{"closeToTray":false}"#)
                .unwrap()
                .close_to_tray,
            false
        );
        assert!(serde_json::from_str::<AppPreferences>(r#"{"unknown":true}"#).is_err());
        assert_eq!(serde_json::to_value(p).unwrap()["theme"], "dark");
    }

    #[test]
    fn preferences_survive_reopening_store_and_corrupt_files_return_defaults() {
        let dir = TempDir::new();
        let store = PreferencesStore::new(&dir.0);
        let mut saved = AppPreferences::default();
        saved.theme = Theme::Light;
        saved.reduce_motion = true;
        store.save(&saved).unwrap();
        assert_eq!(
            PreferencesStore::new(&dir.0).load().unwrap().preferences,
            saved
        );
        fs::write(dir.0.join(PREFERENCES_FILE), b"{").unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.preferences, AppPreferences::default());
        assert!(loaded.diagnostic.is_some());
    }

    #[test]
    fn reconnect_identity_persists_and_rejects_ephemeral_or_malformed_values() {
        let dir = TempDir::new();
        let store = PreferencesStore::new(&dir.0);
        assert_eq!(store.load_last_connected_device().unwrap(), None);
        store
            .save_last_connected_device("AA:BB:CC:DD:EE:FF")
            .unwrap();
        assert_eq!(
            PreferencesStore::new(&dir.0)
                .load_last_connected_device()
                .unwrap()
                .as_deref(),
            Some("AA:BB:CC:DD:EE:FF")
        );
        assert!(store.save_last_connected_device("sony-0001\n").is_err());
        store.clear_last_connected_device().unwrap();
        assert_eq!(store.load_last_connected_device().unwrap(), None);
    }

    #[test]
    fn minimized_start_honors_preference_or_explicit_launch_argument() {
        let p = AppPreferences {
            start_minimized: true,
            ..Default::default()
        };
        assert!(p.should_start_minimized(false));
        let default = AppPreferences::default();
        assert!(!default.should_start_minimized(false));
        assert!(default.should_start_minimized(true));
    }

    #[test]
    fn failed_os_registration_does_not_commit_preferences() {
        let dir = TempDir::new();
        let store = PreferencesStore::new(&dir.0);
        let previous = AppPreferences::default();
        store.save(&previous).unwrap();
        let next = AppPreferences { launch_at_login: true, ..previous.clone() };
        assert!(save_with_autostart(&store, &next, false, |_| Err("OS registration failed".into())).is_err());
        assert_eq!(store.load().unwrap().preferences, previous);
    }

    #[test]
    fn failed_file_save_restores_the_previous_os_launch_entry() {
        use std::cell::Cell;
        let dir = TempDir::new();
        let blocked = dir.0.join("file-instead-of-a-directory");
        fs::write(&blocked, b"not a directory").unwrap();
        let store = PreferencesStore::new(blocked);
        let registered = Cell::new(false);
        let next = AppPreferences { launch_at_login: true, ..Default::default() };
        assert!(save_with_autostart(&store, &next, false, |enabled| { registered.set(enabled); Ok(()) }).is_err());
        assert!(!registered.get());
    }
}
