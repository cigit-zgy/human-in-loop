---
design_id: macos-runtime-deployment
title: macOS runtime deployment and permission stability
status: active
role: design_authority
summary: >
  Defines stable code identity, bounded privileged bootstrap, TCC ownership,
  protected-folder behavior, and minimal user switching for the local macOS
  human-in-loop runtime and dedicated iMessage Bot worker.
operational_projection:
  - scripts/install.sh
  - src-tauri/src/commands/
  - src-tauri/src/channels/imessage.rs
  - src-tauri/src/daemon/
---

# Purpose

Make the local macOS deployment repeatable without asking the User to re-enter an administrator password, re-grant Documents access, or re-approve Automation after ordinary human-in-loop rebuilds and reinstalls.

This topic owns local deployment identity and permission lifecycle. Apple Messages transport semantics, sender/recipient identity, reply correlation, and iMessage-only rules remain owned by `03_imessage_channel.md`.

# Stable runtime code identity

macOS privacy/TCC grants are part of production readiness, so the runtime code identity must remain stable across qualified rebuild/install cycles.

Ad-hoc signing is not accepted for the installed production Bot worker after privacy grants have been established because its code identity may change with each build.

The installed runtime uses a fixed signing identifier and a stable local code-signing identity.

For this single-machine deployment, signing identity preference is:

```text
explicitly configured Apple Development / Developer ID identity
→ otherwise machine-local human-in-loop Code Signing certificate
```

A machine-local certificate:

- is created once;
- is stored only in the local macOS Keychain;
- is not committed or exported by the project;
- is used only to establish stable local code identity;
- is not represented as public distribution/notarization evidence.

The installer records/verifies the installed binary's designated requirement. Before replacing an already-authorized runtime, the candidate must satisfy the expected stable identity. If the expected identity changes, installation fails closed and requires an explicit permission-migration checkpoint rather than silently replacing the requester.

The production worker executable path is stable and outside protected personal folders:

```text
/Users/Shared/human-in-loop/bin/human-in-loop
```

The installed `imsg` path used by the worker is likewise stable and explicit.

# One-time bounded privileged bootstrap

Administrator authentication and macOS TCC consent are separate concerns.

System-level changes are concentrated into one bounded bootstrap operation. The User authenticates administrator privileges once for that operation. The implementation may validate a sudo timestamp or invoke one allow-listed privileged helper, but it must not leave an unrestricted root shell or broad long-lived privileged agent for Codex.

The privileged bootstrap may perform only task-owned system setup such as:

```text
shared runtime directory/group ownership
required local user/group membership
fixed install-path preparation
LaunchAgent/bootstrap files when system ownership requires it
```

Normal runtime, health checks, message sends, MCP calls, deterministic tests, and routine qualified binary replacement do not require repeated administrator-password prompts unless they genuinely change protected system state.

If an operation cannot proceed without new administrator authorization, it reports one explicit `ADMIN_AUTH_REQUIRED` checkpoint rather than repeatedly invoking password prompts from separate commands.

# TCC and privacy ownership

Full Disk Access, Files & Folders, and Automation/Apple Events are independent macOS consent gates. `sudo` does not satisfy or bypass them.

Permissions belong to the user/process context that performs the protected operation.

For Apple Messages production transport:

```text
macOS user      = human-in-loop
Messages.app    = Bot user's Messages session
worker          = stable installed human-in-loop identity
imsg            = stable explicit installed path
```

The Bot user owns its Messages database access and Automation → Messages authorization. The primary user does not read the Bot Messages database directly.

The primary `Guangyao Zhao` runtime receives only permissions required for its own operations. It is not granted Bot-user Full Disk Access merely for convenience.

# Permission bootstrap order

Permission qualification occurs only after the final stable installed identity exists.

Required order:

```text
build candidate
→ sign with stable identity
→ verify designated requirement
→ install final shared binary
→ start exact Bot-user worker identity
→ perform non-mutating permission preflight
→ User grants any missing TCC consent once
→ re-check same final identity
→ mark permission predicates ready
→ only then permit a real message mutation
```

Do not grant Automation to a temporary/debug/ad-hoc binary and then replace that binary before release E2E.

# Automation preflight

Automation → Messages must be proven before the Apple Messages channel may report `bootstrap_required` or `ready`.

The worker exposes/uses a non-message-producing Automation preflight through the same responsible process context used for production sends. The preferred implementation uses the documented AppleEvents authorization mechanism, such as `AEDeterminePermissionToAutomateTarget`, with explicit control over whether the User should be prompted.

The preflight must distinguish at least:

```text
automation_ready
automation_consent_required
automation_denied
automation_target_unavailable
```

A missing/denied Automation grant becomes an actionable permission health state and prevents a real send. Do not use a real iMessage as a permission probe.

# Protected-folder behavior

The Bot worker and `imsg` runtime must not traverse the source repository or ambient protected folders such as Documents, Desktop, or Downloads.

The primary coordinator may access an explicitly supplied repository path only for the bounded operation that requires it, such as canonical GitHub repository identity resolution. It does not recursively scan protected personal folders for ambient project discovery.

If the primary runtime needs access to Documents because the User's repository is located there, grant that access once to the stable runtime identity. Repeated Files & Folders prompts after the stable identity is established are a deployment defect.

# User-switch policy

Fast User Switching is an exceptional setup/recovery action, not a normal operating mechanism.

User switching is required only when the action genuinely belongs to the Bot graphical session:

```text
first Bot Apple Account / Messages login
first iMessage activation
first macOS privacy consent that requires Bot-user UI
post-reboot Bot login
Apple/macOS account or TCC recovery that explicitly requires that session
```

Once the Bot graphical session is established and permissions are qualified, normal operation remains in the primary `Guangyao Zhao` session:

```text
Codex / MCP / coordinator
→ authenticated local IPC
→ Bot-user LaunchAgent worker
→ imsg / Messages.app
```

Do not ask the User to switch accounts for ordinary install checks, worker health, log inspection, deterministic tests, sends, or reply handling when the worker/IPC boundary can perform them.

# Build and update lifecycle

A development rebuild does not automatically become the active production worker.

```text
source change
→ build/test
→ stable-sign candidate
→ verify expected designated requirement
→ install atomically to stable path
→ restart worker
→ non-mutating TCC readiness check
```

If the designated requirement no longer matches the expected identity, stop before replacing the active worker and report `RUNTIME_IDENTITY_MIGRATION_REQUIRED`.

A future public binary release may use Developer ID/notarization. Moving from a machine-local signing identity to a public distribution identity is an explicit migration that may require a one-time TCC reauthorization; it is not silently treated as an ordinary update.

# Health/readiness projection

This topic supplies deployment predicates to the Apple Messages channel:

```text
stable_runtime_identity
bot_worker_session_active
messages_database_permission_ready
automation_permission_ready
```

The iMessage channel may enter `bootstrap_required` only after all non-chat deployment predicates are ready. Absence of a deterministic chat is the only reason for `bootstrap_required`; missing Automation is a permission failure, not bootstrap readiness.

# Design acceptance

The macOS deployment is conforming when:

```text
ordinary rebuild/install does not change the expected runtime designated requirement
TCC grants are requested only for the final installed requester identity
Automation readiness is checked before any real send
one bootstrap administration authentication covers bounded system setup without a persistent root shell
normal operation does not require repeated user switching
Bot worker never reads source repositories or protected personal folders unnecessarily
repeated Documents or Automation prompts after stable identity qualification are treated as defects
real Apple Messages E2E begins only after the exact worker identity is permission-ready
```
