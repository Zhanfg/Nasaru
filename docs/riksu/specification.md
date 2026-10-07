<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Riksu/1

Riksu is Nasaru's local control protocol. It is deliberately small, versioned, binary, and designed for local Unix-domain transport plus a separate shared fast plane.

## Transport

Control plane: `AF_UNIX` + `SOCK_SEQPACKET` where available.

Fast plane: kernel/eBPF maps and bounded shared state. Packet-path decisions must never block on model IPC.

## Header

Riksu/1 uses a fixed 32-byte little-endian header:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | magic = `RKSU` |
| 4 | 1 | major |
| 5 | 1 | minor |
| 6 | 2 | message type |
| 8 | 4 | flags |
| 12 | 4 | payload length |
| 16 | 8 | sequence |
| 24 | 8 | monotonic timestamp ns |

Payloads are TLV encoded. Unknown TLVs must be skippable by older implementations.

## Message types

- `0x0001` HELLO
- `0x0002` STATUS
- `0x0010` EVENT
- `0x0011` DECISION
- `0x0020` QIBITU_GRANT
- `0x0021` QIBITU_REVOKE
- `0x0022` QIBITU_EXPIRE
- `0x0030` POLICY_SNAPSHOT
- `0x0031` POLICY_DELTA
- `0x0040` FEEDBACK
- `0x0050` MODEL_INFO
- `0x0060` AUDIT_EVENT

## Peer identity

A peer-supplied UID/PID is never authoritative. Implementations must bind identity to OS-provided peer credentials and security context where available.

## Qibitu

A Qibitu is a bounded capability lease. It names a subject, resource, action, scope, issuance time and TTL. Long-running legitimate work renews its lease; a past grant is never an unlimited future authorization.
