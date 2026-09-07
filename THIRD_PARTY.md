# Third-party coordinates

This repository intentionally reuses and adapts established open-source components.

## Naituw/AskHuman

- Repository: https://github.com/Naituw/AskHuman
- Pinned inspected baseline: `77e2e576347f94ef203bc2426b73a18749cb4e92` (`0.13.1` release line)
- License: MIT
- Disposition: **ADAPT**
- Role: application/core basis, structured confirmation model, Feishu channel, Agent integration, coordinator, settings/history UI.

The implementation will import/adapt the upstream source while preserving the upstream MIT license notice and recoverable attribution.

## openclaw/imsg

- Repository: https://github.com/openclaw/imsg
- Pinned inspected revision: `646ea7af9616dc3e6406d86aa269bf4fb1b07a76` (`0.15.1` release preparation)
- License: MIT
- Disposition: **REUSE**
- Role: installed macOS dependency for reading, watching, and sending Apple Messages.

`imsg` is not vendored. The application invokes its documented public CLI/JSON surfaces. The product uses only explicit iMessage transport and rejects SMS fallback.

## Reference-only prior art

- OpenAI Agents SDK human-in-the-loop approval flow: https://github.com/openai/openai-agents-python
- LangGraph human-in-the-loop interrupts: https://github.com/langchain-ai/langgraph

Disposition: **REFERENCE_ONLY**. Their structured pause/approve/reject patterns inform semantics; they are not runtime dependencies of this project.
