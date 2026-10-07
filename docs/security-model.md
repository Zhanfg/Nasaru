<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Security model

## Protected against

Nasaru targets unwanted behavior by ordinary Android applications and embedded SDKs, including background acquisition of sensitive resources, cross-context identity correlation, unsolicited file/media access, and application-layer telemetry.

## Not claimed

Nasaru does not claim to remain trustworthy after compromise of the kernel, boot chain, baseband, hypervisor, or an equivalent root trust domain.

## Open design

Security must not depend on hidden source code. Public algorithms are combined with private per-device state and enforcement at unavoidable system boundaries.

## Anti-bypass rule

Prefer invariant enforcement points:

- resource/service boundary for camera, microphone and location;
- file descriptor or equivalent resource boundary for file access;
- cgroup/eBPF and UID attribution for network egress;
- optional KMI-matched kernel enforcement only where userspace/eBPF cannot provide a stable boundary.

Obfuscation may reduce casual binary inspection but is not a security boundary.
