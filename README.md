# human-in-loop

A focused human-in-the-loop bridge for coding agents. The maintained remote delivery surface is **Apple Messages via iMessage only**.

The project originated by adapting the open-source architecture of [`Naituw/AskHuman`](https://github.com/Naituw/AskHuman) and reuses [`openclaw/imsg`](https://github.com/openclaw/imsg) as an external macOS transport dependency. AskHuman is not a maintained runtime dependency or product identity. Carrier messaging is deliberately excluded: no SMS, MMS, RCS, paid messaging gateway, or automatic carrier fallback is permitted.

## v0.1.2 release

`v0.1.2` is a source-only compatibility release. It adds optional multiline
`ask_human.detail` for substantive decision evidence while keeping `context`
as compact metadata. iMessage defaults to 1,000 Unicode characters for detail
and 1,500 for the complete confirmation; users may raise these limits up to
the absolute 4,500/5,000-character safety ceilings. Oversized or invalid
budgets fail closed without silent truncation and expose only fixed redacted
reasons. Transport, Bot/TCC, no-SMS, daemon, notification, and the
v0.1.1 canonical-HOME migration behavior remain unchanged. This release does
not claim a notarized downloadable macOS installer or general binary
distribution.

The production macOS path has been qualified with a real distinct-account Apple Messages round trip:

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

Normal terminal task completion attempts one compact `notify_human` delivery.
Routine progress and heartbeat notifications are disabled by default. A terminal
notification includes its durable locator inside the same application message:
HTTP(S) URLs use `Report: "<unchanged URL>"` to suppress large Apple Rich Link
Previews while remaining readable/copyable, and non-URL paths remain unquoted.

iMessage decisions use a deterministic five-digit decimal correlation token and
the exact reply grammar `<TOKEN>-<OPTION_NUMBER>`, for example `48273-1`.
Bare option numbers and the historical hexadecimal-space form are rejected.

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

1. Install human-in-loop and choose a maintained channel.
2. For iMessage, create the dedicated standard Bot user, log into it once, and
   connect Messages with a distinct Bot Apple Account. Enter Apple credentials
   and 2FA only in Apple's UI.
3. Return to the primary user and run the one setup flow from this repository:

   ```sh
   ./scripts/macos-bootstrap.sh
   ```

4. Follow the single consolidated native macOS permission or authentication
   checkpoint if one is unavoidable. When automatic login is supported, setup
   offers one explicit choice between enabling it for the dedicated non-admin Bot
   and keeping manual Bot login after reboot.
5. Complete the one real notification/reply qualification. Setup then reports
   `Setup COMPLETE`.

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

A repeated permission prompt is treated as a deployment regression or explicit
recovery state, not normal UX. A reboot is not re-onboarding: it never requires
reinstalling human-in-loop or reconfiguring Apple Account, recipient, MCP, Skill,
or healthy TCC grants. When FileVault, managed policy, account type, or User
preference keeps automatic login off, log into the dedicated Bot user once after
reboot and return to the primary account.

See [`design/07_macos_runtime_deployment.md`](design/07_macos_runtime_deployment.md) for the canonical setup/permission lifecycle.

## Local MCP interface

Configure an MCP client to launch the installed `human-in-loop` executable with the argument `mcp`. The local stdio server exposes two public tools:

```text
ask_human
= blocking structured decision

notify_human
= non-blocking informational notification
```

`ask_human` accepts a compact question, optional bounded multiline `detail`, and 2–6 choices with stable semantic IDs. Optional `context` remains compact metadata. Repository-associated requests supply `repository_path`; the server resolves the canonical GitHub repository slug locally. The result returns canonical `request_id`, `selected_choice_id`, and `source_channel_id`, not the phone's numeric option. Decision evidence is never silently truncated; invalid or oversized payloads fail closed.

`notify_human` sends compact task/status information without creating a pending
decision or waiting for acknowledgement. One logical task attempts at most one
terminal notification by default; routine progress notifications remain off.

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
    ↓
Apple Messages renderer
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

- Exactly one maintained remote channel: `imessage`.
- Production iMessage sender and recipient use distinct Apple/iMessage account identities.
- Apple Messages is **iMessage-only** and fails closed if actual iMessage delivery cannot be proven.
- `ask_human` is a blocking correlated decision; `notify_human` is non-blocking informational delivery.
- Structured confirmation semantics are transport-independent.
- iMessage is a bounded mobile decision surface, not free-form agent chat.
- The installed macOS requester has a stable code identity before TCC permissions are qualified.
- `SETUP_COMPLETE` normal operation must not require recurring passwords, user switching, or privacy-consent prompts.
- `openclaw/imsg` remains an external dependency; its source is not vendored.

See [`THIRD_PARTY.md`](THIRD_PARTY.md) for pinned upstream coordinates and reuse decisions.
