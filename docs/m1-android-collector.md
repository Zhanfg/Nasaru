<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# M1 Android collector

The collector is intentionally a tiny Java-only privileged system helper. It has no Activity, UI, policy engine, model, user database or network client.

## Android 17 build baseline

- compileSdk / targetSdk: 37
- Android Gradle Plugin: 9.4.0
- Gradle: 9.6.0
- JDK: 17
- minSdk: 30

## Privilege model

Global `AppOpsManager.startWatchingActive` requires `android.permission.WATCH_APPOPS`; without it Android limits observation to the collector's own UID. Nasaru therefore installs the helper as a privileged system app and supplies a matching privapp permission allowlist entry.

The collector package is `dev.nasaru.collector`.

## Transport

The collector connects with Android's public `LocalSocket(SOCKET_SEQPACKET)` API to the Linux abstract namespace socket:

```text
nasaru.collector
```

The daemon authenticates the peer using kernel socket credentials. The observed UID inside an event is never treated as sender identity.

## Sources implemented in the first Android build

### AppOps active state

The collector observes camera, record-audio, fine-location and coarse-location active state. It aggregates state by UID before emitting a Riksu event, so:

- fine + coarse location do not flap the UID-level location state;
- shared-UID packages do not produce false inactive transitions;
- virtual-device callbacks are kept distinct.

### Package lifecycle

At process start, the collector scans installed applications once and emits baseline events for unique third-party UIDs. It then dynamically registers for package add/remove broadcasts.

System and updated-system applications are excluded from the automatic third-party foreground baseline.

## Still isolated behind the normalized signal ABI

UID importance, foreground-service type and arbitrary companion-association observation require deeper system integration. They are intentionally **not faked** in this collector. The existing Rust SignalState already accepts those normalized events, so an Android system/Binder backend can be added without changing Riksu, Policy Core, Capability Broker or AppOps enforcement.

## Deployment note

The helper is designed for mounting into a system-image `priv-app` path together with a same-partition privapp permission XML. AOSP requires privileged permission allowlisting for `signature|privileged` permissions.
