//! Native-only observation of OS Bluetooth and audio-output changes.
use crate::{
    automatic_reconnect_enabled, device_monitor, discover_devices_internal, emit_state,
    media_automation, schedule_reconnect, transport, Runtime,
};
use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub fn start(app: AppHandle) {
    let runtime = Arc::clone(app.state::<Arc<Runtime>>().inner());
    let _ = std::thread::Builder::new().name("headphone-activity".into()).spawn(move || {
        let mut edges = device_monitor::ActivityEdges::default();
        let mut reported_error = false;
        while !runtime.stopping.load(Ordering::Acquire) {
            match transport::activity_snapshot() {
                Ok(activity) => {
                    reported_error = false;
                    let previous_edges = edges.clone();
                    let change = edges.observe(&activity, automatic_reconnect_enabled(&app));
                    if change.changed {
                        // Paired enumeration runs on output/connection edges, not each tick.
                        if tauri::async_runtime::block_on(discover_devices_internal(app.clone(), Arc::clone(&runtime))).is_err() {
                            edges = previous_edges;
                        } else {
                            let candidates = {
                                let Ok(mut inner) = runtime.inner.lock() else { return; };
                                let paired: Vec<_> = inner.catalogue.values().cloned().collect();
                                let matched_output = activity.active_output.as_ref()
                                    .and_then(|output| transport::match_active_device(output, &paired, &activity.bluetooth_connected));
                                let active_id = matched_output.as_ref().and_then(|output| inner.catalogue.iter()
                                    .find(|(_, paired)| paired.address == output.address).map(|(id, _)| id.clone()));
                                let output_observed = activity.active_output.is_some();
                                let output_changed = inner.active_output_device_id != active_id || inner.audio_output_observed != output_observed;
                                inner.active_output_device_id = active_id;
                                inner.audio_output_observed = output_observed;
                                if output_changed { inner.log("System audio-output selection changed; the studio follows the matched connected headphone."); }
                                let mut candidates = Vec::new();
                                for candidate in &change.connect_candidates {
                                    if let Some(matched) = transport::match_active_device(candidate, &paired, &activity.bluetooth_connected) {
                                        if let Some(id) = inner.catalogue.iter().find(|(_, paired)| paired.address == matched.address).map(|(id, _)| id.clone()) {
                                            // Explicit Disconnect remains respected until a real
                                            // Bluetooth disconnect/reconnect edge occurs.
                                            if change.newly_connected.contains(&candidate.identity) { inner.reconnect_suppressed.remove(&id); }
                                            candidates.push(id);
                                        }
                                    }
                                }
                                candidates
                            };
                            emit_state(&app, &runtime);
                            app.state::<Arc<media_automation::Automation>>().source_configuration_changed();
                            for id in candidates { schedule_reconnect(app.clone(), id); }
                        }
                    }
                }
                Err(_) if !reported_error => {
                    reported_error = true;
                    if let Ok(mut inner) = runtime.inner.lock() { inner.log("System Bluetooth activity is temporarily unavailable. Manual discovery and connection remain available."); }
                    emit_state(&app, &runtime);
                }
                Err(_) => {}
            }
            // Remains native-only in the tray; no audio capture or webview required.
            for _ in 0..16 {
                if runtime.stopping.load(Ordering::Acquire) { return; }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    });
}
