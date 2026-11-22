# Security model

**PlugWard is not an enforcement component.** USBGuard, its daemon configuration, and Linux USB authorization remain the security boundary. PlugWard only reads current authorization decisions.

## Threats considered

- USB devices can lie about manufacturer, name, serial, USB ID, interface, and other descriptors. Displayed attributes are not identity proof.
- Malicious or malformed device names must stay text (not HTML/markup, code, or shell arguments).
- USBGuard runtime numeric device IDs can change after a reconnect. Never assume a device number is stable.
- The D-Bus bridge may be missing or access denied. PlugWard must fail closed: show an error and do nothing to the policy.
- Root hubs and internal devices may be essential to operating the machine. Never broadly overwrite policies to make the GUI work.

## Explicit non-goals for v0.3 stage 1

- No `sudo` invocation, elevated GUI, custom privileged helper, or system policy modification.
- No broad `wheel` or passwordless Polkit grants.
- No auto-approval of devices based on vendor/product ID alone.
- No silent device approval, policy persistence, or calls that affect USB authorization.

## Reporting a vulnerability

Please use GitHub Security Advisories / private vulnerability reporting when available, rather than publicly disclosing an exploitable vulnerability before a fix can be prepared.

## Inspection and freshness checks

The Details view shows USBGuard-reported values, which are **not trusted identities**. For display, PlugWard filters control and bidirectional formatting characters and limits attribute length. It never passes those values to a shell or renders them as GTK markup.

The Recheck button compares the initial listing with a new listing of the same runtime number, including all parsed fields. This detects some stale entries and mistakes but **cannot guarantee device identity**: descriptors are spoofable, IDs are transient, and a device can change immediately after any check (TOCTOU).

No D-Bus write methods are currently called. **Do not add broad Polkit grants, run the GUI as root, or weaken USBGuard's default-deny rules to enable details.** See [Authorization Design](AUTHORIZATION_DESIGN.md) for the proposed next stage.
