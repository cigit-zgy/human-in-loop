# human-in-loop repository context

## Identity

This repository is the maintained human-in-the-loop bridge for coding agents. The maintained remote delivery surfaces are intentionally limited to Feishu and Apple Messages using iMessage only.

## Authority

```text
explicit User instruction
→ design/                                  current accepted project design
→ SKILL.md                                 Codex-facing operational projection
→ existing AskHuman specs/docs             implementation detail when consistent with design/
→ src-tauri/ + src/                         implementation
→ tests                                     conformance/design evidence
```

`reports/concept/` is chronological design exploration/history only; it does not override current `design/`.

Global collaboration authority:
`cigit-zgy/agent-collaboration@ad88170b23920ddac0bff9a2fd467aa0c59917cf`

Do not copy collaboration manuals into this repository. Resolve global collaboration behavior through the pinned collaboration Skill/reference owners when needed.

## Upstream and external dependencies

```text
Naituw/AskHuman@77e2e576347f94ef203bc2426b73a18749cb4e92
= upstream product/code basis; ADAPT

openclaw/imsg@646ea7af9616dc3e6406d86aa269bf4fb1b07a76
= external Apple Messages transport; REUSE as an installed dependency, never vendored
```

Both are MIT-licensed at the pinned inspected coordinates. Preserve required attribution and license notices.

## Ownership

```text
design/                     current living design authority
SKILL.md                    generic Codex checkpoint + terminal-report operating contract
reports/concept/            chronological design history/input
src-tauri/src/models.rs     canonical request/result data model
src-tauri/src/channels/     channel implementations
src-tauri/src/mcp/          public MCP ask_human / notify_human surface
src/views/settings/         channel configuration UI
```

`tmp/` is the project-local Agent ephemeral boundary when local execution needs temporary artifacts.

## Workflow

Current design / conformance:

```text
AGENTS.md
→ design/README.md
→ directly relevant current design topic(s)
→ SKILL.md when Codex-facing behavior is involved
→ implementation/tests
```

For Codex machine-wide integration read only:

```text
design/05_mcp_interface.md
+ design/06_codex_integration.md
+ SKILL.md
```

unless a concrete transport concern requires its owning channel topic.

Historical rationale or new design exploration:

```text
current design topic
+ only relevant reports/concept/YYMMDD_concept_NN.md
→ User + ChatGPT adjudication
→ update design/ if accepted
```

For substantial new architecture/tool choices, complete the pinned collaboration prior-art route before changing `design/`.

For implementation work, preserve the upstream AskHuman verification discipline. Functional/logic changes require the repository's normal build/install/tests, with macOS Messages/iMessage end-to-end evidence delegated to local Codex when required.

## Human / trust checkpoints

- Carrier messaging is prohibited. Apple Messages delivery is iMessage-only and must fail closed if iMessage cannot be used.
- A channel renderer may reduce presentation detail to its bounded surface, but it may never omit information required for a safe user decision.
- iMessage accepts only supported bounded confirmation/notification surfaces; unsupported content is not partially rendered into a misleading interaction.
- Credentials and message contents remain local except where the selected delivery service necessarily transmits them.
- Machine-wide Codex installation/configuration must preserve unrelated existing `$CODEX_HOME/AGENTS.md` and `$CODEX_HOME/config.toml` content; do not replace either file wholesale.

## Hard invariants

- Exactly two maintained remote delivery channels are product-supported: `feishu` and `imessage`.
- No Telegram, Slack, DingTalk, WeChat, SMS, MMS, RCS, paid messaging gateway, or automatic carrier fallback is product-supported.
- Canonical confirmation semantics are transport-independent; Feishu cards and iMessage text render the same decision request/result objects.
- `ask_human` is blocking/correlated decision semantics; `notify_human` is non-blocking informational semantics. Do not collapse them into one fake acknowledgement model.
- iMessage transport always uses explicit iMessage selection and never `auto` or `sms` service selection.
- `openclaw/imsg` remains an external dependency; do not copy its source into this repository.
- `design/` contains one current accepted design set only; no old/draft/versioned alternatives.
