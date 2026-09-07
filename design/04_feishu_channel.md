---
design_id: feishu-channel
title: Feishu channel
status: active
role: design_authority
summary: >
  Retains AskHuman's Feishu long-connection/card channel and aligns its outputs with the shared canonical confirmation semantics.
operational_projection:
  - src-tauri/src/channels/feishu/
  - src-tauri/src/channels/feishu.rs
  - src/views/settings/ChannelsTab.vue
---

# Purpose

Feishu remains the richer remote channel for structured confirmations and interactions that exceed the deliberately narrow iMessage surface.

# Retained upstream behavior

The inspected AskHuman Feishu implementation/design is reused unless it conflicts with current project design:

```text
enterprise self-built app / Feishu agent app
robot direct chat
long connection for incoming events
long connection for card callbacks
interactive Card JSON 2.0
attachments where currently supported
first-answer coordinator semantics
```

# Canonical alignment

For structured confirmations, Feishu card actions map to the same stable choice identities and canonical result used by iMessage. Feishu-specific card ids, callback payloads, Open ID, and rendering state are transport metadata only.

# Richer-surface role

Feishu may render requests that iMessage declines because they require:

```text
longer context
multiple questions
free-text/form input
multiple attachments
interaction not representable safely inside the iMessage budget
```

This is a more capable renderer of the same canonical request; it does not weaken or reinterpret the request.

# Credentials and transport

Existing App ID/App Secret/Open ID/service-domain handling remains under the Feishu channel. Secrets must not appear in reports, logs, or committed configuration.

# Design acceptance

Feishu remains conforming when existing card/send/receive behavior continues to work after legacy channel removal and iMessage introduction, structured choice identities round-trip without transport-specific semantic drift, and Feishu can independently complete a request when iMessage is unavailable or unsupported.
