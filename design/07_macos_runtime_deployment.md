---
design_id: macos-runtime-deployment
title: macOS runtime deployment and permission stability
status: active
role: design_authority
summary: >
  Defines stable code identity, one-time onboarding/bootstrap, TCC ownership,
  setup-complete readiness, recovery states, protected-folder behavior, and
  unattended normal operation for the local macOS runtime and Bot worker.
operational_projection:
  - scripts/install.sh
  - scripts/macos-bootstrap.sh
  - src-tauri/src/commands/
  - src-tauri/src/channels/imessage.rs
  - src-tauri/src/daemon/
---

# Purpose

Make the macOS deployment usable for its actual purpose: the User may be away from the computer while Agents request decisions or report task results.

Interactive setup is permitted once where macOS or Apple genuinely requires it. After setup is complete, ordinary human-in-loop operation must not depend on repeated administrator passwords, Keychain prompts, Fast User Switching, Documents approval, Full Disk Access approval, or Automation approval.

This topic owns deployment identity and permission lifecycle. Apple Messages transport semantics, sender/recipient identity, reply correlation, and iMessage-only rules remain owned by `03_imessage_channel.md`.

# Deployment identities

The deployment separates:

```text
primary macOS user
= runs Codex / MCP / coordinator
= may own repositories, including repositories under protected folders

Bot macOS user
= dedicated Apple Messages transport user
= default documented username: human-in-loop
= owns Bot Messages.app session, Bot Apple Account, worker, imsg, Messages DB and Bot TCC grants
```

The primary local username is configuration, never hard-coded project semantics. The Bot username may also be explicitly configured when an installation needs a different dedicated local account.

# Stable runtime code identity

macOS privacy/TCC grants are part of production readiness, so the installed requester identity must remain stable across qualified rebuild/install cycles.

Ad-hoc signing is not accepted for an already-authorized production worker because ordinary rebuilds may change the requester identity seen by TCC.

The installed runtime uses:

```text
fixed signing identifier
+ stable signing identity
+ stable installed path
```

Signing identity preference:

```text
explicit Apple Development / Developer ID identity
→ otherwise machine-local human-in-loop Code Signing certificate for source/local installs
```

A machine-local certificate:

- is created once during onboarding when needed;
- remains only in the local macOS Keychain;
- is not committed/exported by the project;
- exists only to preserve a stable local code identity;
- is not represented as public notarization/distribution evidence.

The installer records/verifies the installed binary's designated requirement. A candidate that does not satisfy the accepted stable identity is rejected before replacement and reports `RUNTIME_IDENTITY_MIGRATION_REQUIRED`.

Stable shared runtime paths are outside personal protected folders, for example:

```text
/Users/Shared/human-in-loop/bin/human-in-loop
/Users/Shared/human-in-loop/bin/imsg
```

# One-time onboarding and bounded privilege

System-level preparation is concentrated into one explicit setup/bootstrap phase.

The User may authenticate administrator privileges once for a bounded allow-listed bootstrap. The project may use `sudo -v` followed only by predefined `sudo -n` operations required for setup. It must not leave an unrestricted root shell, persistent privileged helper, or broad sudo policy for Codex.

The privileged bootstrap may perform only setup-owned operations such as:

```text
shared runtime directory/group ownership
required local user/group membership
fixed install-path preparation
LaunchAgent/bootstrap-file installation where privileged ownership is genuinely required
```

Normal operation and qualified routine updates do not repeatedly ask for administrator authentication.

If new protected system state genuinely requires authorization, return one consolidated `ADMIN_AUTH_REQUIRED` recovery checkpoint rather than allowing several commands to each request a password.

# TCC and privacy ownership

Administrator authentication and macOS privacy consent are separate gates. `sudo` cannot grant or bypass TCC.

Relevant gates include:

```text
Files & Folders / Documents
Full Disk Access
Automation / Apple Events
```

Permissions belong to the exact user/process context performing the operation.

Apple Messages production context:

```text
Bot macOS user
→ stable installed Bot worker
→ stable external imsg path
→ Bot user's Messages.app
```

The Bot worker owns its Messages database access and Automation → Messages authorization. The primary coordinator does not read the Bot user's Messages database directly.

The primary runtime receives only access required for primary-user responsibilities. If repository identity resolution needs a repository under Documents, that access belongs to the stable primary runtime only. The Bot worker never receives or traverses the repository path.

# Setup state model

Production readiness is derived from health predicates, not from an unchecked “setup done” flag.

Conceptual setup states are:

```text
SETUP_NEEDS_BOT_USER
SETUP_NEEDS_BOT_LOGIN
SETUP_NEEDS_RUNTIME_BOOTSTRAP
SETUP_NEEDS_TCC_CONSENT
SETUP_NEEDS_NOTIFICATION_QUALIFICATION
SETUP_COMPLETE
RECOVERY_REQUIRED
```

The implementation may use more specific health codes, but the user-facing setup flow must remain one coherent state machine rather than a sequence of unrelated prompts.

`SETUP_COMPLETE` requires all current predicates to be true:

```text
stable runtime identity ready
shared runtime paths ready
Bot macOS user exists
Bot graphical/login session active
Bot Messages account and iMessage active
Bot sender and recipient identities verified/distinct
Bot Messages database access ready
Automation → Messages ready
explicit iMessage-only route ready
initial real notification presentation HUMAN_VERIFIED
structured reply/correlation verified
```

A stale historical success does not override current health. If one predicate later becomes false, the installation enters the owning recovery state.

# Permission bootstrap order

Grant privacy permissions only after the final stable requester identity exists.

Required order:

```text
build candidate
→ stable-sign candidate
→ verify designated requirement
→ install final shared runtime
→ launch exact Bot-user worker identity
→ perform non-mutating readiness preflight
→ ask User for only the missing TCC consent, once
→ re-check the same final requester
→ run notification/reply qualification
→ SETUP_COMPLETE
```

Never approve a temporary/debug/ad-hoc requester and replace it afterward.

# Automation preflight

Automation → Messages must be proven before the Apple Messages channel may report `bootstrap_required` or `ready`.

The exact production worker uses a non-message-producing Automation preflight in the same responsible process context used by real sends. The preferred public mechanism is the documented AppleEvents authorization path such as `AEDeterminePermissionToAutomateTarget`.

At minimum distinguish:

```text
automation_ready
automation_consent_required
automation_denied
automation_target_unavailable
```

Do not use a real iMessage as a permission probe.

# Protected-folder contract

The Bot worker and `imsg` must not traverse source repositories or ambient Documents/Desktop/Downloads paths.

The primary coordinator may access only an explicitly supplied repository path for a bounded need such as GitHub-origin identity resolution. It must not recursively scan protected personal folders for ambient projects.

After stable identity qualification, repeated Files & Folders/Documents prompts for the same intended access are a deployment regression.

# User-switch policy

Fast User Switching is an exceptional setup/recovery action, never normal operation.

A Bot-session switch is justified only for actions genuinely owned by that GUI session:

```text
first Bot Messages / Apple Account login
first iMessage activation
first Bot-session TCC consent when macOS requires UI there
post-reboot Bot login
Apple/macOS recovery that explicitly requires that session
```

After `SETUP_COMPLETE`, ordinary work stays in the primary session:

```text
Codex / MCP / coordinator
→ authenticated local IPC
→ Bot-user LaunchAgent worker
→ imsg / Messages.app
```

Normal health checks, tests, sends, replies, notifications, worker restarts, and qualified updates do not ask the User to switch accounts.

# Normal-operation zero-interaction contract

After `SETUP_COMPLETE`, ordinary operation must require:

```text
administrator password prompts    0
Keychain password prompts          0
Fast User Switching                0
new Full Disk Access prompts       0
new Automation prompts             0
new Files & Folders prompts        0
```

If one appears during ordinary operation, treat it as a deployment regression or explicit recovery condition. Do not normalize repeated host prompts as acceptable UX.

Codex shell/sandbox permission mechanics are distinct from product TCC, but maintained setup/installation should avoid causing recurring host-level prompts by changing executable identity or protected paths unnecessarily.

# Recovery model

Human interaction may recur only because actual host/account state changed, including:

```text
Mac reboot and Bot login session not re-established
Bot Messages / Apple Account signed out
User revoked a required TCC grant
stable signing identity deleted or intentionally migrated
major macOS change invalidated a valid grant
Bot/recipient identity intentionally changed
```

Each recovery state should provide one actionable instruction and should suppress repeated duplicate requests for the same unresolved condition.

A real reboot is a known lifecycle boundary: Apple Messages cannot be considered ready until the Bot login session is re-established. Sleep, lock, and ordinary Fast User Switching away from an already logged-in Bot session do not require re-onboarding.

# Build/update lifecycle

A development build does not automatically become the active authorized worker.

```text
source change
→ build/test
→ stable-sign candidate
→ verify accepted designated requirement
→ atomic install to stable path
→ restart worker
→ non-mutating health/TCC checks
```

Routine qualified updates should preserve the same accepted identity and proceed without sudo or new TCC consent once shared paths are prepared.

Moving from a machine-local signing identity to a public Developer ID/notarized distribution identity is an explicit migration. It may require one controlled reauthorization and must not be silently treated as a routine update.

# Open-source onboarding contract

The public user flow is intentionally small and honest:

```text
1. User creates a dedicated standard macOS Bot user.
2. User logs into it and signs Messages into a distinct Bot Apple Account.
3. User returns to the primary account and runs one documented human-in-loop setup/bootstrap command.
4. Setup prepares the stable runtime and reports one consolidated remaining permission checkpoint, if any.
5. User grants the required Bot-session privacy permissions once.
6. Setup performs one real notification/reply qualification.
7. Setup reports SETUP_COMPLETE with a compact readiness table.
8. Ordinary operation thereafter is unattended.
```

The setup command must not pretend Apple Account credentials can be automated or captured safely. Passwords, 2FA, signing private keys, and channel credentials remain outside committed project state.

# Readiness projection

This topic supplies deployment predicates to Apple Messages:

```text
stable_runtime_identity
bot_worker_session_active
messages_database_permission_ready
automation_permission_ready
setup_complete / recovery state
```

`bootstrap_required` means only “all non-chat prerequisites are ready but the direct iMessage chat is not yet deterministic.” Missing Automation or another deployment predicate is not bootstrap readiness.

# Design acceptance

The macOS deployment is conforming when:

```text
one bounded setup path reaches SETUP_COMPLETE
AND ordinary rebuild/install preserves the expected designated requirement
AND TCC grants are requested only for the final stable requester identity
AND Automation readiness is proven before any real send
AND normal operation needs no password, user switching, or new privacy consent
AND the Bot worker never reads source repositories/protected personal folders unnecessarily
AND repeated Documents/Automation/FDA prompts after setup are treated as defects/recovery
AND recovery states consolidate user action instead of producing repeated prompts
AND the public onboarding docs can be followed without exposing Apple/macOS credentials to the project
```
