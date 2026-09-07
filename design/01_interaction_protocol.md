---
design_id: interaction-protocol
title: Structured confirmation protocol
status: active
role: design_authority
summary: >
  Defines the canonical structured confirmation and the bounded, correlated text form used on iMessage.
operational_projection:
  - src-tauri/src/models.rs
  - src-tauri/src/channels/confirm.rs
  - src-tauri/src/channels/imessage.rs
---

# Purpose and boundary

The canonical interaction is the existing AskHuman structured confirmation model, not free-form chat. Channel renderers may change presentation but may not redefine actions, context, recommendation, or result identity.

The initial iMessage channel supports **one structured confirmation at a time per rendered request**. General free-form `AskRequest`, multi-question questionnaires, arbitrary Markdown, and form-like interactions are not downgraded into iMessage text; they are unsupported on iMessage and may still be delivered through Feishu.

# Canonical semantics

A supported confirmation contains, at minimum:

```text
request_id
source/agent context
project context when available
title
decision-relevant context fields
short summary/question
2–6 stable choices
optional recommended choice
optional one decision image supplied by the caller
```

The internal choice identity remains stable and semantic. Numeric positions are only an iMessage rendering convenience.

# iMessage rendering contract

A typical message is:

```text
AskHuman · Codex  [7F32]
Project: water-biomodel-agent
Action: Delete obsolete generated objects

Context
Path: validated/asm3/
Reason: regeneration required

Question
Continue with deletion?

1. Continue  [recommended]
2. Stop

Reply: 7F32 1
```

The short token is derived from the canonical request id and must be collision-safe among currently pending requests. The user reply must carry the token; bare `1` is not accepted because multiple Agents/requests may coexist.

Accepted reply grammar for the initial channel is deliberately narrow:

```text
<TOKEN> <OPTION_NUMBER>
```

Examples:

```text
7F32 1
7F32 2
```

No fuzzy natural-language intent parsing is used. Invalid or ambiguous replies do not resolve the request.

# Length budget

The iMessage renderer enforces a product-level presentation budget independent of iMessage's network capacity:

```text
title                       ≤ 60 Unicode scalar values
question/summary            ≤ 300
context fields              ≤ 5
one context value           ≤ 120
choices                     2–6
one choice label            ≤ 80
complete rendered text      ≤ 1200
outgoing decision images    ≤ 1
```

Budgeting rules:

1. Never truncate a choice label, request token, question, or context field required for safe decision-making.
2. Optional explanatory body may be compacted or omitted before critical fields.
3. If the critical representation still exceeds the budget, iMessage marks the request unsupported and sends nothing for that request. Feishu may continue independently.
4. The renderer never splits one decision across multiple iMessages merely to bypass the budget.

# Image admission

The iMessage renderer never decides by itself that an image is useful. It may send an image only when the canonical request already contains exactly one image selected by the caller for decision evidence.

Initial supported decision-image formats:

```text
PNG
JPEG
```

Initial size ceiling: 5 MiB.

Multiple images, non-image files, oversized images, or unsupported formats make the attachment unsupported for iMessage. They do not cause carrier/MMS fallback. The textual request may still be delivered only if it remains independently sufficient for the decision; otherwise iMessage is skipped for that request.

User replies over iMessage are option-text only in the initial design. Incoming reply attachments do not resolve a confirmation.

# Result normalization

```text
iMessage token + numeric option
→ validate sender/chat + active request + token + option range
→ map numeric position to stable choice id
→ canonical ConfirmResult/ChannelResult
→ coordinator
→ Agent
```

Feishu card callbacks map directly to the same stable choice identity. Agents never receive transport-specific reply syntax as the semantic result.

# Design acceptance

This concern is complete when every supported iMessage reply is unambiguously correlated to one active request and one canonical choice, critical information cannot be silently truncated, and unsupported content fails without generating a misleading partial decision surface.
