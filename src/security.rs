//! Read-only security posture probe. Every failed check is "Unknown", not "Safe".
//! Successful D-Bus listing alone is never proof of a default-deny policy.

use gtk::gio;
use gtk::glib::variant::ToVariant;

const SERVICE: &str = "org.usbguard1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecurityStatus {
    pub daemon_active: Result<String, String>,
    pub boot_enabled: Result<String, String>,
    pub implicit_target: Result<String, String>,
}

impl SecurityStatus {
    pub fn summary(&self) -> &'static str {
        if self.daemon_active.as_deref() == Ok("active")
            && self.boot_enabled.as_deref() == Ok("enabled")
            && self.implicit_target.as_deref() == Ok("block")
        {
            "Expected baseline reported"
        } else if self.daemon_active.as_deref() == Ok("inactive")
            || self.daemon_active.as_deref() == Ok("failed")
            || self.implicit_target.as_deref() == Ok("allow")
        {
            "Warning: unexpected security configuration"
        } else {
            "Unable to fully verify"
        }
    }
}

fn system_bus() -> Result<gio::DBusConnection, String> {
    gio::bus_get_sync(gio::BusType::System, None::<&gio::Cancellable>)
        .map_err(|e| e.to_string())
}

fn systemd_state(bus: &gio::DBusConnection, method: &str) -> Result<String, String> {
    let reply = bus
        .call_sync(
            Some("org.freedesktop.systemd1"),
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            method,
            Some(&("usbguard.service",).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            8_000,
            None::<&gio::Cancellable>,
        )
        .map_err(|e| e.to_string())?;
    if method == "GetUnitFileState" {
        return reply
            .get::<(String,)>()
            .map(|value| value.0)
            .ok_or_else(|| "Unexpected systemd unit-file-state response".to_string());
    }
    let unit_path = reply
        .child_value(0)
        .str()
        .map(str::to_string)
        .ok_or_else(|| "Unexpected systemd unit path".to_string())?;
    let response = bus
        .call_sync(
            Some("org.freedesktop.systemd1"),
            &unit_path,
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&("org.freedesktop.systemd1.Unit", "ActiveState").to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            8_000,
            None::<&gio::Cancellable>,
        )
        .map_err(|e| e.to_string())?;
    let (value,): (gtk::glib::Variant,) = response
        .get()
        .ok_or_else(|| "Unexpected systemd property response".to_string())?;
    value
        .get::<String>()
        .ok_or_else(|| "Unexpected ActiveState type".to_string())
}

fn daemon_parameter(bus: &gio::DBusConnection, name: &str) -> Result<String, String> {
    let reply = bus
        .call_sync(
            Some(SERVICE),
            "/org/usbguard1",
            "org.usbguard1",
            "getParameter",
            Some(&(name,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            8_000,
            None::<&gio::Cancellable>,
        )
        .map_err(|e| e.to_string())?;
    reply
        .get::<(String,)>()
        .map(|value| value.0)
        .ok_or_else(|| "Unexpected USBGuard parameter response".to_string())
}

/// Blocking D-Bus I/O — call on a worker thread.
pub fn probe() -> SecurityStatus {
    match system_bus() {
        Ok(bus) => SecurityStatus {
            daemon_active: systemd_state(&bus, "GetUnit"),
            boot_enabled: systemd_state(&bus, "GetUnitFileState"),
            implicit_target: daemon_parameter(&bus, "ImplicitPolicyTarget"),
        },
        Err(error) => SecurityStatus {
            daemon_active: Err(error.clone()),
            boot_enabled: Err(error.clone()),
            implicit_target: Err(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_expected_running_enabled_block_baseline_is_favorable() {
        let status = SecurityStatus {
            daemon_active: Ok("active".into()),
            boot_enabled: Ok("enabled".into()),
            implicit_target: Ok("block".into()),
        };
        assert_eq!(status.summary(), "Expected baseline reported");
    }

    #[test]
    fn unknown_is_never_reported_as_protected() {
        let status = SecurityStatus {
            daemon_active: Ok("active".into()),
            boot_enabled: Err("denied".into()),
            implicit_target: Ok("block".into()),
        };
        assert_eq!(status.summary(), "Unable to fully verify");
    }

    #[test]
    fn allow_by_default_is_a_warning() {
        let status = SecurityStatus {
            daemon_active: Ok("active".into()),
            boot_enabled: Ok("enabled".into()),
            implicit_target: Ok("allow".into()),
        };
        assert!(status.summary().contains("Warning"));
    }
}
