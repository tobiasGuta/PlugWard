# Architecture

PlugWard is a **read-only GNOME front end** for the USBGuard daemon. It does not provide USB authorization enforcement; the existing USBGuard daemon and Linux kernel do that.

## Data path

1. A normal unprivileged desktop application opens the **system D-Bus**.
2. A background worker calls the official `org.usbguard.Devices1.listDevices("match")` method at `/org/usbguard1/Devices` on the `org.usbguard1` service.
3. USBGuard's D-Bus bridge decides whether the method is authorized, using the system's policy.
4. Device-specific rule text is parsed as untrusted data and displayed with GTK text setters, never as markup or executable shell commands.
5. The GNOME main loop updates its rows from the worker's result.

Currently the dashboard polls at 10-second intervals, plus manual refresh. It does **not** claim instant event delivery. A later version could subscribe to device presence and policy signals.

## Separation of privileges

- PlugWard does not run as root.
- PlugWard does not edit `/etc/usbguard/` or install permissive Polkit rules.
- PlugWard does not call `applyDevicePolicy`, `appendRule`, or any other write method.
- A failed query results in an error display, not an attempt to relax the daemon's policy.
- Device numbers are transient; any future write feature must re-resolve the device before acting.

## Scope of the first release

Supported: Fedora GNOME desktop with USBGuard and optional `usbguard-dbus`; readable authorized/blocked/rejected device list.

Not supported: permanent/temporary device authorization, policy editing, rule explanation based on full policy evaluation, persistent audit/event history, or system service management.

Reference: https://usbguard.github.io/documentation/dbus/doc-org.usbguard.Devices.html
