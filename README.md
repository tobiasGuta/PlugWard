# PlugWard

**Control what connects. Protect what matters.**

PlugWard is an experimental, native GNOME desktop dashboard for [USBGuard](https://usbguard.github.io/). It is written in Rust using GTK4 and libadwaita.

> **Development status:** Early, read-only preview. It is not a substitute for USBGuard or a malware scanner. Do not depend on PlugWard to enforce USB policy.

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

PlugWard **never** authorizes devices, changes rules, starts/stops USBGuard, or elevates its own privileges.

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

## Development

```bash
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

See [architecture](docs/ARCHITECTURE.md) and [security](docs/SECURITY.md) for implementation and threat-model details.

## License

MIT; see [LICENSE](LICENSE). PlugWard is an independent project and is not affiliated with the USBGuard maintainers.
