# Current design map

This directory is the sole current living-design authority. Historical reasoning belongs in `reports/concept/`.

| File | design_id | Responsibility |
| --- | --- | --- |
| `00_overview.md` | `system-overview` | Product scope, architecture, upstream/reuse boundary, whole-system invariants |
| `01_interaction_protocol.md` | `interaction-protocol` | Canonical structured confirmation, bounded iMessage rendering, reply correlation, image admission |
| `02_channel_coordination.md` | `channel-coordination` | Exactly-two-channel activation, racing, support/fallback semantics |
| `03_imessage_channel.md` | `imessage-channel` | Free iMessage-only transport through `openclaw/imsg`, permissions, send/watch behavior, fail-closed rules |
| `04_feishu_channel.md` | `feishu-channel` | Feishu card transport retained from AskHuman and aligned with the canonical protocol |

Routine reading: start here, then load only the topic that owns the active concern. Load `00_overview.md` only when whole-system context is needed.
