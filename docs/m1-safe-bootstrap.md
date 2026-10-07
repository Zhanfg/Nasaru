<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Safe bootstrap baseline

Nasaru does not immediately force every already-installed UID into foreground-only AppOps when the module starts.

At startup, `BootstrapUid` marks an existing third-party UID as pending. The baseline is applied only after the collector observes a safe lifecycle point:

- the UID becomes foreground;
- the UID becomes visible; or
- the UID is gone.

Foreground/visible is safe because foreground-only AppOps still permits the active user task. Gone is safe because no process is using the resource.

Newly installed third-party UIDs still receive the baseline immediately.

This avoids a startup race where Nasaru is enabled during an already-running navigation, call, upload, or device synchronization session before callback sources have reconstructed enough context.
