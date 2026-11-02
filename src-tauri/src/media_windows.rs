//! Windows Global System Media Transport Controls observer.
//!
//! Only the current session's app identity and playback state are inspected.

use std::sync::mpsc::SyncSender;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Media::MediaPlaybackType;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

pub struct Observation {
    pub source: String,
    /// The AUMID is useful for transient matching, but is never a title or display name.
    pub key: String,
    pub kind: String,
    pub browser: bool,
    pub playing: bool,
}

pub struct Observer {
    manager: GlobalSystemMediaTransportControlsSessionManager,
    wake: SyncSender<()>,
    sessions_token: i64,
    current_token: i64,
    session: Option<GlobalSystemMediaTransportControlsSession>,
    media_token: Option<i64>,
    playback_token: Option<i64>,
    // Declared last so COM stays initialized until all WinRT objects above are dropped.
    _apartment: ComApartment,
}

struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

impl Observer {
    /// Construct on the dedicated worker thread. That thread owns this observer and its COM apartment.
    pub fn new(wake: SyncSender<()>) -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|_| "Could not initialize the Windows media observer.".to_string())?;
        let apartment = ComApartment;

        let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .and_then(|op| op.join())
        {
            Ok(manager) => manager,
            Err(_) => {
                return Err("Could not connect to Windows media sessions.".into());
            }
        };

        let signal = wake.clone();
        let sessions_token = match manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
                let _ = signal.try_send(());
                Ok(())
            })) {
            Ok(token) => token,
            Err(_) => {
                return Err("Could not observe Windows media sessions.".into());
            }
        };

        let signal = wake.clone();
        let current_token = match manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
            let _ = signal.try_send(());
            Ok(())
        })) {
            Ok(token) => token,
            Err(_) => {
                let _ = manager.RemoveSessionsChanged(sessions_token);
                return Err("Could not observe the current Windows media session.".into());
            }
        };

        Ok(Self {
            manager,
            wake,
            sessions_token,
            current_token,
            session: None,
            media_token: None,
            playback_token: None,
            _apartment: apartment,
        })
    }

    /// Return a privacy-safe snapshot of the current session, if any.
    pub fn current(&mut self) -> Result<Option<Observation>, String> {
        let current = self.manager.GetCurrentSession().ok();
        self.set_session(current.as_ref());
        let Some(session) = current else {
            return Ok(None);
        };

        let key = session
            .SourceAppUserModelId()
            .map_err(|_| "Could not read the current media app identity.".to_string())?
            .to_string();
        let lower = key.to_ascii_lowercase();
        let browser = ["chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "browser"]
            .iter()
            .any(|needle| lower.contains(needle));
        let (source, kind) = if browser {
            ("Browser", "other")
        } else if lower.contains("netflix") {
            ("Netflix", "movie")
        } else if lower.contains("videolan") || lower.contains("vlc") {
            let playback = session
                .GetPlaybackInfo()
                .ok()
                .and_then(|info| info.PlaybackType().ok())
                .and_then(|value| value.Value().ok());
            (
                "VLC",
                if playback == Some(MediaPlaybackType::Video) {
                    "movie"
                } else {
                    "other"
                },
            )
        } else if lower.contains("applemusic") || lower.contains("apple.music") {
            ("Apple Music", "music")
        } else if lower.contains("tidal") {
            ("Tidal", "music")
        } else {
            ("Other player", "other")
        };

        let playing = session
            .GetPlaybackInfo()
            .ok()
            .and_then(|info| info.PlaybackStatus().ok())
            == Some(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing);

        Ok(Some(Observation {
            source: source.to_string(),
            key,
            kind: kind.to_string(),
            browser,
            playing,
        }))
    }

    fn set_session(&mut self, current: Option<&GlobalSystemMediaTransportControlsSession>) {
        let changed = match (&self.session, current) {
            (Some(old), Some(new)) => old != new,
            (None, None) => false,
            _ => true,
        };
        if !changed {
            return;
        }
        self.detach_session();
        let Some(session) = current.cloned() else { return };
        let signal = self.wake.clone();
        let media_token = session
            .MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                let _ = signal.try_send(());
                Ok(())
            }))
            .ok();
        let signal = self.wake.clone();
        let playback_token = session
            .PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                let _ = signal.try_send(());
                Ok(())
            }))
            .ok();
        self.session = Some(session);
        self.media_token = media_token;
        self.playback_token = playback_token;
    }

    fn detach_session(&mut self) {
        if let Some(session) = self.session.take() {
            if let Some(token) = self.media_token.take() {
                let _ = session.RemoveMediaPropertiesChanged(token);
            }
            if let Some(token) = self.playback_token.take() {
                let _ = session.RemovePlaybackInfoChanged(token);
            }
        }
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.detach_session();
        let _ = self.manager.RemoveCurrentSessionChanged(self.current_token);
        let _ = self.manager.RemoveSessionsChanged(self.sessions_token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Read-only Windows media-session smoke test; never opens a headphone connection"]
    fn native_media_observer_smoke() {
        let (wake, _) = std::sync::mpsc::sync_channel(1);
        let mut observer = Observer::new(wake).expect("Windows media observer initialization");
        let sessions = observer.manager.GetSessions().expect("Enumerate media sessions");
        let mut vlc_sessions = 0;
        let mut playing_sessions = 0;
        for session in sessions {
            let id = session.SourceAppUserModelId().unwrap_or_default().to_string().to_ascii_lowercase();
            if id.contains("vlc") || id.contains("videolan") { vlc_sessions += 1; }
            if session.GetPlaybackInfo().and_then(|info| info.PlaybackStatus()).ok() == Some(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing) { playing_sessions += 1; }
        }
        println!("Windows media sessions: VLC={vlc_sessions}, playing={playing_sessions}");
        match observer.current().expect("Read current Windows media session") {
            Some(observation) => {
                assert!(["movie", "music", "other"].contains(&observation.kind.as_str()));
                println!("Media observer: source={}, kind={}, browser={}, playing={}", observation.source, observation.kind, observation.browser, observation.playing);
            }
            None => println!("Media observer initialized; no current Windows media session."),
        }
    }
}
