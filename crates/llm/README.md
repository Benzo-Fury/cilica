# margin-llm

Provider-neutral streaming chat with tool use.

- `Provider` trait: `stream(ChatRequest, CancellationToken) -> ChatStream` of `TextDelta`, `ToolCall` and a final `Done(Completion)` (stop reason, usage, full content to store and replay); `verify()` for a cheap key/server check.
- `anthropic::AnthropicProvider` (default): Messages API `2026-10-21` over reqwest + SSE, caching (system breakpoint + automatic tail), retries with backoff on 429/5xx/overloaded (before any output), `fallbacks: "default"` on models that support it, thinking blocks kept verbatim.
- `openai::OpenAiCompatProvider`: OpenAI, OpenRouter, Groq, LM Studio, and Ollama (`OpenAiCompatProvider::ollama`, via its `/v1` endpoint). Retries once without tools when a model lacks them and says so in `Completion::notice`.
- `claude_code`: detect the `claude` CLI (`detect`), run bridged turns (`run`, stream-json parser, `--resume`, Margin's MCP server only, no built-in tools), and `ClaudeCodeTextProvider` for text-only calls. Never reads Claude credentials.
- `mock::MockProvider`: scripted responses (text, parallel tool calls, errors, hang-until-cancelled) for tests.

Keys are passed as a `KeyLoader` closure and never appear in errors or logs. `LlmError` messages are written for people.

Must not depend on tauri, the store or the agent. Test with `cargo test -p margin-llm` (SSE fixtures in `tests/fixtures`, a local canned HTTP server for retries and auth errors). The live smoke test is `#[ignore]` and needs `ANTHROPIC_API_KEY`.
