# Security Policy

Nasaru is security-sensitive root software and is not production-ready yet.

Do not include real user datasets, device secrets, API keys, tokens, private identifiers, or sensitive logs in public reports.

## Core invariants

- A learned model cannot override a hard security invariant.
- Peer identity is derived from trusted OS/kernel credentials, not caller claims.
- User history and adapters are local-first and are not GitHub Actions training inputs.
- Unsupported KMI modules must not be force-loaded.
- Network fast-path enforcement must not synchronously wait for model inference.

For a newly discovered vulnerability, avoid publishing exploit details before maintainers have had a reasonable opportunity to investigate and publish a fix.
