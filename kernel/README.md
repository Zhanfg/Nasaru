# Kernel work

Kernel-side code is intentionally not required for the first userspace bootstrap.

The implementation plan is:

- Network Guard fast path: cgroup/eBPF first.
- Optional KO fallback: only for enforcement that cannot be made stable through supported Android/userspace/eBPF boundaries.
- Every KO artifact must be built against an exact supported Android/GKI/KMI target.
- Unsupported KMI means **do not load**; fall back to supported enforcement rather than force-loading an approximate module.

Kernel and eBPF source files use GPL-2.0-only.
