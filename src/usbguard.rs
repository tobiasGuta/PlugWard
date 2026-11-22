//! Read-only client for the USBGuard system D-Bus bridge.
//!
//! Intentionally does not expose authorization or policy-editing methods.

use gtk::gio;
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
    pub connection_type: String,
    pub interfaces: Vec<String>,
    /// Values reported by the device/USBGuard; never proof of authenticity.
    pub serial: String,
    pub hash: String,
    pub parent_hash: String,
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

    pub fn is_blocked(&self) -> bool {
        matches!(self.target.as_str(), "block" | "reject")
    }


    /// A presentation hint based on reported USB descriptors, not a trust check.
    pub fn kind(&self) -> DeviceKind {
        if matches!(self.usb_id.as_str(), "1d6b:0002" | "1d6b:0003") {
            DeviceKind::Controller
        } else if self.usb_id == "8087:0026"
            || self.interfaces.iter().any(|code| code == "e0:01:01") {
            DeviceKind::Bluetooth
        } else if self.interfaces.iter().any(|code| code.starts_with("0e:")) {
            DeviceKind::Camera
        } else if self.interfaces.iter().any(|code| code.starts_with("03:")) {
            DeviceKind::Input
        } else if self.interfaces.iter().any(|code| code.starts_with("08:")) {
            DeviceKind::Storage
        } else if self.interfaces.iter().any(|code| code.starts_with("02:") || code.starts_with("0a:")) {
            DeviceKind::Network
        } else {
            DeviceKind::Other
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self.kind() {
            DeviceKind::Controller => "computer-symbolic",
            DeviceKind::Bluetooth => "bluetooth-symbolic",
            DeviceKind::Camera => "camera-web-symbolic",
            DeviceKind::Input => "input-mouse-symbolic",
            DeviceKind::Storage => "drive-removable-media-symbolic",
            DeviceKind::Network => "network-wired-symbolic",
            DeviceKind::Other => "drive-removable-media-symbolic",
        }
    }

    pub fn display_name(&self) -> String {
        if self.name.is_empty() || self.name == "Unnamed USB device" {
            return match self.kind() {
                DeviceKind::Bluetooth => "Bluetooth adapter (reported)".to_string(),
                DeviceKind::Camera => "USB camera (reported)".to_string(),
                _ => "Unnamed USB device".to_string(),
            };
        }
        let cleaned: String = self.name.chars()
            .filter(|c| !c.is_control() && !is_bidi_control(*c))
            .take(80).collect();
        if cleaned.trim().is_empty() { "Unnamed USB device".to_string() } else { cleaned }
    }

    pub fn connection_label(&self) -> &'static str {
        match self.connection_type.as_str() {
            "hardwired" => "Internal (reported)",
            "hotplug" => "Hotplug",
            _ => "Connection unknown",
        }
    }


    /// Compare a currently listed device to the snapshot displayed to the user.
    /// Matching metadata is only a consistency check, NOT authentication of USB hardware.
    /// It cannot prevent a device with spoofed descriptors or a reconnect between
    /// this check and a later privileged request.
    pub fn matches_snapshot(&self, current: &Self) -> bool {
        self.number == current.number
            && self.target == current.target
            && self.usb_id == current.usb_id
            && self.name == current.name
            && self.port == current.port
            && self.connection_type == current.connection_type
            && self.interfaces == current.interfaces
            && self.serial == current.serial
            && self.hash == current.hash
            && self.parent_hash == current.parent_hash
    }

    /// Sanitize a value for the GUI. Never use in authorization comparisons.
    pub fn display_attribute(value: &str) -> String {
        let clean: String = value
            .chars()
            .filter(|c| !c.is_control() && !is_bidi_control(*c))
            .take(192)
            .collect();
        if clean.is_empty() { "Not reported".to_string() } else { clean }
    }

    /// D-Bus errors can include untrusted device metadata; preserve enough
    /// details to diagnose failures, without rendering control or bidi codes.
    pub fn display_diagnostic(value: &str) -> String {
        let clean: String = value
            .chars()
            .filter(|c| !c.is_control() && !is_bidi_control(*c))
            .take(900)
            .collect();
        if clean.is_empty() { "No further details".to_string() } else { clean }
    }

    pub fn priority(&self) -> u8 {
        match self.target.as_str() {
            "block" | "reject" => 0,
            "allow" => 1,
            _ => 2,
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind { Controller, Camera, Bluetooth, Input, Storage, Network, Other }

fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

// Only parse class codes in the with-interface attribute, not in the USB name.
fn interface_codes(tokens: &[String]) -> Vec<String> {
    let Some(index) = tokens.iter().position(|part| part == "with-interface") else { return Vec::new(); };
    let rest = &tokens[index + 1..];
    let Some(first) = rest.first() else { return Vec::new(); };
    let candidates: Vec<&str> = if first == "{" {
        rest.iter().skip(1).take_while(|part| part.as_str() != "}")
            .map(String::as_str).collect()
    } else { vec![first.as_str()] };
    candidates.into_iter().filter(|code| {
        let chunks: Vec<&str> = code.split(':').collect();
        chunks.len() == 3 && chunks.iter().all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
    }).map(str::to_ascii_lowercase).collect()
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
            INTERFACE,
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
            .then_with(|| a.display_name().to_lowercase().cmp(&b.display_name().to_lowercase()))
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
        connection_type: value("with-connect-type").unwrap_or("").to_string(),
        interfaces: interface_codes(&tokens),
        serial: value("serial").unwrap_or("").to_string(),
        hash: value("hash").unwrap_or("").to_string(),
        parent_hash: value("parent-hash").unwrap_or("").to_string(),
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
    #[test]
    fn categorizes_reported_devices_and_groups_root_hubs() {
        let hub = parse_device_rule(8, r#"allow id 1d6b:0002 name "xHCI Host Controller""#);
        assert_eq!(hub.kind(), DeviceKind::Controller);
        let bt = parse_device_rule(14, r#"allow id 8087:0026 name "" with-interface { e0:01:01 e0:01:01 } with-connect-type "hardwired""#);
        assert_eq!(bt.kind(), DeviceKind::Bluetooth);
        assert_eq!(bt.display_name(), "Bluetooth adapter (reported)");
        assert_eq!(bt.connection_label(), "Internal (reported)");
        let camera = parse_device_rule(13, r#"allow id 04f2:b76f name "ACER HD User Facing" with-interface { 0e:01:00 0e:02:00 fe:01:01 }"#);
        assert_eq!(camera.kind(), DeviceKind::Camera);
        assert_eq!(camera.interfaces.len(), 3);
    }

    #[test]
    fn fake_interface_in_name_does_not_affect_category() {
        let fake = parse_device_rule(99, r#"block id 1234:5678 name "e0:01:01""#);
        assert_eq!(fake.kind(), DeviceKind::Other);
    }

    #[test]
    fn removes_bidi_controls_from_untrusted_names() {
        let fake = parse_device_rule(99, "block id 1234:5678 name \"Bad\u{202e}USB\"");
        assert_eq!(fake.display_name(), "BadUSB");
    }

    #[test]
    fn keeps_untrusted_markup_as_plain_text() {
        let fake = parse_device_rule(99, r#"block id 1234:5678 name "<script> USB""#);
        assert_eq!(fake.display_name(), "<script> USB");
    }

    #[test]
    fn snapshot_comparison_detects_reconnected_or_retargeted_device() {
        let initial = parse_device_rule(
            16,
            r#"block id 046d:c542 serial "" name "Wireless Receiver" hash "h1" parent-hash "parent" via-port "3-3" with-interface 03:01:02 with-connect-type "hotplug""#,
        );
        assert!(initial.matches_snapshot(&initial));
        let port_changed = parse_device_rule(16, r#"block id 046d:c542 serial "" name "Wireless Receiver" hash "h1" parent-hash "parent" via-port "3-1" with-interface 03:01:02 with-connect-type "hotplug""#);
        let number_changed = parse_device_rule(17, r#"block id 046d:c542 serial "" name "Wireless Receiver" hash "h1" parent-hash "parent" via-port "3-3" with-interface 03:01:02 with-connect-type "hotplug""#);
        let hash_changed = parse_device_rule(16, r#"block id 046d:c542 serial "" name "Wireless Receiver" hash "h2" parent-hash "parent" via-port "3-3" with-interface 03:01:02 with-connect-type "hotplug""#);
        let target_changed = parse_device_rule(16, r#"allow id 046d:c542 serial "" name "Wireless Receiver" hash "h1" parent-hash "parent" via-port "3-3" with-interface 03:01:02 with-connect-type "hotplug""#);
        assert!(!initial.matches_snapshot(&port_changed));
        assert!(!initial.matches_snapshot(&number_changed));
        assert!(!initial.matches_snapshot(&hash_changed));
        assert!(!initial.matches_snapshot(&target_changed));
    }

    #[test]
    fn parses_serial_hash_and_parent_hash_for_details() {
        let device = parse_device_rule(1, r#"allow id abcd:0123 serial "serial-1" name "Demo" hash "hash1" parent-hash "hash2""#);
        assert_eq!(device.serial, "serial-1");
        assert_eq!(device.hash, "hash1");
        assert_eq!(device.parent_hash, "hash2");
    }

    #[test]
    fn sanitizes_attributes_without_making_them_identity_tokens() {
        let raw = "A\u{202e}B\nC";
        assert_eq!(Device::display_attribute(raw), "ABC");
        assert_eq!(Device::display_attribute(""), "Not reported");
    }

    #[test]
    fn diagnostic_display_preserves_important_error_context() {
        let message = format!("{}AccessDenied: Not authorized", "x".repeat(210));
        assert!(Device::display_diagnostic(&message).contains("AccessDenied"));
        assert_eq!(Device::display_diagnostic("x\u{202e}y"), "xy");
    }

}
