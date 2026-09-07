---
design_id: imessage-channel
title: Apple Messages channel (iMessage only)
status: active
role: design_authority
summary: >
  Defines a free iMessage-only channel backed by the external openclaw/imsg CLI,
  including distinct-peer and same-Apple-Account operation with strict request
  correlation, compact notification presentation, and explicit no-SMS fail-closed behavior.
operational_projection:
  - src-tauri/src/channels/imessage.rs
  - src-tauri/src/commands/
  - src/views/settings/ChannelsTab.vue
---

# Purpose

Deliver bounded AskHuman confirmations to the user's iPhone through Apple's iMessage service using the Mac's existing Messages.app account, then watch the same conversation for a strictly correlated option reply.

The supported topology includes both:

```text
distinct_peer
same_account
```

`same_account` means the Mac and iPhone use the same Apple Account / iMessage identity. This is a first-class supported topology; a second Apple Account is not required.

The channel does not claim device-level provenance. In `same_account` mode, the trust decision is that a new message in the configured user-controlled iMessage conversation satisfies the strict post-send request-correlation contract. The public Messages database does not provide a reliable basis for asserting that a correlated message physically originated on the iPhone rather than another trusted device on the same Apple Account.

# Dependency boundary

Runtime dependency:

```text
openclaw/imsg
```

The application invokes documented `imsg` interfaces. `imsg` source is not vendored or copied.

Use the smallest reliable public path:

```text
finite commands: imsg chats / history / send --json where applicable
long-lived receive: imsg watch --chat-id <id> --json
```

Do not use Advanced IMCore, SIP disabling, private framework injection, typing/read-receipt features, edit/unsend/delete bridge features, or other advanced bridge features.

# Configuration and identity mode

One iMessage destination is configured locally with:

```text
recipient handle: E.164 phone number or iMessage email
identity mode: distinct_peer | same_account
resolved direct chat id/guid when available
observed service = iMessage when available
```

The recipient is private runtime configuration. It must not be committed into repository source, fixtures, examples, reports, screenshots, or logs.

For an already existing direct conversation, setup resolves and stores the direct chat identity before normal operation.

For `same_account`, the recipient may be one of the user's own iMessage handles. The absence of an existing direct conversation is not itself an error. First use may bootstrap that direct conversation under the bounded rules below.

Required macOS permissions are those documented by `imsg` for the used features:

```text
Full Disk Access      read/watch Messages database
Automation → Messages send through Messages.app
```

# First-use bootstrap

When no deterministic existing direct iMessage chat can be resolved, the channel may bootstrap only if all are true:

```text
recipient was explicitly configured/approved by the user
AND identity mode is known
AND imsg is available
AND required local database access is available
AND the canonical request is supported by the iMessage renderer
```

Bootstrap is not a separate probe message. The first real structured confirmation is sent directly to the configured handle using the same production mutation path:

```text
imsg send --to <handle> --service imessage --no-sms-fallback ...
```

Before dispatch, create the normal request token and establish a pre-send database/cursor boundary. After a successful iMessage mutation, resolve the resulting direct conversation and the actual outgoing request row from documented local data. Publish/persist the chat identity only when resolution is deterministic and the service is iMessage.

The post-bootstrap state must establish enough request evidence for correlation, including where available:

```text
chat_id / chat_guid
sent message row id
sent message guid
request token
send-time/cursor boundary
```

If the mutation is reported as not started, the target cannot be used as iMessage, the resulting chat cannot be uniquely resolved, or service identity is inconsistent, fail closed. An uncertain mutation outcome must not be blindly retried.

# Absolute no-carrier invariant

Every direct send uses explicit iMessage selection:

```text
imsg send --to <handle> --service imessage --no-sms-fallback ...
```

`--no-sms-fallback` is retained as defense in depth even though explicit `--service imessage` disables fallback in the inspected `imsg` behavior.

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

1. Validate configuration, identity mode, and `imsg` availability/version.
2. If a resolved direct chat exists, validate that it still represents the configured destination and iMessage service as far as documented local data permits.
3. Render the compact bounded structured notification defined by `01_interaction_protocol.md` and allocate the collision-safe request token before mutation.
4. Establish a pre-send cursor/time boundary.
5. If one admitted decision image exists, stage/send it through the permitted iMessage file path; otherwise send text only.
6. Use explicit iMessage service selection for every direct send.
7. Confirm or resolve the actual outgoing request row/chat after send when local database evidence is available.
8. Treat uncertain send outcomes according to `imsg`'s reported disposition; do not blindly retry a mutation with an uncertain outcome.
9. One canonical request causes at most one application send mutation unless the caller initiates a new canonical request; platform synchronization duplicates are not application retries.

# Receive semantics

Maintain one watcher scoped to the resolved direct chat while iMessage channel operation requires an answer:

```text
imsg watch --chat-id <id> --json
```

The watcher must begin from a post-send boundary that prevents the outgoing request row and older history from being accepted as a reply. History/cursor recovery may be used so that a fast reply occurring between send confirmation and watcher startup is not lost.

For all modes, an answer candidate must satisfy:

```text
same configured direct chat
AND message is strictly after the request send/cursor boundary
AND message guid differs from the sent request guid when both are available
AND text exactly matches <TOKEN> <OPTION_NUMBER>
AND token maps to exactly one active request
AND option number is valid
AND request has not terminated
AND message is not a reaction-only or attachment/image-only answer
```

If `reply_to_guid` is present on the candidate, it must equal the sent request message guid. An inline reply therefore provides additional correlation evidence but is not mandatory for normal use.

Mode-specific authorship rule:

```text
distinct_peer:
  is_from_me must be false

same_account:
  is_from_me may be true or false and is not used as the decisive human/device identity test
```

In `same_account` mode, accepting `is_from_me=true` is safe only because the complete strict post-send correlation contract above remains mandatory. Do not weaken the token, chat, cursor, request-state, reaction/attachment, or option checks to compensate for same-account synchronization.

Other chat traffic, malformed answers, stale tokens, wrong-chat messages, pre-send history, duplicate/late replies, reactions, and images are ignored for terminal resolution. Exactly one terminal answer may win.

# Same-account synchronization and presentation

A self-addressed iMessage under one Apple Account may be represented by Apple Messages as synchronized sender/recipient-visible copies across the user's devices. The application cannot reliably force Apple Messages to display one native bubble only while preserving the public, SIP-intact transport boundary.

This behavior is treated as a **presentation limitation**, not as a duplicate-send condition, provided application evidence shows one canonical request produced exactly one `imsg send` mutation.

The supported mitigation is the compact renderer in `01_interaction_protocol.md`.

The application MUST NOT attempt visual deduplication through:

```text
message delete/unsend after delivery
private IMCore bridge calls
dylib injection
SIP disabling
direct mutation of chat.db
disabling or altering the user's Messages/iCloud synchronization settings
```

The user keeps normal Apple Account and Messages synchronization behavior. Product correctness is defined by one application send and one accepted terminal result, not by the number of bubbles Apple chooses to display for a same-account self-addressed message.

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
bootstrap_required
recipient_not_imessage
watch_failed
send_failed
ready
```

`bootstrap_required` means a user-approved recipient and identity mode are configured but no deterministic direct chat exists yet. It is a valid first-use state, not authorization to use another transport.

Do not collapse `recipient_not_imessage` into a generic network failure because it is a hard safety boundary.

# Design acceptance

The iMessage channel is conforming only when static review and real macOS E2E evidence show:

```text
no reachable SMS/carrier delivery path
first-use bootstrap remains explicit iMessage-only and fail-closed
resolved send/watch remain scoped to one configured direct conversation
strict post-send correlation rejects ambiguous/stale/wrong-chat replies
same_account works without requiring a second Apple Account
self-authored synchronization cannot make the outgoing request itself resolve as an answer
one canonical request causes exactly one application send mutation
compact rendering keeps same-account duplicate presentation bounded and decision-readable
no delete/unsend/private-bridge workaround is introduced
exactly one terminal answer is accepted
watcher processes are terminated/reaped on every terminal path
```

A non-iMessage recipient must fail closed without intentionally sending an SMS/MMS/RCS negative test.
