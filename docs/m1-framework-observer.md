<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# M1 framework observer

Nasaru uses one small `app_process` helper as a compatibility bridge for framework callbacks that do not have stable command-line event streams.

It observes AppOps active state for camera, microphone, fine/coarse location and foreground-service type changes from `IProcessObserver.onForegroundServicesChanged`. Both are callbacks; there is no periodic `dumpsys` or AppOps polling.

The helper writes only normalized metadata to stdout:

```text
NASARU_READY
NASARU_APPOP<TAB>uid<TAB>CAMERA|MICROPHONE|FINE_LOCATION|COARSE_LOCATION<TAB>0|1
NASARU_FGS<TAB>uid<TAB>stable_bitmask
```

The native collector remains the only Riksu sender. Android FGS flags are translated to Nasaru's stable seven-bit mask inside the helper.

`IProcessObserver` is hidden API. A compile-only shape is used for javac and is deliberately excluded from the output DEX; at runtime the reference resolves to Android's boot-classpath interface. Reflection and hidden framework coupling stay isolated to this helper.

If the helper fails on an OEM build, the native UID/package collector continues. This fails privacy-conservatively: fewer background sessions can be promoted, but no new sensitive access is opened.
