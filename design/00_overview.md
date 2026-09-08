---
design_id: system-overview
title: System overview
status: active
role: design_authority
summary: >
  Defines a two-channel human-in-the-loop product with a narrow MCP surface,
  Feishu, and a dedicated distinct-account Apple Messages Bot transport that is
  interactively configured once and unattended during normal operation.
operational_projection:
  - SKILL.md
  - AGENTS.md
  - src-tauri/src/channels/
  - src-tauri/src/mcp/
  - scripts/install.sh
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
    ├── ask_human      blocking decision
    └── notify_human   non-blocking notification
    ↓
canonical coordinator + dispatch
    ├── Feishu renderer/transport
    └── Apple Messages renderer in the primary macOS user
             ↓
       narrow authenticated local IPC
             ↓
       Bot transport worker in a dedicated macOS user
             ├── independent Bot Apple Account in Messages.app
             └── openclaw/imsg
                        ↓
                  iMessage only
                        ↓
       personal iPhone / personal Apple Account
```

The default documented Bot macOS username is `human-in-loop`, but the implementation may support another explicitly configured dedicated account. The primary user's local account name is deployment state, not project semantics.

The product originated by adapting the pinned AskHuman application/core basis and reuses `openclaw/imsg` as an external transport dependency. AskHuman is not a maintained runtime dependency or product identity; required upstream license/provenance remains preserved.

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

The maintained production runtime is headless. A desktop GUI may exist only if a future accepted design gives it an independent necessary responsibility; it is not required for the current daemon/MCP/channel architecture.

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

1. **Distinct Apple identities in production.** The Apple Messages production sender account and recipient account are different (`sender_account != recipient_account`). `03_imessage_channel.md` owns identity evidence and fail-closed consequences.
2. **Free Apple path only.** Apple Messages delivery explicitly uses iMessage. Any state where iMessage cannot be proven fails closed; it never falls back to carrier messaging.
3. **One canonical decision object.** Channel presentation never becomes a second decision-semantic source. Feishu and iMessage render the same canonical confirmation/result identities.
4. **Decision and notification are distinct.** `ask_human` blocks for one correlated semantic choice; `notify_human` never manufactures an acknowledgement decision and never waits for a response.
5. **First valid answer wins.** When both channels participate in one confirmation, only the first valid terminal result is accepted.
6. **Terminal notification cannot falsify task truth.** A failed `notify_human` attempt is reported separately and does not rewrite the established task verdict.
7. **External transport stays external.** `imsg` is installed/version-checked as an external dependency; its source is not vendored into this project.
8. **One-time interactive setup, unattended normal operation.** Installation may require explicit User interaction for Bot-user creation, Apple Account login, one bounded administrator bootstrap, macOS privacy consent, and initial notification qualification. After `SETUP_COMPLETE`, ordinary Codex/MCP/channel operation must not require passwords, Fast User Switching, or repeated TCC/privacy prompts.
9. **Permission recurrence is not normal UX.** A new password/TCC/user-switch requirement after setup is a deployment regression or explicit recovery state unless the host state materially changed.
10. **Machine policy stays thin.** Codex-home AGENTS activates the maintained Skill; full checkpoint/protocol/deployment semantics remain in the project authority rather than being copied into machine instructions.

# Setup and runtime phases

The product has two explicit phases:

```text
ONBOARDING / RECOVERY
→ interactive only where macOS or Apple genuinely requires it
→ establish stable runtime identity + Bot session + permissions + notification qualification
→ SETUP_COMPLETE

NORMAL OPERATION
→ primary user may be away from the Mac
→ no administrator/Keychain password prompt
→ no ordinary Fast User Switching
→ no new Full Disk Access / Automation / Files & Folders prompt
→ Agent ↔ MCP ↔ coordinator ↔ Bot worker ↔ phone runs unattended
```

A real reboot may invalidate only the Bot login-session predicate; until the dedicated Bot user has logged in again, the Apple Messages channel reports its explicit session-recovery state rather than pretending to be ready. Other recovery states are owned by `07_macos_runtime_deployment.md`.

# Upstream/reuse coordinates

- AskHuman baseline inspected/adapted: `Naituw/AskHuman@77e2e576347f94ef203bc2426b73a18749cb4e92`.
- imsg baseline inspected/reused: `openclaw/imsg@646ea7af9616dc3e6406d86aa269bf4fb1b07a76`.
- Collaboration authority for this design work: `cigit-zgy/agent-collaboration@8601466216515125bf8b17893b2a8e8673bab79e`.

# Design acceptance

The whole design is release-qualifiable when:

```text
blocking decisions remain exactly correlated and fail closed
AND terminal notifications remain non-blocking and verdict-preserving
AND Apple Messages uses a distinct Bot Apple Account in a dedicated logged-in macOS session
AND iMessage has no reachable SMS/MMS/RCS/carrier path
AND Feishu remains independently functional
AND the local MCP/Skill integration uses one canonical coordinator
AND the installation can reach SETUP_COMPLETE through a bounded documented onboarding flow
AND after SETUP_COMPLETE normal operation is unattended with zero recurring password, user-switch, or privacy-consent prompts
AND explicit recovery states replace repeated ad-hoc permission prompting
```
