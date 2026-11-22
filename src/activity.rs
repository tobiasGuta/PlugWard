//! Session-only activity timeline derived from successive USBGuard snapshots.
//!
//! This is *observed* history: it does not claim to be a complete audit log.
//! Events between polls and events before launch may be missed.

use std::collections::{HashMap, VecDeque};

use crate::usbguard::Device;

const MAX_EVENTS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub when: String,
    pub title: String,
    pub description: String,
}

#[derive(Default)]
pub struct ActivityHistory {
    last: Option<HashMap<u32, Device>>,
    entries: VecDeque<Event>,
}

impl ActivityHistory {
    pub fn entries(&self) -> Vec<Event> {
        self.entries.iter().rev().cloned().collect()
    }

    pub fn observe(&mut self, devices: &[Device], now: &str) {
        let current: HashMap<u32, Device> = devices
            .iter()
            .cloned()
            .map(|device| (device.number, device))
            .collect();

        if let Some(last) = self.last.take() {
            for (id, old) in &last {
                match current.get(id) {
                    None => self.record(now, "Device removed", old),
                    Some(new) if old.usb_id != new.usb_id
                        || old.hash != new.hash
                        || old.port != new.port =>
                    {
                        self.record(now, "Device identity/port changed", old);
                        self.record(now, "Device observed", new);
                    }
                    Some(new) if old.target != new.target => {
                        self.record(now, &format!("Authorization changed: {}", new.status()), new);
                    }
                    _ => {}
                }
            }
            for (id, new) in &current {
                if !last.contains_key(id) {
                    let title = if new.is_blocked() {
                        "Device observed blocked"
                    } else if new.target == "allow" {
                        "Device observed allowed"
                    } else {
                        "Device observed"
                    };
                    self.record(now, title, new);
                }
            }
        }
        // Never mislabel the initial inventory as new connection events.
        self.last = Some(current);
    }

    pub fn reset_observation(&mut self) {
        // Called after a failed D-Bus refresh: don't fabricate disconnects.
        self.last = None;
    }

    fn record(&mut self, now: &str, title: &str, device: &Device) {
        if self.entries.len() == MAX_EVENTS {
            self.entries.pop_front();
        }
        self.entries.push_back(Event {
            when: now.to_string(),
            title: title.to_string(),
            description: format!(
                "{} · ID {} · Port {} · runtime #{}",
                device.display_name(),
                Device::display_attribute(&device.usb_id),
                Device::display_attribute(&device.port),
                device.number
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usbguard::parse_device_rule;

    fn sample(target: &str) -> Device {
        parse_device_rule(
            21,
            &format!(
                "{target} id 13fe:4200 name \"USB DISK 2.0\" hash \"abc\" via-port \"3-4\" with-interface 08:06:50 with-connect-type \"hotplug\""
            ),
        )
    }

    #[test]
    fn first_scan_is_baseline_not_a_fabricated_insert_event() {
        let mut history = ActivityHistory::default();
        history.observe(&[sample("block")], "10:00");
        assert!(history.entries().is_empty());
    }

    #[test]
    fn detects_block_allow_disconnect_reconnect() {
        let mut history = ActivityHistory::default();
        history.observe(&[], "10:00");
        history.observe(&[sample("block")], "10:01");
        history.observe(&[sample("allow")], "10:02");
        history.observe(&[], "10:03");
        history.observe(&[sample("block")], "10:04");
        let titles: Vec<String> = history.entries().iter().map(|e| e.title.clone()).collect();
        assert_eq!(
            titles,
            vec!["Device observed blocked", "Device removed", "Authorization changed: Allowed", "Device observed blocked"]
        );
    }

    #[test]
    fn read_failure_does_not_fabricate_removal() {
        let mut history = ActivityHistory::default();
        history.observe(&[sample("allow")], "10:00");
        history.reset_observation();
        history.observe(&[], "10:01");
        assert!(history.entries().is_empty());
    }

    #[test]
    fn old_events_are_bounded() {
        let mut history = ActivityHistory::default();
        history.observe(&[], "10:00");
        for i in 0..250 {
            history.observe(&[sample("block")], &i.to_string());
            history.observe(&[], &i.to_string());
        }
        assert_eq!(history.entries().len(), 200);
    }
}
