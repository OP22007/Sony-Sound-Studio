# Desktop application settings

Desktop settings are stored by Rust in the application's configuration directory. The browser preview keeps its own local preferences and never changes OS startup registration or Bluetooth connections.

| Preference | Default | Behavior |
| --- | --- | --- |
| Appearance | Dark | Dark, light, or follow the system appearance. |
| Reduce motion | Off | Combines with the operating system's reduced-motion preference. |
| Close to tray | On | Closing destroys the webview and leaves the native tray/connections active. When off, closing disconnects and exits. |
| Launch at login | Off | Uses the native Tauri autostart plugin. The UI reads the OS registration rather than assuming a saved boolean is effective. |
| Automatic reconnect | On | Restores the last connected device on startup and retries an unexpected loss with bounded backoff. Explicit disconnect cancels pending retries. |
| Start minimized | Off | Starts with the tray and native workers, without retaining the studio webview. The tray opens the studio. |

`get_app_preferences` returns the current typed preferences. `update_app_preferences({ preferences })` accepts a complete preferences object. Updates are serialized, atomically replace the preference file, and return errors if persistence or OS registration fails. A persistence failure attempts to restore the previous autostart registration. An update is not reported as saved merely because the user clicked a switch.

The last connected transport identity is stored privately for reconnect and excluded from diagnostics. Runtime UI device IDs are not used as persistent identities. At startup, discovery runs immediately and retries after 2, 5, 15 and 30 seconds if Bluetooth is not ready or the remembered entry is absent. Connection retries wait 2, 5, 15, 30 and 60 seconds, stop after those bounded attempts, and can be cancelled by explicit disconnect, shutdown, or disabling the preference. Explicit disconnect suppresses retries for that device until a manual connect or the next app launch; the remembered target remains saved.

The native preferences tests cover defaults, typed input, reopening persisted state, corrupt-file recovery, launch policy, OS-registration failure, and file-save failure. Browser fixtures cover UI behavior separately. Real startup, tray and OS-login behavior must be checked on each target operating system; a passing fixture does not prove those OS interactions.
