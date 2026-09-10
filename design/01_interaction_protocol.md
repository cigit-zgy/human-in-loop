---
design_id: interaction-protocol
title: Structured confirmation protocol
status: active
role: design_authority
summary: >
  Defines the canonical structured confirmation and the compact, bounded,
  correlated notification form used on iMessage.
operational_projection:
  - src-tauri/src/models.rs
  - src-tauri/src/channels/confirm.rs
  - src-tauri/src/channels/imessage.rs
---

# Purpose and boundary

The canonical interaction is the existing AskHuman structured confirmation model, not free-form chat. Channel renderers may change presentation but may not redefine actions, context, recommendation, or result identity.

The iMessage surface is a **remote decision notification**, not a complete task viewer. Its job is to expose only the information required to make a safe bounded choice on a phone. Detailed logs, diffs, commands, long explanations, and complete task context remain in the local AskHuman/Codex surface.

The iMessage channel supports **one structured confirmation at a time per rendered request**. General free-form `AskRequest`, multi-question questionnaires, arbitrary Markdown, and form-like interactions are not downgraded into iMessage text; they are unsupported by the maintained remote surface and fail closed.

# Canonical semantics

A supported confirmation contains, at minimum:

```text
request_id
source/agent context
repository/project identity when the request belongs to a GitHub repository
title
decision-relevant context
short summary/question
2–6 stable choices
optional recommended choice
optional one decision image supplied by the caller
```

The internal choice identity remains stable and semantic. Numeric positions are only an iMessage rendering convenience.

# Repository identity

For every confirmation associated with a GitHub repository, the phone notification MUST identify that repository.

Canonical display value:

```text
GitHub repository name only
```

Examples:

```text
cigit-zgy/water-biomodel-agent  -> water-biomodel-agent
cigit-zgy/human-in-loop         -> human-in-loop
```

Do not display the GitHub owner, full remote URL, local filesystem path, task worktree name, branch name, or temporary directory as the project label.

The repository name is part of the decision surface, not optional explanatory context. It must not be removed by normal compaction for repository-associated requests.

Resolution should prefer an already known canonical repository identity from the request/agent/project context. When runtime derivation is necessary, derive the repository name from the canonical GitHub remote identity rather than guessing from an arbitrary local directory basename.

For a genuinely non-repository interaction, a repository label is not required; use the canonical project label only when one exists.

# Compact iMessage rendering contract

The default renderer uses the smallest phone-readable representation that still preserves decision-critical meaning.

Preferred repository-associated shape:

```text
[HIL · 48273]
Codex · water-biomodel-agent
ASM3 Stage 4

Use exact fraction grammar for 1/14?

1  Accept
2  Stop

Reply: 48273-1
```

For repository-associated requests, the source/repository line is mandatory even when no additional task context is needed:

```text
[HIL · 48273]
Codex · human-in-loop

Continue execution?

1  Continue
2  Stop

Reply: 48273-1
```

Only genuinely non-repository interactions may omit the repository portion:

```text
[HIL · 48273]
Codex

Continue execution?

1  Continue
2  Stop

Reply: 48273-1
```

Presentation rules:

1. First line is always `[HIL · <TOKEN>]`.
2. For a repository-associated request, the next line is always `<source> · <repository-name>`, where `<repository-name>` is the GitHub repository name only.
3. The repository label is mandatory decision context and is never removed by ordinary compaction.
4. Additional context is limited to at most two compact lines and is included only when required for the decision.
5. Do not emit `Context`, `Question`, `Action`, or similar label-only headings when ordinary line structure is sufficient.
6. The question is one compact paragraph.
7. Choices use one-based numeric positions and one line each.
8. The recommendation, when decision-relevant, is rendered compactly on the corresponding choice line, e.g. `[recommended]`.
9. The final line is always `Reply: <TOKEN>-<OPTION_NUMBER>`.
10. Do not copy full commands, stack traces, diffs, logs, report bodies, or long local context into iMessage.
11. Do not split one decision across multiple text messages merely to display more context.

The token is deterministically derived from the canonical request id, uses ASCII decimal digits with no leading zero, and must be collision-safe among currently pending requests. Its normal length is five digits; an active collision extends it deterministically by two digits at a time. The user reply must carry the token; bare `1` is not accepted because multiple Agents/requests may coexist.

Accepted reply grammar remains deliberately narrow:

```text
<TOKEN>-<OPTION_NUMBER>
```

Examples:

```text
48273-1
48273-2
```

No fuzzy natural-language intent parsing is used. Invalid or ambiguous replies do not resolve the request.

# Compact presentation budget

The renderer optimizes for a target complete text length of approximately **300–500 Unicode scalar values**. The hard admission ceiling is **700 Unicode scalar values**.

Field budgets:

```text
source + repository/project line    ≤ 80
additional context lines            ≤ 2
each additional context line        ≤ 80
question/summary                     ≤ 160
choices                              2–6
preferred routine choices            2–4
one choice label                     ≤ 60
complete rendered text hard limit    ≤ 700
outgoing decision images             ≤ 1
```

Budgeting rules:

1. Never truncate a repository label, choice label, request token, question, or context required for a safe decision.
2. Remove optional explanatory/context lines before compacting decision-critical information.
3. For repository-associated requests, preserve the compact `<source> · <repository-name>` line before removing other optional context.
4. If the critical representation still exceeds the hard limit, iMessage marks the request unsupported and sends nothing for that request.
5. A request may exceed the 500-character target when genuinely necessary but must remain within the 700-character hard limit.
6. The renderer never splits one decision across multiple iMessages merely to bypass the budget.

# Human notification checkpoint

Production Apple Messages release qualification includes a `HUMAN_NOTIFICATION_CHECKPOINT`. The automated path proves the structured request/result lifecycle; the User separately confirms that the incoming iMessage notification was presented on the locked or non-foreground iPhone as a lock-screen notification or banner.

Qualification records:

```text
notification_presentation = HUMAN_VERIFIED
```

The iPhone has Messages notifications enabled and Focus/DND does not suppress the qualification message. Audible or haptic perception may be recorded as an optional observation but is not an automated or hard acceptance assertion. Apple Messages deployment, identity, and session details remain owned by `03_imessage_channel.md`.

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
→ validate chat/request correlation + token + option range
→ map numeric position to stable choice id
→ canonical ConfirmResult/ChannelResult
→ coordinator
→ Agent
```

Mode-specific identity/correlation semantics are owned by `03_imessage_channel.md`; renderer compaction must not weaken them.

Agents never receive transport-specific reply syntax as the semantic result.

# Design acceptance

This concern is complete when every supported iMessage reply is unambiguously correlated to one active request and one canonical choice, every repository-associated phone notification visibly identifies its GitHub repository by repository name, the phone surface contains only decision-relevant content, the production release E2E includes the human-verified notification checkpoint, critical information cannot be silently truncated, and unsupported content fails without generating a misleading partial decision surface.
