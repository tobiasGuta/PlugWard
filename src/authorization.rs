//! Guarded, *temporary* authorization of one user-confirmed hotplug device.
//!
//! The only write method is applyDevicePolicy(..., allow, permanent=false).
//! USBGuard owns the actual security policy; this client never bypasses Polkit
//! or changes rules. Snapshot verification narrows stale-ID mistakes but does
//! NOT eliminate a reconnect race or authenticate USB descriptors.

use gtk::gio;
use gtk::glib::variant::ToVariant;

use crate::usbguard::{self, Device, DeviceKind};

const SERVICE: &str = "org.usbguard1";
const PATH: &str = "/org/usbguard1/Devices";
const INTERFACE: &str = "org.usbguard.Devices1";
const TARGET_ALLOW: u32 = 0;

#[derive(Debug, PartialEq, Eq)]
pub enum AllowResult {
    /// Post-write USBGuard listing confirmed the same reported device is allowed.
    Confirmed,
    /// The write returned success, but re-reading the device didn't confirm it.
    Unverified(String),
}

/// Only expose an actionable button for a blocked, nonessential, external
/// device with enough USBGuard fields for an exact pre-write comparison.
/// This guard does NOT certify device safety.
pub fn eligibility(device: &Device) -> Result<(), &'static str> {
    if device.target != "block" {
        return Err("Only currently blocked devices are supported in this preview.");
    }
    if device.kind() == DeviceKind::Controller
        || device.connection_type != "hotplug"
        || device.port == "Not reported"
        || device.port.starts_with("usb")
    {
        return Err("System and hardwired devices cannot be approved in this preview.");
    }
    if device.usb_id == "Unknown" || device.hash.is_empty() || device.parent_hash.is_empty() {
        return Err("Insufficient USBGuard identity data for a cautious temporary approval.");
    }
    if matches!(device.kind(), DeviceKind::Input | DeviceKind::Network | DeviceKind::Bluetooth) {
        return Err("Input, network, and Bluetooth devices are excluded from this first write-enabled preview.");
    }
    Ok(())
}

/// Pre-write check: never act on an earlier list entry whose attributes, port
/// or authorization status have changed. A match does not eliminate TOCTOU.
fn validate_current<'a>(
    selected: &Device,
    devices: &'a [Device],
) -> Result<&'a Device, String> {
    eligibility(selected).map_err(str::to_string)?;
    let current = devices
        .iter()
        .find(|device| device.number == selected.number)
        .ok_or_else(|| "The selected USB device is no longer connected. No action was taken.".to_string())?;
    if !selected.matches_snapshot(current) {
        return Err("The USB device, port, or authorization changed. No action was taken; reopen its Details window.".to_string());
    }
    eligibility(current).map_err(str::to_string)?;
    Ok(current)
}

/// Compares a post-write listing with the selected snapshot, permitting only
/// the expected target change. Descriptor matches are NOT identity proof.
fn verify_allowed(selected: &Device, devices: &[Device]) -> bool {
    let Some(current) = devices.iter().find(|device| device.number == selected.number) else {
        return false;
    };
    let mut expected = selected.clone();
    expected.target = "allow".to_string();
    expected.matches_snapshot(current)
}

/// Must execute on a background thread, after an explicit user confirmation.
/// No shell, no sudo, no persistent rule, no automatic permission grants.
pub fn allow_once(selected: &Device) -> Result<AllowResult, String> {
    let before = usbguard::list_devices().map_err(|error| format!("Cannot recheck device: {error}"))?;
    validate_current(selected, &before)?;

    let bus = gio::bus_get_sync(gio::BusType::System, None::<&gio::Cancellable>)
        .map_err(|error| format!("Cannot connect to the system D-Bus: {error}"))?;

    // Official USBGuard API: (u id, u target, b permanent) -> (u rule_id).
    // permanent=false is essential: this must not modify the allowlist.
    let reply = bus
        .call_sync(
            Some(SERVICE),
            PATH,
            INTERFACE,
            "applyDevicePolicy",
            Some(&(selected.number, TARGET_ALLOW, false).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            120_000,
            None::<&gio::Cancellable>,
        )
        .map_err(|error| format!(
            "USBGuard did not confirm temporary authorization: {error}. \
             No permission overrides were applied; check the current device state before retrying."
        ))?;

    let _: (u32,) = reply.get().ok_or_else(|| {
        "The authorization response was unexpected; recheck the USBGuard state before attempting another action.".to_string()
    })?;

    match usbguard::list_devices() {
        Ok(devices) if verify_allowed(selected, &devices) => Ok(AllowResult::Confirmed),
        Ok(_) => Ok(AllowResult::Unverified(
            "USBGuard acknowledged the request, but the latest device record did not match an allowed state. Inspect the device list before retrying.".to_string(),
        )),
        Err(error) => Ok(AllowResult::Unverified(format!(
            "USBGuard acknowledged the request, but its latest state could not be read: {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Device {
        usbguard::parse_device_rule(
            41,
            r#"block id 1234:5678 serial "trusted-test" name "Test USB storage" hash "hash-one" parent-hash "parent-one" via-port "3-3" with-interface 08:06:50 with-connect-type "hotplug""#,
        )
    }

    #[test]
    fn blocked_external_storage_is_eligible() {
        assert!(eligibility(&sample()).is_ok());
        assert!(validate_current(&sample(), &[sample()]).is_ok());
    }

    #[test]
    fn no_approval_on_changed_port_or_hash_or_runtime_number() {
        let selected = sample();
        let mut changed = selected.clone();
        changed.port = "3-1".to_string();
        assert!(validate_current(&selected, &[changed]).is_err());
        let mut changed = selected.clone();
        changed.hash = "changed".to_string();
        assert!(validate_current(&selected, &[changed]).is_err());
        let mut changed = selected.clone();
        changed.number += 1;
        assert!(validate_current(&selected, &[changed]).is_err());
        assert!(validate_current(&selected, &[]).is_err());
    }

    #[test]
    fn no_approval_after_policy_or_interface_change() {
        let selected = sample();
        let mut changed = selected.clone();
        changed.target = "allow".to_string();
        assert!(validate_current(&selected, &[changed]).is_err());
        let mut changed = selected.clone();
        changed.interfaces = vec!["03:01:01".to_string()];
        assert!(validate_current(&selected, &[changed]).is_err());
    }

    #[test]
    fn system_devices_and_input_network_are_not_eligible() {
        let mut internal = sample();
        internal.connection_type = "hardwired".to_string();
        assert!(eligibility(&internal).is_err());
        let mut hub = sample();
        hub.usb_id = "1d6b:0002".to_string();
        assert!(eligibility(&hub).is_err());
        let mut input = sample();
        input.interfaces = vec!["03:01:01".to_string()];
        assert!(eligibility(&input).is_err());
        let mut network = sample();
        network.interfaces = vec!["02:06:00".to_string()];
        assert!(eligibility(&network).is_err());
    }

    #[test]
    fn allowed_or_rejected_or_incomplete_device_is_not_eligible() {
        let mut device = sample();
        device.target = "allow".to_string();
        assert!(eligibility(&device).is_err());
        device.target = "reject".to_string();
        assert!(eligibility(&device).is_err());
        device.target = "block".to_string();
        device.hash.clear();
        assert!(eligibility(&device).is_err());
    }

    #[test]
    fn postwrite_check_needs_expected_allowed_target_and_same_metadata() {
        let selected = sample();
        assert!(!verify_allowed(&selected, std::slice::from_ref(&selected)));
        let mut allowed = selected.clone();
        allowed.target = "allow".to_string();
        assert!(verify_allowed(&selected, &[allowed.clone()]));
        allowed.port = "3-1".to_string();
        assert!(!verify_allowed(&selected, &[allowed]));
    }

    #[test]
    fn spoofed_identical_descriptors_still_match_and_are_not_authentication() {
        let first = sample();
        let indistinguishable_spoof = sample();
        assert!(validate_current(&first, &[indistinguishable_spoof]).is_ok());
        // Documented inherent limitation of client-side matching.
    }
}
