---
design_id: channel-coordination
title: Channel coordination
status: active
role: design_authority
summary: >
  Defines activation, support checks, racing, interruption, and fallback for the Feishu and iMessage channels.
operational_projection:
  - src-tauri/src/channels/mod.rs
  - src-tauri/src/app/
  - src-tauri/src/autochannel.rs
  - src/views/settings/ChannelsTab.vue
---

# Supported channels

The maintained product exposes exactly two remote delivery channels:

```text
feishu
imessage
```

Existing AskHuman Telegram, Slack, DingTalk, and local popup delivery are outside the maintained delivery surface. Their code/config/docs should be removed during implementation when removal does not destroy unrelated settings/history functionality.

# Channel support is request-specific

A configured channel may decline a request before sending:

```text
Feishu
→ supports the canonical structured confirmations and the retained broader AskHuman interactions permitted by its current card/text implementation

iMessage
→ supports only the bounded structured confirmation profile defined in interaction-protocol
```

Declining an unsupported request is not a channel failure and must not emit a partial message.

# Channel readiness

The iMessage channel may participate only when its `ready` state satisfies the Bot session and sender/recipient identity prerequisites owned by `03_imessage_channel.md`. In particular, `BOT_SESSION_LOGIN_REQUIRED`, `BOT_MESSAGES_ACCOUNT_UNAVAILABLE`, `BOT_SENDER_IDENTITY_UNVERIFIED`, and `SELF_MESSAGE_UNSUPPORTED` make iMessage ineligible without relaxing any transport rule.

Readiness remains channel-local. When iMessage is `BOT_SESSION_LOGIN_REQUIRED` after a reboot, an independently ready Feishu channel may still deliver and complete the request. The coordinator does not reinterpret that iMessage health state as a Feishu failure.

# Parallel delivery and first-answer semantics

When both channels are enabled and both support a request:

```text
request
→ start Feishu session
→ start iMessage session
→ first valid terminal answer reaches Coordinator
→ Coordinator accepts exactly once
→ losing channel receives interruption/finalization
```

The existing AskHuman coordinator/preemption pattern is retained. Duplicate or late replies from the losing channel are ignored as terminal answers.

# Failure and fallback

Channel failures remain independent:

- Feishu network/configuration failure does not relax iMessage transport rules.
- iMessage unavailable/unsupported does not cause SMS or any other transport fallback.
- If one channel fails or declines and the other remains valid, the surviving channel continues.
- If no enabled channel can safely deliver the request, AskHuman returns the existing no-channel/error outcome rather than inventing a third path.

# Configuration surface

Settings expose only:

```text
Feishu
Apple Messages (iMessage only)
```

Legacy channel credentials/settings may be migrated or removed during implementation. Do not preserve dormant compatibility UI solely for old channel types unless migration is required to avoid corrupting existing configuration files; any such migration must end in the two-channel normal form.

# History and source identity

History records the canonical request/result plus winning `source_channel_id`. Channel display names are presentation metadata and do not alter canonical answer semantics.

# Design acceptance

This concern is complete when unsupported or non-ready iMessage requests never partially send, exactly one terminal answer can win, `BOT_SESSION_LOGIN_REQUIRED` leaves an independently ready Feishu path usable, removing legacy channels leaves no hidden auto-routing path, and channel failure cannot bypass iMessage-only, distinct-identity, or structured-decision invariants.
