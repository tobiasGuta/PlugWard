# PlugWard

**Control what connects. Protect what matters.**

PlugWard is an experimental, native GNOME desktop dashboard for [USBGuard](https://usbguard.github.io/). It is written in Rust using GTK4 and libadwaita.

> **Development status:** Early, read-only preview. It is not a substitute for USBGuard or a malware scanner. Do not depend on PlugWard to enforce USB policy.

## What the first version does

- Show connected USB devices and their current USBGuard authorization targets
- Display device name, USB vendor/product ID, port, and USBGuard device number
- Refresh every 10 seconds, with a manual Refresh button
- Explain when the USBGuard D-Bus bridge is missing or cannot be accessed

PlugWard **never** authorizes devices, changes rules, starts/stops USBGuard, or elevates its own privileges.

## Fedora setup

Keep your existing working USBGuard daemon and policy unchanged. To install the optional, distribution-provided D-Bus bridge required by the read-only dashboard:

```bash
sudo dnf install usbguard-dbus gtk4 libadwaita
```

For a development build on Fedora:

```bash
sudo dnf install rust cargo gcc pkgconf-pkg-config gtk4-devel libadwaita-devel
git clone https://github.com/tobiasGuta/PlugWard.git
cd PlugWard
cargo run
```

Run PlugWard **as your normal desktop user**, not with `sudo`. If the bridge refuses access, PlugWard reports the error rather than modifying your USBGuard configuration. Installing `usbguard-dbus` does not replace the existing daemon, but its service may be D-Bus-activated when queried.

## Development

```bash
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

See [architecture](docs/ARCHITECTURE.md) and [security](docs/SECURITY.md) for implementation and threat-model details.

## License

MIT; see [LICENSE](LICENSE). PlugWard is an independent project and is not affiliated with the USBGuard maintainers.
