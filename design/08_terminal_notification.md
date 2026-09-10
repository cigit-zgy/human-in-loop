---
design_id: terminal-notification
title: Terminal notification policy and presentation
status: active
role: design_authority
summary: >
  Defines when notify_human may send proactive task messages, enforces one
  terminal notification per task by default, forbids routine progress
  notifications unless explicitly requested, and keeps durable locators inside
  the same compact message with a semantic label.
operational_projection:
  - SKILL.md
  - src-tauri/src/channels/notify.rs
  - src-tauri/src/mcp/human.rs
---

# Purpose

Keep human-in-loop quiet during ordinary autonomous work while still ensuring that the User is told when a task has reached a terminal state. Distinguish decision checkpoints from informational completion reporting and avoid link-only message fragments.

# Proactive-message policy

By default human-in-loop may proactively contact the User for only two reasons:

```text
1. ask_human
   → a real unresolved semantic decision requires the User's choice

2. notify_human
   → the current task has reached one normal terminal state
```

Routine progress, heartbeat, percentage-complete, periodic status, and "still working" messages are disabled by default.

A project/task may explicitly request progress notifications. Such opt-in behavior is outside the default machine-wide contract and must not be inferred merely because a task is long-running.

# One terminal task, one terminal notification

For each logical task execution, after its terminal truth is established, attempt at most one terminal `notify_human` message.

Terminal states are:

```text
PASS
PASS_WITH_LIMITATIONS
BLOCKED
FAIL
```

The rule is:

```text
terminal truth established
→ durable report/result finalized
→ required commit/push/fresh-fetch synchronization
→ exactly one notify_human attempt at most
→ normal final agent response
```

Do not send separate "completed", "report ready", "pushed", and "release published" notifications for the same terminal event. Collapse those facts into one compact terminal notification.

A failed or uncertain terminal notification is never retried when duplicate delivery is possible. It does not rewrite the task verdict.

# Notification content

The terminal notification should contain only what is useful on a phone:

```text
status
source/repository
optional task id
short result or blocker summary
up to two short context fields when materially useful
optional durable locator
```

Do not include complete reports, long logs, credentials, private transport identifiers, raw message metadata, or large task bodies.

# Locator and link presentation

A locator belongs to the same application message as the terminal summary. Never send a second link-only iMessage merely to expose the report URL.

For an HTTP(S) locator, render it as a semantic line in the same text message:

```text
Report: https://github.com/.../report
```

For a non-URL durable locator, use the same semantic form:

```text
Report: reports/codex/260910_codex_01.md
```

`Report:` is presentation text; the underlying MCP field remains `locator`. Maintained renderers may localize the label when a stable locale is available, but must not alter the locator value.

Keep the raw HTTP(S) URL intact so Apple Messages and other clients may auto-detect it as a tappable link. Do not rely on Markdown link syntax because plain iMessage text does not guarantee Markdown rendering.

Whether Apple Messages chooses to show a rich link preview is client behavior and is not a product correctness requirement. The product correctness requirement is one application send containing both the summary and labeled locator.

# Non-regression boundary

This policy does not change:

- `ask_human` checkpoint classification;
- `ask_human` blocking/correlation semantics;
- rich multiline decision `detail`;
- iMessage-only/no-SMS transport;
- Bot user / Apple Account topology;
- Feishu transport;
- daemon lifecycle;
- TCC/bootstrap/signing;
- MCP public tool count or result schemas.

# Verification

At minimum verify:

```text
ordinary non-terminal progress does not cause notify_human
one logical terminal task causes at most one notify_human attempt
PASS / PASS_WITH_LIMITATIONS / BLOCKED / FAIL remain accepted terminal statuses
notification failure does not rewrite task truth
locator is rendered in the same application message as the summary
HTTP(S) locator is rendered as `Report: <raw-url>`
no second link-only send occurs
existing notify_human dispatch/result behavior remains unchanged
privacy/redaction tests remain green
```

# Design acceptance

The terminal-notification behavior is conforming when:

```text
ask_human is reserved for unresolved decisions
AND default progress notification count is zero
AND every terminal task attempts no more than one compact notify_human
AND summary + locator are delivered in one application message
AND raw HTTP(S) locators remain tappable-client-compatible
AND notification failure never changes established task truth
```
