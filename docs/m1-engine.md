<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# M1 orchestration engine

The M1 engine closes the loop between normalized Android evidence, Qibitu leases and AppOps enforcement.

## Flow

```text
Android collector event
        |
        v
SignalState
        |
        v
SignalEffect
        |
        +--> third-party UID baseline
        |
        +--> trusted session transition
                  |
                  v
           CapabilityBroker
                  |
                  v
          AppOps override/revert
```

The engine is event-driven. Lease expiration uses `next_expiry_ns()` so the daemon can arm one monotonic timer for the nearest expiry rather than polling.

## Transaction rule

A trusted-session transition first updates a cloned broker snapshot. If an AppOps change fails, broker state is rolled back. Any already-applied AppOps changes are restored to the state implied by the pre-transition snapshot.

This prevents a partial transition such as granting network capability while location AppOps failed to open.

## Bootstrap

The collector emits `BootstrapUid` for existing UIDs when Nasaru starts. Existing third-party UIDs therefore receive the same foreground-only camera/microphone/location baseline as newly installed applications.

System UIDs are not automatically placed into the third-party baseline.
