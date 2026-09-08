# human-in-loop

A focused human-in-the-loop bridge for coding agents. The maintained remote delivery surfaces are **Feishu** and **Apple Messages via iMessage only**.

The project adapts the open-source architecture of [`Naituw/AskHuman`](https://github.com/Naituw/AskHuman) and reuses [`openclaw/imsg`](https://github.com/openclaw/imsg) as an external macOS transport dependency. Carrier messaging is deliberately excluded: no SMS, MMS, RCS, paid messaging gateway, or automatic carrier fallback is permitted.

## Current status

The working application is adapted from the pinned AskHuman 0.13.1 source baseline. The current accepted design lives under [`design/`](design/README.md); historical reasoning under `reports/concept/` never overrides it.

Feishu retains its long-connection and interactive-card flow. Apple Messages support requires the external `imsg` executable plus Full Disk Access and Messages automation permission on macOS. Configure an approved recipient and choose `distinct_peer` or `same_account`; an existing direct iMessage chat is revalidated, while the first structured confirmation can bootstrap a missing chat through explicit iMessage-only delivery.

## Local MCP interface

Configure an MCP client to launch the installed `human-in-loop` executable with the argument `mcp`. The server uses local stdio and exposes exactly two tools: `ask_human` for blocking decisions and `notify_human` for informational dispatch through the configured channels.

`ask_human` requires `source_agent`, `question`, and 2–6 `choices`, each with a unique stable `id` and compact `label`. Supply `repository_path` for every repository-associated decision; the server resolves the GitHub origin locally and displays its repository slug, such as `Codex · human-in-loop`. Optional fields are `context`, `recommended_choice` (a choice id), and `request_id`.

The blocking call returns `request_id`, `selected_choice_id`, and `source_channel_id`. The selected id is the semantic choice id, not the phone's numeric option. Cancellation or client disconnect cancels the pending confirmation and cleans up its channel watchers. Keep decision questions and labels compact; the existing channel admission limits still apply.

`notify_human` requires `source_agent`, `status` (`PASS`, `PASS_WITH_LIMITATIONS`, `BLOCKED`, or `FAIL`), and a compact `summary`. Supply `repository_path` for repository-associated notifications; optional fields are `context`, `task_id`, `locator`, and `notification_id`. Repository identity uses the same canonical GitHub-origin resolution as decisions and fails closed when that identity is unavailable.

`context` is an array of up to two `{ "label": "…", "value": "…" }` fields. The summary is limited to 160 characters, the locator to 240, and the rendered notification to 700; oversized content is rejected without truncation. HTTP(S) locators must not contain URL credentials.

Notification dispatch returns `notification_id`, `delivery_status`, and participating channel types in `channel_ids`. It creates no pending decision and requires no reply or acknowledgement. A dispatch result does not establish that the user read the message. Keep summaries and evidence locators compact and free of secrets or private transport details.

Recipient identity, channel credentials, raw transport commands, generic file operations, and free-form questionnaires are not MCP inputs. Configuration stays in the local application. This interface provides no public HTTP endpoint or GitHub message relay. Remote MCP deployment and authentication are separate work; local stdio support does not establish ChatGPT Pro remote mutation support. See the [MCP contract](design/05_mcp_interface.md).

## Codex machine-wide integration

Expose the repository-root [`SKILL.md`](SKILL.md) through supported user-scoped Codex Skill discovery, bound to an immutable `cigit-zgy/human-in-loop@<accepted-commit>:SKILL.md` coordinate. Verify that the discovery copy matches that revision; a symlink to a moving checkout is not a version pin. In the effective `$CODEX_HOME/config.toml`, register the installed production executable with `mcp` as its argument. Preserve unrelated settings and discovery entries; keep private channel configuration in the application.

Add only a thin activation rule to the effective `$CODEX_HOME/AGENTS.md`, pointing to that pinned Skill. Required decisions use `ask_human` and fail closed to `BLOCKED` if no valid correlated result is available. Already-authorized routine work needs no redundant confirmation. After the normal task result, report, and required commit/push/fresh-fetch synchronization, attempt `notify_human` before the final Codex response. Notification failure preserves the task verdict and is reported separately as `HUMAN_NOTIFICATION: FAILED`. See the [Codex integration contract](design/06_codex_integration.md) for installation ownership and lifecycle details.

## Architecture

```text
Codex / Agent
    ↓
Human-in-loop core
    ↓ canonical decision / notification
Decision coordination / notification dispatch
    ├── Feishu
    └── Apple Messages renderer
             ↓
        openclaw/imsg
             ↓
       Messages.app
             ↓
       iMessage only
             ↓
           iPhone
```

## Design invariants

- Exactly two maintained remote channels: `feishu` and `imessage`.
- Apple Messages is **iMessage-only** and fails closed if iMessage cannot be used.
- Structured confirmation semantics are transport-independent.
- Notifications create no pending decision and do not wait for acknowledgement.
- iMessage is a bounded mobile decision surface, not free-form agent chat.
- Images are sent only when already supplied as decision evidence and admitted by the iMessage renderer.
- `openclaw/imsg` remains an external dependency; its source is not vendored.

See [`THIRD_PARTY.md`](THIRD_PARTY.md) for pinned upstream coordinates and reuse decisions.
