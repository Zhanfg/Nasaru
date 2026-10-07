<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# M1: Application Guard and Capability Broker

M1 establishes the userspace decision core for application-layer privacy enforcement.

## Trust boundary

Applications never get to assert that they possess a capability. The Application Guard computes `active_capability` by querying the local Capability Broker. In the eventual Android adapter, foreground state, FGS type, role, Companion Device state and similar evidence must be collected from trusted OS services rather than caller-provided fields.

## Qibitu capability broker

Capabilities are:

- bound to a UID and resource;
- scoped to a purpose such as navigation, health sync or file transfer;
- origin-tagged;
- monotonic-time limited;
- renewable and revocable;
- bounded in count;
- expired lazily on events, so no periodic timer is required.

The broker intentionally uses a small bounded vector in M1. Typical active lease counts are expected to be tiny, and this avoids a permanently active cleanup worker.

## Decision order

1. Query broker state.
2. Run deterministic Policy Core.
3. If a hard allow/deny exists, return immediately and do not invoke the model.
4. Only ambiguous cases may invoke Nasaru Micro.
5. A model allow becomes a bounded Qibitu lease.
6. Model abstention defers to the Android platform rather than inventing a denial.

This keeps hard privacy invariants outside model control while preserving compatibility in uncertain cases.

## Android integration target

The Android adapter will translate trusted system state into `TrustedContext` and enforcement operations. The initial enforcement strategy is:

- camera, microphone, location: AppOps foreground baseline;
- legitimate background sessions: temporary bounded override backed by a Qibitu lease;
- files: preserve already-open descriptors/transfers while preventing unrelated new sensitive opens;
- ambiguous identity/clipboard/contact/Bluetooth cases: model or platform fallback until a stable enforcement path is implemented.

M1 does not yet claim that these Android service hooks are wired on-device. That is the next integration step.
