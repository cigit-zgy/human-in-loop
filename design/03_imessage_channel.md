---
design_id: imessage-channel
title: Apple Messages channel (iMessage only)
status: active
role: design_authority
summary: >
  Defines the dedicated Bot Apple Account and macOS-user transport boundary,
  lifecycle and health states, strict request correlation, human-verified
  notification qualification, and explicit no-SMS fail-closed behavior.
operational_projection:
  - src-tauri/src/channels/imessage.rs
  - src-tauri/src/commands/
  - src/views/settings/ChannelsTab.vue
---

# Purpose

Deliver bounded AskHuman confirmations and notifications as genuine incoming iMessages from a dedicated Bot Apple Account to the user's personal iPhone, then watch the Bot-owned direct conversation for a strictly correlated option reply.

The production-qualified topology is:

```text
Guangyao Zhao macOS user
├── personal Apple Account and personal Messages.app remain unchanged
├── Codex / Agent
└── main human-in-loop coordinator
          ↓
    narrow authenticated local IPC
          ↓
human-in-loop macOS user
├── independent Bot Apple Account in Bot Messages.app
├── Bot transport worker
└── Bot-owned imsg process
          ↓
      iMessage only
          ↓
personal iPhone / personal Apple Account
```

The distinct sender/recipient account requirement is the whole-system invariant defined in `00_overview.md`. This topic owns how Apple Messages establishes that identity evidence, becomes ready, and fails closed when the evidence is absent or self-addressed.

# Dependency and ownership boundary

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

The Bot transport worker, `imsg`, Messages.app, Messages database, Apple session, and Apple-channel TCC grants belong to the `human-in-loop` macOS user. The main coordinator under `Guangyao Zhao` does not directly read the Bot user's `chat.db`, launch the Bot user's `imsg`, or automate the Bot user's Messages.app across UID boundaries.

The future runtime boundary is:

```text
main coordinator
→ narrow authenticated local IPC
→ Bot transport worker
→ imsg
→ Messages.app
```

The Bot worker is expected to be managed by a `human-in-loop` user-session service such as a LaunchAgent. This design freezes only ownership and authentication boundaries; it does not select the IPC protocol or daemon/LaunchAgent implementation.

# Configuration and identity evidence

The Apple Messages channel is configured locally with only the transport state it needs:

```text
bot macOS user = human-in-loop
Bot sender handle
recipient handle: E.164 phone number or iMessage email
resolved direct chat id/guid when available
observed service = iMessage when available
Bot session and channel health state
```

Bot and recipient handles are private runtime configuration. They must not be committed into repository source, fixtures, examples, reports, screenshots, or logs.

Before production readiness, documented local evidence must verify both the Bot sender identity and recipient identity and establish that they belong to distinct Apple/iMessage account topologies. Inability to verify either identity fails closed. A self-addressed topology becomes `SELF_MESSAGE_UNSUPPORTED` before any send.

The Bot Apple Account is a real Apple Account already owned by the User and dedicated to human-in-loop use. The project never stores or requests Apple Account passwords, 2FA codes, trusted-phone credentials, Apple session secrets, or unrelated Messages history.

Recommended privacy settings for the Bot account are:

```text
Photos sync          OFF
Contacts sync        OFF unless explicitly required
iCloud Drive         OFF
Keychain sync        OFF
Calendar             OFF
other personal data  OFF
```

Messages in iCloud is not a human-in-loop requirement for a single-Mac Bot transport host.

# First-time setup and session lifecycle

First deployment requires the User to perform normal macOS and Apple setup:

1. Create or confirm the dedicated `human-in-loop` macOS user.
2. Log in to that macOS user.
3. Sign the Bot Messages.app into the independent Bot Apple Account.
4. Confirm iMessage activation.
5. Grant the Bot runtime/`imsg` Full Disk Access and Automation → Messages through normal macOS controls.
6. Fast User Switch back to `Guangyao Zhao`.

Apple Account and Messages credentials remain in the normal macOS/Apple profile. The human-in-loop project does not copy or manage them.

After first setup:

- sleep does not require another Bot login;
- screen lock does not require another Bot login;
- Fast User Switching may leave the Bot transport available while the `human-in-loop` graphical session remains logged in;
- loss or termination of that user session removes Apple Messages readiness.

A real reboot is a hard lifecycle boundary. Saved Apple credentials do not imply that the Bot graphical/login session has been re-established. Until the User logs in once to `human-in-loop` after restart, the channel is `BOT_SESSION_LOGIN_REQUIRED`, not `ready`.

The user-facing recovery instruction is explicit:

```text
Apple Messages requires the dedicated “human-in-loop” macOS user to be logged in once after restart.
Log in to “human-in-loop”, then Fast User Switch back to “Guangyao Zhao”.
```

Do not ask for the Apple Account password again unless Messages or Apple itself reports that its saved account/session is no longer valid.

# Direct-conversation bootstrap

For an already existing direct conversation, setup resolves and stores the Bot-owned direct chat identity before normal operation.

When no deterministic existing direct iMessage chat can be resolved, the channel may enter `bootstrap_required` only if all are true:

```text
recipient was explicitly configured/approved by the User
AND Bot sender and recipient identities are verified and distinct
AND the human-in-loop Bot session is active
AND Bot Messages account and iMessage activation are available
AND imsg is available
AND required local database access is available
AND the canonical request is supported by the iMessage renderer
```

Bootstrap is not a separate probe message. The first real structured confirmation is sent directly from the Bot worker to the configured handle using the production mutation path:

```text
imsg send --to <handle> --service imessage --no-sms-fallback ...
```

Before dispatch, create the normal request token and establish a pre-send database/cursor boundary. After a successful iMessage mutation, resolve the resulting direct conversation and actual outgoing request row from documented local data. Publish/persist the chat identity only when resolution is deterministic and the service is iMessage.

The post-bootstrap state establishes enough request evidence for correlation, including where available:

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

The implementation MUST NOT invoke or expose:

```text
--service auto
--service sms
SMS fallback
carrier relay
MMS
RCS
paid messaging gateway
```

The UI contains no switch that can enable these paths. If the handle is unavailable via iMessage, the channel becomes unavailable and sends nothing through carrier transport.

After each production send, documented local evidence must establish that the actual outgoing service was iMessage. If the service cannot be proven, fail closed.

# Send semantics

For each supported request:

1. Require either `ready` or the explicitly admitted `bootstrap_required` path. Both require the active Bot session, Bot Messages account, verified distinct identities, `imsg`, permissions, and iMessage-only eligibility; only bootstrap may begin before a deterministic direct conversation exists.
2. Render the compact bounded surface defined by `01_interaction_protocol.md` and allocate the collision-safe request token before mutation.
3. Establish a pre-send cursor/time boundary within the Bot-owned Messages context.
4. If one admitted decision image exists, stage/send it through the permitted iMessage file path; otherwise send text only.
5. Dispatch through the authenticated local IPC to the Bot worker, which uses explicit iMessage service selection.
6. Confirm or resolve the actual outgoing request row/chat and actual iMessage service from documented local evidence.
7. Treat uncertain send outcomes according to `imsg`'s reported disposition; do not blindly retry a mutation with an uncertain outcome.
8. One canonical request causes at most one application send mutation unless the caller initiates a new canonical request.

# Receive semantics

The Bot worker maintains one watcher scoped to the resolved direct chat while a confirmation requires an answer:

```text
imsg watch --chat-id <id> --json
```

The watcher begins from a post-send boundary that prevents the outgoing request row and older history from being accepted as a reply. History/cursor recovery may be used so that a fast reply occurring between send confirmation and watcher startup is not lost.

An answer candidate must satisfy:

```text
same configured direct chat
AND message is strictly after the request send/cursor boundary
AND message guid differs from the sent request guid when both are available
AND is_from_me is false for the production distinct-peer topology
AND text exactly matches <TOKEN> <OPTION_NUMBER>
AND token maps to exactly one active request
AND option number is valid
AND request has not terminated
AND message is not a reaction-only or attachment/image-only answer
```

If `reply_to_guid` is present on the candidate, it must equal the sent request message guid. An inline reply therefore provides additional correlation evidence but is not mandatory for normal use.

Other chat traffic, malformed answers, stale tokens, wrong-chat messages, pre-send history, duplicate/late replies, reactions, and images are ignored for terminal resolution. Exactly one terminal answer may win. Watchers are terminated and reaped on every terminal or cancellation path.

# Same-account compatibility boundary

Same-account self-message handling may remain as development or diagnostic compatibility so existing local correlation probes are not misrepresented as production evidence. It never establishes production `ready`, release qualification, or notification qualification.

When sender and recipient resolve to the same Apple/iMessage account topology, production health is `SELF_MESSAGE_UNSUPPORTED`; the channel explains that a dedicated Bot Apple Account is required and sends nothing. It does not wait until after a message mutation to discover the unsupported topology.

Any retained diagnostic same-account path keeps the complete post-send chat/cursor/GUID/token/option/active-request/reaction/attachment correlation contract, including protection against the outgoing request resolving itself. Diagnostic compatibility does not weaken the production distinct-peer requirement that `is_from_me` be false.

# Image behavior

The channel may send at most one admitted PNG/JPEG decision image from the canonical request. Sending a file stays on the iMessage path; attachment handling may never cause SMS/MMS fallback.

Incoming images are not accepted as confirmation answers in the initial design.

# Health states and readiness

At minimum distinguish:

```text
not_configured
BOT_SESSION_LOGIN_REQUIRED
BOT_MESSAGES_ACCOUNT_UNAVAILABLE
BOT_SENDER_IDENTITY_UNVERIFIED
SELF_MESSAGE_UNSUPPORTED
imsg_missing
permission_missing
messages_unavailable
bootstrap_required
recipient_not_imessage
watch_failed
send_failed
ready
```

State meanings specific to the production identity/runtime topology are:

- `BOT_SESSION_LOGIN_REQUIRED`: the Bot account/profile is configured, but the dedicated `human-in-loop` graphical/login session has not been established after reboot or is no longer active.
- `BOT_MESSAGES_ACCOUNT_UNAVAILABLE`: the Bot user session exists, but its Messages account or iMessage activation is unavailable.
- `BOT_SENDER_IDENTITY_UNVERIFIED`: the active Bot sender identity cannot be established from documented local evidence.
- `SELF_MESSAGE_UNSUPPORTED`: verified sender and recipient identities belong to the same Apple/iMessage account topology, which is not a production-supported route.
- `bootstrap_required`: identities and prerequisites are valid, but no deterministic direct chat exists yet; only the first real structured confirmation may bootstrap it through the iMessage-only production path.

Apple Messages is `ready` only when all of these predicates hold:

```text
bot_macos_user == human-in-loop
AND bot_user_session_active
AND bot_messages_account_active
AND bot_sender_identity_verified
AND recipient_identity_verified
AND distinct sender/recipient account evidence satisfies the whole-system invariant
AND direct conversation resolves deterministically
AND service == iMessage
AND imsg available
AND required permissions available
```

`recipient_not_imessage` remains a hard safety boundary rather than a generic network error. None of the health states authorizes another transport or carrier fallback.

# Production notification qualification

A real Apple Messages release E2E must prove the complete production topology, not only `imsg` process success or a row appearing in a chat:

```text
Bot Messages identity
→ personal iPhone
→ genuine incoming iMessage
→ iPhone notification presentation
→ User reply
→ Bot imsg watch
→ strict correlation
→ canonical choice
```

The test includes the `HUMAN_NOTIFICATION_CHECKPOINT` defined by `01_interaction_protocol.md`. The iPhone is locked or Messages is not foreground, Messages notifications are enabled, and Focus/DND does not suppress the qualification. The User explicitly confirms a lock-screen notification or banner and the evidence records:

```text
notification_presentation = HUMAN_VERIFIED
```

Sound or vibration may be noted but is not a hard or automated assertion. The E2E must also prove one canonical request, exactly one application send mutation, actual outgoing service `iMessage`, one strictly correlated canonical result, and watcher cleanup.

# Design acceptance

The iMessage channel is conforming only when implementation and real macOS E2E evidence show:

```text
the dedicated human-in-loop macOS session owns Bot Messages/imsg/worker state
the main coordinator crosses the user boundary only through narrow authenticated local IPC
sender and recipient identities satisfy the whole-system distinct-account invariant before send
self-addressed production configuration fails as SELF_MESSAGE_UNSUPPORTED before mutation
post-reboot absence of the Bot login session reports BOT_SESSION_LOGIN_REQUIRED with actionable recovery
no reachable SMS/MMS/RCS/carrier/paid-gateway delivery path
first-use bootstrap remains explicit iMessage-only and fail-closed
actual outgoing service is proven to be iMessage
resolved send/watch remain scoped to one configured direct conversation
strict post-send correlation rejects ambiguous, stale, wrong-chat, outgoing, reaction, and attachment candidates
one canonical request causes exactly one application send mutation
exactly one terminal answer is accepted
watcher processes are terminated/reaped on every terminal path
notification presentation is HUMAN_VERIFIED during production release qualification
```

A non-iMessage recipient must fail closed without intentionally sending an SMS/MMS/RCS negative test.
