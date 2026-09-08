---
design_id: system-overview
title: System overview
status: active
role: design_authority
summary: >
  Defines a two-channel human-in-the-loop product built by adapting AskHuman and
  reusing imsg, with Feishu and a dedicated Bot Apple Account for iMessage-only
  delivery plus a narrow MCP surface for decisions and task notifications.
operational_projection:
  - SKILL.md
  - AGENTS.md
  - src-tauri/src/channels/
  - src-tauri/src/mcp/
  - src/views/settings/
---

# Purpose

Provide a small, dependable bridge for Codex/other coding agents to:

```text
pause at a real human decision boundary
→ deliver a structured request to the user's phone
→ receive one correlated structured answer
→ resume execution

and

reach a terminal task state
→ send a compact informational result to the user's phone
→ terminate without requiring acknowledgement
```

# Accepted architecture

```text
Agent / Codex
    ↓
human-in-loop Skill / policy
    ↓
MCP surface
    ├── ask_human   (blocking decision)
    └── notify_human (non-blocking notification)
    ↓
AskHuman-compatible core / coordinator + dispatch
    ├── Feishu renderer/transport
    └── Apple Messages renderer (Guangyao Zhao macOS user)
             ↓
       narrow authenticated local IPC
             ↓
       Bot transport worker (human-in-loop macOS user)
             ├── independent Bot Apple Account in Messages.app
             └── openclaw/imsg
                        ↓
                  iMessage only
                        ↓
       personal iPhone / personal Apple Account
```

The product adapts `Naituw/AskHuman` as the application/core basis and reuses `openclaw/imsg` as an external transport dependency. It does not merge or vendor the two codebases.

# Product scope

Supported remote delivery channels are exactly:

```text
feishu
imessage
```

Supported Agent-facing interaction classes are exactly:

```text
blocking structured human decision
non-blocking compact informational notification
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
GitHub as runtime message relay
public unauthenticated MCP HTTP service
```

# Whole-system invariants

1. **Distinct Apple identities in production.** The Apple Messages production sender account and recipient account are different (`sender_account != recipient_account`). `03_imessage_channel.md` owns the identity evidence, health states, and runtime consequences of this whole-system invariant.
2. **Free Apple path only.** Apple Messages delivery explicitly uses iMessage. Any state where the destination cannot be sent through iMessage fails closed; it never falls back to carrier messaging.
3. **One canonical decision object.** Channel presentation never becomes a second decision-semantic source. Feishu cards and iMessage text render the same structured confirmation.
4. **Decision and notification are distinct.** `ask_human` blocks for one correlated semantic choice; `notify_human` never manufactures an acknowledgement decision and never waits for a human response.
5. **Bounded mobile surface.** iMessage is for short structured confirmations and compact notifications. Complex/unbounded interactions remain Feishu-only rather than being truncated into an unsafe approximation.
6. **Images are evidence, not decoration.** A confirmation can send at most one already-supplied decision image over iMessage. The renderer never invents or automatically adds images.
7. **First valid answer wins for decisions.** When both channels participate in one supported confirmation, the first valid terminal result wins and the other channel is finalized/interrupted consistently.
8. **Terminal notification cannot falsify task truth.** A failed `notify_human` attempt is reported separately and does not rewrite the already-established task verdict.
9. **External transport stays external.** `imsg` is installed and version-checked as a runtime dependency; source is not copied into the project.
10. **Machine policy stays thin.** Codex-home AGENTS activates the maintained Skill; the full checkpoint classifier/protocol remains in the Skill/design rather than being copied into machine/project instructions.

# Upstream/reuse coordinates

- AskHuman baseline inspected: `Naituw/AskHuman@77e2e576347f94ef203bc2426b73a18749cb4e92`.
- imsg baseline inspected: `openclaw/imsg@646ea7af9616dc3e6406d86aa269bf4fb1b07a76`.
- Collaboration authority for this design work: `cigit-zgy/agent-collaboration@8601466216515125bf8b17893b2a8e8673bab79e`.

# Design acceptance

The whole design is ready for implementation/qualification when blocking decisions remain exactly correlated and fail closed, terminal notifications remain non-blocking and verdict-preserving, the Apple Messages production path uses a distinct Bot Apple Account in its dedicated logged-in macOS session, iMessage has no SMS/carrier path, unsupported iMessage content fails without partial unsafe rendering, Feishu remains functional, and Codex machine-wide integration can activate the Skill through a thin global rule plus MCP registration without duplicating project semantics.
