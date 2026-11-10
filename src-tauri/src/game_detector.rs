//! Read-only detection of a running game for automatic listening. Sends nothing to any device.
//!
//! A game is the foreground app when Windows reports a full-screen Direct3D application, or
//! when its executable is in the known-game list. Browsers, media players and presentation
//! apps are excluded from the full-screen rule. Launchers (Steam, Epic, Battle.net) are not
//! games by themselves.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Foreground {
    /// Lower-case executable file name, for example `cs2.exe`.
    pub exe: String,
    /// Windows reports a full-screen Direct3D application (`QUNS_RUNNING_D3D_FULL_SCREEN`).
    pub fullscreen_d3d: bool,
}

pub trait GameProbe {
    fn foreground(&self) -> Option<Foreground>;
}

const NEVER_GAMES: &[&str] = &[
    "chrome.exe", "msedge.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe",
    "vlc.exe", "mpc-hc64.exe", "mpc-hc.exe", "mpc-be64.exe", "potplayermini64.exe", "wmplayer.exe",
    "spotify.exe", "applemusic.exe", "tidal.exe", "netflix.exe", "applicationframehost.exe",
    "powerpnt.exe", "teams.exe", "ms-teams.exe", "zoom.exe", "explorer.exe",
    "steam.exe", "steamwebhelper.exe", "epicgameslauncher.exe", "battle.net.exe",
    "galaxyclient.exe", "ubisoftconnect.exe", "upc.exe", "origin.exe", "eadesktop.exe",
];

const KNOWN_GAMES: &[&str] = &[
    "cs2.exe", "csgo.exe", "dota2.exe", "valorant-win64-shipping.exe", "fortniteclient-win64-shipping.exe",
    "leagueclient.exe", "league of legends.exe", "overwatch.exe", "apexlegends.exe", "r5apex.exe",
    "rocketleague.exe", "gta5.exe", "gtav.exe", "rdr2.exe", "eldenring.exe", "cyberpunk2077.exe",
    "witcher3.exe", "minecraft.exe", "minecraftlauncher.exe", "javaw.exe", "robloxplayerbeta.exe",
    "genshinimpact.exe", "yuanshen.exe", "starrail.exe", "zenlesszonezero.exe", "pubg.exe",
    "tslgame.exe", "destiny2.exe", "cod.exe", "modernwarfare.exe", "bf2042.exe", "fifa23.exe",
    "eafc24.exe", "eafc25.exe", "forzahorizon5.exe", "hogwartslegacy.exe", "baldursgate3.exe",
    "bg3.exe", "bg3_dx11.exe", "helldivers2.exe", "palworld-win64-shipping.exe", "fallguys_client.exe",
    "bgmi.exe", "brawlhalla.exe", "wow.exe", "wowclassic.exe", "hearthstone.exe", "diablo iv.exe",
    "stardewvalley.exe", "terraria.exe", "factorio.exe", "hades.exe", "hollowknight.exe",
];

pub fn is_game(foreground: &Foreground) -> bool {
    let exe = foreground.exe.to_ascii_lowercase();
    // Unreal Engine shipping builds are named `<Game>-Win64-Shipping.exe`.
    let known = KNOWN_GAMES.contains(&exe.as_str()) || exe.ends_with("-win64-shipping.exe");
    if known && exe != "javaw.exe" && exe != "minecraftlauncher.exe" {
        return true;
    }
    if NEVER_GAMES.contains(&exe.as_str()) {
        return false;
    }
    // `javaw.exe` hosts Minecraft but also any Java app, so it needs the full-screen signal.
    foreground.fullscreen_d3d
}

/// Name of the detected game's executable, if any.
pub fn detect(probe: &dyn GameProbe) -> Option<String> {
    probe.foreground().filter(is_game).map(|f| f.exe)
}

#[cfg(windows)]
pub struct SystemProbe;

#[cfg(windows)]
impl GameProbe for SystemProbe {
    fn foreground(&self) -> Option<Foreground> {
        use windows::core::PWSTR;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
        unsafe {
            let window = GetForegroundWindow();
            if window.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = [0u16; 520];
            let mut length = buffer.len() as u32;
            let queried = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            );
            let _ = CloseHandle(handle);
            queried.ok()?;
            let path = String::from_utf16_lossy(&buffer[..length as usize]);
            let exe = path.rsplit(['\\', '/']).next()?.to_ascii_lowercase();
            let fullscreen_d3d = SHQueryUserNotificationState()
                .map(|state| state == QUNS_RUNNING_D3D_FULL_SCREEN)
                .unwrap_or(false);
            Some(Foreground { exe, fullscreen_d3d })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fg(exe: &str, fullscreen_d3d: bool) -> Foreground {
        Foreground { exe: exe.into(), fullscreen_d3d }
    }
    struct Fake(Option<Foreground>);
    impl GameProbe for Fake {
        fn foreground(&self) -> Option<Foreground> {
            self.0.clone()
        }
    }

    #[test]
    fn known_games_and_unreal_shipping_builds_are_games_without_fullscreen() {
        assert!(is_game(&fg("cs2.exe", false)));
        assert!(is_game(&fg("SomeNewGame-Win64-Shipping.exe", false)));
    }

    #[test]
    fn fullscreen_d3d_marks_an_unknown_app_as_a_game() {
        assert!(is_game(&fg("indiegame.exe", true)));
        assert!(!is_game(&fg("indiegame.exe", false)));
    }

    #[test]
    fn browsers_players_and_launchers_are_never_games_even_fullscreen() {
        for exe in ["chrome.exe", "vlc.exe", "POWERPNT.EXE", "steam.exe", "spotify.exe"] {
            assert!(!is_game(&fg(exe, true)), "{exe}");
        }
    }

    #[test]
    fn javaw_needs_the_fullscreen_signal() {
        assert!(!is_game(&fg("javaw.exe", false)));
        assert!(is_game(&fg("javaw.exe", true)));
    }

    #[test]
    fn detect_returns_the_executable_or_nothing() {
        assert_eq!(detect(&Fake(Some(fg("cs2.exe", false)))), Some("cs2.exe".into()));
        assert_eq!(detect(&Fake(Some(fg("notepad.exe", false)))), None);
        assert_eq!(detect(&Fake(None)), None);
    }
}
