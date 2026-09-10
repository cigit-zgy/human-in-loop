# Current design map

This directory is the sole current living-design authority. Historical reasoning belongs in `reports/concept/`.

| File | design_id | Responsibility |
| --- | --- | --- |
| `00_overview.md` | `system-overview` | Product scope, architecture, upstream/reuse boundary, whole-system invariants |
| `01_interaction_protocol.md` | `interaction-protocol` | Canonical structured confirmation, bounded iMessage rendering, reply correlation, image admission |
| `02_channel_coordination.md` | `channel-coordination` | Exactly-two-channel activation, racing, support/fallback semantics |
| `03_imessage_channel.md` | `imessage-channel` | Dedicated Bot Apple Account/macOS-user transport through `openclaw/imsg`, session/identity health, strict send/watch behavior, notification qualification, and iMessage-only fail-closed rules |
| `04_feishu_channel.md` | `feishu-channel` | Feishu card transport retained from AskHuman and aligned with the canonical protocol |
| `05_mcp_interface.md` | `mcp-interface` | Minimal public MCP `ask_human` blocking decision + `notify_human` non-blocking notification surface |
| `06_codex_integration.md` | `codex-integration` | Machine-wide Codex activation, semantic checkpoint classification, terminal reporting, and consolidated host setup/recovery behavior |
| `07_macos_runtime_deployment.md` | `macos-runtime-deployment` | Stable macOS code identity, one-time onboarding/bootstrap, TCC lifecycle, `SETUP_COMPLETE`, recovery, protected-folder behavior, and unattended normal operation |
| `08_terminal_notification.md` | `terminal-notification` | Default-zero progress notifications, one terminal notification per task, and same-message labeled report/link presentation |

Routine reading: start here, then load only the topic that owns the active concern. Load `00_overview.md` only when whole-system context is needed.

Codex-facing operational behavior is projected into repository-root `SKILL.md`; transport implementation details remain in their owning design topics and code.
