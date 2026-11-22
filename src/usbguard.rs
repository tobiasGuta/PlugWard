//! Read-only client for the USBGuard system D-Bus bridge.
//!
//! Intentionally does not expose authorization or policy-editing methods.

use gtk::gio;
use gtk::gio::prelude::*;
use gtk::glib;
use gtk::glib::prelude::*;
use gtk::glib::variant::ToVariant;

const SERVICE: &str = "org.usbguard1";
const PATH: &str = "/org/usbguard1/Devices";
const INTERFACE: &str = "org.usbguard.Devices1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// USBGuard's transient runtime device number, NOT the USB VID:PID.
    pub number: u32,
    pub target: String,
    pub usb_id: String,
    pub name: String,
    pub port: String,
}

impl Device {
    pub fn status(&self) -> &'static str {
        match self.target.as_str() {
            "allow" => "Allowed",
            "block" => "Blocked",
            "reject" => "Rejected",
            _ => "Unknown",
        }
    }

    pub fn priority(&self) -> u8 {
        match self.target.as_str() {
            "block" | "reject" => 0,
            "allow" => 1,
            _ => 2,
        }
    }
}

/// Connects to the *system* bus and asks USBGuard to list devices.
///
/// This function performs blocking I/O; call it from a worker thread only.
pub fn list_devices() -> Result<Vec<Device>, String> {
    let bus = gio::bus_get_sync(gio::BusType::System, None::<&gio::Cancellable>)
        .map_err(|error| format!("Cannot connect to the system D-Bus: {error}"))?;

    let response = bus
        .call_sync(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "listDevices",
            Some(&("match",).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            10_000,
            None::<&gio::Cancellable>,
        )
        .map_err(|error| {
            format!(
                "Cannot list USBGuard devices: {error}. \
                 Check that usbguard-dbus is installed and that your user is permitted to read devices."
            )
        })?;

    let (raw,): (Vec<(u32, String)>,) = response
        .get()
        .ok_or_else(|| "Unexpected listDevices response from USBGuard D-Bus".to_string())?;

    let mut devices: Vec<Device> = raw
        .into_iter()
        .map(|(number, rule)| parse_device_rule(number, &rule))
        .collect();
    devices.sort_by(|a, b| {
        a.priority()
            .cmp(&b.priority())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.number.cmp(&b.number))
    });
    Ok(devices)
}

/// Parse the device-specific rule returned by USBGuard. Never execute or
/// interpolate any part of a device's self-reported name.
pub fn parse_device_rule(number: u32, rule: &str) -> Device {
    let tokens = tokenize_rule(rule);
    let value = |key: &str| -> Option<&str> {
        tokens
            .windows(2)
            .find(|pair| pair[0] == key)
            .map(|pair| pair[1].as_str())
    };

    Device {
        number,
        target: tokens.first().cloned().unwrap_or_default(),
        usb_id: value("id").unwrap_or("Unknown").to_string(),
        name: value("name")
            .filter(|name| !name.is_empty())
            .unwrap_or("Unnamed USB device")
            .to_string(),
        port: value("via-port").unwrap_or("Not reported").to_string(),
    }
}

/// Small quote-aware tokenizer for attributes such as name "Wireless Receiver".
/// It handles escaped characters inside quotes and does not treat device
/// metadata as commands, markup, or paths.
fn tokenize_rule(rule: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut started = false;

    for ch in rule.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            started = true;
            continue;
        }
        match ch {
            '\\' if quoted => {
                escaped = true;
                started = true;
            }
            '"' => {
                quoted = !quoted;
                started = true;
            }
            ch if ch.is_whitespace() && !quoted => {
                if started {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            _ => {
                current.push(ch);
                started = true;
            }
        }
    }
    if started {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_logitech_receiver_and_port() {
        let device = parse_device_rule(
            16,
            r#"allow id 046d:c542 serial "" name "Wireless Receiver" hash "abc" via-port "3-1" with-interface 03:01:02"#,
        );
        assert_eq!(device.number, 16);
        assert_eq!(device.usb_id, "046d:c542");
        assert_eq!(device.name, "Wireless Receiver");
        assert_eq!(device.port, "3-1");
        assert_eq!(device.status(), "Allowed");
    }

    #[test]
    fn parses_blocked_device_and_preserves_untrusted_name_as_data() {
        let device = parse_device_rule(
            17,
            r#"block id 1111:2222 name "<untrusted> USB" via-port "3-3""#,
        );
        assert_eq!(device.status(), "Blocked");
        assert_eq!(device.priority(), 0);
        assert_eq!(device.name, "<untrusted> USB");
    }

    #[test]
    fn parses_quoted_escapes() {
        let device = parse_device_rule(18, r#"allow id 0000:0000 name "A \"quoted\" name""#);
        assert_eq!(device.name, "A \"quoted\" name");
    }

    #[test]
    fn gracefully_handles_missing_attributes() {
        let device = parse_device_rule(19, "reject");
        assert_eq!(device.name, "Unnamed USB device");
        assert_eq!(device.usb_id, "Unknown");
        assert_eq!(device.port, "Not reported");
        assert_eq!(device.status(), "Rejected");
    }

    #[test]
    fn empty_quoted_string_does_not_shift_following_fields() {
        let device = parse_device_rule(20, r#"allow id 1234:5678 serial "" name "Test Device""#);
        assert_eq!(device.name, "Test Device");
    }
}
