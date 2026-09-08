---
design_id: imessage-channel
title: Apple Messages channel (iMessage only)
status: active
role: design_authority
summary: >
  Defines the production distinct-account Bot topology, dedicated macOS-user
  worker boundary, identity/session health, strict request correlation,
  human-verified notification qualification, and explicit no-carrier behavior.
operational_projection:
  - src-tauri/src/channels/imessage.rs
  - src-tauri/src/commands/
  - src-tauri/src/daemon/
  - scripts/macos-bootstrap.sh
---

# Purpose

Deliver bounded confirmations and notifications as genuine incoming iMessages from a dedicated Bot Apple Account to the user's personal iPhone, then receive a strictly correlated structured reply through the Bot-owned Messages conversation.

# Production topology

```text
primary macOS user
├── personal Apple Account / personal Messages remain unchanged
├── Codex / Agent / MCP
└── human-in-loop coordinator
          ↓
    authenticated local Unix-socket IPC
          ↓
dedicated Bot macOS user (documented default: human-in-loop)
├── independent Bot Apple Account in Messages.app
├── LaunchAgent-managed Bot transport worker
└── Bot-owned openclaw/imsg
          ↓
      iMessage only
          ↓
personal iPhone / personal Apple Account
```

The production sender and recipient account identities are distinct. Same-account/self-message behavior is not production-qualified.

The primary local username and the Bot username are deployment configuration, not hard-coded product semantics.

# External dependency and public-path boundary

Runtime dependency:

```text
openclaw/imsg
```

`imsg` is installed/version-checked externally; its source is not vendored or copied.

Use only the public/basic path needed by this product:

```text
chats / history / send / watch and equivalent documented JSON surfaces
```

Do not use private IMCore injection, SIP disabling, edit/unsend/delete bridge features, carrier relay, or another Messages server.

# Cross-user ownership and IPC

The Bot macOS user owns:

```text
Messages.app session
Bot Apple Account
iMessage activation
Messages database
Bot transport configuration
Bot worker process
imsg process
Bot-user TCC grants
```

The primary coordinator does not read the Bot user's `chat.db`, directly automate Bot Messages.app, or launch ad-hoc `imsg` across UID boundaries.

The accepted local boundary is a narrow Unix-domain socket between coordinator and Bot worker. The worker authenticates the local peer using operating-system peer identity/UID-group evidence and exposes only bounded channel operations/health needed by the coordinator. It never exposes raw Messages DB access or Apple credentials.

The worker runs under the Bot user's graphical/login session through a user-session service/LaunchAgent and uses stable shared executable paths owned by the deployment contract in `07_macos_runtime_deployment.md`.

# Configuration and private identity

The channel retains only necessary local transport state, such as:

```text
dedicated Bot macOS user
Bot sender iMessage handle
personal recipient iMessage handle
resolved direct chat id/guid when available
observed service
session/permission/channel health
```

Sender/recipient handles are private runtime configuration. They are never committed into source, fixtures, examples, reports, screenshots, or logs.

The project never stores or requests Apple Account passwords, 2FA codes, trusted-phone credentials, Apple session secrets, or unrelated Messages history.

Recommended Bot privacy posture disables unrelated iCloud data surfaces unless explicitly needed; Messages in iCloud is not required for a single-Mac transport host.

# Identity readiness

Before production readiness, documented local evidence must establish:

```text
Bot sender identity verified
recipient identity verified
sender and recipient belong to distinct Apple/iMessage account topology
recipient is available through iMessage
```

If sender/recipient topology is self-addressed, health is `SELF_MESSAGE_UNSUPPORTED` before any send.

If Bot sender identity cannot be established, health is `BOT_SENDER_IDENTITY_UNVERIFIED`.

# Session lifecycle

First setup requires the User to create/log into the dedicated Bot macOS account and sign that account's Messages.app into a distinct Bot Apple Account. Apple credentials remain in normal Apple/macOS UI and profile state.

After setup:

- sleep does not require another Bot login;
- screen lock does not require another Bot login;
- switching back to the primary user leaves the Bot transport available while the Bot login session remains active;
- logout/termination of the Bot session removes Apple Messages readiness.

A real reboot is a lifecycle boundary. Until the Bot graphical/login session is re-established, health is `BOT_SESSION_LOGIN_REQUIRED`, not `ready`.

Normal-operation user switching and TCC/password recurrence are prohibited by `07_macos_runtime_deployment.md`; this channel consumes those deployment health predicates rather than re-prompting independently.

# Permission prerequisites

The Bot worker must prove the exact stable requester has:

```text
Messages database / Full Disk Access readiness
Automation → Messages readiness
```

Automation is checked with a non-message-producing preflight before any real send. Missing Automation cannot be represented as `bootstrap_required`.

The channel may enter `bootstrap_required` only after all non-chat deployment, session, identity, permission, and iMessage predicates are ready.

# Direct-conversation bootstrap

An existing deterministic direct iMessage conversation is resolved/revalidated before normal operation.

If no deterministic direct chat exists but every other prerequisite is ready, the first real structured request may bootstrap the conversation using the production mutation path:

```text
imsg send --to <recipient> --service imessage --no-sms-fallback ...
```

Before mutation, establish the normal request token and pre-send database/cursor boundary.

After a successful send, resolve the actual outgoing row and direct conversation using bounded read-only retries because Messages database visibility may be eventually consistent immediately after creating a new chat. The retry may repeat only metadata/history lookup; it must never repeat the send mutation.

Persist/publish a chat identity only when the actual outgoing row, conversation, and service are deterministic and service is iMessage.

Uncertain send outcomes are never blindly retried.

# Absolute no-carrier invariant

Every direct mutation is explicit iMessage with defense-in-depth no-SMS fallback:

```text
--service imessage
--no-sms-fallback
```

The maintained implementation must not expose or invoke:

```text
--service auto
--service sms
SMS
MMS
RCS
carrier relay
paid messaging gateway
```

After a production send, documented local evidence must establish actual outgoing service `iMessage`; otherwise fail closed.

# Send semantics

For each supported request:

1. Require `ready`, or `bootstrap_required` where only the deterministic direct chat is missing.
2. Render the bounded canonical phone surface and allocate the collision-safe token before mutation.
3. Establish the Bot-owned pre-send cursor/time boundary.
4. Dispatch one bounded request through authenticated local IPC to the Bot worker.
5. The worker performs exactly one explicit iMessage-only send mutation.
6. Resolve/confirm the outgoing row, chat, and actual service with bounded read-only recovery where needed.
7. Start/continue the request watcher only after correlation boundaries are established.
8. Treat uncertain transport outcomes conservatively; never blindly resend.

One canonical request causes at most one application send mutation unless a new canonical request is explicitly authorized after diagnosis/repair.

# Receive and correlation semantics

The Bot worker watches the resolved direct chat:

```text
imsg watch --chat-id <id> --json
```

A candidate decision reply must satisfy all applicable evidence:

```text
same configured direct chat
message strictly after request send/cursor boundary
candidate GUID differs from sent request GUID
is_from_me == false in production distinct-peer topology
text exactly matches <TOKEN> <OPTION_NUMBER>
token maps to exactly one active request
option number is valid
request is still active
not reaction-only
not attachment/image-only
reply_to_guid, when present, matches sent request GUID
```

Malformed/stale/wrong-chat/outgoing/duplicate/late/reaction/attachment candidates are ignored. Exactly one terminal answer can win. Watchers are terminated/reaped on every terminal/cancellation path.

# Same-account compatibility boundary

Same-account behavior may remain only for development/diagnostic compatibility. It cannot establish:

```text
production ready
release qualification
notification qualification
```

Production self-addressed configuration fails before mutation as `SELF_MESSAGE_UNSUPPORTED`.

# Health states

At minimum distinguish actionable states equivalent to:

```text
not_configured
BOT_SESSION_LOGIN_REQUIRED
BOT_MESSAGES_ACCOUNT_UNAVAILABLE
BOT_SENDER_IDENTITY_UNVERIFIED
SELF_MESSAGE_UNSUPPORTED
imsg_missing
permission_missing / automation_consent_required / automation_denied
messages_unavailable
bootstrap_required
recipient_not_imessage
watch_failed
send_failed
ready
```

`ready` requires all current deployment/session/identity/permission/route predicates plus a deterministic direct iMessage conversation.

`bootstrap_required` means exactly: all prerequisites are ready except a deterministic direct chat.

None of these states authorizes another transport or carrier fallback.

# Notification qualification

Release qualification must prove more than process/send success:

```text
Bot Messages identity
→ genuine incoming iMessage on personal iPhone
→ notification presentation
→ User structured reply
→ Bot worker/watch
→ strict correlation
→ canonical result
```

The User explicitly verifies lock-screen/banner notification presentation under a controlled qualification state and evidence records:

```text
notification_presentation = HUMAN_VERIFIED
```

Sound/haptic perception is optional observation, not an automated hard assertion.

The real qualification also proves actual iMessage service, one send mutation for the qualified canonical request, exactly one canonical terminal result, and watcher/request cleanup.

# Design acceptance

The channel is conforming when:

```text
Bot Messages/imsg/worker state is owned by the dedicated Bot session
AND cross-user access uses only the authenticated local IPC boundary
AND distinct sender/recipient identity is proven before send
AND self-addressed production configuration fails before mutation
AND missing Bot session/permissions have explicit actionable health states
AND no SMS/MMS/RCS/carrier/paid-gateway path is reachable
AND Automation is proven before mutation
AND first-chat bootstrap uses one send plus bounded read-only resolution recovery
AND actual outgoing service is proven iMessage
AND strict reply correlation accepts exactly one terminal result
AND watcher/request processes clean up on terminal/cancellation paths
AND release qualification includes HUMAN_VERIFIED incoming notification presentation
AND normal post-setup operation requires no recurring local permission interaction
```
