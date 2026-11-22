# PlugWard authorization design — Stage 2 (partial implementation)

**Implemented in v0.3.0-alpha.2:** A guarded Allow Once request through the existing system D-Bus bridge using `applyDevicePolicy(id, 0, false)`, confirmation, fresh device-list consistency checks, disallowing critical/internal/HID/network/Bluetooth devices, and post-write verification. It requires the system's existing authorization and does not change Polkit or USBGuard IPC rules.

**Not implemented:** Permanent trust, Block, Reject, automated device approval, and guarantees against TOCTOU or forged USB descriptors. Mock D-Bus authorization tests and independent security review remain goals before broadening supported device types. Do not describe this early preview as fully security-audited.



This document describes the remaining security gates and the first explicitly confirmed temporary-approval implementation.

## API and privilege boundary

- Keep PlugWard an unprivileged GTK4/libadwaita process.
- Use the distribution-provided `org.usbguard1` **system D-Bus** bridge. Do not call `sudo`, write USBGuard configuration directly, or ship a setuid helper.
- USBGuard's documented `org.usbguard.Devices1.applyDevicePolicy(id: u32, target: u32, permanent: bool) -> rule_id: u32` uses target `0=allow`, `1=block`, `2=reject`. A permanent request may append **or modify** a stored policy rule.
- Respect the host's existing D-Bus, Polkit, and USBGuard IPC permissions. Never install permissive Polkit rules or automatically add the desktop user to broad privileged groups.
- A write may fail with permission denial. Display the error and leave the state unchanged. Do not probe permissions by writing to a device.

Official API: https://usbguard.github.io/documentation/dbus/doc-org.usbguard.Devices.html

## Intent-specific UX

1. **Allow once**: proposed `applyDevicePolicy(id, 0, false)`. This only changes current authorization, not a persistent allowlist.
2. **Block this connection**: proposed `applyDevicePolicy(id, 1, false)`. Do not permit this action on root hubs or critical hardwired devices in the first write-enabled release.
3. **Trust permanently**: proposed `applyDevicePolicy(id, 0, true)`. This changes persistent policy, so start disabled until we inspect the generated rule's matching strength and precedence and validate rollback. Clearly differentiate from Allow once.
4. **Reject**: postpone until Block is validated; rejecting can affect host device re-enumeration and user input.

Display a confirmation showing both the device's **reported descriptors** and the resulting scope (current connection vs future connections), including the risk that descriptors can be spoofed. Do not auto-authorize based on USB ID or name.

## Freshness and TOCTOU

- Before enabling a write action, fetch a fresh `listDevices("match")` response; do not rely solely on a periodic GUI cache.
- Validate the runtime USBGuard number, target, VID:PID, port, serial, hash, parent hash, interface codes, connection type, and name against the user-selected snapshot.
- If any field differs or the device is gone, abort and require a fresh user selection/confirmation.
- Reject operations on root hubs and default to rejecting actions on hardwired devices. Add a tested recovery path before considering them.
- Understand the limitation: no client-side recheck can atomically bind a USBGuard numeric ID to physical hardware across a separate D-Bus write. Device ID reuse, rapid reconnects, and descriptor spoofing remain possible. A matching hash/serial is **not cryptographic device authentication**.
- Re-fetch after the operation, verify the actual daemon state, and show truthful success/failure.

## Review and test gates

- Unit tests for stale numbers, differing ports, changed hashes/serials/targets, vanished devices, and deliberately spoofed identical descriptors.
- Mocked D-Bus tests for permitted and denied writes, changed device listings between confirmation and apply, and permanent-rule side effects.
- Runtime testing on Fedora GNOME with a **nonessential spare USB device**; never use the only working keyboard/mouse or a system hub.
- Back up and verify the original USBGuard policy before first permanent-rule tests, and document rollback.
- Require deliberate human confirmation for every authorization change and clear feedback from USBGuard.
- Security review of Polkit policy and each installed package's effective D-Bus/IPC privileges, not just the GUI's code.

**Stage 2 preview** includes only a temporary Allow Once action. The broader actions above remain unimplemented.
