//! Decides when a headphone's battery is low enough to tell the user, and only once per drop.
use std::collections::HashSet;

/// Alert when a connected, discharging headphone falls below this percentage.
pub const LOW_PERCENT: u8 = 20;
/// The alert is armed again once the battery recovers to this level (or starts charging).
pub const REARM_PERCENT: u8 = 25;

#[derive(Default)]
pub struct BatteryAlerts {
    alerted: HashSet<String>,
}

impl BatteryAlerts {
    /// Returns true when a low-battery notification should be shown for this reading.
    pub fn observe(&mut self, id: &str, connected: bool, level: Option<u8>, charging: bool) -> bool {
        let Some(level) = level.filter(|_| connected) else {
            self.alerted.remove(id);
            return false;
        };
        if charging || level >= REARM_PERCENT {
            self.alerted.remove(id);
            return false;
        }
        level < LOW_PERCENT && self.alerted.insert(id.to_string())
    }

    pub fn forget_all(&mut self) {
        self.alerted.clear();
    }
}

pub fn message(name: &str, level: u8) -> (String, String) {
    let name: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
    (
        "Headphone battery low".into(),
        format!("{name} is at {level}%. Charge it soon."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alerts_once_when_the_battery_drops_below_twenty() {
        let mut alerts = BatteryAlerts::default();
        assert!(!alerts.observe("a", true, Some(35), false));
        assert!(!alerts.observe("a", true, Some(20), false));
        assert!(alerts.observe("a", true, Some(19), false));
        assert!(!alerts.observe("a", true, Some(18), false));
        assert!(!alerts.observe("a", true, Some(5), false));
    }

    #[test]
    fn rearms_after_charging_or_recovering_and_not_on_a_small_wobble() {
        let mut alerts = BatteryAlerts::default();
        assert!(alerts.observe("a", true, Some(15), false));
        assert!(!alerts.observe("a", true, Some(22), false), "still inside the hysteresis band");
        assert!(!alerts.observe("a", true, Some(19), false), "no second alert for the same drop");
        assert!(!alerts.observe("a", true, Some(40), false));
        assert!(alerts.observe("a", true, Some(19), false));
        assert!(!alerts.observe("a", true, Some(10), true), "charging never alerts");
        assert!(alerts.observe("a", true, Some(10), false), "alerts again after charging stops");
    }

    #[test]
    fn disconnected_or_unknown_levels_never_alert_and_reset() {
        let mut alerts = BatteryAlerts::default();
        assert!(!alerts.observe("a", false, Some(5), false));
        assert!(!alerts.observe("a", true, None, false));
        assert!(alerts.observe("a", true, Some(5), false));
        assert!(!alerts.observe("a", false, Some(5), false));
        assert!(alerts.observe("a", true, Some(5), false), "reconnecting a still-low headphone reminds once");
    }

    #[test]
    fn devices_are_tracked_independently_and_names_are_cleaned() {
        let mut alerts = BatteryAlerts::default();
        assert!(alerts.observe("a", true, Some(10), false));
        assert!(alerts.observe("b", true, Some(10), false));
        let (title, body) = message("WH-1000XM6\u{7}", 12);
        assert_eq!(title, "Headphone battery low");
        assert_eq!(body, "WH-1000XM6 is at 12%. Charge it soon.");
    }
}
