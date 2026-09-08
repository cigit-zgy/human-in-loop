# humaninloop

Node wrapper for the independent **human-in-loop** project. The maintained
remote channels are Feishu and Apple Messages with strict iMessage-only
delivery.

The repository's `v0.1.0` publication is a source release only; it does not
publish or promise notarized downloadable binaries or an npm binary release.

For a future npm distribution, the primary command is:

```bash
npm i -g humaninloop
human-in-loop --help
```

The `AskHuman` command remains a legacy compatibility alias for existing
wrapper consumers. It is not the project or package identity.

Programmatic consumers may resolve the installed binary:

```js
import { getBinaryPath, isAvailable } from "humaninloop";
```

Resolution prefers `HUMANINLOOP_BINARY`, the matching `@humaninloop/*`
platform package, and `human-in-loop` on `PATH`. The older
`ASKHUMAN_BINARY` variable and `AskHuman` executable name are checked only as
compatibility fallbacks.

The wrapper contains no channel credentials. Apple Messages still requires
the external `imsg` CLI on macOS and always selects iMessage with SMS fallback
disabled.

See the project repository for source installation and the current contract:
<https://github.com/cigit-zgy/human-in-loop>.
