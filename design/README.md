# Current design map

This directory is the sole current living-design authority. Historical reasoning belongs in `reports/concept/`.

| File | design_id | Responsibility |
| --- | --- | --- |
| `00_overview.md` | `system-overview` | Product scope, architecture, upstream/reuse boundary, whole-system invariants |
| `01_interaction_protocol.md` | `interaction-protocol` | Canonical structured confirmation, bounded iMessage rendering, reply correlation, image admission |
| `02_channel_coordination.md` | `channel-coordination` | Single maintained remote-channel readiness, support/fail-closed semantics, and canonical result normalization |
| `03_imessage_channel.md` | `imessage-channel` | Dedicated Bot Apple Account/macOS-user transport through `openclaw/imsg`, session/identity health, strict send/watch behavior, notification qualification, and iMessage-only fail-closed rules |
| `05_mcp_interface.md` | `mcp-interface` | Minimal public MCP `ask_human` blocking decision + `notify_human` non-blocking notification surface |
| `06_codex_integration.md` | `codex-integration` | Machine-wide Codex activation, semantic checkpoint classification, terminal reporting, and consolidated host setup/recovery behavior |
| `07_macos_runtime_deployment.md` | `macos-runtime-deployment` | Stable macOS code identity, one-time onboarding/bootstrap, TCC lifecycle, `SETUP_COMPLETE`, recovery, protected-folder behavior, and unattended normal operation |
| `08_terminal_notification.md` | `terminal-notification` | Default-zero progress notifications, one terminal notification per task, and same-message report/link presentation |
| `09_optional_bot_autologin.md` | `optional-bot-autologin` | Explicit opt-in automatic login for the dedicated non-admin Bot user, host feasibility/security boundaries, and manual post-reboot fallback |
| `10_mobile_reply_and_link_presentation.md` | `mobile-reply-and-link-presentation` | Decimal-only `TOKEN-OPTION` phone replies and no-large-preview report-link qualification |
| `11_imessage_only_product_scope.md` | `imessage-only-product-scope` | Sole maintained remote channel = iMessage; complete removal of Feishu from the current product surface and recovery of the blocked daemon startup path |

`11_imessage_only_product_scope.md` supersedes any stale Feishu/two-channel clause remaining in older current-design topics until the implementation task mechanically purges those clauses. Feishu has no current design authority.

Routine reading: start here, then load only the topic that owns the active concern. Load `00_overview.md` only when whole-system context is needed.

Codex-facing operational behavior is projected into repository-root `SKILL.md`; transport implementation details remain in their owning design topics and code.
