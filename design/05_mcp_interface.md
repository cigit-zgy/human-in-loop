---
design_id: mcp-interface
title: MCP control interface
status: active
role: design_authority
summary: >
  Defines the minimal MCP-facing control surface for invoking the existing
  canonical human-in-the-loop confirmation flow without exposing transport
  secrets, recipient identity, or channel-specific semantics.
operational_projection:
  - src-tauri/src/mcp/
  - src-tauri/src/confirm/
  - src-tauri/src/app/
---

# Purpose

Expose the existing `human-in-loop` capability to external MCP-capable clients through one small transport-independent tool rather than using GitHub, issues, Actions, or repository state as a message queue.

The MCP layer is an invocation boundary only. It does not redefine confirmation semantics and does not talk to Apple Messages or Feishu directly.

```text
MCP client
→ ask_human
→ canonical confirmation request
→ existing coordinator
→ configured channel(s)
→ canonical result
→ MCP tool result
```

# Why GitHub is not the runtime transport

GitHub remains source-control and project-identity authority. It is not the blocking confirmation transport.

Do not implement the normal runtime as:

```text
GitHub issue / comment / commit / workflow event
→ local watcher
→ human-in-loop
```

Such a route adds asynchronous polling/event state, persists decision payloads in repository infrastructure, complicates exact request/result lifetime, and couples local confirmation availability to repository permissions and GitHub service state.

A GitHub repository slug may still be used as canonical project identity where the existing renderer requires it.

# MCP tool surface

Initial public tool set is exactly one mutation-capable tool:

```text
ask_human
```

Input semantics:

```text
repository_path      required for repository-associated requests
source_agent         required short agent/source identity
question             required decision question
choices              required 2–6 stable choices
context              optional compact decision context
recommended_choice   optional stable choice id
request_id            optional caller-provided id; otherwise generated locally
```

The MCP tool does not accept:

```text
recipient
phone number
Apple Account / iMessage address
chat id / GUID
Feishu credentials
channel-specific send arguments
SMS/carrier settings
raw imsg commands
```

Channel configuration remains local application state.

# Repository identity

For repository-associated confirmations, `repository_path` identifies the local project root. Existing canonical GitHub remote resolution owns conversion to the repository slug displayed to the user.

The MCP caller must not provide an arbitrary rendered repository label to bypass canonical GitHub-origin resolution.

If the supplied path is a Git repository but canonical GitHub repository identity cannot be resolved, preserve the existing fail-closed behavior.

# Choice model

Each choice has a stable semantic id plus a compact label:

```text
id
label
```

Numeric positions remain channel-rendering details. MCP clients receive the stable selected choice id, never an iMessage option number as the canonical answer.

# Tool result

Successful tool completion returns only transport-independent result data:

```text
request_id
selected_choice_id
source_channel_id
```

Optional non-sensitive status metadata may be returned when useful. Do not expose private recipient, chat id/GUID, raw Messages rows, tokens, credentials, or private transport logs.

# Blocking and cancellation semantics

`ask_human` is a bounded blocking tool call over the same request lifecycle already used by AskHuman:

```text
start canonical request
→ await exactly one terminal result
→ return result
```

Cancellation or client disconnect must propagate to the coordinator and terminate/reap channel watchers. It must not leave orphaned active requests.

The tool must use the existing first-terminal-answer semantics when multiple configured channels race.

# Security boundary

MCP exposure must not weaken any existing channel invariant.

In particular:

```text
iMessage remains explicit iMessage-only
no SMS/MMS/RCS/carrier fallback
same-account correlation remains unchanged
repository label remains canonical
one canonical request produces at most one send mutation per participating channel session
exactly one terminal result wins
private recipient/configuration remains local
```

Do not expose a generic shell-command tool, arbitrary local-file reader, arbitrary Messages command, transport debug endpoint, or channel credential endpoint through MCP.

# Server scope

Implement the smallest standard MCP server surface that can expose `ask_human` and reuse the existing application runtime.

Prefer local process/stdio transport for local MCP clients first. Keep protocol/tool implementation transport-neutral so a later supported remote MCP/tunnel deployment can reuse the same tool contract without changing confirmation semantics.

Do not add a public unauthenticated HTTP endpoint merely to make ChatGPT connectivity easier.

Remote exposure, authentication, and a Secure MCP Tunnel or equivalent deployment belong to a separate deployment concern once the target client/product supports the required write-action capability.

# Verification

At minimum verify:

```text
tool schema and validation
repository identity resolution
canonical request construction
stable choice mapping
one terminal result
cancellation cleanup
same-account iMessage regression
Feishu/coordinator regression
no transport secrets in MCP result/logs
no generic command/file capability exposed
```

A local MCP-client E2E must invoke `ask_human`, deliver one real production-path confirmation, accept the human reply, and return the canonical stable choice id.

# Design acceptance

The MCP interface is conforming when an MCP-capable local client can block on `ask_human` and receive exactly one canonical result through the existing coordinator, while all recipient/channel configuration remains local and no GitHub relay, generic local execution surface, or weakened transport invariant is introduced.
