---
design_id: imessage-only-product-scope
title: iMessage-only product scope
status: active
role: design_authority
summary: >
  Removes Feishu from the maintained product and defines Apple Messages using
  iMessage as the sole remote human-in-loop delivery channel.
operational_projection:
  - AGENTS.md
  - SKILL.md
  - design/00_overview.md
  - design/02_channel_coordination.md
  - design/05_mcp_interface.md
  - src-tauri/src/config.rs
  - src-tauri/src/channels/
  - src-tauri/src/daemon/
  - src-tauri/src/mcp/
  - src/
  - README.md
---

# Decision

The maintained remote delivery surface is now exactly one channel:

```text
imessage
```

Feishu is removed from the current product. This is an explicit User product-scope decision and supersedes every older current-design clause that describes a two-channel Feishu+iMessage product.

# Meaning of complete removal

The current product tree must contain no active Feishu capability in:

```text
runtime modules
channel dispatch/racing/fallback logic
configuration schema/defaults
Keychain secret resolution/migration
MCP/channel result enums or maintained public semantics
settings/UI surfaces
setup/onboarding/runtime health
current design authority
README/current documentation
release notes for the new candidate
active tests/fixtures whose purpose is Feishu support
Feishu-only dependencies
```

Delete Feishu-specific source modules/directories when they are no longer referenced. Remove dependencies that exist only for Feishu after proving they are not shared by the retained iMessage path.

Current Git history and already-committed historical reports are audit evidence and MUST NOT be rewritten merely to erase old Feishu mentions. The acceptance target is zero Feishu support in the current maintained product surface, not destructive history rewriting.

# Local secret migration boundary

The runtime must never load, resolve, request, decrypt, or otherwise touch a historical Feishu Keychain item after this change. In particular, daemon/MCP/CLI startup with the current iMessage-only configuration must not invoke SecurityAgent because of a dormant Feishu secret.

Do not silently delete a User's pre-existing Keychain credential merely because product support was removed. Old Feishu Keychain state may remain inert on the machine, but no maintained runtime path may read it. Optional documentation may state that obsolete credentials can be removed manually; this is not a runtime prerequisite.

Unknown legacy `feishu` fields in an old config file should be ignored or safely migrated away without blocking startup, prompting for credentials, or recreating Feishu defaults. The resulting canonical normal form is iMessage-only.

# Coordinator simplification

With one maintained remote channel, generic semantic request/result normalization remains useful, but channel racing/fallback complexity must not survive solely for removed Feishu support.

Required normal path:

```text
ask_human
→ canonical request
→ iMessage support/readiness
→ one iMessage request
→ strict correlated result
→ canonical MCP result

notify_human
→ canonical terminal notification
→ one iMessage dispatch attempt
→ bounded dispatch result
```

Do not introduce another remote channel as replacement fallback. If iMessage cannot safely handle a mandatory decision, fail closed.

# Preserved product behavior

Removing Feishu must not weaken or redesign the accepted iMessage/HIL path:

```text
public MCP tools exactly ask_human + notify_human
rich multiline ask_human.detail
decimal TOKEN-OPTION reply grammar
strict chat/token/GUID/time/choice correlation
first terminal result only / duplicate-late rejection
explicit --service imessage + --no-sms-fallback
dedicated non-admin Bot macOS user + distinct Bot Apple Account
stable signing/TCC/worker IPC/Messages DB boundary
canonical shared runtime config / no isolated MCP HOME
optional Bot automatic-login design
default progress notifications = 0
at most one terminal notify_human per logical task
terminal report locator remains in the same application message
```

# Completion of the currently blocked mobile task

The Feishu-removal implementation is also the recovery for the current daemon startup blocker. After Feishu is gone, the same v0.1.3 candidate line must complete the previously blocked mobile qualification:

```text
production daemon starts without Feishu/Keychain authorization
→ installed candidate MCP tools exactly ask_human + notify_human
→ one real decimal TOKEN-OPTION iMessage ask_human round trip
→ exactly one canonical selected_choice_id from source_channel_id=imessage
→ no duplicate/late acceptance; watcher/client cleanup
→ bounded terminal report-link phone presentation probe
```

For the report link, retain the accepted goal from design/10: one application message, no large Rich Link Preview on the qualified iPhone, and a full readable/copyable locator; direct tap navigation is preferred when achievable without private APIs or corrupting the URL. If the current quoted wrapper fails, use at most the already-authorized single ordinary-text fallback probe.

No real scientific/project decision or production WME gold label may be mutated by qualification.

# Legacy non-maintained code

Telegram, Slack, DingTalk and other AskHuman-era channel remnants are not made supported by this decision. They remain outside the maintained remote surface. This task's mandatory removal target is Feishu; Codex may remove obviously unreachable legacy channel remnants only when required for a clean iMessage-only compile/configuration surface and when doing so does not expand risk. Do not turn this task into an unrelated full repository rewrite.

# Acceptance

This scope is conforming when:

```text
CURRENT_MAINTAINED_REMOTE_CHANNELS = [imessage]
AND no active Feishu code/config/UI/dispatch/secret-loading surface remains
AND old config containing Feishu data cannot cause startup prompting or Feishu secret access
AND daemon/MCP startup is unattended on the current qualified iMessage host
AND all retained iMessage/MCP/security/privacy contracts pass regression
AND the real decimal-token phone round trip passes
AND the bounded terminal-link phone qualification reaches a recorded outcome
AND no Git history rewrite or private credential deletion was used as cleanup
```
