---
design_id: imessage-channel
title: Apple Messages channel (iMessage only)
status: active
role: design_authority
summary: >
  Defines a free iMessage-only channel backed by the external openclaw/imsg CLI, with explicit no-SMS fail-closed behavior.
operational_projection:
  - src-tauri/src/channels/imessage.rs
  - src-tauri/src/commands/
  - src/views/settings/ChannelsTab.vue
---

# Purpose

Deliver bounded AskHuman confirmations to the user's iPhone through Apple's iMessage service using the Mac's existing Messages.app account, then watch the same conversation for a strictly correlated option reply.

# Dependency boundary

Runtime dependency:

```text
openclaw/imsg
```

The application invokes documented `imsg` interfaces. `imsg` source is not vendored or copied.

Initial integration favors the smallest reliable public path:

```text
finite commands: imsg chats / history / send --json where applicable
long-lived receive: imsg watch --chat-id <id> --json
```

Do not use Advanced IMCore, SIP disabling, private framework injection, typing/read-receipt features, or other advanced bridge features.

# Setup

The user configures one existing direct iMessage conversation/recipient. Setup resolves and stores enough identity to validate both directions:

```text
recipient handle (E.164 phone number or iMessage email)
resolved direct chat id/guid
observed service = iMessage
```

The initial implementation may require the conversation to already exist in Messages.app rather than creating new chats automatically.

Required macOS permissions are those documented by `imsg` for the used features:

```text
Full Disk Access      read/watch Messages database
Automation → Messages send through Messages.app
```

# Absolute no-carrier invariant

Every direct send uses explicit iMessage selection:

```text
imsg send --to <handle> --service imessage --no-sms-fallback ...
```

`--no-sms-fallback` is retained as defense in depth even though explicit `--service imessage` already disables fallback in the inspected `imsg` behavior.

The implementation MUST NOT invoke:

```text
--service auto
--service sms
SMS fallback
carrier relay
MMS
RCS
```

The UI contains no switch that can enable these paths.

If `imsg` reports that the handle is not available via iMessage, the channel becomes unavailable for that request and sends nothing through carrier transport.

# Send semantics

For each supported request:

1. Validate configuration and `imsg` availability/version.
2. Validate that the resolved direct chat still represents the configured peer and iMessage service as far as documented local data permits.
3. Render the bounded structured text.
4. If one admitted decision image exists, stage/send it through `imsg --file`; otherwise send text only.
5. Use explicit iMessage service selection for every direct send.
6. Treat uncertain send outcomes according to `imsg`'s reported disposition; do not blindly retry a mutation with an uncertain outcome.

# Receive semantics

Maintain one watcher scoped to the configured direct chat while iMessage channel operation requires inbound answers:

```text
imsg watch --chat-id <id> --json
```

For each inbound candidate:

```text
must be incoming (not from self)
AND from configured direct chat
AND text matches exact reply grammar
AND token maps to an active request
AND option number is valid
→ accept candidate
```

Other chat traffic, reactions, images, malformed answers, stale tokens, and late answers are ignored for terminal resolution.

# Image behavior

The channel may send at most one admitted PNG/JPEG decision image from the canonical request. Sending a file must stay on the iMessage path; attachment handling may never cause SMS/MMS fallback.

Incoming images are not accepted as confirmation answers in the initial design.

# Health states

At minimum distinguish:

```text
not_configured
imsg_missing
permission_missing
messages_unavailable
recipient_not_imessage
watch_failed
send_failed
ready
```

Do not collapse `recipient_not_imessage` into a generic network failure because it is a hard safety boundary.

# Design acceptance

The iMessage channel is conforming only when static review and real macOS E2E evidence show there is no reachable code path from an AskHuman request to SMS/carrier delivery, send/watch are scoped to the configured peer, correlation rejects ambiguous replies, and a non-iMessage recipient fails closed.
