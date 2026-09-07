---
design_id: system-overview
title: System overview
status: active
role: design_authority
summary: >
  Defines a two-channel human-in-the-loop product built by adapting AskHuman and reusing imsg, with Feishu and iMessage-only delivery.
operational_projection:
  - AGENTS.md
  - src-tauri/src/channels/
  - src/views/settings/
---

# Purpose

Provide a small, dependable bridge for Codex/other coding agents to pause at a human checkpoint, deliver a structured decision request to the user's phone, receive a correlated structured answer, and resume execution.

# Accepted architecture

```text
Agent / Codex
    ↓
AskHuman-compatible core
    ↓ canonical structured confirmation
Coordinator
    ├── Feishu renderer/transport
    └── Apple Messages renderer
             ↓
          openclaw/imsg
             ↓
       macOS Messages.app
             ↓
        iMessage only
             ↓
           iPhone
```

The product adapts `Naituw/AskHuman` as the application/core basis and reuses `openclaw/imsg` as an external transport dependency. It does not merge or vendor the two codebases.

# Product scope

Supported remote delivery channels are exactly:

```text
feishu
imessage
```

The desktop application may retain configuration/history UI needed to operate the product. Delivery through AskHuman's local popup, Telegram, Slack, DingTalk, or other historical channels is outside the maintained product surface and should be removed or disabled during implementation rather than preserved behind compatibility layers.

# Explicitly out of scope

```text
WeChat
SMS
MMS
RCS
carrier fallback
paid messaging gateways
BlueBubbles/server relay
private IMCore injection
SIP-disabling features
arbitrary rich iMessage UI/card protocols
```

# Whole-system invariants

1. **Free Apple path only.** Apple Messages delivery must explicitly use iMessage. Any state where the destination cannot be sent through iMessage fails closed; it never falls back to carrier messaging.
2. **One canonical decision object.** Channel-specific presentation never becomes a second semantic source. Feishu cards and iMessage text render the same structured confirmation.
3. **Bounded mobile decision surface.** iMessage is for short structured confirmations. Complex/unbounded interactions remain Feishu-only rather than being truncated into an unsafe approximation.
4. **Images are evidence, not decoration.** A request can send at most one already-supplied decision image over iMessage. The renderer never invents or automatically adds images.
5. **First valid answer wins.** When both channels are active for the same supported request, the first valid terminal result wins and the other channel is finalized/interrupted consistently.
6. **External transport stays external.** `imsg` is installed and version-checked as a runtime dependency; source is not copied into the project.

# Upstream/reuse coordinates

- AskHuman baseline inspected: `Naituw/AskHuman@77e2e576347f94ef203bc2426b73a18749cb4e92`.
- imsg baseline inspected: `openclaw/imsg@646ea7af9616dc3e6406d86aa269bf4fb1b07a76`.
- Collaboration authority: `cigit-zgy/agent-collaboration@8601466216515125bf8b17893b2a8e8673bab79e`.

# Design acceptance

The design is ready for implementation when each supported request can be answered through a transport-independent canonical result, iMessage has no code path to SMS/carrier delivery, unsupported iMessage requests fail without partial rendering, and Feishu remains fully functional under the same coordinator semantics.
