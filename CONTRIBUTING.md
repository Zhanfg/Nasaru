# Contributing

Nasaru values small, reviewable changes and explicit security reasoning.

Before adding a new policy or hook, document:

1. the resource or system boundary being protected;
2. the legitimate background behavior that must remain functional;
3. the bypass path the change closes;
4. whether the behavior belongs in a hard invariant, explicit user rule, learned adapter, or generic model;
5. the expected power and performance cost.

Do not add package-name or tracker-domain lists to the L0 core policy. Such lists, if ever supported, belong in optional higher layers.

Never commit user data, device secrets, credentials, tokens, or private telemetry.
