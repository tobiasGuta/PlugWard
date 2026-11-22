# Architecture

PlugWard is a **GNOME front end** for the USBGuard daemon, with guarded temporary approvals in its v0.3 alpha. It does not provide USB authorization enforcement; the existing USBGuard daemon and Linux kernel do that.

## Data path

1. A normal unprivileged desktop application opens the **system D-Bus**.
2. A background worker calls the official `org.usbguard.Devices1.listDevices("match")` method at `/org/usbguard1/Devices` on the `org.usbguard1` service.
3. USBGuard's D-Bus bridge decides whether the method is authorized, using the system's policy.
4. Device-specific rule text is parsed as untrusted data and displayed with GTK text setters, never as markup or executable shell commands.
5. The GNOME main loop updates its rows from the worker's result.

The v0.2 dashboard polls at 10-second intervals, plus manual refresh. A device-list equality check retains widget state and avoids rebuilding unchanged rows. It does **not** claim instant event delivery. A later version could subscribe to device presence and policy signals.

## v0.2 presentation model

- Parse interface class codes only from the USBGuard with-interface field, not from arbitrary device names.
- Categorize known classes (HID, video, Bluetooth, mass storage, network) and Linux root hubs for display, never to grant trust.
- Render reported hardware labels as normal GTK text, stripped of control and bidirectional formatting characters, and capped at 80 characters.
- Keep root controllers in a collapsible section; prioritize blocked/rejected devices above allowed peripherals.
- Retain the user's controller expander state when devices are refreshed.
- Do not represent a successful D-Bus connection as proof that the daemon is configured for default-deny enforcement.

## Separation of privileges

- PlugWard does not run as root.
- PlugWard does not edit `/etc/usbguard/` or install permissive Polkit rules.
- The only write call is a user-confirmed `applyDevicePolicy(device_number, 0, false)` for a blocked eligible external device. No persistent rules are saved intentionally.
- A failed query results in an error display, not an attempt to relax the daemon's policy.
- Device numbers are transient; any future write feature must re-resolve the device before acting.

## Scope of the current preview

Supported: Fedora GNOME desktop with USBGuard and optional `usbguard-dbus`; readable authorized/blocked/rejected device list.

Not supported: permanent device authorization, block/reject controls, policy editing, rule explanation based on full policy evaluation, persistent audit/event history, or system service management.

Reference: https://usbguard.github.io/documentation/dbus/doc-org.usbguard.Devices.html

## v0.3 alpha: device inspector

- Each device row includes an explicit Details button. A modal native GTK window shows a snapshot from the most recently received USBGuard listing, not a live trusted identity.
- Displayed metadata includes USB ID, reported name, serial, hashes, port, class interfaces, connection type, transient USBGuard number, and authorization state at opening.
- A user-initiated Recheck performs another `listDevices("match")` call on a worker thread. It only reports whether the current record with the same numeric USBGuard number matches all parsed fields of the initial snapshot.
- Changes in number, authorization target, port, hash, parent hash, serial, interface classes, name, or connection type invalidate the snapshot. The UI requests reopening details after mismatch.
- A match can still result from spoofed USB descriptors or disconnect/reconnect between polling intervals. This is an observation aid, **not a security proof or authorization preflight guarantee**.
- Stage 2 adds a guarded, explicit temporary allow method; the inspector itself still only reads. No permanent rule edits are supported.

Future write actions are scoped in [AUTHORIZATION_DESIGN.md](AUTHORIZATION_DESIGN.md) and require separate review and tests. 


## Stage 2: temporary authorization

- Confirmation happens in a separate modal. Only clicking the final **Allow once** button invokes the worker.
- The worker queries `listDevices("match")`, matches all available attributes against the selected snapshot and checks eligibility. If anything changed, it aborts without writing.
- The worker sends `applyDevicePolicy(id, 0, false)` through the same official system D-Bus name, then re-reads to verify the expected allowed status and matching reported fields.
- All D-Bus I/O stays off the GTK main thread. The GUI neither launches `sudo` nor supplies a permissive Polkit policy.
- The pre-write match is not atomic with the write, and cannot prove that physical hardware is authentic. We explicitly surface this caveat rather than implying that rechecking prevents sophisticated spoofing or reconnect races.
