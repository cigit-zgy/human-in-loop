---
name: human-in-loop
description: >
  Enforce human checkpoints and terminal human reporting for Codex/agent tasks.
  Use when a task may require a User decision, destructive/public authorization,
  design/scientific adjudication, security/credential permission, or when a task
  reaches PASS, PASS_WITH_LIMITATIONS, BLOCKED, or FAIL and should report the
  terminal result through the configured human-in-loop MCP tools.
---

# Human in Loop

Canonical maintained source:

```text
cigit-zgy/human-in-loop
```

Current project design authority lives in `design/`. This Skill is the Codex-facing operational projection of `design/05_mcp_interface.md`, `design/06_codex_integration.md`, and the setup/recovery boundary in `design/07_macos_runtime_deployment.md`.

## Capability boundary

Two public MCP tools have different semantics:

```text
ask_human
= blocking human decision

notify_human
= non-blocking informational notification
```

Never use `notify_human` as a substitute for a required decision. Never manufacture an acknowledgement choice merely to turn a notification into `ask_human`.

## Determine whether a semantic human decision is required

Set `ASK_HUMAN_REQUIRED` only when current higher authority has not already resolved/authorized the decision and at least one condition applies:

```text
EXPLICIT_CHECKPOINT
USER_PREFERENCE_DECISION
DESTRUCTIVE_OR_IRREVERSIBLE
EXTERNAL_OR_PUBLIC_SIDE_EFFECT
AUTHORITY_OR_SCIENTIFIC_CHANGE
SECURITY_CREDENTIAL_PERMISSION
```

Interpretation:

- `EXPLICIT_CHECKPOINT`: User/project/task authority explicitly requires a decision.
- `USER_PREFERENCE_DECISION`: multiple materially valid outcomes remain and only User preference selects among them.
- `DESTRUCTIVE_OR_IRREVERSIBLE`: persistent/shared state may be materially deleted, overwritten, replaced, or irreversibly migrated.
- `EXTERNAL_OR_PUBLIC_SIDE_EFFECT`: a meaningful outside-world action such as publication, release, public visibility, repository/account administration, or externally visible creation needs authorization.
- `AUTHORITY_OR_SCIENTIFIC_CHANGE`: current design, scientific/product semantics, trust/provenance, or another authority-bearing contract must be reopened.
- `SECURITY_CREDENTIAL_PERMISSION`: a genuinely new sensitive permission/credential/trust boundary is required and current authority has not already authorized it.

Routine reversible engineering work under a clear contract does not qualify.

Do not ask again for an action the User/current task already explicitly authorized unless a materially new decision boundary appears.

Normal task-authorized Git fetch/commit/push, testing, formatting, build, bounded retry, task-local tmp/worktree creation, stable-signed routine install/update, and health/preflight checks are not new checkpoints by themselves.

## Host setup/recovery mechanics are separate

Codex shell/sandbox prompts, administrator authentication, Apple Account login, Bot-user login, Full Disk Access, Files & Folders, and Automation/TCC controls are host/deployment mechanics. They are not automatically semantic `ask_human` decisions.

For human-in-loop's own macOS deployment:

```text
initial onboarding / explicit recovery
→ diagnose with non-mutating health/preflight first
→ surface one consolidated actionable USER_CHECKPOINT
→ User performs the minimum unavoidable host action
→ resume the same task

SETUP_COMPLETE + normal operation
→ administrator/Keychain password prompts: 0
→ ordinary Fast User Switching: 0
→ new Full Disk Access prompts: 0
→ new Automation prompts: 0
→ new Files & Folders prompts: 0
```

If one of these reappears after setup without an actual host-state change, classify it as deployment regression/recovery evidence. Do not repeatedly invoke commands merely to trigger the same permission prompt again.

A host step already authorized by the committed task does not need a second semantic `ask_human` choice just because macOS itself needs the User to enter a password or click a native consent control.

## Mandatory semantic decision lifecycle

When `ASK_HUMAN_REQUIRED` is true:

```text
1. Stop the affected path before the decision side effect.
2. Construct the smallest canonical structured request preserving all evidence needed for a safe decision.
3. Call ask_human.
4. Wait for exactly one valid correlated canonical result.
5. Continue only from returned stable selected_choice_id.
6. If the choice stops/cancels the path, terminate that path accordingly.
```

Do not substitute default approval, timeout approval, guessed preference, uncorrelated free-form interpretation, or silent fallback.

If `ask_human` is required but unavailable/fails before a valid result:

```text
→ BLOCKED
→ do not cross the checkpoint
```

## Request quality

A decision request normally includes only what the User needs to choose safely:

```text
repository_path when repository-associated
source_agent
concise question/action
optional multiline detail containing substantive decision evidence
2–6 stable choices
optional compact context/metadata
recommended choice only when justified
```

Use the fields semantically:

```text
question
→ concise decision to answer

detail
→ bounded multiline evidence/body; preserve meaningful line breaks

context
→ short metadata only, not a substitute for long-form evidence
```

Do not flatten a substantive multiline review/design/release card into one `context` line merely to satisfy a transport renderer. Do not remove evidence required for a safe scientific, design, security, or publication decision just to make the message shorter.

The normal iMessage decision-body defaults are intentionally generous enough for substantive decisions: approximately 1000 Unicode characters of `detail` and 1500 characters for the fully rendered message. The User may configure a larger operational budget within the product's absolute defensive ceiling. Never silently truncate content. If a payload exceeds the active safe budget, fail closed and surface the typed/redacted payload-size reason.

Do not include secrets, credentials, raw private transport identifiers, irrelevant logs, complete reports, or unbounded document/model dumps.

## Setup/recovery checkpoint quality

When host deployment rather than semantic choice blocks progress, do not create fake approval choices. Report one direct checkpoint in the normal local response/report, for example:

```text
USER_CHECKPOINT:
Log in once to the dedicated Bot macOS user after reboot, then return to the primary user.
```

or:

```text
USER_CHECKPOINT:
Open System Settings → Privacy & Security → Automation and enable Messages for the stable human-in-loop runtime.
```

After the User resolves it, continue the same still-valid task when semantics have not changed.

## Terminal reporting — required attempt

At every normal terminal task state:

```text
PASS
PASS_WITH_LIMITATIONS
BLOCKED
FAIL
```

attempt `notify_human` once the normal task result is already established.

For repository-changing work, ordering is:

```text
result/report complete
→ commit
→ push owning branch
→ fresh fetch
→ required local HEAD == fetched upstream proof
→ notify_human
→ final Codex response
```

The notification is compact: project, task id when available, verdict, one short summary/blocker, branch/commit/report locator when available.

Do not send complete reports, task bodies, private channel configuration, credentials, or long logs.

## Notification failure does not rewrite task truth

`notify_human` is non-blocking/best-effort after the verdict exists.

If notification fails:

```text
preserve underlying task verdict
+ surface HUMAN_NOTIFICATION: FAILED
```

Do not convert a valid PASS into FAIL solely because notification transport failed. Do not wait indefinitely for acknowledgement.

## Authority / stale-copy rule

When Codex uses this Skill machine-wide, the discovery copy must correspond to the exact accepted GitHub revision named by machine policy/task.

A similarly named local folder or stale checkout is not authority.

## Completion

The human-in-loop contract is satisfied for a task when:

```text
all mandatory semantic checkpoints were resolved by correlated ask_human results or caused fail-closed BLOCKED state
AND already-authorized routine work received no redundant semantic confirmation
AND substantive evidence was preserved in the proper decision-body surface rather than silently truncated
AND host setup/recovery mechanics were consolidated instead of repeatedly prompted
AND normal terminal truth was established honestly
AND notify_human was attempted
AND notification failure, if any, did not rewrite that truth
```
