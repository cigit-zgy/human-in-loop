---
design_id: codex-integration
title: Codex machine-wide human-in-the-loop integration
status: active
role: design_authority
summary: >
  Defines global Codex activation, checkpoint classification, fail-closed decision
  handling, terminal notification ordering, and the separation between Codex-home
  AGENTS policy, MCP configuration, project checkpoints, and task checkpoints.
operational_projection:
  - SKILL.md
  - $CODEX_HOME/AGENTS.md
  - $CODEX_HOME/config.toml
---

# Purpose

Make `human-in-loop` a dependable machine-wide Codex capability without copying its full protocol into every repository or task.

The integration has four distinct owners:

```text
$CODEX_HOME/AGENTS.md
= global activation + hard fail-closed invariant

human-in-loop SKILL.md
= generic checkpoint classifier + ask/notify lifecycle

project AGENTS.md
= project-specific additional checkpoints

reports/chatgpt task
= task-specific additional checkpoints
```

MCP availability is configured separately in `$CODEX_HOME/config.toml`.

# OpenAI harness boundary

Codex aggregates persistent instructions from `$CODEX_HOME/AGENTS.md` and more specific project-scoped AGENTS files. Therefore the machine-wide AGENTS layer is intentionally thin: it activates the maintained Skill and states only invariants that must apply to every Codex task.

The MCP server supplies `ask_human` and `notify_human` tools to the Codex agent loop. MCP tool safety remains owned by `human-in-loop`; the Codex shell sandbox is not a substitute for MCP guardrails.

# Global activation contract

The Codex-home AGENTS rule must make the following behavior explicit:

```text
for every Codex task
→ load/obey the maintained human-in-loop Skill

when the Skill or active project/task declares a mandatory human checkpoint
→ stop the affected execution path
→ call ask_human
→ do not continue until one valid correlated human result exists

at every normal terminal task state
→ first finish the normal durable task result/report + required repository synchronization
→ then attempt notify_human
→ then return the normal final Codex response
```

The Codex-home AGENTS file MUST NOT copy the complete checkpoint classifier, MCP schemas, transport details, iMessage grammar, Feishu details, or implementation manual. Those remain in the Skill/project.

# Authority and override

Instruction precedence remains the Codex/project authority model. Human-in-loop adds a global safety/decision layer; it does not supersede explicit User instructions.

Project AGENTS and committed tasks may add checkpoints. They do not silently weaken global checkpoints.

An explicit User instruction may authorize an action that would otherwise require a checkpoint. Do not ask the User again merely for ceremony when the current authoritative task already contains that explicit authorization.

# Generic checkpoint classifier

`ASK_HUMAN_REQUIRED` is true when at least one of these conditions applies and current higher authority has not already explicitly resolved/authorized the decision:

## EXPLICIT_CHECKPOINT

The User, current project design, project AGENTS, or committed task explicitly requires a human choice/confirmation.

## USER_PREFERENCE_DECISION

Two or more materially valid outcomes remain and the correct selection depends on User preference rather than existing design/task semantics.

Routine reversible engineering choices under a clear contract do not qualify.

## DESTRUCTIVE_OR_IRREVERSIBLE

The proposed action can materially delete, overwrite, replace, or irreversibly migrate persistent/shared state and has not already been explicitly authorized.

## EXTERNAL_OR_PUBLIC_SIDE_EFFECT

The proposed action crosses a meaningful outside-world boundary such as publication, release, external send, public visibility, account/repository administration, or externally visible creation, when authorization is not already explicit.

Ordinary task-authorized `git push` to the declared task branch is not reclassified as a new checkpoint merely because it is remote.

## AUTHORITY_OR_SCIENTIFIC_CHANGE

Execution reveals that current project design, scientific/product semantics, trust/provenance rules, or another authority-bearing contract must be changed/reopened before continuing.

## SECURITY_CREDENTIAL_PERMISSION

Execution requires a new credential, permission, security-boundary change, sensitive access grant, or comparable trust decision not already authorized.

# What must NOT trigger ask_human

Do not turn human-in-loop into permission spam.

Examples that normally proceed without an additional checkpoint when already covered by current authority:

```text
routine reversible implementation choice
normal test/format/build command
expected retry within accepted recovery semantics
creating task-local tmp state
committing/pushing to the task-authorized branch
choosing an equivalent low-level implementation detail
following an explicit choice already made in current design/task
```

Codex's own shell/sandbox permission prompts remain host permission mechanics and are not replaced by the semantic `ask_human` protocol.

# Mandatory checkpoint lifecycle

```text
checkpoint detected
→ construct the smallest safe canonical AskHuman request
→ include the real project/task context needed for the decision
→ call ask_human
→ pause affected path
→ accept only one correlated canonical result
→ map stable selected_choice_id to the authorized continuation
→ continue or stop exactly according to that choice
```

Do not substitute:

```text
default approval
timeout approval
guessed User preference
free-form reply interpretation outside canonical correlation
silent fallback to a local terminal question when the mandatory remote/tool checkpoint is unavailable
```

If `ask_human` is required but unavailable or cannot obtain a valid result:

```text
TASK STATE → BLOCKED
```

The affected action is not executed.

# Terminal notification lifecycle

Human notification is attempted for every normal terminal state:

```text
PASS
PASS_WITH_LIMITATIONS
BLOCKED
FAIL
```

Ordering is strict for repository-changing tasks:

```text
complete normal task execution/result
→ write required FORMAL report when applicable
→ commit task-scoped repository changes
→ push owning branch
→ fresh fetch
→ prove local task HEAD == fetched upstream HEAD when the task requires repository publication
→ call notify_human
→ return final Codex response
```

For non-repository tasks, notify after the normal terminal result is established.

A terminal phone message is compact. It contains only decision-relevant status and a durable locator where available:

```text
project
task_id
verdict
one short summary or blocker
branch/commit or report locator
```

Do not send the complete task body, report, logs, stack traces, private recipient/configuration, or long test output.

# Notification failure semantics

`notify_human` is best-effort after the task verdict exists.

```text
TASK_VERDICT: PASS
notify_human: FAILED
→ TASK_VERDICT remains PASS
→ final Codex response/report evidence surfaces HUMAN_NOTIFICATION: FAILED
```

The same rule applies to other terminal verdicts: notification transport failure does not rewrite the underlying task result.

No indefinite retry or wait-for-acknowledgement loop is allowed. A bounded retry may be used only when transport semantics justify it.

# Skill authority and discovery

The maintained Skill is first-party and remote-canonical:

```text
cigit-zgy/human-in-loop@<accepted-commit>:SKILL.md
```

Codex-home policy or local Skill discovery must resolve to an exact accepted revision. A local path/symlink is convenience only and must not silently substitute stale Skill content.

When a new accepted Skill revision is adopted globally, update the machine-wide coordinate deliberately rather than following unreviewed `latest`.

# MCP registration

`$CODEX_HOME/config.toml` owns only MCP capability registration/runtime configuration. It should expose the production local human-in-loop MCP server without embedding policy semantics or secrets in AGENTS/Skill text.

Preserve existing unrelated Codex configuration. Do not replace the whole config file merely to add this MCP server.

Private channel recipient/configuration remains in the human-in-loop application's local configuration owner, not in Codex AGENTS or committed repository files.

# Project-specific extension

Projects add only checkpoints that are genuinely domain-specific, for example:

```text
scientific interpretation ambiguity
validated scientific-object destructive replacement
login/MFA/CAPTCHA boundary
publication/release authorization
repository-visibility/licensing change
```

These belong in the owning project `AGENTS.md` or committed task, not in the global Skill unless they generalize across projects.

# Design acceptance

This integration is conforming when:

```text
all Codex tasks receive one thin global human-in-loop activation rule
AND ask_human is used only for real mandatory decision boundaries
AND required checkpoints fail closed when the tool/result is unavailable
AND ordinary already-authorized work does not receive redundant approval prompts
AND every normal terminal verdict attempts a compact notify_human report
AND notification failure never falsifies the established task verdict
AND MCP registration is machine-local and preserves existing Codex configuration
AND global/project/task rules remain separate owners rather than copied manuals
```
