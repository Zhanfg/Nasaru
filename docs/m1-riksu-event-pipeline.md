<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# M1 Riksu event pipeline

Android-specific collectors do not call Policy Core or Capability Broker directly. They emit normalized `SignalEvent` frames using Riksu/1.

## Flow

```text
Android collector
    |
    | Riksu/1 EVENT
    v
nasarud
    |
    +--> strict wire decode
    +--> SignalState
    +--> trusted-session derivation
    +--> CapabilityBroker
    +--> AppOps backend
```

Unknown TLVs are ignored for forward compatibility, but duplicate security-significant fields are rejected to avoid ambiguous interpretation.

The frame header carries the monotonic timestamp. The payload carries a stable Nasaru event kind, UID and event-specific fields. Raw Android enum values are not part of the Riksu ABI.

## Fast-path boundary

This pipeline runs only on lifecycle/state events. It is not used for packet-path decisions and does not require polling.

## Remaining collector work

The next Android-specific component must subscribe to trusted platform callbacks and translate them into these normalized events. Global AppOps observation requires a trusted/root-capable execution context because ordinary apps may only observe their own UID without `WATCH_APPOPS`.

The transport endpoint itself remains local-only; peer credentials and SELinux context must be checked before accepting a collector connection.
