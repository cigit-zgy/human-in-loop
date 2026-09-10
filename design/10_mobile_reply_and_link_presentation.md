---
design_id: mobile-reply-and-link-presentation
title: Mobile reply token and compact link presentation
status: active
role: design_authority
summary: >
  Defines the low-friction numeric iMessage reply token grammar and the
  best-effort no-rich-preview report-link presentation used by phone surfaces.
operational_projection:
  - design/01_interaction_protocol.md
  - design/03_imessage_channel.md
  - design/08_terminal_notification.md
  - src-tauri/src/channels/imessage.rs
  - src-tauri/src/channels/notify.rs
---

# Purpose

Reduce phone interaction cost without weakening request correlation, and keep report locators visually integrated with the compact terminal message instead of allowing Apple Messages to expand them into a large Rich Link Preview card.

This topic is the current authority for the two narrow behaviors below. Where older text in `01_interaction_protocol.md`, `03_imessage_channel.md`, or `08_terminal_notification.md` still shows the historical hexadecimal-space reply grammar or treats Apple Rich Link Preview as acceptable, this topic supersedes those clauses until they are mechanically consolidated during the implementation task.

# 1. Canonical mobile reply grammar

The correlation token remains mandatory. Bare option replies such as `1` are not accepted in the default protocol.

The phone-facing canonical form is:

```text
48273-1
```

Semantics:

```text
48273
= request correlation token

-
= literal ASCII hyphen separator

1
= one-based rendered choice position
```

The rendered confirmation shape is therefore:

```text
[HIL · 48273]
Codex · project
...

1  First choice
2  Second choice

Reply: 48273-1
```

## Token requirements

The token is a correlation identifier, not an authentication secret.

Required properties:

```text
ASCII decimal digits only
first digit 1-9
no leading zero
normal length 5 digits
collision-safe among all currently pending requests
deterministically derived from canonical request identity using the existing cryptographic hash primitive
no new RNG dependency required
```

A normal five-digit decimal token has 90,000 canonical values (`10000` through `99999`), which is larger than the historical four-hex-character space of 65,536 values.

If the normal five-digit candidate collides with another active token, extend the deterministic decimal token by two digits at a time (`5 -> 7 -> 9 -> ...`) until the active token is unique. If additional hash material is needed, use the existing deterministic salted-rehash pattern rather than weakening uniqueness or introducing a user-visible random workflow.

The same active request must retain the same allocated token until terminal cleanup/release.

## Parser requirements

After trimming only leading/trailing whitespace around the complete incoming message, the canonical parser accepts exactly:

```text
<TOKEN>-<OPTION_NUMBER>
```

The token must be decimal-only, no-leading-zero, and correspond to one currently pending request. The option must be a valid one-based position for that request.

Do not require the User to switch to an alphabetic keyboard.

Do not accept as the canonical new grammar:

```text
48273 1
48273 - 1
48273--1
48273_1
A7F3-1
1
```

No fuzzy natural-language interpretation is added.

All existing correlation evidence remains mandatory after syntax parsing:

```text
same configured direct chat
strictly after send/cursor boundary
correct active token
valid option
request not terminal/expired/cancelled
incoming production peer, not outgoing self row
GUID/reply-to constraints
not reaction-only
not attachment-only
first terminal result wins
duplicate/late replies ignored
watcher/request cleanup preserved
```

This protocol change is presentation/input syntax only; it must not reduce correlation strictness.

# 2. Terminal report-link presentation

A terminal notification still uses exactly one application send containing the summary and locator. No second link-only message is allowed.

The User has visually rejected the current raw-URL form because Apple Messages expands the URL into a large Rich Link Preview card below the compact notification. Therefore Rich Link Preview suppression is now a phone-UX acceptance requirement, not merely an optional client behavior.

## Transport boundary

Do not add or invent a private Messages API or unsupported `imsg` flag. The pinned maintained `imsg` text-send path has no accepted `no-preview` control in the current product contract. Continue to send ordinary text through the existing explicit iMessage-only/no-SMS path.

## Presentation objective

For HTTP(S) locators, choose a plain-text wrapper that satisfies, in priority order:

```text
1. same application message as the terminal summary
2. no large Apple Rich Link Preview card on the qualified iPhone Messages client
3. URL remains directly tappable when the client can data-detect it
4. full locator remains human-readable/copyable
```

The qualified and accepted wrapper is a quoted raw URL:

```text
Report: "https://github.com/..."
```

Real-phone qualification observed one ordinary application message, no large Rich Link Preview, successful tap navigation, and an intact readable/copyable URL. This wrapper is frozen in `08_terminal_notification.md` and implementation tests. A future platform regression must be reported rather than worked around with zero-width characters, inserted spaces, a private API, or a second message.

For non-HTTP(S) durable locators, keep the ordinary semantic form without link-preview workarounds:

```text
Report: reports/codex/260910_codex_XX.md
```

# Qualification

Before accepting this change, perform one combined real iMessage qualification on the installed/candidate path where practical:

- the message uses a newly rendered decimal token and `TOKEN-OPTION` reply grammar;
- the body includes a harmless test report URL using the preferred no-preview wrapper;
- the User can answer with the new numeric-hyphen reply without changing keyboard class;
- the User reports whether the link preview is suppressed and whether the link remains tappable;
- if the preferred wrapper fails, at most one fallback visual probe is permitted;
- no real project/scientific decision is mutated by this qualification.

The qualification must still prove exactly one canonical decision result, no duplicate/late acceptance, and complete watcher/request cleanup.

# Non-regression boundary

Do not redesign or weaken:

- public MCP tool surface (`ask_human`, `notify_human` only);
- rich multiline `detail`;
- iMessage explicit service / no-SMS behavior;
- Bot user / distinct Apple Account topology;
- Messages DB and worker IPC boundary;
- send/watch/correlation lifecycle except reply syntax/token rendering;
- daemon lifecycle;
- TCC/signing/bootstrap;
- canonical HOME migration;
- optional Bot automatic-login design;
- default-zero progress and one-terminal-notification policy.

# Design acceptance

This topic is conforming when the User can normally answer an iMessage decision using a decimal-only `TOKEN-OPTION` string such as `48273-1`, correlation remains as strict as before, terminal report locators remain in the same application message, the qualified phone presentation no longer expands the report URL into a large Rich Link Preview, and the URL remains tappable whenever that can be achieved without private APIs or malformed locator text.
