# human-in-loop

A focused human-in-the-loop bridge for coding agents. The maintained remote delivery surfaces are **Feishu** and **Apple Messages via iMessage only**.

The project adapts the open-source architecture of [`Naituw/AskHuman`](https://github.com/Naituw/AskHuman) and reuses [`openclaw/imsg`](https://github.com/openclaw/imsg) as an external macOS transport dependency. Carrier messaging is deliberately excluded: no SMS, MMS, RCS, paid messaging gateway, or automatic carrier fallback is permitted.

## Current status

The current accepted design lives under [`design/`](design/README.md). Historical design reasoning lives under `reports/concept/` and never overrides the living design.

Implementation is intentionally deferred until the accepted design and prior-art decision are committed; local Codex execution will then import/adapt the upstream AskHuman baseline and implement the iMessage channel against the pinned design.

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
