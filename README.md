# PlugWard

**Control what connects. Protect what matters.**

PlugWard is an experimental, native GNOME desktop dashboard for [USBGuard](https://usbguard.github.io/). It is written in Rust using GTK4 and libadwaita.

> **Development status:** v0.4 alpha, security-focused preview; permanent-trust and revocation features have not yet passed live Fedora validation. It is not a substitute for USBGuard or a malware scanner. Do not depend on PlugWard to enforce USB policy.

## v0.4 alpha: USB Security Center

The v0.4 alpha adds five requested capabilities while keeping USBGuard itself as the policy enforcement service:

- **Activity** — a session-only timeline of device arrivals, removals, and authorization transitions observed through USBGuard device-list snapshots. The dashboard samples every **3 seconds**. It can miss fast events, is not persistent, and must not be treated as an audit log.
- **Security** — read-only checks of the USBGuard systemd service state, boot enablement, and the daemon-reported \`ImplicitPolicyTarget\`. It reports **Unable to fully verify** when anything cannot be checked; even a favorable result is not proof that all policy rules are safe.
- **Device inspector** — switch between **Overview** and **Advanced** for easy-to-read status and technical USB descriptors/hashes.
- **Block this connection** — temporary block for an authorized, nonessential, single-interface external USB storage device, with confirmation, recheck and post-action verification. **This does not delete any pre-existing permanent allow rule**; the device may be authorized on reconnection.
- **Permanent Trust** — for a personally trusted, single-interface external storage device only. It displays the exact proposed allow rule, requires a nonempty serial, USB ID, hash, parent hash, interface and specific port, rechecks the selected device, verifies the daemon reports default-deny, and saves an **0600 user-owned policy snapshot** before calling USBGuard's \`appendRule\` via interactive Polkit. The rule is re-read afterward. A successful result offers **Undo this trust**, which only removes the exact rule ID and content PlugWard created, after another confirmation. **Undo is conditional on USBGuard authorization and is not guaranteed**.

**Critical:** Never test with unknown/found USB devices, your only keyboard/mouse, a USB hub, or built-in devices. USB serials, hashes, ports and interface descriptors are not cryptographic proof of authenticity. Client-side rechecks cannot eliminate device-ID reuse or a disconnect during a policy write.

**Before the first permanent-trust test**, independently back up your verified policy:

\`\`\`bash
sudo cp -a /etc/usbguard/rules.conf /etc/usbguard/rules.conf.before-plugward-v04
sudo usbguard list-rules
\`\`\`

PlugWard's own snapshots are stored under \`$XDG_DATA_HOME/plugward/policy-backups\`, or \`~/.local/share/plugward/policy-backups\` if XDG_DATA_HOME is unset. Snapshots are **not** a substitute for backing up the original rules file; they contain device metadata, so keep them private. After using Permanent Trust, inspect \`sudo usbguard list-rules\`, and after Undo recheck that the created rule is absent. A new rule may not authorize the currently blocked device until it is reconnected; do not assume it did.

### PlugWard app icon and GNOME launcher

PlugWard includes its own USB-and-shield SVG icon, stored at \`data/icons/hicolor/scalable/apps/io.github.tobiasguta.PlugWard.svg\`. To install the app, launcher, and icon for **your own user account only**, run:

\`\`\`bash
bash scripts/install-user.sh
\`\`\`

This compiles the release binary and installs it to \`~/.local/bin\`, plus a \`~/.local/share/applications\` launcher and \`~/.local/share/icons\` icon. It **does not** change USBGuard policies, services, Polkit, or system packages. Depending on your GNOME session you may need to log out and back in to see the launcher.

## v0.3 alpha 2: guarded Allow Once (stage 2 preview)

**New:** Open **Details** on a blocked, personally trusted USB peripheral and choose **Allow once…**. PlugWard requires a separate confirmation that identifies the reported device and port; it re-fetches USBGuard's current device list and aborts if its number, reported attributes, port, hash, interfaces, or authorization changed. It then requests `applyDevicePolicy(device_number, 0, false)` through USBGuard's system D-Bus bridge and checks the new state. It never saves a permanent allow rule.

**Deliberate restrictions:** In this preview, only currently `block`-targeted, external `hotplug` devices with nonempty hashes and known port/USB ID can be approved. Root USB controllers, internal/hardwired devices, input/HID, network, and Bluetooth classes are excluded. The device's class is *reported* and can be spoofed. Do not plug in untrusted or found devices to test it.

A matching device snapshot is **not proof of physical identity**, and a reconnect can occur between a fresh check and the separate D-Bus write. If permissions are denied, PlugWard reports the error. Never install broad passwordless Polkit or USBGuard IPC rules to make this feature work. The system may request administrator authentication, depending on existing policy.

### Fedora test procedure (trusted nonessential spare USB storage)

1. Save your work and keep your built-in keyboard/touchpad available.
2. Back up your current verified USBGuard rules before testing: `sudo cp -a /etc/usbguard/rules.conf /etc/usbguard/rules.conf.before-plugward-test`.
3. Launch PlugWard as your normal user using `cargo run`; do not use `sudo`.
4. Plug in a **personally owned, known-safe, nonessential** USB storage device that is not on your allowlist. USBGuard should show it as blocked.
5. Open Details, inspect its reported fields, then click **Allow once…** and read the confirmation. Confirm only if you trust it. If the system asks for authorization, follow your normal Polkit prompt.
6. Use Refresh and `sudo usbguard list-devices` to verify current state. Unplug the device; on reconnection it should be evaluated against your original policy again.
7. Verify no permanent rule was added with `sudo usbguard list-rules`.

**No permanent trust, Block, or Reject UI exists in this version.** Stop testing if authorization is denied or the device listing unexpectedly changes; don't weaken your system policy. See [authorization design](docs/AUTHORIZATION_DESIGN.md).

## v0.3 alpha: device inspection (stage 1)

- Every connected, blocked, or rejected device now has an accessible **Details** button.
- Details show USBGuard authorization at opening, runtime device number, reported USB ID, name, port, connection type, serial, hash, parent hash, and interface codes.
- **Recheck** queries USBGuard again and compares the current device record with the snapshot. A mismatch or disappearance is displayed clearly.
- These are read-only operations. Rechecking does not authenticate the hardware, prevent descriptor spoofing, or eliminate device-reconnection races.
- No USBGuard policy edits, device authorization operations, privileged helpers, or Polkit overrides have been added.

This is **stage 1** of v0.3. See [authorization design](docs/AUTHORIZATION_DESIGN.md) for the security gates planned before implementing Allow Once / Block / Trust Permanently. These controls are **not available yet**.

## What's new in v0.2 (development preview)

- Adaptive, centered GNOME dashboard that avoids stretching across large monitors
- Read-only connection state and total/allowed/blocked summary cards
- Icons and descriptive categories inferred from reported USB interface classes
- Separately emphasized blocked and rejected devices
- Peripherals shown first, with USB root controllers tucked into a collapsible group
- Expanded/collapsed state retained during automatic refreshes
- Updated every 10 seconds and manually with Refresh; unchanged lists keep their current focus/expanded state
- Safer rendering of USB-reported names (display length cap, bidi-control filtering, no markup execution)
- Clear errors for missing Fedora D-Bus bridge and access-denied situations

**Important:** USB names, IDs, and class descriptors can be spoofed. These categories are **display hints**, not proof of device identity. A working dashboard connection does **not** independently verify that USBGuard is configured with a default-deny policy.

PlugWard supports **explicit temporary approval** of eligible blocked devices; it never adds permanent rules, starts/stops USBGuard, or elevates its own process privileges.

## Fedora setup

Keep your existing working USBGuard daemon and policy unchanged. To install the optional, distribution-provided D-Bus bridge required by the read-only dashboard:

```bash
sudo dnf install usbguard-dbus gtk4 libadwaita
sudo systemctl enable --now usbguard-dbus.service
systemctl is-active usbguard-dbus.service
```

For a development build on Fedora:

```bash
sudo dnf install rust cargo gcc pkgconf-pkg-config gtk4-devel libadwaita-devel
git clone https://github.com/tobiasGuta/PlugWard.git
cd PlugWard
cargo run
```

Run PlugWard **as your normal desktop user**, not with `sudo`. If the bridge refuses access, PlugWard reports the error rather than modifying your USBGuard configuration. Installing `usbguard-dbus` does not replace or reconfigure the existing USBGuard daemon. PlugWard depends on the separate `usbguard-dbus.service` to expose the `org.usbguard1` D-Bus name.

### Troubleshooting

If PlugWard reports `NameHasNoOwner`, `unknown unit`, or says that the USBGuard D-Bus bridge is unavailable, first check:

```bash
rpm -q usbguard-dbus
systemctl status usbguard-dbus.service --no-pager
```

Then install and start the bridge as shown above. If the bridge is active but PlugWard reports an access-denied error, check D-Bus and Polkit authorization. **Do not add blanket allow or passwordless management rules** simply to make the GUI work.

### Troubleshooting: “Not authorized” on Allow once

PlugWard's first write-enabled alpha omitted the GIO **interactive authorization**
flag on its USBGuard D-Bus write request. This could prevent Fedora's Polkit
authentication prompt. A confirmed **Allow once** request now explicitly sets
`Gio.DBusCallFlags.ALLOW_INTERACTIVE_AUTHORIZATION`; read-only listing does
not request authentication.

If a new attempt is still denied, **do not** add passwordless Polkit rules or
weaken your USBGuard IPC access control. Inspect the installed action and
existing authorization logs (these commands do not authorize a USB device):

```bash
pkaction --action-id org.usbguard.Devices1.applyDevicePolicy --verbose
systemctl is-active polkit.service usbguard-dbus.service usbguard.service
sudo journalctl -b -u usbguard-dbus.service -n 40 --no-pager
sudo journalctl -b -u polkit.service -n 40 --no-pager
sudo usbguard list-devices --blocked
```

The denial may come from the system D-Bus bus policy, Polkit, or USBGuard's
own IPC privileges. The GUI cannot safely assume which one rejected a request
based solely on an AccessDenied string. If there was no password dialog,
check your GNOME authentication agent and the installed system policy
before adjusting any authorization configuration.

## Development

```bash
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

See [architecture](docs/ARCHITECTURE.md) and [security](docs/SECURITY.md) for implementation and threat-model details.

## License

MIT; see [LICENSE](LICENSE). PlugWard is an independent project and is not affiliated with the USBGuard maintainers.
