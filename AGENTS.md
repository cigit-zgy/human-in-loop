# human-in-loop repository context

## Identity

This repository is the maintained human-in-the-loop bridge for coding agents. The maintained remote delivery surface is Apple Messages using iMessage only.

The maintained production identity is `human-in-loop`. AskHuman is upstream provenance/code history, not an installed runtime or product identity.

## Authority

```text
explicit User instruction
→ design/                                  current accepted project design
→ SKILL.md                                 Codex-facing operational projection
→ existing AskHuman-derived implementation implementation detail when consistent with current design
→ src-tauri/ + src/ + scripts/             implementation/deployment
→ tests                                     conformance evidence
```

`reports/concept/` is chronological exploration/history only; it never overrides current `design/`.

Global collaboration authority for the current accepted project workflow:

```text
cigit-zgy/agent-collaboration@8601466216515125bf8b17893b2a8e8673bab79e
```

Do not substitute another collaboration revision implicitly. Adoption of a newer collaboration revision is an explicit project-authority update.

Do not copy collaboration manuals into this repository; resolve behavior through the pinned source.

## Upstream and external dependencies

```text
Naituw/AskHuman@77e2e576347f94ef203bc2426b73a18749cb4e92
= historical/upstream product and code basis; ADAPTED
= not a maintained runtime dependency

openclaw/imsg@646ea7af9616dc3e6406d86aa269bf4fb1b07a76
= external Apple Messages transport; REUSE as installed dependency, never vendored
```

Preserve required upstream license/provenance files even though independent AskHuman runtime/install state is removed.

## Ownership

```text
design/                     sole current living-design authority
SKILL.md                    generic Codex checkpoint + terminal-report contract
reports/concept/            chronological design history/input
reports/chatgpt/            committed FORMAL task authority
reports/codex/              Codex execution/verification evidence
src-tauri/src/models.rs     canonical request/result data model
src-tauri/src/channels/     maintained iMessage channel and shared canonical channel logic
src-tauri/src/mcp/          public MCP ask_human / notify_human surface
scripts/                    installation/bootstrap/deployment projection
```

`tmp/` is the only project-local Agent ephemeral boundary for local task state.

## Development workflow

Routine current-state reading:

```text
AGENTS.md
→ design/README.md
→ directly relevant design topic(s)
→ SKILL.md when Codex-facing behavior is involved
→ implementation/tests
```

New design-bearing work:

```text
current design + relevant evidence/history
→ reports/concept/ exploration when material
→ User + ChatGPT adjudication
→ atomic current design/ update
→ update SKILL.md when operational behavior changed
→ only then implementation/tests
```

FORMAL local work follows the pinned collaboration lifecycle:

```text
ChatGPT DIRECT concept/design/Skill/code that can be authored remotely
→ committed task branch baseline
→ committed reports/chatgpt task
→ Codex remaining local implementation/verification
→ reports/codex evidence
→ ChatGPT acceptance review
→ permitted integration/release checkpoint
```

Codex must fresh-fetch before repository-changing local execution and preserve pre-existing User state. User-machine scratch belongs under `<PROJECT_ROOT>/tmp/<TASK_ID>/`.

## Product release boundary

The core functional path is considered proven only by real evidence for:

```text
MCP / Codex
→ canonical coordinator
→ dedicated Bot-user worker
→ distinct Bot Apple Account
→ explicit iMessage-only delivery
→ genuine incoming iPhone notification
→ structured reply
→ strict canonical correlation
```

Release readiness additionally requires the onboarding/deployment contract in `design/07_macos_runtime_deployment.md`.

Once the installation reaches `SETUP_COMPLETE`, normal operation is expected to be unattended. Repeated administrator/Keychain passwords, Fast User Switching, Full Disk Access prompts, Automation prompts, or Files & Folders prompts are defects/recovery states rather than normal workflow.

### Standing release authorization

The User has explicitly authorized routine publication without a separate release-confirmation checkpoint after ChatGPT has accepted the implementation candidate.

```text
ChatGPT acceptance PASS
+ release gates remain satisfied
→ integrate / install / qualify / pin / tag / publish directly
```

Do not call `ask_human` merely to reconfirm merge, tag, version publication, or GitHub Release creation after such acceptance. This standing authorization does not waive a genuinely new material decision involving security/privacy boundary expansion, licensing or repository visibility, destructive history/state change, new external service/credential scope, or another public side effect materially different from the already accepted release plan. Such a new boundary still fails closed to the normal human-decision contract.

## Human / trust checkpoints

- Carrier messaging is prohibited. Apple Messages is iMessage-only and fails closed if actual iMessage delivery cannot be proven.
- Production Apple Messages uses a distinct Bot Apple Account and a dedicated logged-in Bot macOS user.
- Credentials, Apple passwords, 2FA, signing private keys, and private message contents are never committed or requested into project reports/logs.
- A renderer may compact presentation but never omit information required for a safe decision.
- `ask_human` is blocking/correlated decision semantics; `notify_human` is non-blocking informational semantics.
- Setup/recovery host mechanics are not repeated semantic approval decisions. Surface one owning actionable checkpoint and resume the same task after the User resolves it.
- Machine-wide Codex installation/configuration must preserve unrelated `$CODEX_HOME/AGENTS.md` and `$CODEX_HOME/config.toml` content.

## Hard invariants

- Exactly one maintained remote delivery channel: `imessage`.
- Feishu is not a maintained capability and must not remain in current runtime/config/UI/dispatch/secret-loading surfaces.
- No Telegram, Slack, DingTalk, WeChat, SMS, MMS, RCS, paid gateway, or automatic carrier fallback.
- Canonical confirmation semantics remain transport-independent even though only one maintained remote transport exists.
- iMessage direct sends always use explicit iMessage selection and defense-in-depth no-SMS fallback.
- `openclaw/imsg` remains external.
- `design/` contains exactly one current accepted design set; no old/draft/versioned alternatives.
- Stable installed runtime identity is required before macOS privacy grants are qualified.
- `SETUP_COMPLETE` normal operation requires no recurring password, user-switch, or TCC consent interaction.
- Historical reports and Git history are audit evidence and are not rewritten solely to erase retired Feishu references.
