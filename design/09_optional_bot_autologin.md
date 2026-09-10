---
design_id: optional-bot-autologin
title: Optional post-reboot Bot automatic login
status: active
role: design_authority
summary: >
  Defines the opt-in macOS automatic-login convenience path for the dedicated
  non-admin Bot user, including explicit human authorization, feasibility
  checks, secret-handling boundaries, safe fallback, and unchanged iMessage/TCC
  security invariants.
operational_projection:
  - SKILL.md
  - README.md
  - scripts/macos-bootstrap.sh
  - scripts/macos-setup.mjs
  - src-tauri/src/commands/
---

# Purpose

Minimize post-reboot user interaction without weakening the established human-in-loop security model.

The dedicated Bot macOS user normally needs a graphical login session after a real reboot so that Messages.app, the Bot Apple Account, the LaunchAgent worker and `imsg` are available. Reboot is not re-onboarding: Apple Account sign-in, Messages setup, TCC grants, recipient configuration, MCP registration, Skill installation and runtime bootstrap must not be repeated merely because the Mac restarted.

This topic adds an optional convenience path: when macOS permits automatic login for the dedicated Bot user, human-in-loop may configure it only after one explicit human decision.

# Ownership and precedence

`07_macos_runtime_deployment.md` remains authority for deployment identity, TCC, stable signing, setup/recovery, and unattended operation.

This topic owns only the optional automatic-login choice and its safety/fallback behavior. It must not redefine Apple Messages transport, channel correlation, carrier restrictions, MCP semantics, or terminal-notification policy.

# User decision contract

Automatic login is never silently enabled.

During initial setup, or later when the User explicitly opens this option, run a non-mutating feasibility preflight first. If automatic login is technically available and the target is the dedicated non-admin Bot user, ask exactly one semantic decision equivalent to:

```text
Enable automatic login for the dedicated human-in-loop Bot user after Mac restart?

This reduces post-reboot manual work, but anyone with physical access after restart may gain access to that Bot user session. It does not enable automatic login for the primary/admin user and does not weaken FileVault, SIP, TCC or Apple Account protections.

choices:
enable_bot_autologin   Enable automatic login
manual_bot_login       Keep manual Bot login after reboot
```

No recommended choice is required by default. The prompt must state the security trade-off compactly and truthfully.

If the User chooses `enable_bot_autologin`, Codex/setup may perform the supported configuration automatically within the boundaries below.

If the User chooses `manual_bot_login`, preserve the current safe behavior: after a real reboot the User logs into the dedicated Bot macOS user once, then returns to the primary account. No other reconfiguration is required.

Do not ask this question repeatedly after a stable choice is recorded unless the User changes the preference or the host capability materially changes.

# Feasibility and platform constraints

Automatic login is conditional on the current macOS host permitting it.

The implementation must detect and classify at least:

```text
autologin_supported
autologin_already_enabled_for_bot
autologin_disabled_by_user
autologin_unavailable_filevault
autologin_unavailable_managed_policy
autologin_unavailable_account_type
autologin_configuration_failed
```

Current Apple platform behavior includes hosts where automatic login is unavailable, including FileVault-enabled Macs and Macs whose organization/profile or account configuration prohibits the feature. These are platform constraints, not reasons to disable or weaken those protections.

If automatic login is unavailable, do not offer an impossible success path and do not mutate security settings to make it available. Surface the compact reason and retain `manual_bot_login` as the fallback.

# Security boundary

Automatic login may target only the dedicated human-in-loop Bot user and only when that account is a standard/non-admin account.

Never automatically enable login for:

```text
primary macOS user
administrator account
root
an arbitrary caller-supplied account not proven to be the configured Bot user
```

Never make automatic login possible by:

```text
disabling FileVault
changing FileVault policy
weakening SIP
editing TCC databases
removing the Bot user's login password
turning the Bot user into an administrator
storing an Apple Account password or 2FA secret
storing a macOS account password in project files, logs, reports, environment variables, shell history, MCP payloads, or plaintext config
installing a broad privileged helper or persistent sudo rule
```

If macOS requires account-password or administrator authentication to enable the setting, the User may enter it only through an appropriate native macOS authentication/UI surface. Codex/human-in-loop must not read, echo, persist or report the secret.

Use supported/public macOS configuration mechanisms where available. Do not depend on private frameworks, undocumented credential extraction, private loginwindow injection, or unsupported security bypasses.

# Setup behavior

The public setup flow should remain conceptually small:

```text
install human-in-loop
→ connect iMessage / establish dedicated Bot account once
→ run one setup flow
→ satisfy one-time native permission prompts
→ when supported, ask whether to enable Bot automatic login
→ qualify notification/reply
→ SETUP_COMPLETE
```

Internal implementation details such as MCP HOME, daemon paths, worker sockets, signing paths, chat GUIDs and LaunchAgent plumbing remain automatic implementation detail and are not user configuration concepts.

If automatic login is enabled successfully, the setup state records/verifies the preference without storing authentication secrets.

If automatic login is declined or unavailable, setup still reaches `SETUP_COMPLETE` when every existing production predicate is satisfied. Manual post-reboot Bot login is a recovery action, not setup failure.

# Reboot behavior

With verified Bot automatic login enabled and supported:

```text
Mac reboot
→ macOS establishes the dedicated Bot user session automatically
→ Bot Messages/LaunchAgent become available
→ human-in-loop health returns to ready when normal predicates recover
→ no manual Bot-user login is expected
```

Without automatic login:

```text
Mac reboot
→ BOT_SESSION_LOGIN_REQUIRED
→ User logs into Bot user once
→ return to primary user
→ health becomes ready
```

In both cases reboot must not require re-onboarding, re-entering Apple Account credentials, re-granting healthy TCC permissions, reinstalling MCP/Skill, recreating the Bot user, or rebuilding the human-in-loop configuration.

# Verification

At minimum test:

```text
explicit ask_human decision required before enabling
no repeated ask after stable recorded choice
Bot target identity exact and non-admin
primary/admin target rejected
FileVault/policy/account-type unsupported states fail closed
unsupported state never disables security controls
manual fallback remains valid and reaches SETUP_COMPLETE
no password/credential persistence or logging
idempotent repeated setup
existing Bot/TCC/MCP/iMessage/Feishu/notification behavior unchanged
```

Machine-bound qualification must not reboot the User's Mac unless a separate current User/task instruction explicitly authorizes that disruptive action. Non-mutating state inspection plus supported configuration-state verification is sufficient for normal implementation qualification; a future real reboot may provide additional operational evidence.

# Design acceptance

This design is conforming when:

```text
automatic login is opt-in and explicitly authorized
AND only the dedicated non-admin Bot user can be targeted
AND supported hosts can be configured with minimum user interaction
AND secrets are never captured or stored by human-in-loop
AND FileVault/SIP/TCC/account security is never weakened to force availability
AND unsupported/declined hosts retain the one-login-after-reboot fallback
AND reboot is never confused with full re-onboarding
AND all existing transport and runtime invariants remain unchanged
```
