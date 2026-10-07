<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# M1 trusted Android signals

Nasaru does not use package-name allowlists to recognize legitimate background work. Instead, the Android adapter is expected to observe trusted platform state and translate verified sessions into short-lived Qibitu capabilities.

Initial mappings:

| Trusted session | Capability resources |
|---|---|
| Navigation | location, network egress |
| Companion health sync | Bluetooth scan, network egress |
| Voice call | microphone, network egress |
| Video call | microphone, camera, network egress |
| User-started file transfer | sensitive-file continuation, network egress |

These mappings do **not** mean that an application may send a Riksu message claiming that a session exists. Session creation is reserved for the trusted Android bridge.

## AppOps baseline

Camera, microphone and location use a foreground-only baseline. A verified Qibitu may temporarily request an `Allowed` override, and session stop/lease expiry restores `Foreground`.

The actual Binder/AppOps backend is intentionally behind an interface in M1. This keeps the policy layer testable on ordinary CI and prevents Android-version-specific Binder details from leaking into the core ABI.

## Expiry scheduling

The broker exposes the nearest monotonic expiry. The daemon can arm a single timer for that deadline and sleep. This is event-driven expiration, not a polling loop. Expired leases are returned to the integration layer so any temporary enforcement override can be reverted precisely.
