<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# M1 trusted signal collector

The signal collector is deliberately separated from policy and enforcement.

A single Android signal is evidence, not authorization. Nasaru derives a trusted session only from a composite of independent platform signals.

## Event sources

The first collector contract accepts:

- UID importance transitions;
- foreground-service type changes;
- AppOps active/inactive callbacks;
- companion-device presence;
- package add/remove events.

The state machine is event-driven. It does not continuously scan processes or `dumpsys`.

## High-confidence session rules

### Navigation

Requires all of:

- location foreground-service type;
- location AppOp currently active;
- current or recently visible user context.

A package merely declaring a location FGS type is insufficient.

### Voice/video

Voice requires:

- microphone or phone-call foreground-service type;
- microphone AppOp active;
- current or recently visible user context.

Video additionally requires camera FGS type and active camera AppOp.

### Companion health sync

Requires both:

- an associated companion device currently present;
- a health or connected-device foreground-service type.

## Package lifecycle

A newly installed third-party UID emits an `ApplyThirdPartyBaseline` effect. System packages are not automatically pushed into the third-party foreground-only baseline.

A full package removal drops the UID state. Package replacement retains state and is treated as an ambiguous context change until fresh evidence arrives.

## Privileged collector requirement

Global AppOps active-state observation requires `WATCH_APPOPS`; without it Android only allows an app to observe its own UID. The production collector therefore must run in a trusted/root-capable context rather than as an ordinary third-party app.

The current Rust state machine intentionally contains no OEM-specific Binder calls. A later Android collector backend can feed these normalized events through Riksu without changing session derivation.
