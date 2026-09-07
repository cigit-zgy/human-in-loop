# askhuman

Human-in-the-loop interaction tool with two maintained remote channels: Feishu and Apple Messages (iMessage only). The `AskHuman` CLI returns a structured human decision to the calling AI assistant.

Human-in-the-loop 交互工具，维护的远程渠道仅为飞书与 Apple 信息（仅 iMessage）。`AskHuman` CLI 会把结构化的人类决定返回给调用它的 AI 助手。

Under the hood it's a single executable (Tauri 2 / Rust). This npm package distributes it via per-platform subpackages: installing fetches only the one binary matching your current platform.

## Standalone use

```bash
npm i -g askhuman
AskHuman "Continue?" -o "Continue" -o "Stop"
```

## As a dependency (programmatic use)

```bash
npm i askhuman
```

```js
import { getBinaryPath, isAvailable } from "askhuman";
import { spawnSync } from "node:child_process";

if (!isAvailable()) {
  // Binary not in place: skip the human-confirmation step to avoid blocking the flow
} else {
  const r = spawnSync(getBinaryPath(), ["Continue?", "-o", "Continue", "-o", "Stop"], {
    encoding: "utf8",
  });
  if (r.status === 3) {
    // No maintained channel is configured: degrade gracefully
  } else if (r.status === 0) {
    // Success: parse the result blocks from r.stdout
    console.log(r.stdout);
  }
}
```

`getBinaryPath()` resolution order: env var `ASKHUMAN_BINARY` (legacy `HUMANINLOOP_BINARY` still works) → platform subpackage → system `PATH`.

## Exit code contract

| Exit code | Meaning |
|---|---|
| `0` | Got a result, or the user cancelled (emits `[Status]`) |
| `3` | No maintained channel is configured — downstream should degrade |
| `1` | Other error |

stdout contains only the result blocks (`[Selected options]` / `[User input]` / `[Images]` / `[Files]` / `[Status]`); all logs and errors go to stderr.

## Platforms and system dependencies

Supports macOS (arm64/x64) and Linux (x64). Feishu uses its long-connection and interactive-card integration. Apple Messages requires the external `imsg` CLI on macOS and always selects iMessage with SMS fallback disabled.

More info in the project repo: <https://github.com/cigit-zgy/human-in-loop>
