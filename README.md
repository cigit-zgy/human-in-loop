# human-in-loop

A focused human-in-the-loop bridge for coding agents. The maintained remote delivery surfaces are **Feishu** and **Apple Messages via iMessage only**.

The project originated by adapting the open-source architecture of [`Naituw/AskHuman`](https://github.com/Naituw/AskHuman) and reuses [`openclaw/imsg`](https://github.com/openclaw/imsg) as an external macOS transport dependency. AskHuman is not a maintained runtime dependency or product identity. Carrier messaging is deliberately excluded: no SMS, MMS, RCS, paid messaging gateway, or automatic carrier fallback is permitted.

## Current status

The core release-candidate path has been qualified on macOS with a real distinct-account Apple Messages round trip:

```text
Codex / MCP
→ human-in-loop coordinator
→ dedicated Bot-user worker
→ Bot Apple Account
→ iMessage only
→ locked iPhone notification
→ structured reply
→ canonical result
```

The current accepted design lives under [`design/`](design/README.md); historical reasoning under `reports/concept/` never overrides it.

The project is in **release-finalization**: the functional architecture is proven, while public onboarding/packaging, reboot/recovery qualification, default-branch integration, and final release tagging remain explicit release work.

## Production Apple Messages topology

Production Apple Messages uses a **dedicated macOS Bot user** and a **distinct Bot Apple Account**. The personal macOS user and personal Messages account remain unchanged.

```text
primary macOS user
├── Codex / Agent / MCP / coordinator
└── authenticated local IPC
       ↓
dedicated Bot macOS user (documented default: human-in-loop)
├── Bot Messages.app signed into a separate Apple Account
├── Bot transport worker
└── openclaw/imsg
       ↓
   iMessage only
       ↓
personal iPhone / personal Apple Account
```

A self-message/same-Apple-Account topology may remain diagnostic compatibility, but it is **not production-qualified** and cannot establish production readiness.

All direct Apple sends remain explicit iMessage with no SMS fallback. If actual iMessage eligibility cannot be proven, the channel fails closed.

## One-time setup, unattended normal operation

The product is designed for users who may be away from the Mac when an Agent needs a decision. Therefore setup may be interactive once, but normal operation must be unattended.

Expected onboarding:

Prerequisites are macOS, Rust/Cargo, Node.js with pnpm, and the external pinned
`openclaw/imsg` 0.15.1 executable plus its companion
`PhoneNumberKit_PhoneNumberKit.bundle` on `PATH`.

1. Create a dedicated standard macOS Bot user named `human-in-loop`.
2. Log into that user once and sign Messages.app into a separate Bot Apple Account.
   Complete Apple Account credentials and 2FA only in Apple's UI, then leave that
   graphical session logged in.
3. Return to the primary user, open a terminal at this repository root, and run:

   ```sh
   ./scripts/macos-bootstrap.sh
   ```

4. Enter only the non-secret Bot sender and personal recipient iMessage handles
   when the script requests them. On an unprepared host, authenticate the one
   bounded administrator bootstrap once.
5. If the command reports `SETUP_NEEDS_TCC_CONSENT`, grant Full Disk Access and
   Automation → Messages to the final stable Bot worker in the Bot graphical
   session, then rerun the same command.
6. When prompted for the one initial qualification, lock the personal iPhone or
   keep Messages out of the foreground, continue, and reply from the phone using
   the generated token and option number.
7. The readiness table reports `Setup COMPLETE` only after the live runtime,
   iMessage-only route, notification presentation, and correlated reply all pass.

The setup command never accepts an Apple Account password or 2FA code. Sender and
recipient handles remain in owner-only local configuration and are not written to
repository artifacts. Re-running the same command after `Setup COMPLETE` performs
a stable-signed routine update and live health check without sudo, TCC prompts, or
another qualification message unless the qualified identity/route has changed.

After `SETUP_COMPLETE`, ordinary Codex/MCP/channel operation is expected to require:

```text
administrator password prompts    0
Keychain password prompts          0
ordinary Fast User Switching       0
new Full Disk Access prompts       0
new Automation prompts             0
new Files & Folders prompts        0
```

A repeated permission prompt is treated as a deployment regression or explicit recovery state, not normal UX. A real reboot remains a known lifecycle boundary: the dedicated Bot macOS login session must be re-established before Apple Messages can become ready again.

See [`design/07_macos_runtime_deployment.md`](design/07_macos_runtime_deployment.md) for the canonical setup/permission lifecycle.

## Local MCP interface

Configure an MCP client to launch the installed `human-in-loop` executable with the argument `mcp`. The local stdio server exposes two public tools:

```text
ask_human
= blocking structured decision

notify_human
= non-blocking informational notification
```

`ask_human` accepts a compact question plus 2–6 choices with stable semantic IDs. Repository-associated requests supply `repository_path`; the server resolves the canonical GitHub repository slug locally. The result returns canonical `request_id`, `selected_choice_id`, and `source_channel_id`, not the phone's numeric option.

`notify_human` sends compact task/status information without creating a pending decision or waiting for acknowledgement.

Recipient identity, channel credentials, Apple credentials, raw transport commands, generic file operations, and private Messages data are not MCP inputs. Configuration remains local. The interface exposes no public unauthenticated HTTP endpoint and does not use GitHub as a runtime message relay.

See [`design/05_mcp_interface.md`](design/05_mcp_interface.md).

## Codex machine-wide integration

Expose the repository-root [`SKILL.md`](SKILL.md) through supported Codex Skill discovery, pinned to an immutable accepted repository commit. Register the installed production executable as the local MCP server while preserving unrelated `$CODEX_HOME` configuration.

The global rule stays thin:

- real semantic decisions use `ask_human` and fail closed when no valid result is available;
- already-authorized routine work receives no redundant approval;
- host setup/recovery mechanics are consolidated into one actionable checkpoint rather than repeated permission prompts;
- every normal terminal task state attempts a compact `notify_human` after its durable task/Git result exists.

See [`design/06_codex_integration.md`](design/06_codex_integration.md).

## Architecture

```text
Codex / Agent
    ↓
human-in-loop Skill + MCP
    ↓
canonical coordinator / notification dispatch
    ├── Feishu
    └── Apple Messages renderer
             ↓
       Bot-user local worker
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
- Production iMessage sender and recipient use distinct Apple/iMessage account identities.
- Apple Messages is **iMessage-only** and fails closed if actual iMessage delivery cannot be proven.
- `ask_human` is a blocking correlated decision; `notify_human` is non-blocking informational delivery.
- Structured confirmation semantics are transport-independent.
- iMessage is a bounded mobile decision surface, not free-form agent chat.
- The installed macOS requester has a stable code identity before TCC permissions are qualified.
- `SETUP_COMPLETE` normal operation must not require recurring passwords, user switching, or privacy-consent prompts.
- `openclaw/imsg` remains an external dependency; its source is not vendored.

See [`THIRD_PARTY.md`](THIRD_PARTY.md) for pinned upstream coordinates and reuse decisions.
