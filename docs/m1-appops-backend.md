<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# M1 AppOps backend strategy

Nasaru keeps Android-version-specific enforcement behind the `AppOpsBackend` interface.

## Primary rule

Policy and capability state are keyed by UID. Nasaru must not mix per-package AppOps modes and per-UID AppOps modes for the same operation.

## Compatibility backend

The first executable compatibility backend invokes Android's service command directly:

```text
/system/bin/cmd appops set <UID> <OP> <MODE>
```

It uses `exec`-style argument passing through `std::process::Command`; it does not invoke a shell and therefore does not perform shell interpolation.

Mode values are emitted numerically:

- `0` = allowed
- `1` = ignored
- `4` = foreground-only

The compatibility backend is intentionally limited to infrequent lifecycle changes such as module initialization, capability grant/revoke, and lease expiry. It is not part of per-access or packet fast paths.

## Future Binder backend

A direct Binder implementation can replace this backend without changing:

- Policy Core
- Capability Broker
- Application Guard
- Riksu ABI
- trusted-session derivation

This separation is deliberate: hidden or vendor-specific Binder details must not become part of the stable Nasaru core.
