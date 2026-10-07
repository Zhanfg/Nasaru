<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Signal wire compatibility

Signal events are carried as Riksu EVENT frames. Event-kind numeric values are append-only within Riksu/1.

Current assignments:

| Kind | Event |
|---:|---|
| 1 | UID importance |
| 2 | foreground-service types |
| 3 | AppOps active state |
| 4 | companion presence |
| 5 | package added |
| 6 | package removed |
| 7 | bootstrap existing UID |

Adding `BootstrapUid` used a new event kind rather than renumbering package lifecycle events. Older decoders therefore fail closed with an unknown event kind instead of silently interpreting a bootstrap event as another lifecycle action.
