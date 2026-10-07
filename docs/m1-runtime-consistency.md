<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# M1 runtime consistency

`nasarud` uses `nasaru-runtime` as its live orchestration path.

Runtime session transitions and lease expiry are transactional with respect to the Capability Broker:

1. snapshot broker state;
2. apply the logical transition;
3. apply required AppOps changes;
4. if AppOps fails, restore broker state;
5. restore already-changed AppOps resources to the pre-transition intent.

This prevents a partial state where a Qibitu exists without its Android enforcement change, or where a lease has expired in the broker while an AppOps background override remains open.

Third-party baseline application is intentionally conservative: a partial baseline failure can only leave some sensitive operations more restricted, never more permissive.
