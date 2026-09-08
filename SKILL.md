---
name: human-in-loop
description: >
  Enforce human checkpoints and terminal human reporting for Codex/agent tasks.
  Use when a task may require a User decision, confirmation, destructive or public
  authorization, design/scientific adjudication, security/credential permission,
  or when a task reaches PASS, PASS_WITH_LIMITATIONS, BLOCKED, or FAIL and should
  report the terminal result through the configured human-in-loop MCP tools.
---

# Human in Loop

Canonical maintained source:

```text
cigit-zgy/human-in-loop
```

Current project design authority lives in `design/`. This Skill is the Codex-facing operational projection of `design/05_mcp_interface.md` and `design/06_codex_integration.md`.

## Capability boundary

Two public MCP tools have different semantics:

```text
ask_human
= blocking human decision

notify_human
= non-blocking informational notification
```

Never use `notify_human` as a substitute for a required decision. Never manufacture an acknowledgement choice just to turn a notification into `ask_human`.

## Determine whether a human decision is required

Set `ASK_HUMAN_REQUIRED` only when current higher authority has not already resolved or explicitly authorized the decision and at least one condition applies:

```text
EXPLICIT_CHECKPOINT
User / project AGENTS / current design / committed task explicitly requires confirmation.

USER_PREFERENCE_DECISION
Multiple materially valid outcomes remain and User preference determines the choice.

DESTRUCTIVE_OR_IRREVERSIBLE
Persistent/shared state may be materially deleted, overwritten, replaced, or irreversibly migrated.

EXTERNAL_OR_PUBLIC_SIDE_EFFECT
A meaningful outside-world action such as publication, release, external send,
public visibility, repository/account administration, or external creation needs authorization.

AUTHORITY_OR_SCIENTIFIC_CHANGE
Current design, scientific/product semantics, trust/provenance, or another authority-bearing contract must be reopened or changed.

SECURITY_CREDENTIAL_PERMISSION
A new credential, permission, sensitive access grant, or security-boundary change is required.
```

Do not ask merely because a routine reversible implementation decision exists under an already-clear contract.

Do not ask again for an action the User/current committed task has already explicitly authorized unless a materially new decision boundary appears.

Normal task-authorized Git fetch/commit/push, testing, formatting, build, bounded retry, and task-local tmp creation are not new checkpoints by themselves.

Codex shell/sandbox permission prompts are host permission mechanics and remain separate from semantic human-in-loop decisions.

## Mandatory decision lifecycle

When `ASK_HUMAN_REQUIRED` is true:

```text
1. Stop the affected execution path before the decision side effect.
2. Construct the smallest canonical structured request that preserves safe decision context.
3. Call `ask_human`.
4. Wait for exactly one valid correlated canonical result.
5. Continue only from the returned stable `selected_choice_id`.
6. If the selected choice stops/cancels the path, terminate that path accordingly.
```

Do not substitute:

```text
default approval
timeout approval
guessed User preference
uncorrelated free-form reply
silent fallback around the required tool
```

If `ask_human` is required but unavailable, fails before a valid decision, or cannot return a correlated result:

```text
→ BLOCKED
→ do not cross the checkpoint
→ state that the human-in-loop decision capability was unavailable
```

## Request quality

A decision request should normally include only what the User needs to choose safely:

```text
repository_path when repository-associated
source_agent
short question/action
2–6 stable choices
compact context
recommended choice only when an authoritative/reasoned recommendation exists
```

Do not include secrets, credentials, raw private transport identifiers, irrelevant logs, or long task/report bodies.

## Terminal reporting — required attempt

At every normal terminal task state:

```text
PASS
PASS_WITH_LIMITATIONS
BLOCKED
FAIL
```

attempt `notify_human` once the normal task result is already established.

For repository-changing work, first satisfy the task's Git publication/synchronization contract:

```text
normal result/report complete
→ commit
→ push owning branch
→ fresh fetch
→ required local HEAD == fetched upstream HEAD proof
→ notify_human
→ final Codex response
```

The notification is compact:

```text
project
task_id when available
verdict
one short summary or blocker
branch/commit/report locator when available
```

Do not send complete reports, task bodies, long test matrices, private transport configuration, or raw logs.

## Notification failure does not rewrite task truth

`notify_human` is non-blocking/best-effort after the task verdict exists.

If notification fails:

```text
preserve the already-established task verdict
+ surface HUMAN_NOTIFICATION: FAILED in the final local response/report evidence
```

Do not convert a valid PASS into FAIL solely because notification transport failed. Do not wait indefinitely for acknowledgement.

## Project/task extensions

Project `AGENTS.md` and committed task specifications may add domain-specific checkpoints. Apply them in addition to this generic classifier.

Examples may include scientific interpretation ambiguity, destructive replacement of validated scientific objects, login/MFA/CAPTCHA, publication authorization, or repository visibility/licensing changes.

Do not move project-specific rules into this Skill unless they generalize across maintained projects.

## Authority / stale-copy rule

When Codex uses this Skill machine-wide, the local discovery copy must correspond to the exact accepted GitHub revision named by the machine policy/task.

A similarly named local folder or stale checkout is not sufficient authority.

## Completion

The human-in-loop contract is satisfied for a task when:

```text
all mandatory human checkpoints were either resolved through correlated ask_human results or caused fail-closed BLOCKED state
AND no already-authorized routine work received redundant confirmation prompts
AND the normal terminal task result was established truthfully
AND notify_human was attempted for the terminal state
AND notification failure, if any, was reported without changing the underlying task verdict
```
