---
design_id: imessage-inbound-sync-recovery
title: iMessage inbound synchronization recovery
status: active
role: design_authority
summary: >
  Defines fail-closed diagnosis and bounded recovery when an exact phone reply
  is Apple-delivered but does not become visible as a new Bot Messages row.
operational_projection:
  - design/03_imessage_channel.md
  - src-tauri/src/channels/imessage.rs
  - src-tauri/src/channels/imessage_worker.rs
  - scripts/macos-setup.mjs
---

# Purpose

Close the remaining production gap observed during the v0.1.3 qualification:

```text
iPhone exact TOKEN-OPTION reply
→ Apple shows Delivered
→ dedicated Bot Messages database exposes no new inbound row
→ imsg watcher cannot correlate
→ ask_human remains pending
```

The absence of a watcher-visible row is not permission to weaken correlation or infer a result from the User's transcription. It is an explicit Bot Messages inbound-synchronization recovery state.

# New owning state

When outbound send/readiness remains valid but an expected real reply does not become visible through the Bot-owned public/basic Messages surfaces within the bounded qualification/recovery window, classify the condition as equivalent to:

```text
BOT_MESSAGES_INBOUND_SYNC_STALLED
```

This is distinct from:

```text
BOT_SESSION_LOGIN_REQUIRED
BOT_MESSAGES_ACCOUNT_UNAVAILABLE
BOT_SENDER_IDENTITY_UNVERIFIED
automation_denied
watch_failed
send_failed
```

The public/user-facing message may be compact, but diagnostics must preserve the distinction without exposing private message contents or identifiers.

# Diagnostic order

Diagnose from the Bot-user execution boundary without bypassing it:

```text
1. establish exact installed runtime / daemon / worker identity
2. prove Bot graphical/login session active
3. prove Messages account/iMessage identity still active
4. prove existing direct chat identity and outgoing service remain deterministic iMessage
5. inspect Bot-owned public/basic imsg chats/history/watch behavior through the authorized worker boundary
6. determine whether the inbound reply exists in history but was missed by watch, or is absent from Messages storage entirely
7. inspect relevant Bot-session Apple messaging processes/health using public macOS process/service surfaces
8. only then select bounded recovery
```

Do not access the Bot `chat.db` directly from the primary user, do not use sudo to bypass its privacy boundary, and do not introduce private IMCore injection.

# Bounded recovery

If evidence shows the Bot Messages session is active but synchronization is stalled, the task may perform only reversible, public recovery actions that preserve account identity and TCC, for example:

```text
restart the human-in-loop Bot worker
restart/relaunch Messages.app or its ordinary user-session service only when justified by evidence
re-establish the existing watch from a safe cursor/history boundary
revalidate the same deterministic direct iMessage chat
```

Recovery must not:

```text
sign out the Bot Apple Account
change sender/recipient identity
reset TCC
recreate the Bot user
clear/delete Messages data
modify SIP/FileVault
use private frameworks/injection
switch to SMS/MMS/RCS/carrier delivery
manufacture a result
```

If a native Apple Account reauthentication or Bot-user GUI action is genuinely required, stop at one consolidated recovery checkpoint instead of repeatedly prompting.

# Watch/history recovery semantics

A watcher failure and a Messages synchronization failure are different.

If the inbound row exists in Bot-owned history but the live watcher missed it, recovery may consume that row only when all original strict correlation predicates can still be proven from canonical evidence:

```text
same configured direct chat
strictly after original send boundary
valid numeric TOKEN-OPTION
valid option ledger
request not already terminal
incoming peer direction
GUID/reply-to constraints when present
not reaction-only / attachment-only
exactly one terminal winner
```

Do not broaden time windows or accept ambiguous historical rows.

If the inbound row does not exist in Bot Messages storage, no canonical result exists. Recovery must restore synchronization before a new qualification request is authorized.

# Resend policy

Never resend the original real decision while its delivery/result state is uncertain.

For a controlled synthetic qualification after the old request has been cancelled and cleanup is proven:

```text
old request terminally cancelled + watcher/client cleanup proven
+ synchronization recovery health proven
→ exactly one new harmless qualification request may be sent
```

A production/scientific request is never replayed automatically merely to test recovery.

# Qualification

Release acceptance requires a fresh installed-MCP real round trip after recovery:

```text
ask_human
→ exactly one explicit iMessage-only send
→ phone receives request
→ User replies exact decimal TOKEN-OPTION
→ Bot Messages exposes one post-send inbound row
→ strict correlation accepts it
→ MCP returns exactly one canonical result
→ selected_choice_id proven
→ source_channel_id = imessage
→ watcher/request/client cleanup proven
```

No result field may be inferred from phone-side text alone.

# Non-regression boundary

Do not redesign:

- decimal TOKEN-OPTION grammar;
- rich multiline decision body;
- no-SMS/iMessage-only send;
- dedicated Bot user/distinct Apple Account topology;
- MCP public surface (`ask_human`, `notify_human` only);
- terminal notification policy/link presentation;
- optional Bot automatic login;
- canonical HOME migration;
- stable signing/TCC/bootstrap.

# Design acceptance

This concern is complete when an Apple-delivered reply reliably reaches a Bot-owned public/basic Messages row and canonical MCP result, watcher-vs-storage failure is diagnostically distinguishable, bounded recovery preserves the privacy/trust boundary, and no private database bypass or weakened correlation is introduced.
