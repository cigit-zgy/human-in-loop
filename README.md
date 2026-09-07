# human-in-loop

A focused human-in-the-loop bridge for coding agents. The maintained remote delivery surfaces are **Feishu** and **Apple Messages via iMessage only**.

The project adapts the open-source architecture of [`Naituw/AskHuman`](https://github.com/Naituw/AskHuman) and reuses [`openclaw/imsg`](https://github.com/openclaw/imsg) as an external macOS transport dependency. Carrier messaging is deliberately excluded: no SMS, MMS, RCS, paid messaging gateway, or automatic carrier fallback is permitted.

## Current status

The working application is adapted from the pinned AskHuman 0.13.1 source baseline. The current accepted design lives under [`design/`](design/README.md); historical reasoning under `reports/concept/` never overrides it.

Feishu retains its long-connection and interactive-card flow. Apple Messages support requires the external `imsg` executable plus Full Disk Access and Messages automation permission on macOS. Configure an existing direct iMessage conversation by saving its exact recipient handle, chat ID, and chat GUID; the adapter revalidates all three before each explicit iMessage-only send.

## Architecture

```text
Codex / Agent
    ↓
Human-in-loop core
    ↓ canonical structured confirmation
Coordinator
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
- iMessage is a bounded mobile decision surface, not free-form agent chat.
- Images are sent only when already supplied as decision evidence and admitted by the iMessage renderer.
- `openclaw/imsg` remains an external dependency; its source is not vendored.

See [`THIRD_PARTY.md`](THIRD_PARTY.md) for pinned upstream coordinates and reuse decisions.
