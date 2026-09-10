---
design_id: codex-integration
title: Codex machine-wide human-in-the-loop integration
status: active
role: design_authority
summary: >
  Defines global Codex activation, checkpoint classification, fail-closed human
  decisions, terminal reporting, and the rule that host permission mechanics are
  consolidated setup/recovery events rather than recurring semantic prompts.
operational_projection:
  - SKILL.md
  - $CODEX_HOME/AGENTS.md
  - $CODEX_HOME/config.toml
---

# Purpose

Make `human-in-loop` a dependable machine-wide Codex capability without copying its full protocol into every repository/task and without turning macOS permission mechanics into repeated human-approval spam.

The integration has four policy owners:

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

MCP registration is owned separately by `$CODEX_HOME/config.toml`. macOS installation/TCC lifecycle is owned by `07_macos_runtime_deployment.md`.

# Global activation contract

For every Codex task:

```text
load/obey the maintained human-in-loop Skill

mandatory semantic checkpoint
→ stop affected path
→ call ask_human
→ continue only from one correlated canonical human result

normal terminal task state
→ complete durable result/report and required Git synchronization
→ attempt notify_human
→ return the normal final Codex response
```

The Codex-home AGENTS layer stays thin. It does not copy the full classifier, MCP schema, transport details, iMessage grammar, or macOS setup manual.

# Checkpoint classifier

`ASK_HUMAN_REQUIRED` is true only when current higher authority has not already resolved/authorized the decision and at least one condition applies:

```text
EXPLICIT_CHECKPOINT
USER_PREFERENCE_DECISION
DESTRUCTIVE_OR_IRREVERSIBLE
EXTERNAL_OR_PUBLIC_SIDE_EFFECT
AUTHORITY_OR_SCIENTIFIC_CHANGE
SECURITY_CREDENTIAL_PERMISSION
```

Routine reversible engineering work under a clear contract does not qualify.

Do not ask again for an action already explicitly authorized by the User/current task unless a materially new decision boundary appears.

# Host permission mechanics are not semantic decisions

Codex shell/sandbox prompts, administrator authentication, macOS TCC consent, Apple Account login, and Bot-user login are host/deployment mechanics. They are not converted into repeated `ask_human` choices merely because execution needs them.

Their behavior is:

```text
initial onboarding / explicit recovery
→ owning deployment state reports one consolidated actionable USER_CHECKPOINT
→ User performs the minimum unavoidable host action
→ Codex resumes the same task

SETUP_COMPLETE + ordinary operation
→ no recurring password/user-switch/TCC prompt is acceptable
→ recurrence is deployment regression/recovery, not normal checkpoint behavior
```

Codex must not deliberately trigger the same denied/missing permission through several commands in order to obtain repeated prompts. Diagnose with non-mutating health/preflight first, then surface one owning recovery action.

When a host permission state is already covered by the committed task/User authorization, routine setup commands may proceed without an additional semantic `ask_human` decision; macOS itself may still require the User to enter a password or click a consent control once.

# What must NOT trigger ask_human

Examples that normally proceed without another semantic checkpoint when already covered by authority:

```text
routine reversible implementation choice
normal test/format/build command
expected bounded retry under accepted recovery semantics
task-local tmp/worktree creation
task-authorized commit/push/fresh-fetch
routine stable-signed install/update after onboarding
worker health/preflight checks
an already-authorized setup step whose only remaining action is the host's native password/TCC UI
```

# Mandatory decision lifecycle

```text
checkpoint detected
→ construct smallest safe canonical request
→ include real project/task context
→ call ask_human
→ pause affected path
→ accept one correlated canonical result
→ map stable selected_choice_id to continuation
→ continue/stop exactly according to that choice
```

Never substitute default approval, timeout approval, guessed preference, uncorrelated free-form interpretation, or silent fallback.

If a required semantic checkpoint cannot obtain a valid human result:

```text
TASK STATE → BLOCKED
```

# Deployment/recovery lifecycle

If the product is not `SETUP_COMPLETE`, or a material host predicate regresses, Codex reports the single owning state from the deployment/channel health model, for example:

```text
ADMIN_AUTH_REQUIRED
BOT_SESSION_LOGIN_REQUIRED
BOT_MESSAGES_ACCOUNT_UNAVAILABLE
permission_missing / automation_consent_required
RUNTIME_IDENTITY_MIGRATION_REQUIRED
```

The final user-facing instruction must be compact and actionable. Repeated copies of the same unresolved host checkpoint are suppressed.

After setup, ordinary Codex/MCP/channel operation is expected to run while the User is away from the Mac. A task that unexpectedly requires a new local password, user switch, or TCC consent must treat that as recovery/defect evidence and not silently normalize it.

# Terminal notification lifecycle

For every normal terminal verdict:

```text
PASS
PASS_WITH_LIMITATIONS
BLOCKED
FAIL
```

ordering for repository-changing work is:

```text
complete task result/report
→ commit task-scoped changes
→ push owning branch
→ fresh fetch / required equality proof
→ attempt notify_human
→ final Codex response
```

The notification is compact: project, task id when available, verdict, one short summary/blocker, and durable locator when available. It never contains credentials, raw transport ids, complete reports, or long logs.

Notification failure does not rewrite the established task verdict.

# Skill authority and MCP registration

The maintained Skill is first-party and remote-canonical:

```text
cigit-zgy/human-in-loop@<accepted-commit>:SKILL.md
```

Machine-wide discovery must resolve an exact accepted revision; a stale local folder is not authority.

`$CODEX_HOME/config.toml` registers the production local MCP server only. It does not embed transport credentials or duplicate policy. Existing unrelated Codex configuration is preserved.

Production MCP registration MUST use the same canonical human-in-loop runtime configuration as the interactive CLI and daemon. The managed Codex MCP entry MUST NOT create, select, or preserve a separate product-owned runtime home such as a legacy `human-in-loop-codex` configuration root. In particular, a human-in-loop-managed `HUMAN_IN_LOOP_HOME` override that points to the known legacy isolated MCP home is migration debt and must be removed by install/update migration.

Unknown User-authored custom runtime-home overrides are not automatically owned by human-in-loop. If an installer encounters a custom `HUMAN_IN_LOOP_HOME` value that is not a known managed legacy value, it must preserve it or surface an explicit migration decision rather than silently overwrite it.

The production responsibility split is:

```text
Skill
= policy: when to ask_human / notify_human

MCP
= primary structured invocation surface

canonical human-in-loop config + daemon
= shared runtime/channel state

CLI
= supported recovery/debug/fallback surface, not a second production configuration world
```

MCP and CLI may have different process lifecycles, but they must not diverge into separate human-in-loop configuration universes merely because they are different invocation surfaces.

# Project/task extensions

Projects/tasks add only genuinely domain-specific checkpoints, for example scientific ambiguity, destructive replacement of validated artifacts, login/MFA/CAPTCHA, publication/release authorization, or repository visibility/licensing changes.

Do not copy such project-specific semantics into the global Skill unless they generalize.

# Design acceptance

This integration is conforming when:

```text
all Codex tasks receive one thin global human-in-loop activation rule
AND ask_human is used only for real semantic decision boundaries
AND required decisions fail closed when no valid result exists
AND already-authorized routine work receives no redundant approval prompts
AND setup/recovery host mechanics are consolidated rather than repeatedly prompted
AND SETUP_COMPLETE normal operation requires no recurring password/user-switch/TCC interaction
AND every normal terminal verdict attempts compact notify_human reporting
AND Codex MCP uses the same canonical runtime configuration as CLI/daemon
AND human-in-loop does not maintain a separate product-owned MCP configuration home
AND unknown User custom runtime-home overrides are not silently destroyed
AND CLI remains a recovery/debug fallback rather than a competing production configuration owner
AND MCP registration/policy/project/task concerns remain separate owners
```
