<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Architecture

Nasaru has two independent first-order enforcement planes:

1. **Application Guard** controls acquisition of sensitive resources.
2. **Network Guard** controls per-UID egress.

Both consume the same capability state and event semantics. Neither depends on ad blocking, DNS lists, VPN implementations, LSPosed modules, or vendor package/domain allowlists.

## Decision order

1. Security invariant.
2. Explicit user rule.
3. Learned user adapter.
4. Generic base student.
5. Conservative compatibility fallback.

A learned model can refine ambiguous cases but cannot override a hard invariant.

## Runtime rule

The model is event-driven. There is no continuous polling loop and no requirement for a permanently active accelerator. Obvious events are handled by the deterministic fast path.

## Data ownership

Raw user events, feedback, explicit rules, device secrets and user adapters stay on-device by default. Generic training data and generic student weights may be built publicly.
