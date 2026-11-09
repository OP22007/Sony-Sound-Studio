//! Pure edge tracking for the background Bluetooth observer. No device I/O.
use crate::transport::{ActiveAudioDevice, DeviceActivitySnapshot};
use std::collections::BTreeSet;

#[derive(Clone, Default)]
pub struct ActivityEdges {
    connected: BTreeSet<String>,
    active: Option<ActiveAudioDevice>,
    initialized: bool,
    automatic_enabled: bool,
}

pub struct ActivityChange {
    pub changed: bool,
    pub newly_connected: BTreeSet<String>,
    pub connect_candidates: Vec<ActiveAudioDevice>,
}

impl ActivityEdges {
    pub fn observe(
        &mut self,
        snapshot: &DeviceActivitySnapshot,
        automatic_enabled: bool,
    ) -> ActivityChange {
        let connected: BTreeSet<_> = snapshot
            .bluetooth_connected
            .iter()
            .map(|device| device.identity.clone())
            .collect();
        let newly_connected = connected.difference(&self.connected).cloned().collect();
        let active = snapshot.active_output.clone();
        let output_changed = active != self.active;
        let enabled_now = automatic_enabled && !self.automatic_enabled;
        let changed =
            !self.initialized || connected != self.connected || output_changed || enabled_now;
        let mut connect_candidates = Vec::new();
        if automatic_enabled {
            if !self.initialized || output_changed || enabled_now {
                if let Some(output) = &snapshot.active_output {
                    connect_candidates.push(output.clone());
                }
            }
            for device in &snapshot.bluetooth_connected {
                if (!self.initialized || enabled_now || !self.connected.contains(&device.identity))
                    && !connect_candidates
                        .iter()
                        .any(|candidate| candidate.identity == device.identity)
                {
                    connect_candidates.push(device.clone());
                }
            }
        }
        self.connected = connected;
        self.active = active;
        self.initialized = true;
        self.automatic_enabled = automatic_enabled;
        ActivityChange {
            changed,
            newly_connected,
            connect_candidates,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::AudioTransport;
    fn device(identity: &str) -> ActiveAudioDevice {
        ActiveAudioDevice {
            identity: identity.into(),
            name: "WH-1000XM6".into(),
            transport: AudioTransport::Bluetooth,
        }
    }
    fn snapshot(active: Option<&str>, connected: &[&str]) -> DeviceActivitySnapshot {
        DeviceActivitySnapshot {
            active_output: active.map(device),
            bluetooth_connected: connected.iter().map(|id| device(id)).collect(),
        }
    }
    #[test]
    fn monitor_connects_new_devices_once_and_prioritizes_the_active_output() {
        let mut edges = ActivityEdges::default();
        let initial = edges.observe(&snapshot(Some("b"), &["a", "b"]), true);
        assert_eq!(
            initial
                .connect_candidates
                .iter()
                .map(|d| d.identity.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert!(
            !edges
                .observe(&snapshot(Some("b"), &["a", "b"]), true)
                .changed
        );
        assert!(edges
            .observe(&snapshot(Some("b"), &["a", "b"]), true)
            .connect_candidates
            .is_empty());
        let changed = edges.observe(&snapshot(Some("a"), &["a", "b", "c"]), true);
        assert_eq!(
            changed
                .connect_candidates
                .iter()
                .map(|d| d.identity.as_str())
                .collect::<Vec<_>>(),
            ["a", "c"]
        );
        assert_eq!(changed.newly_connected, BTreeSet::from(["c".into()]));
    }
    #[test]
    fn monitor_respects_opt_out_and_detects_a_real_reconnection() {
        let mut edges = ActivityEdges::default();
        assert!(edges
            .observe(&snapshot(Some("a"), &["a"]), false)
            .connect_candidates
            .is_empty());
        assert_eq!(
            edges
                .observe(&snapshot(Some("a"), &["a"]), true)
                .connect_candidates
                .len(),
            1
        );
        edges.observe(&snapshot(None, &[]), true);
        assert!(edges
            .observe(&snapshot(Some("a"), &["a"]), true)
            .newly_connected
            .contains("a"));
    }
}
