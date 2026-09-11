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

Current project design authority lives in `design/`. This Skill is the Codex-facing operational projection of `design/05_mcp_interface.md`, `design/06_codex_integration.md`, `design/08_terminal_notification.md`, `design/09_optional_bot_autologin.md`, `design/10_mobile_reply_and_link_presentation.md`, and the setup/recovery boundary in `design/07_macos_runtime_deployment.md`.

The maintained remote delivery channel is Apple Messages using explicit iMessage only. If that channel is unavailable, mandatory decisions fail closed; no alternate remote or carrier transport is selected.

## Capability boundary

Two public MCP tools have different semantics:

```text
ask_human
= blocking human decision

notify_human
= non-blocking informational notification
```

Never use `notify_human` as a substitute for a required decision. Never manufacture an acknowledgement choice merely to turn a notification into `ask_human`.

By default, proactive messaging is intentionally sparse:

```text
unresolved semantic decision
→ ask_human

normal terminal task state
→ at most one notify_human attempt

ordinary progress / heartbeat / percentage-complete
→ no proactive message
```

Progress notifications require explicit project/task/User opt-in. Do not infer them merely because a task is long-running.

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

A Mac reboot is not re-onboarding. If the dedicated Bot graphical/login session is absent after reboot, the only normal recovery action is to establish that Bot login session once. Existing Apple Account/Message activation, TCC grants, signing identity, channel configuration, MCP registration, and Skill configuration must not be redone merely because the machine rebooted.

If one of these permissions or identities reappears as missing after setup without an actual host-state change, classify it as deployment regression/recovery evidence. Do not repeatedly invoke commands merely to trigger the same permission prompt again.

A host step already authorized by the committed task does not need a second semantic `ask_human` choice just because macOS itself needs the User to enter a password or click a native consent control.

## Optional dedicated-Bot automatic login

Automatic login is an opt-in setup convenience, never a normal-operation prompt or silent default.

```text
non-mutating feasibility preflight
→ unsupported host: keep manual Bot login after reboot
→ supported exact dedicated non-admin Bot: ask_human once
→ enable_bot_autologin: open the supported native macOS configuration surface and verify afterward
→ manual_bot_login: record the non-secret preference and keep the manual fallback
```

The decision request uses the stable choices `enable_bot_autologin` and `manual_bot_login`, states the physical-access trade-off, and has no recommended choice by default. A stable recorded choice is not asked again unless the User changes it or host capability materially changes.

Only the configured dedicated non-admin Bot user may be targeted. Never target the primary user, an administrator, root, or an arbitrary caller-supplied account.

FileVault, managed policy, or an incompatible account type makes automatic login unavailable. Keep setup valid and use this recovery path after a reboot:

```text
BOT_SESSION_LOGIN_REQUIRED
→ User logs into the dedicated Bot user once
→ User returns to the primary account
```

Never disable FileVault, weaken SIP/TCC, install a broad privileged helper, use a private loginwindow bypass, or capture a macOS/Apple password or 2FA secret. When macOS requires authentication, the User enters it only in the native macOS interface; human-in-loop reads and stores only the resulting non-secret configuration state.

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

Meaningful paragraph spacing is part of the human-readable evidence surface. Preserve application-supplied blank lines between distinct content blocks when within budget; do not collapse headings, evidence paragraphs, choices, or other logical blocks into visually dense adjacent lines merely to save space.

For phone decisions, the canonical reply footer is copy-oriented and rendered as:

```text
Reply:

<TOKEN>-<OPTION_NUMBER>
```

The reply value is its own paragraph. Do not render it on the same line as `Reply:`.

Do not flatten a substantive multiline review/design/release card into one `context` line merely to satisfy a transport renderer. Do not remove evidence required for a safe scientific, design, security, or publication decision just to make the message shorter.

The normal iMessage decision-body defaults are intentionally generous enough for substantive decisions: approximately 1000 Unicode characters of `detail` and 1500 characters for the fully rendered message. The User may configure a larger operational budget within the product's absolute defensive ceiling. Never silently truncate content. If a payload exceeds the active safe budget, fail closed and surface the typed/redacted payload-size reason.

Do not include secrets, credentials, raw private transport identifiers, irrelevant logs, complete reports, or unbounded document/model dumps.

## Setup/recovery checkpoint quality

When host deployment rather than semantic choice blocks progress, do not create fake approval choices. Report one direct checkpoint in the normal local response/report, for example:

```text
USER_CHECKPOINT:
After a reboot, log in once to the dedicated Bot macOS user so its Messages session is active, then return to the primary user. Do not rerun onboarding or reconfigure TCC/Apple Account unless health explicitly shows that state was actually lost.
```

or:

```text
USER_CHECKPOINT:
Open System Settings → Privacy & Security → Automation and enable Messages for the stable human-in-loop runtime.
```

After the User resolves it, continue the same still-valid task when semantics have not changed.

## Terminal reporting — one terminal message by default

For every logical task execution reaching a normal terminal state:

```text
PASS
PASS_WITH_LIMITATIONS
BLOCKED
FAIL
```

attempt `notify_human` at most once after the normal task result is already established.

For repository-changing work, ordering is:

```text
result/report complete
→ commit
→ push owning branch
→ fresh fetch
→ required local HEAD == fetched upstream proof
→ one terminal notify_human attempt
→ final Codex response
```

Do not send separate completion, report-ready, push-complete, release-complete, or other progress notifications for the same terminal event. Collapse materially useful facts into the single terminal message.

The notification is compact: project, task id when available, verdict, one short summary/blocker, and durable locator when available. It may include up to two short context fields when materially useful.

When a durable locator exists, keep it inside the same application message as the summary. Render HTTP(S) locators as a labeled quoted URL line so the full unchanged URL stays readable/copyable without expanding into a large Apple Rich Link Preview:

```text
Report: "https://github.com/.../report"
```

Do not send a separate link-only message. Keep every URL character intact inside the wrapper so clients such as Apple Messages may still auto-detect it as tappable. Do not rely on Markdown link syntax for plain iMessage rendering.

Do not send complete reports, task bodies, private channel configuration, credentials, or long logs.

## Notification failure does not rewrite task truth

`notify_human` is non-blocking/best-effort after the verdict exists.

If notification fails:

```text
preserve underlying task verdict
+ surface HUMAN_NOTIFICATION: FAILED
```

Do not convert a valid PASS into FAIL solely because notification transport failed. Do not wait indefinitely for acknowledgement. Do not retry an uncertain terminal delivery when a duplicate message could result.

## Authority / stale-copy rule

When Codex uses this Skill machine-wide, the discovery copy must correspond to the exact accepted GitHub revision named by machine policy/task.

A similarly named local folder or stale checkout is not authority.

## Completion

The human-in-loop contract is satisfied for a task when:

```text
all mandatory semantic checkpoints were resolved by correlated ask_human results or caused fail-closed BLOCKED state
AND already-authorized routine work received no redundant semantic confirmation
AND substantive evidence was preserved in the proper decision-body surface rather than silently truncated
AND meaningful paragraph spacing and copy-friendly reply presentation were preserved
AND default progress-notification count remained zero unless explicitly opted in
AND no more than one terminal notification attempt was made for the logical task execution
AND any durable report/link locator remained in the same terminal application message
AND host setup/recovery mechanics were consolidated instead of repeatedly prompted
AND a reboot alone did not trigger re-onboarding or reconfiguration
AND normal terminal truth was established honestly
AND notification failure, if any, did not rewrite that truth
```
