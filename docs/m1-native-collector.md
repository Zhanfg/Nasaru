<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# M1 native Android collector

The first production-oriented collector is a small native process intended to run in the root module domain.

## UID state

It launches:

```text
/system/bin/cmd activity watch-uids
```

and blocks on its stdout. Android's ActivityManager shell implementation registers an `IUidObserver` and emits state changes, so this path is event-driven rather than a process polling loop.

The collector accepts both current four-character AOSP process-state tokens and older compact tokens, then maps them into Nasaru's stable `UidImportance` enum. Raw Android process-state constants never enter the Riksu ABI.

## Package lifecycle

At startup the collector snapshots third-party UIDs with:

```text
/system/bin/cmd package list packages -3 -U
```

and emits `BootstrapUid` for each unique UID.

After bootstrap it blocks in Linux `inotify` on `/data/system`. A change to `packages.xml` or `packages.list` triggers a fresh third-party UID snapshot and a set difference. There is no periodic package scan.

Shared-UID packages are deduplicated by UID before events are produced.

## Transport

All source threads send normalized `SignalEvent` values into a single in-process channel. One sender owns the Riksu `SOCK_SEQPACKET` connection, which gives every event a single monotonically increasing sequence number.

## Deliberate limits

This native collector currently covers UID importance and package lifecycle. AppOps active-state observation remains a separate privileged callback source because AOSP's supported global callback is `AppOpsManager.startWatchingActive` with `WATCH_APPOPS`; AppOps itself is treated as evidence rather than a sole authorization signal.
