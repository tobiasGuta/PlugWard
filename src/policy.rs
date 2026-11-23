//! Strict, reversible USBGuard policy edits for one personally trusted
//! external USB storage device. No broad VID-only policies are generated.
//! Never call from the GTK main thread.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::gio;
use gtk::glib::variant::ToVariant;

use crate::security;
use crate::usbguard::{self, Device, DeviceKind};

const SERVICE: &str = "org.usbguard1";
const PATH: &str = "/org/usbguard1/Policy";
const INTERFACE: &str = "org.usbguard.Policy1";

// USBGuard Policy1.appendRule has signature (s rule, u parent_id, b temporary).
// This boolean is the inverse of the underlying IPC client permanent flag.
// LastID (UINT32_MAX-2) appends after existing rules, not after rule ID zero.
const LAST_RULE_ID: u32 = u32::MAX - 2;
const PERSISTENT_RULE_IS_TEMPORARY: bool = false;
const ALL_RULES_LABEL: &str = "";

fn append_rule_parameters(rule: &str) -> gtk::glib::Variant {
    (rule, LAST_RULE_ID, PERSISTENT_RULE_IS_TEMPORARY).to_variant()
}


#[derive(Clone, Debug)]
pub struct TrustReceipt {
    pub rule_id: u32,
    pub rule: String,
    pub backup_path: PathBuf,
}

#[derive(Clone, Debug)]
pub enum TrustResult {
    Confirmed(TrustReceipt),
    /// Write may have occurred: never claim that a failed verification means
    /// USBGuard automatically rolled anything back.
    Unverified(String),
}

fn strict_token(value: &str, allowed: fn(char) -> bool, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && value.chars().all(allowed)
}

fn ascii_serial(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.'
}

fn ascii_hash(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=')
}

fn port_char(ch: char) -> bool {
    ch.is_ascii_digit() || matches!(ch, '-' | '.')
}

/// Generate a narrow rule from the *entire* USBGuard snapshot. We require
/// a serial, hash, parent hash, exact port and one mass-storage interface.
/// This is not hardware authentication; descriptors can be impersonated.
pub fn scoped_rule(device: &Device) -> Result<String, String> {
    if !matches!(device.target.as_str(), "block" | "allow")
        || device.connection_type != "hotplug"
        || device.kind() != DeviceKind::Storage
        || device.interfaces.len() != 1
        || !device.interfaces[0].starts_with("08:")
    {
        return Err("Permanent trust is restricted to single-interface external USB storage.".into());
    }
    if device.usb_id.len() != 9
        || device.usb_id.as_bytes()[4] != b':'
        || !device.usb_id.chars().enumerate().all(|(index, c)| {
            index == 4 && c == ':' || index != 4 && c.is_ascii_hexdigit()
        })
        || !strict_token(&device.serial, ascii_serial, 120)
        || !strict_token(&device.hash, ascii_hash, 120)
        || !strict_token(&device.parent_hash, ascii_hash, 120)
        || !strict_token(&device.port, port_char, 35)
        || !device.port.contains('-')
    {
        return Err("Device is missing a valid serial, hash, parent hash, USB ID, or exact port.".into());
    }
    Ok(format!(
        "allow id {} serial \"{}\" hash \"{}\" parent-hash \"{}\" via-port \"{}\" with-interface {} with-connect-type \"hotplug\"",
        device.usb_id.to_lowercase(),
        device.serial,
        device.hash,
        device.parent_hash,
        device.port,
        device.interfaces[0]
    ))
}

fn system_bus() -> Result<gio::DBusConnection, String> {
    gio::bus_get_sync(gio::BusType::System, None::<&gio::Cancellable>)
        .map_err(|e| format!("System D-Bus unavailable: {e}"))
}

fn read_rules(bus: &gio::DBusConnection) -> Result<Vec<(u32, String)>, String> {
    let reply = bus.call_sync(
        Some(SERVICE), PATH, INTERFACE, "listRules",
        Some(&(ALL_RULES_LABEL,).to_variant()),
        None, gio::DBusCallFlags::NONE, 15_000, None::<&gio::Cancellable>,
    ).map_err(|e| format!("Cannot read USBGuard policy: {e}"))?;
    let (rules,): (Vec<(u32, String)>,) = reply.get()
        .ok_or_else(|| "Unexpected USBGuard listRules response".to_string())?;
    Ok(rules)
}

fn write_backup(rules: &[(u32, String)]) -> Result<PathBuf, String> {
    let base = match env::var_os("XDG_DATA_HOME") {
        Some(home) => PathBuf::from(home),
        None => PathBuf::from(env::var_os("HOME").ok_or(
            "No user home was found for a policy backup."
        )?).join(".local/share"),
    };
    let directory = base.join("plugward/policy-backups");
    fs::create_dir_all(&directory).map_err(|e| format!("Cannot create policy backup directory: {e}"))?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?.as_nanos();
    let filename = directory.join(format!("usbguard-before-trust-{timestamp}.txt"));
    let mut file = OpenOptions::new().create_new(true).write(true).mode(0o600)
        .open(&filename).map_err(|e| format!("Cannot open policy snapshot: {e}"))?;
    writeln!(file, "# PlugWard snapshot of USBGuard policy before one permanent trust operation")
        .map_err(|e| e.to_string())?;
    for (id, rule) in rules {
        writeln!(file, "{id}: {rule}").map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| format!("Cannot sync policy backup: {e}"))?;
    Ok(filename)
}

/// Interactive authorization is only invoked after explicit confirmation.
/// The new rule affects future connections; current device authorization is
/// not assumed or automatically changed.
pub fn trust_permanently(selected: &Device) -> Result<TrustResult, String> {
    let desired = scoped_rule(selected)?;
    let posture = security::probe();
    if posture.daemon_active.as_deref() != Ok("active")
        || posture.implicit_target.as_deref() != Ok("block")
    {
        return Err("Permanent trust refused: daemon active/default-deny policy could not be verified.".into());
    }

    let listing = usbguard::list_devices()?;
    if !listing.iter().any(|device| selected.matches_snapshot(device)) {
        return Err("Device changed or disconnected. No policy was modified.".into());
    }
    let bus = system_bus()?;
    let rules = read_rules(&bus)?;
    if rules.iter().any(|(_, rule)| rule == &desired) {
        return Err("An identical persistent rule already exists. No duplicate was added.".into());
    }
    let backup_path = write_backup(&rules)?;

    // Append *after* existing rules. USBGuard still decides how rules match.
    // No USBGuard policy is modified if these steps are denied or aborted.
    let reply = bus.call_sync(
        Some(SERVICE), PATH, INTERFACE, "appendRule",
        Some(&append_rule_parameters(desired.as_str())),
        None, gio::DBusCallFlags::ALLOW_INTERACTIVE_AUTHORIZATION,
        120_000, None::<&gio::Cancellable>,
    ).map_err(|e| format!(
        "USBGuard did not confirm rule creation: {e}. Check listRules before retrying."
    ))?;
    let (rule_id,): (u32,) = reply.get().ok_or_else(||
        "USBGuard response was ambiguous. Inspect the persistent rules before retrying.".to_string()
    )?;

    match read_rules(&bus) {
        Ok(now) if now.iter().any(|(id, rule)| *id == rule_id && rule == &desired) => {
            Ok(TrustResult::Confirmed(TrustReceipt { rule_id, rule: desired, backup_path }))
        }
        Ok(_) => Ok(TrustResult::Unverified(format!(
            "A rule may have been written (returned ID {rule_id}), but the exact saved rule was not verified. Check USBGuard's policy before retrying. Backup: {}",
            backup_path.display()
        ))),
        Err(e) => Ok(TrustResult::Unverified(format!(
            "USBGuard may have written rule {rule_id}, but verification failed: {e}. Backup: {}",
            backup_path.display()
        ))),
    }
}

/// Undo only the exact rule that PlugWard just created, after the user
/// explicitly approves removal. A changed rule ID/content cannot be removed.
pub fn undo_trust(receipt: &TrustReceipt) -> Result<bool, String> {
    let bus = system_bus()?;
    let current = read_rules(&bus)?;
    if !current.iter().any(|(id, rule)| *id == receipt.rule_id && rule == &receipt.rule) {
        return Err("The saved rule has changed or vanished. Nothing was removed.".into());
    }
    bus.call_sync(
        Some(SERVICE), PATH, INTERFACE, "removeRule",
        Some(&(receipt.rule_id,).to_variant()),
        None, gio::DBusCallFlags::ALLOW_INTERACTIVE_AUTHORIZATION,
        120_000, None::<&gio::Cancellable>,
    ).map_err(|e| format!("Unable to undo trust: {e}. Review the rule manually."))?;
    match read_rules(&bus) {
        Ok(now) => Ok(!now.iter().any(|(id, _)| *id == receipt.rule_id)),
        Err(e) => Err(format!("Remove was requested but verification failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usbguard::parse_device_rule;

    fn storage() -> Device {
        parse_device_rule(42,
            r#"block id 13fe:4200 serial "07036358AE0AA655" name "USB DISK 2.0" hash "AB+/==" parent-hash "CD+/==" via-port "3-4" with-interface 08:06:50 with-connect-type "hotplug""#
        )
    }

    #[test]
    fn rule_is_scoped_to_specific_hash_serial_port_and_usb_storage_interface() {
        let rule = scoped_rule(&storage()).unwrap();
        assert!(rule.starts_with("allow id 13fe:4200 serial"));
        for expected in ["serial \"07036358AE0AA655\"", "hash \"AB+/==\"", "parent-hash \"CD+/==\"", "via-port \"3-4\"", "with-interface 08:06:50"] {
            assert!(rule.contains(expected));
        }
    }

    #[test]
    fn refuse_hid_composite_internal_or_missing_serial() {
        let mut device = storage();
        device.interfaces.push("03:01:02".into());
        assert!(scoped_rule(&device).is_err());
        device = storage();
        device.connection_type = "hardwired".into();
        assert!(scoped_rule(&device).is_err());
        device = storage();
        device.serial.clear();
        assert!(scoped_rule(&device).is_err());
    }

    #[test]
    fn reject_injected_attributes_and_overbroad_unknown_usb_id() {
        let mut device = storage();
        device.serial = "id *:*".into();
        assert!(scoped_rule(&device).is_err());
        device = storage();
        device.usb_id = "*:*".into();
        assert!(scoped_rule(&device).is_err());
        device = storage();
        device.port = "3-4\" allow id *:*".into();
        assert!(scoped_rule(&device).is_err());
    }
    #[test]
    fn append_rule_uses_correct_dbus_signature_and_persistence_flag() {
        let rule = scoped_rule(&storage()).unwrap();
        let parameters = append_rule_parameters(&rule);
        assert_eq!(parameters.type_().as_str(), "(sub)");
        let (received_rule, parent_id, temporary): (String, u32, bool) =
            parameters.get().expect("valid USBGuard appendRule arguments");
        assert_eq!(received_rule, rule);
        assert_eq!(parent_id, u32::MAX - 2);
        assert!(!temporary, "permanent trust must write the policy file");
    }

    #[test]
    fn list_rules_query_is_unfiltered_for_complete_backup() {
        assert_eq!(ALL_RULES_LABEL, "");
    }

}
