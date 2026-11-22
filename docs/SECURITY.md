# Security model

**PlugWard is not an enforcement component.** USBGuard, its daemon configuration, and Linux USB authorization remain the security boundary. From v0.3 alpha 2 PlugWard can make **explicit, temporary, user-confirmed Allow Once requests** through the existing D-Bus bridge.

## Threats considered

- USB devices can lie about manufacturer, name, serial, USB ID, interface, and other descriptors. Displayed attributes are not identity proof.
- Malicious or malformed device names must stay text (not HTML/markup, code, or shell arguments).
- USBGuard runtime numeric device IDs can change after a reconnect. Never assume a device number is stable.
- The D-Bus bridge may be missing or access denied. PlugWard must fail closed: show an error and do nothing to the policy.
- Root hubs and internal devices may be essential to operating the machine. Never broadly overwrite policies to make the GUI work.

## Explicit non-goals for v0.3 stage 2

- No `sudo` invocation, elevated GUI, custom privileged helper, or system policy modification.
- No broad `wheel` or passwordless Polkit grants.
- No auto-approval of devices based on vendor/product ID alone.
- No silent device approval or policy persistence. Only the explicitly confirmed Allow Once action calls the write method with permanent=false.

## Reporting a vulnerability

Please use GitHub Security Advisories / private vulnerability reporting when available, rather than publicly disclosing an exploitable vulnerability before a fix can be prepared.

## Inspection and freshness checks

The Details view shows USBGuard-reported values, which are **not trusted identities**. For display, PlugWard filters control and bidirectional formatting characters and limits attribute length. It never passes those values to a shell or renders them as GTK markup.

The Recheck button compares the initial listing with a new listing of the same runtime number, including all parsed fields. This detects some stale entries and mistakes but **cannot guarantee device identity**: descriptors are spoofable, IDs are transient, and a device can change immediately after any check (TOCTOU).

The only added D-Bus write method is `applyDevicePolicy(id, target=0, permanent=false)`, called after a separate confirmation and fresh list comparison. **Do not add broad Polkit grants, run the GUI as root, or weaken USBGuard's default-deny rules to enable details.** See [Authorization Design](AUTHORIZATION_DESIGN.md) for the proposed next stage.


## Allow Once security limitations

- Only blocked external/hotplug devices with stable-looking reported fields and nonempty hashes are eligible. Root hubs, internal devices, HID/input, Bluetooth, and network devices are refused by this version.
- Device ID, USB class, name, hash, serial, and port are *not cryptographic authentication*. A malicious device may spoof descriptors.
- A new `listDevices` operation checks that the selected snapshot still matches immediately before a separate `applyDevicePolicy` call; this does not make the two operations atomic. Rapid reconnect and device-ID reuse are still possible.
- Never bypass, disable, or auto-grant Polkit authorization. Keep the GUI unprivileged. Permission denials appear in the result window.
- The request includes `permanent=false`. No on-disk rule is intentionally appended. Recheck both the device list and the saved rules during manual Fedora tests.
- A successful D-Bus response and a confirmed matching post-write listing are needed before the interface reports authorization confirmed. An ambiguous outcome is labeled **unverified**, not as a guaranteed failure.
- Do not test with unknown/found USB media. Use a known-safe, nonessential, personally owned test peripheral.

## Interactive authentication

Only the `applyDevicePolicy(id, 0, false)` call triggered by an explicit
confirmation sends `Gio.DBusCallFlags.ALLOW_INTERACTIVE_AUTHORIZATION`.
USBGuard's read-only `listDevices` calls never request interactive privileges.
This allows the system's existing Polkit policy to show an authentication
prompt where permitted. It does not grant authorization by itself and does not
change the host's D-Bus, Polkit, USBGuard IPC or permanent policy settings.

AccessDenied can originate at multiple authorization layers; a denial is
not a reason to add broad passwordless rules. Confirm the device remains blocked
before diagnosing the local policies. Details are in the README.
