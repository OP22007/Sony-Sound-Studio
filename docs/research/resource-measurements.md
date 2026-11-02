# Native process resource sample

Measured on 2026-10-31 on the Windows 11 test computer (12 logical processors), using the locally built Tauri executable. This is one smoke sample, not a performance benchmark or guarantee.

| Phase | Process tree | Private memory | Working set | Stable-process CPU |
| --- | --- | ---: | ---: | ---: |
| Studio window open, after initial interaction | Native app plus six WebView2 child processes | 211,394,560 bytes (201.6 MiB) | 467,427,328 bytes (445.7 MiB) | 1.0 CPU-second over 6.33 seconds (15.8% of one core) |
| Window destroyed to tray | Rust app only; WebView2 children exited | 5,808,128 bytes (5.5 MiB) | 32,800,768 bytes (31.3 MiB) | 0 CPU-seconds over 6.28 seconds (0% of one core) |

The first sample includes separate browser helper processes and shared working-set pages. It was taken after interaction and a screenshot, so its CPU value does not describe a sustained idle window. The tray sample demonstrates that destroying the window releases the WebView2 process tree while the Rust process remains available. Closing to tray, rather than quitting, was observed; the tray reopen action was not independently verified in this measurement run.

Reproduce with `scripts/measure-native.ps1 -RootProcessId <app-pid> -Phase <label> -DurationSeconds 6`. It samples processes whose parent chain starts at the app PID. CPU counts only processes present in both samples. This initial result shows idle-tray savings; it also shows that active-window memory needs more profiling before any “minimum memory” claim.
