# Security model

**PlugWard is not an enforcement component.** USBGuard, its daemon configuration, and Linux USB authorization remain the security boundary. PlugWard only reads current authorization decisions.

## Threats considered

- USB devices can lie about manufacturer, name, serial, USB ID, interface, and other descriptors. Displayed attributes are not identity proof.
- Malicious or malformed device names must stay text (not HTML/markup, code, or shell arguments).
- USBGuard runtime numeric device IDs can change after a reconnect. Never assume a device number is stable.
- The D-Bus bridge may be missing or access denied. PlugWard must fail closed: show an error and do nothing to the policy.
- Root hubs and internal devices may be essential to operating the machine. Never broadly overwrite policies to make the GUI work.

## Explicit non-goals for v0.1

- No `sudo` invocation, elevated GUI, custom privileged helper, or system policy modification.
- No broad `wheel` or passwordless Polkit grants.
- No auto-approval of devices based on vendor/product ID alone.
- No silent device approval, policy persistence, or calls that affect USB authorization.

## Reporting a vulnerability

Please use GitHub Security Advisories / private vulnerability reporting when available, rather than publicly disclosing an exploitable vulnerability before a fix can be prepared.
