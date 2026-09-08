---
design_id: mcp-interface
title: MCP control interface
status: active
role: design_authority
summary: >
  Defines the minimal MCP-facing control surface for blocking human decisions
  and non-blocking human notifications over the existing configured delivery
  infrastructure without exposing transport secrets or channel-specific semantics.
operational_projection:
  - SKILL.md
  - src-tauri/src/mcp/
  - src-tauri/src/confirm/
  - src-tauri/src/app/
---

# Purpose

Expose `human-in-loop` to MCP-capable clients through two deliberately different transport-independent capabilities:

```text
ask_human
= blocking decision checkpoint

notify_human
= non-blocking notification
```

The MCP layer is an invocation boundary only. It does not redefine channel transport semantics and does not expose recipient credentials, raw Messages data, or generic local execution.

```text
MCP client
├── ask_human
│   → canonical confirmation request
│   → existing coordinator
│   → configured channel(s)
│   → canonical correlated result
│   → MCP result
│
└── notify_human
    → canonical compact notification
    → configured channel renderer/transport(s)
    → bounded dispatch result
    → MCP result without waiting for a human reply
```

# Why GitHub is not the runtime transport

GitHub remains source-control and project-identity authority. It is not the runtime decision or notification transport.

Do not implement either capability through GitHub issues, comments, commits, Actions, repository dispatch, polling, or a repository-backed message queue.

A GitHub repository slug may still be derived from canonical repository identity when the renderer needs project context.

# Public MCP tool surface

The maintained public mutation-capable MCP tool set is exactly:

```text
ask_human
notify_human
```

Do not re-expose legacy/internal MCP operations as public tools merely because implementation code exists.

## ask_human

Purpose: obtain one explicit human decision and block the caller until one canonical terminal result exists.

Input semantics:

```text
repository_path      required for repository-associated requests
source_agent         required short agent/source identity
question             required decision question
choices              required 2–6 stable semantic choices
context              optional compact decision context
recommended_choice   optional stable choice id
request_id            optional caller-provided id; otherwise generated locally
```

Each choice contains:

```text
id
label
```

Numeric positions are renderer details only. The MCP client receives the stable semantic selected choice id.

Successful result exposes only transport-independent data equivalent to:

```text
request_id
selected_choice_id
source_channel_id
```

`ask_human` is bounded blocking:

```text
start canonical request
→ await exactly one terminal result
→ return result
```

Client cancellation/disconnect and server shutdown propagate to the coordinator and reap request-owned channel watchers/processes. No orphaned active request remains.

## notify_human

Purpose: send a compact informational notification without requiring a human answer.

It is not a degenerate confirmation and MUST NOT manufacture hidden choices such as `acknowledge` or `continue` simply to reuse `ask_human` result semantics.

Initial input semantics:

```text
repository_path      required for repository-associated notifications
source_agent         required short agent/source identity
status               required terminal/informational status
summary              required compact message
context              optional compact context fields
task_id              optional durable task identity
locator               optional safe evidence URL/path/branch+commit locator
notification_id       optional caller-provided id; otherwise generated locally
```

Initial `status` values needed for Codex terminal reporting are:

```text
PASS
PASS_WITH_LIMITATIONS
BLOCKED
FAIL
```

The tool does not accept arbitrary reply choices and does not wait for an incoming answer.

A successful result exposes only bounded transport-independent dispatch data, for example:

```text
notification_id
delivery_status
channel_ids
```

`channel_ids` identifies participating maintained channel types only; it must not expose recipient, chat GUID, phone/address, credentials, raw message rows, tokens, or private transport logs.

`notify_human` returns after bounded dispatch attempt(s). It does not wait for the user to read, acknowledge, or reply.

# Shared repository identity

For repository-associated calls, `repository_path` identifies the local project root. Existing canonical GitHub-origin resolution owns conversion to the repository slug displayed to the user.

The caller cannot supply an arbitrary rendered repository label to bypass canonical origin resolution.

If the supplied repository path cannot produce the required canonical identity, preserve the existing fail-closed repository-identity behavior rather than substituting a local basename.

# Shared privacy and security boundary

Neither public tool accepts or returns:

```text
recipient
phone number
Apple Account / iMessage address
chat id / GUID
Feishu credentials
channel-specific send arguments
SMS/carrier settings
raw imsg commands
raw Messages database access
generic shell command
generic local file read/write
```

Channel configuration remains local application state.

MCP exposure must not weaken current channel invariants:

```text
iMessage remains explicit iMessage-only
no SMS/MMS/RCS/carrier fallback
same-account reply correlation remains unchanged for ask_human
repository label remains canonical
private recipient/configuration remains local
```

# Decision versus notification lifecycle

The tool distinction is semantic and must remain visible in code/tests:

```text
ask_human
→ creates an active decision request
→ may race configured channels
→ waits for exactly one correlated terminal answer
→ cancellation cleans active request state

notify_human
→ creates no pending human decision
→ dispatches informational content
→ does not register a decision watcher solely to await acknowledgement
→ returns after bounded dispatch result
```

Do not reuse confirmation waiting/correlation machinery in a way that leaves fake pending decisions for notifications.

Shared rendering/transport helpers may be reused where semantics remain clear.

# Server scope

Use the existing standards-conforming local MCP server/runtime. Do not introduce a second server stack or a second coordinator.

Local stdio/process transport remains the first supported Codex integration path. The tool implementation should remain transport-neutral enough that a future explicitly accepted remote deployment can reuse the same contracts.

Do not add a public unauthenticated HTTP endpoint, GitHub relay, cloud backend, or arbitrary webhook executor.

# Failure semantics

## ask_human

Tool/runtime failure before a mandatory human decision is obtained means the caller cannot safely cross the decision boundary. The caller owns escalation to `BLOCKED`.

The tool must never synthesize a default decision.

## notify_human

Notification transport failure is returned truthfully to the caller. The tool does not alter the caller's already-established task verdict.

A bounded retry may be implemented when it has a concrete transport-level justification. No indefinite retry or acknowledgement wait loop is allowed.

# Verification

At minimum verify:

```text
tools/list exposes exactly ask_human + notify_human
closed input schemas
repository identity resolution
ask_human canonical request construction
ask_human stable choice mapping
ask_human exactly-one terminal result
ask_human cancellation/shutdown cleanup
notify_human creates no pending decision request
notify_human does not wait for human reply
notify_human compact status/summary/task/locator propagation
notify_human bounded dispatch result
no transport secrets in either schema/result/logs
same-account iMessage decision regression
Feishu/coordinator regression
no generic command/file capability exposed
```

A local MCP-client E2E must invoke both production tools through the server boundary rather than calling internal Rust functions directly.

For `ask_human`, complete one real correlated decision round trip. For `notify_human`, deliver one harmless terminal-style message and prove the MCP call returns without waiting for a human response.

# Design acceptance

The MCP interface is conforming when:

```text
ask_human
= one safe blocking correlated human decision path

notify_human
= one safe non-blocking informational dispatch path

AND both reuse the existing maintained channel infrastructure
AND neither exposes transport secrets or generic local execution
AND no second confirmation/coordinator/server model is introduced
AND existing iMessage/Feishu invariants do not regress
```
