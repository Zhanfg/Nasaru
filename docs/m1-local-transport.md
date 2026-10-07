<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# M1 local collector transport

Nasaru uses a local Unix `SOCK_SEQPACKET` channel for collector-to-daemon Riksu events.

## Why SOCK_SEQPACKET

- message boundaries are preserved;
- no IP stack is involved;
- a single collector event maps to a single Riksu frame;
- the receiver can reject oversized packets before decoding;
- Linux peer credentials are available on accepted sockets.

## Peer authentication

The daemon authenticates the process that owns the socket connection using `SO_PEERCRED`. The UID found inside an event payload is the UID **being observed**, not the identity of the collector and is never used to authenticate the sender.

The transport can additionally require an exact `SO_PEERSEC` security context once the Android SELinux domain for the production collector is fixed.

The initial production policy should be root-only unless a dedicated privileged collector UID/domain is explicitly configured.

## Filesystem safety

The listener:

- creates its private parent directory with mode `0700`;
- creates the socket with mode `0600`;
- refuses to unlink an existing non-socket path;
- only replaces a stale socket owned by the current effective UID;
- on drop, removes only the same socket inode it created.

## Replay protection

Each accepted collector connection has an independent monotonically increasing Riksu sequence. Duplicate or decreasing sequence numbers are rejected before an event reaches SignalState or CapabilityBroker.

Sequence state is committed only after the event has been decoded and applied successfully, allowing a failed backend operation to be retried without silently losing the event.

## Power behavior

The server blocks in `accept4` / `recv`. This is event-driven sleep, not process polling.
