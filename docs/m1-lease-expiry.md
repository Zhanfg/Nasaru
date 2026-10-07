<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Lease expiry wakeups

`nasarud` now owns the Riksu listening socket and remains blocked while idle.

The local `SOCK_SEQPACKET` transport supports a one-shot receive timeout. Before each wait the daemon asks the Capability Broker for the nearest lease expiry and uses exactly that deadline as the blocking timeout.

When the timeout fires, the daemon expires leases and restores any AppOps override whose last active capability disappeared.

There is no periodic timer and no `while sleep` polling loop. With no active lease and no collector event, the daemon blocks indefinitely in the kernel.
