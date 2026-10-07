# Nasaru

**Adaptive Privacy Substrate for Android**

Nasaru is an open, local-first privacy substrate for Android. Its goal is to stop data access and egress that lack a legitimate user-intent context while preserving legitimate background work such as device synchronization, navigation, media playback, and user-initiated transfers.

> Status: early architecture and executable core bootstrap. Do not treat the current tree as production-ready enforcement.

## Design principles

- **Open algorithm, private state, hardened enforcement.**
- **Rules first, model only for ambiguity.** Deterministic invariants decide obvious cases; the micro model is advisory and cannot override hard safety rules.
- **Two independent L0 planes.** Application Guard governs sensitive resource access; Network Guard governs per-UID egress.
- **No private user data in CI.** GitHub Actions trains only generic student models. User feedback, raw history, adapters, and device secrets stay on-device.
- **Low maintenance.** Event-driven operation, no polling loops, no dependency on ad-blocking lists or package/domain allowlists for core safety.
- **Fail closed on security invariants, fail compatible on unsupported optional enforcement.**

## Names

- **Nasaru** — project and privacy substrate.
- **Riksu** — local control protocol.
- **Qibitu** — bounded capability lease/decision record.
- **Nasaru Micro** — tiny structured decision runtime distilled from a larger teacher model.

## Repository layout

- `crates/riksu` — Riksu/1 binary protocol primitives.
- `crates/policy` — deterministic privacy policy core.
- `crates/nasaru-daemon` — minimal userspace daemon bootstrap.
- `schemas` — stable versioned event/decision schemas.
- `training` — generic privacy student training/evaluation pipeline.
- `kernel/ebpf` — Network Guard eBPF work.
- `kernel/ko` — optional KMI-matched kernel fallback work.
- `docs` — architecture, security model and protocol specification.

## Licensing

Userspace code is intended to use **GPL-3.0-or-later**. Kernel/eBPF code uses **GPL-2.0-only**. Riksu protocol documentation uses **CC-BY-SA-4.0**. See `LICENSES/README.md` for the repository licensing map.

## Security boundary

Nasaru is designed to strongly constrain ordinary Android applications, embedded SDKs and application-layer telemetry. It does not claim to defend against an adversary that already controls the kernel, baseband, boot chain, or an equivalent root trust level.
