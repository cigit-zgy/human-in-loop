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
│   → configured iMessage channel
│   → canonical correlated result
│   → MCP result
│
└── notify_human
    → canonical compact notification
    → configured iMessage renderer/transport
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
question             required concise decision question
detail               optional multiline decision evidence/body
choices              required 2–6 stable semantic choices
context              optional compact metadata/context
recommended_choice   optional stable choice id
request_id            optional caller-provided id; otherwise generated locally
```

`question` and `detail` are intentionally different:

```text
question
= the concise decision the human must answer

detail
= bounded multiline evidence needed to make that decision

context
= short metadata only; it is not the long-form body
```

The public `detail` field is optional and backward-compatible. Existing callers that omit it retain current behavior. `detail` is mapped into the canonical confirmation detail/body representation and preserves meaningful internal newlines/paragraph separation. It MUST NOT be normalized through `split_whitespace()` or otherwise collapsed into one long context line.

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

### Decision-body budget policy

`human-in-loop` must support substantive decisions, not only short yes/no approvals. Scientific review cards, release decisions, security decisions and design choices may require hundreds of Unicode characters of evidence.

The product therefore distinguishes a user-facing configurable operational budget from an absolute defensive ceiling:

```text
default detail budget          = 1000 Unicode characters
default fully rendered budget  = 1500 Unicode characters
absolute rendered safety cap   = 5000 Unicode characters
```

The default budget is not a hard product capability boundary. The User may configure a larger iMessage decision-body/rendered budget when needed, up to the absolute safety cap. The absolute cap is retained to prevent accidental transport of unbounded logs, documents, credentials or model dumps through one decision message.

Structural limits that protect the interaction contract remain bounded independently, including choice count and compact choice labels. `context` remains a compact metadata surface and is not made into an unlimited substitute for `detail`.

Configuration validation must fail closed on invalid values. It must not silently truncate decision evidence. If the configured budget is exceeded, the request fails before mutation with a typed/redacted payload-size reason rather than silently dropping content.

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
obsolete channel credentials
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
→ starts one configured iMessage request
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

Payload/rendering rejection remains fail-closed. The daemon/channel diagnostic boundary should preserve a fixed redacted reason such as `detail_too_long`, `rendered_text_too_long`, or another typed coarse status so a caller/operator can distinguish payload incompatibility from channel/session failure without exposing the decision body or private transport identifiers.

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
ask_human detail omitted -> historical behavior unchanged
ask_human detail preserves multiline content
ask_human detail 999 chars -> pass default budget
ask_human detail 1000 chars -> pass default budget
ask_human detail 1001 chars -> explicit fail under default budget
larger valid user-configured budget -> pass up to configured value
configured/absolute ceiling violations -> fail closed with typed redacted reason
legacy compact context remains compatible
notify_human creates no pending decision request
notify_human does not wait for human reply
notify_human compact status/summary/task/locator propagation
notify_human bounded dispatch result
no transport secrets in either schema/result/logs
same-account iMessage decision regression
iMessage-only coordinator regression
no generic command/file capability exposed
```

A local MCP-client E2E must invoke both production tools through the server boundary rather than calling internal Rust functions directly.

For this decision-body extension, synthetic E2E must include a WME-sized Chinese multiline decision body in the approximate 600–800 character range. Before release qualification, one real installed-Codex-MCP iMessage round trip must use a similarly sized harmless multiline body and return exactly one correlated canonical result.

# Design acceptance

The MCP interface is conforming when:

```text
ask_human
= one safe blocking correlated human decision path
  with concise question + optional bounded multiline evidence

notify_human
= one safe non-blocking informational dispatch path

AND existing callers remain backward-compatible
AND users may raise the operational decision-body budget within the absolute safety ceiling
AND evidence is never silently truncated or flattened into compact metadata
AND both reuse the existing maintained channel infrastructure
AND neither exposes transport secrets or generic local execution
AND no second confirmation/coordinator/server model is introduced
AND existing iMessage invariants do not regress
```
