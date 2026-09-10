---
design_id: channel-coordination
title: Channel coordination
status: active
role: design_authority
summary: >
  Defines iMessage activation, request support, readiness, interruption, and
  fail-closed canonical result normalization.
operational_projection:
  - src-tauri/src/channels/mod.rs
  - src-tauri/src/app/
  - src-tauri/src/autochannel.rs
  - src/views/settings/ChannelsTab.vue
---

# Maintained remote channel

The maintained product exposes exactly one remote delivery channel:

```text
imessage
```

Existing AskHuman-era remote channels and local popup delivery are outside the maintained delivery surface. Historical implementation provenance does not make them supported runtime capabilities.

# Channel support is request-specific

A configured iMessage channel may decline a request before sending. It supports only the bounded structured confirmation profile defined in `01_interaction_protocol.md`. Declining an unsupported request is not a transport failure and must not emit a partial message.

# Channel readiness

The iMessage channel may participate only when its `ready` state satisfies the Bot session and sender/recipient identity prerequisites owned by `03_imessage_channel.md`. In particular, `BOT_SESSION_LOGIN_REQUIRED`, `BOT_MESSAGES_ACCOUNT_UNAVAILABLE`, `BOT_SENDER_IDENTITY_UNVERIFIED`, and `SELF_MESSAGE_UNSUPPORTED` make iMessage ineligible without relaxing any transport rule.

When iMessage reports `BOT_SESSION_LOGIN_REQUIRED` after a reboot, the remote request remains unavailable until the dedicated Bot session is restored. The coordinator does not reinterpret that health state or invent a fallback.

# Exactly-once terminal semantics

For each supported request:

```text
request
→ start one iMessage session
→ first valid terminal answer reaches Coordinator
→ Coordinator accepts exactly once
→ watcher/session receives interruption/finalization
```

The existing canonical coordinator/terminal-gate pattern is retained. Duplicate or late replies are ignored as terminal answers.

# Failure and fallback

iMessage unavailable/unsupported does not cause SMS or any other transport fallback. If it cannot safely deliver the request, AskHuman returns the existing no-channel/error outcome rather than inventing another path.

# Configuration surface

Settings expose only:

```text
Apple Messages (iMessage only)
```

Unknown fields from older configuration files are ignored or migrated without accessing obsolete credentials. Canonical serialization contains only the maintained iMessage channel configuration.

# History and source identity

History records the canonical request/result plus winning `source_channel_id`. Channel display names are presentation metadata and do not alter canonical answer semantics.

# Design acceptance

This concern is complete when unsupported or non-ready iMessage requests never partially send, exactly one terminal answer can win, no hidden remote fallback path exists, and channel failure cannot bypass iMessage-only, distinct-identity, or structured-decision invariants.
