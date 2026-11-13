//! Anthropic Messages API over reqwest + SSE: streaming, tool use, prompt caching, retries.
//!
//! - API version `2026-10-21`; no sampling parameters are sent.
//! - Caching: an explicit breakpoint on the system prompt (tools render before it, so they are
//!   covered too) plus top-level automatic caching for the growing conversation tail.
//! - Thinking blocks come back on current models; they are kept verbatim in the completion so the
//!   caller can replay them unchanged.
//! - Refusals: on models with server-side fallback, `fallbacks: "default"` is sent; the `fallback`
//!   block is kept (as [`ContentBlock::Opaque`]) and the blocks before it are pruned per the API's
//!   echo rules.

use std::collections::BTreeMap;

use futures::StreamExt;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::http::{self, RetryPolicy, SseEvent};
use crate::{
    CancellationToken, ChatRequest, ChatRole, ChatStream, Completion, ContentBlock, Effort, KeyLoader, LlmError,
    Provider, StopReason, StreamEvent, ToolCall, Usage,
};

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
pub const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const NAME: &str = "Anthropic";
const PROVIDER_TAG: &str = "anthropic";

/// Models that accept `fallbacks: "default"`.
fn supports_server_fallback(model: &str) -> bool {
    matches!(model, "claude-sonnet-5-5" | "claude-opus-5-5" | "claude-fable-5-1" | "claude-fable-5-2")
}

pub struct AnthropicProvider {
    http: reqwest::Client,
    base_url: String,
    key: KeyLoader,
    retry: RetryPolicy,
    refusal_fallback: bool,
}

impl AnthropicProvider {
    pub fn new(key: KeyLoader) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            base_url: DEFAULT_BASE_URL.into(),
            key,
            retry: RetryPolicy::default(),
            refusal_fallback: true,
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Turn off server-side refusal fallbacks.
    pub fn with_refusal_fallback(mut self, on: bool) -> Self {
        self.refusal_fallback = on;
        self
    }

    /// The request body. Public for tests and debugging; contains no key.
    pub fn build_body(&self, req: &ChatRequest, with_fallback: bool) -> Value {
        let mut body = json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "stream": true,
            "messages": to_wire_messages(req),
            // Automatic caching for the growing conversation tail.
            "cache_control": {"type": "ephemeral"},
        });
        if !req.system.is_empty() {
            body["system"] = json!([{ "type": "text", "text": req.system, "cache_control": {"type": "ephemeral"} }]);
        }
        if !req.tools.is_empty() {
            body["tools"] = Value::Array(
                req.tools
                    .iter()
                    .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.input_schema }))
                    .collect(),
            );
        }
        if let Some(effort) = req.effort {
            let level = match effort {
                Effort::Low => "low",
                Effort::Medium => "medium",
                Effort::High => "high",
            };
            body["output_config"] = json!({ "effort": level });
        }
        if with_fallback {
            body["fallbacks"] = json!("default");
        }
        body
    }

    async fn run(&self, req: ChatRequest, cancel: CancellationToken, tx: mpsc::Sender<Result<StreamEvent, LlmError>>) {
        let result = self.run_inner(&req, &cancel, &tx).await;
        if let Err(e) = result {
            let _ = tx.send(Err(e)).await;
        }
    }

    async fn run_inner(
        &self,
        req: &ChatRequest,
        cancel: &CancellationToken,
        tx: &mpsc::Sender<Result<StreamEvent, LlmError>>,
    ) -> Result<(), LlmError> {
        let key = (self.key)().filter(|k| !k.trim().is_empty()).ok_or(LlmError::MissingKey { provider: NAME.into() })?;
        let mut with_fallback = self.refusal_fallback && supports_server_fallback(&req.model);
        let mut attempt = 0u32;
        loop {
            if cancel.is_cancelled() {
                return Err(LlmError::Cancelled);
            }
            let body = self.build_body(req, with_fallback);
            let mut builder = self
                .http
                .post(format!("{}/v1/messages", self.base_url))
                .header("x-api-key", &key)
                .header("anthropic-version", API_VERSION)
                .header("content-type", "application/json")
                .header("accept", "text/event-stream")
                .json(&body);
            if with_fallback {
                builder = builder.header("anthropic-beta", FALLBACK_BETA);
            }
            let sent = tokio::select! {
                r = builder.send() => r,
                _ = cancel.cancelled() => return Err(LlmError::Cancelled),
            };
            let (error, hint) = match sent {
                Err(e) => (http::transport_error(NAME, &e), None),
                Ok(resp) if resp.status().is_success() => {
                    let events = http::sse_events(resp.bytes_stream());
                    match parse_stream(events, cancel, tx).await {
                        Ok(()) => return Ok(()),
                        // Overloaded before any output: safe to retry the whole request.
                        Err(StreamFailure { error, emitted: false }) if error.is_retryable() => (error, None),
                        Err(StreamFailure { error, .. }) => return Err(error),
                    }
                }
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let hint = http::retry_after(resp.headers());
                    let text = resp.text().await.unwrap_or_default();
                    // A model or account without server-side fallbacks: drop them and go again.
                    if with_fallback && status == 400 && text.contains("fallback") {
                        with_fallback = false;
                        continue;
                    }
                    (http::status_error(NAME, &req.model, status, &text), hint)
                }
            };
            if !error.is_retryable() || attempt >= self.retry.max_retries {
                return Err(error);
            }
            tracing::info!(attempt, "retrying Anthropic request after a temporary failure");
            if !http::sleep_or_cancel(self.retry.delay(attempt, hint), cancel).await {
                return Err(LlmError::Cancelled);
            }
            attempt += 1;
        }
    }
}

#[async_trait::async_trait]
impl Provider for AnthropicProvider {
    fn display_name(&self) -> &str {
        NAME
    }

    fn stream(&self, request: ChatRequest, cancel: CancellationToken) -> ChatStream {
        let (tx, rx) = mpsc::channel(64);
        let this = Self {
            http: self.http.clone(),
            base_url: self.base_url.clone(),
            key: self.key.clone(),
            retry: self.retry,
            refusal_fallback: self.refusal_fallback,
        };
        tokio::spawn(async move { this.run(request, cancel, tx).await });
        crate::receiver_stream(rx)
    }

    /// Lists one model: free, and fails with 401 on a bad key.
    async fn verify(&self) -> Result<(), LlmError> {
        let key = (self.key)().filter(|k| !k.trim().is_empty()).ok_or(LlmError::MissingKey { provider: NAME.into() })?;
        let resp = self
            .http
            .get(format!("{}/v1/models?limit=1", self.base_url))
            .header("x-api-key", key)
            .header("anthropic-version", API_VERSION)
            .send()
            .await
            .map_err(|e| http::transport_error(NAME, &e))?;
        if resp.status().is_success() {
            return Ok(());
        }
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        Err(http::status_error(NAME, "", status, &text))
    }
}

// ── Request conversion ───────────────────────────────────────────────────

fn to_wire_messages(req: &ChatRequest) -> Value {
    let mut out = Vec::new();
    for msg in &req.messages {
        let blocks: Vec<Value> = msg.content.iter().filter_map(to_wire_block).collect();
        if blocks.is_empty() {
            continue;
        }
        let role = match msg.role {
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
        };
        out.push(json!({ "role": role, "content": blocks }));
    }
    Value::Array(out)
}

fn to_wire_block(block: &ContentBlock) -> Option<Value> {
    Some(match block {
        ContentBlock::Text { text } if text.is_empty() => return None,
        ContentBlock::Text { text } => json!({"type": "text", "text": text}),
        ContentBlock::ToolUse { id, name, input } => json!({"type": "tool_use", "id": id, "name": name, "input": input}),
        ContentBlock::ToolResult { tool_use_id, content, is_error } => {
            let mut v = json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content});
            if *is_error {
                v["is_error"] = json!(true);
            }
            v
        }
        ContentBlock::Thinking { thinking, signature } => json!({"type": "thinking", "thinking": thinking, "signature": signature}),
        ContentBlock::RedactedThinking { data } => json!({"type": "redacted_thinking", "data": data}),
        ContentBlock::Opaque { provider, block } if provider == PROVIDER_TAG => block.clone(),
        ContentBlock::Opaque { .. } => return None,
    })
}

// ── Stream parsing ───────────────────────────────────────────────────────

pub(crate) struct StreamFailure {
    pub error: LlmError,
    /// Whether any delta had been sent to the caller before the failure.
    pub emitted: bool,
}

enum Building {
    Text(String),
    ToolUse { id: String, name: String, json: String },
    Thinking { thinking: String, signature: String },
    Redacted(String),
    Other(Value),
}

/// Parse an Anthropic SSE stream into events on `tx`.
pub(crate) async fn parse_stream<S>(
    events: S,
    cancel: &CancellationToken,
    tx: &mpsc::Sender<Result<StreamEvent, LlmError>>,
) -> Result<(), StreamFailure>
where
    S: futures::Stream<Item = Result<SseEvent, LlmError>> + Send,
{
    futures::pin_mut!(events);
    let mut emitted = false;
    let fail = |error: LlmError, emitted: bool| StreamFailure { error, emitted };
    let mut blocks: BTreeMap<u64, Building> = BTreeMap::new();
    let mut done: BTreeMap<u64, ContentBlock> = BTreeMap::new();
    let mut usage = Usage::default();
    let mut stop_reason: Option<StopReason> = None;

    loop {
        let next = tokio::select! {
            n = events.next() => n,
            _ = cancel.cancelled() => return Err(fail(LlmError::Cancelled, emitted)),
        };
        let Some(item) = next else { break };
        let ev = item.map_err(|e| fail(e, emitted))?;
        if ev.data.trim().is_empty() {
            continue;
        }
        let data: Value = serde_json::from_str(&ev.data)
            .map_err(|e| fail(LlmError::Protocol(format!("bad event data: {e}")), emitted))?;
        let kind = data.get("type").and_then(Value::as_str).unwrap_or(ev.event.as_str()).to_string();
        match kind.as_str() {
            "message_start" => {
                if let Some(u) = data.pointer("/message/usage") {
                    read_usage(u, &mut usage);
                }
            }
            "content_block_start" => {
                let index = data["index"].as_u64().unwrap_or(0);
                let cb = &data["content_block"];
                let building = match cb["type"].as_str().unwrap_or("") {
                    "text" => {
                        let text = cb["text"].as_str().unwrap_or("").to_string();
                        if !text.is_empty() {
                            emitted = true;
                            let _ = tx.send(Ok(StreamEvent::TextDelta(text.clone()))).await;
                        }
                        Building::Text(text)
                    }
                    "tool_use" => Building::ToolUse {
                        id: cb["id"].as_str().unwrap_or("").to_string(),
                        name: cb["name"].as_str().unwrap_or("").to_string(),
                        json: String::new(),
                    },
                    "thinking" => Building::Thinking {
                        thinking: cb["thinking"].as_str().unwrap_or("").to_string(),
                        signature: cb["signature"].as_str().unwrap_or("").to_string(),
                    },
                    "redacted_thinking" => Building::Redacted(cb["data"].as_str().unwrap_or("").to_string()),
                    _ => Building::Other(cb.clone()),
                };
                blocks.insert(index, building);
            }
            "content_block_delta" => {
                let index = data["index"].as_u64().unwrap_or(0);
                let delta = &data["delta"];
                match (blocks.get_mut(&index), delta["type"].as_str().unwrap_or("")) {
                    (Some(Building::Text(text)), "text_delta") => {
                        let piece = delta["text"].as_str().unwrap_or("");
                        text.push_str(piece);
                        if !piece.is_empty() {
                            emitted = true;
                            let _ = tx.send(Ok(StreamEvent::TextDelta(piece.to_string()))).await;
                        }
                    }
                    (Some(Building::ToolUse { json, .. }), "input_json_delta") => {
                        json.push_str(delta["partial_json"].as_str().unwrap_or(""));
                    }
                    (Some(Building::Thinking { thinking, .. }), "thinking_delta") => {
                        thinking.push_str(delta["thinking"].as_str().unwrap_or(""));
                    }
                    (Some(Building::Thinking { signature, .. }), "signature_delta") => {
                        signature.push_str(delta["signature"].as_str().unwrap_or(""));
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = data["index"].as_u64().unwrap_or(0);
                if let Some(b) = blocks.remove(&index) {
                    done.insert(index, finish_block(b).map_err(|e| fail(e, emitted))?);
                }
            }
            "message_delta" => {
                if let Some(reason) = data.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    stop_reason = Some(match reason {
                        "end_turn" | "stop_sequence" => StopReason::EndTurn,
                        "tool_use" => StopReason::ToolUse,
                        "max_tokens" => StopReason::MaxTokens,
                        "refusal" => StopReason::Refusal,
                        other => StopReason::Other(other.to_string()),
                    });
                }
                if let Some(u) = data.get("usage") {
                    read_usage(u, &mut usage);
                }
            }
            "message_stop" => break,
            "error" => {
                let etype = data.pointer("/error/type").and_then(Value::as_str).unwrap_or("");
                let message = data.pointer("/error/message").and_then(Value::as_str).unwrap_or("").to_string();
                let error = match etype {
                    "overloaded_error" => LlmError::Overloaded { provider: NAME.into() },
                    "rate_limit_error" => LlmError::RateLimited { provider: NAME.into() },
                    "api_error" => LlmError::Server { provider: NAME.into(), status: 500 },
                    "authentication_error" | "permission_error" => LlmError::Auth { provider: NAME.into() },
                    _ => LlmError::BadRequest { provider: NAME.into(), message },
                };
                return Err(fail(error, emitted));
            }
            _ => {} // ping and anything new
        }
    }

    // Blocks still open when the stream ended (max_tokens mid-block) are finished best-effort.
    for (index, b) in std::mem::take(&mut blocks) {
        if let Ok(block) = finish_block(b) {
            done.insert(index, block);
        }
    }
    let Some(stop_reason) = stop_reason else {
        return Err(fail(LlmError::Protocol("the response ended before it finished".into()), emitted));
    };
    let content = prune_before_fallback(done.into_values().collect());
    for block in &content {
        if let ContentBlock::ToolUse { id, name, input } = block {
            let call = ToolCall { id: id.clone(), name: name.clone(), input: input.clone() };
            let _ = tx.send(Ok(StreamEvent::ToolCall(call))).await;
        }
    }
    let _ = tx.send(Ok(StreamEvent::Done(Completion { stop_reason, usage, content, notice: None }))).await;
    Ok(())
}

fn finish_block(b: Building) -> Result<ContentBlock, LlmError> {
    Ok(match b {
        Building::Text(text) => ContentBlock::Text { text },
        Building::ToolUse { id, name, json } => {
            let input = if json.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(&json).map_err(|e| LlmError::Protocol(format!("tool input for {name} is not valid JSON: {e}")))?
            };
            ContentBlock::ToolUse { id, name, input }
        }
        Building::Thinking { thinking, signature } => ContentBlock::Thinking { thinking, signature },
        Building::Redacted(data) => ContentBlock::RedactedThinking { data },
        Building::Other(block) => ContentBlock::Opaque { provider: PROVIDER_TAG.into(), block },
    })
}

/// After a mid-output fallback, thinking, tool_use and unknown model-internal blocks that came
/// before the last `fallback` block must not be echoed (or acted on). Text and the fallback block stay.
fn prune_before_fallback(content: Vec<ContentBlock>) -> Vec<ContentBlock> {
    let is_fallback = |b: &ContentBlock| matches!(b, ContentBlock::Opaque { block, .. } if block["type"] == "fallback");
    let Some(last) = content.iter().rposition(is_fallback) else { return content };
    content
        .into_iter()
        .enumerate()
        .filter(|(i, b)| *i >= last || matches!(b, ContentBlock::Text { .. }))
        .map(|(_, b)| b)
        .collect()
}

fn read_usage(u: &Value, usage: &mut Usage) {
    let get = |k: &str| u.get(k).and_then(Value::as_u64).map(|v| v.min(u64::from(u32::MAX)) as u32);
    if let Some(v) = get("input_tokens") {
        usage.input_tokens = v;
    }
    if let Some(v) = get("output_tokens") {
        usage.output_tokens = v;
    }
    if let Some(v) = get("cache_read_input_tokens") {
        usage.cache_read_tokens = v;
    }
    if let Some(v) = get("cache_creation_input_tokens") {
        usage.cache_write_tokens = v;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::http::test_server::{Canned, serve};
    use crate::{ChatMessage, ToolSpec, complete};

    const TOOL_STREAM: &str = include_str!("../tests/fixtures/anthropic_tool_use.sse");
    const TEXT_STREAM: &str = include_str!("../tests/fixtures/anthropic_text.sse");
    const OVERLOADED_STREAM: &str = include_str!("../tests/fixtures/anthropic_overloaded.sse");
    const FALLBACK_STREAM: &str = include_str!("../tests/fixtures/anthropic_fallback.sse");

    async fn parse_fixture(body: &str, chunk: usize) -> (Vec<StreamEvent>, Result<(), StreamFailure>) {
        let (tx, mut rx) = mpsc::channel(256);
        let cancel = CancellationToken::new();
        let result = parse_stream(http::sse_events(http::chunked(body, chunk)), &cancel, &tx).await;
        drop(tx);
        let mut out = Vec::new();
        while let Some(Ok(ev)) = rx.recv().await {
            out.push(ev);
        }
        (out, result)
    }

    fn key() -> KeyLoader {
        Arc::new(|| Some("sk-ant-test-key".to_string()))
    }

    #[tokio::test]
    async fn parses_text_thinking_and_tool_use_across_chunk_boundaries() {
        for chunk in [1, 7, 64, 4096] {
            let (events, result) = parse_fixture(TOOL_STREAM, chunk).await;
            assert!(result.is_ok());
            let text: String = events
                .iter()
                .filter_map(|e| if let StreamEvent::TextDelta(t) = e { Some(t.as_str()) } else { None })
                .collect();
            assert_eq!(text, "I'll search your notes for Acme.");
            let calls: Vec<_> = events.iter().filter_map(|e| if let StreamEvent::ToolCall(c) = e { Some(c) } else { None }).collect();
            assert_eq!(calls.len(), 2);
            assert_eq!(calls[0].name, "search_notes");
            assert_eq!(calls[0].input, json!({"query": "Acme kickoff", "limit": 10}));
            assert_eq!(calls[1].name, "list_categories");
            let StreamEvent::Done(done) = events.last().unwrap() else { panic!("no done") };
            assert_eq!(done.stop_reason, StopReason::ToolUse);
            assert_eq!(done.usage.input_tokens, 2048);
            assert_eq!(done.usage.cache_read_tokens, 1800);
            assert_eq!(done.usage.output_tokens, 87);
            // Thinking replayed verbatim, signature included.
            assert!(matches!(&done.content[0], ContentBlock::Thinking { signature, .. } if signature == "EqQBCkYIBRgCKkB0c2lnbmF0dXJl"));
            assert_eq!(done.content.len(), 4);
        }
    }

    #[tokio::test]
    async fn overloaded_error_event_is_reported_as_retryable_before_output() {
        let (_, result) = parse_fixture(OVERLOADED_STREAM, 16).await;
        let failure = result.expect_err("should fail");
        assert!(matches!(failure.error, LlmError::Overloaded { .. }));
        assert!(!failure.emitted);
    }

    #[tokio::test]
    async fn fallback_prunes_internal_blocks_before_the_boundary() {
        let (events, result) = parse_fixture(FALLBACK_STREAM, 32).await;
        assert!(result.is_ok());
        let StreamEvent::Done(done) = events.last().unwrap() else { panic!() };
        // The refused attempt's tool_use and thinking are dropped; its text, the fallback block and the new answer stay.
        assert!(!events.iter().any(|e| matches!(e, StreamEvent::ToolCall(_))));
        assert!(matches!(&done.content[0], ContentBlock::Text { .. }));
        assert!(matches!(&done.content[1], ContentBlock::Opaque { block, .. } if block["type"] == "fallback"));
        assert!(matches!(&done.content[2], ContentBlock::Text { text } if text == "Here is the summary."));
        assert_eq!(done.content.len(), 3);
    }

    #[test]
    fn body_caches_system_and_tail_and_replays_thinking() {
        let p = AnthropicProvider::new(key());
        let mut req = ChatRequest::new(
            "claude-sonnet-5-5",
            "You are Cilica.",
            vec![
                ChatMessage::user("hi"),
                ChatMessage {
                    role: ChatRole::Assistant,
                    content: vec![
                        ContentBlock::Thinking { thinking: String::new(), signature: "sig".into() },
                        ContentBlock::text(""),
                        ContentBlock::ToolUse { id: "t1".into(), name: "list_categories".into(), input: json!({}) },
                        ContentBlock::Opaque { provider: "openai".into(), block: json!({"x": 1}) },
                    ],
                },
            ],
        );
        req.tools = vec![ToolSpec { name: "list_categories".into(), description: "List".into(), input_schema: json!({"type": "object"}) }];
        req.effort = Some(Effort::Medium);
        let body = p.build_body(&req, true);
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["cache_control"]["type"], "ephemeral");
        assert_eq!(body["output_config"]["effort"], "medium");
        assert_eq!(body["fallbacks"], "default");
        assert!(body.get("temperature").is_none());
        let assistant = &body["messages"][1]["content"];
        assert_eq!(assistant.as_array().unwrap().len(), 2, "empty text and foreign opaque blocks are skipped");
        assert_eq!(assistant[0]["signature"], "sig");
    }

    #[tokio::test]
    async fn retries_overloaded_and_429_then_succeeds() {
        let (url, seen) = serve(vec![
            Canned { headers: vec![("retry-after".into(), "0".into())], ..Canned::json(429, r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#) },
            Canned::json(529, r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#),
            Canned::sse(OVERLOADED_STREAM),
            Canned::sse(TEXT_STREAM),
        ])
        .await;
        let p = AnthropicProvider::new(key()).with_base_url(url).with_retry(RetryPolicy::immediate(4));
        let req = ChatRequest::new("claude-haiku-5-5", "", vec![ChatMessage::user("hello")]);
        let (text, done) = complete(&p, req, CancellationToken::new()).await.unwrap();
        assert_eq!(text, "Hello from Cilica.");
        assert_eq!(done.stop_reason, StopReason::EndTurn);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 4);
        assert!(seen[0].contains("anthropic-version: 2026-10-21"));
        // haiku has no server-side fallback
        assert!(!seen[0].contains("fallbacks"));
    }

    #[tokio::test]
    async fn bad_key_and_missing_model_get_clear_errors_without_the_key() {
        let (url, _) = serve(vec![
            Canned::json(401, r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#),
            Canned::json(404, r#"{"type":"error","error":{"type":"not_found_error","message":"model: claude-nope"}}"#),
        ])
        .await;
        let p = AnthropicProvider::new(key()).with_base_url(url).with_retry(RetryPolicy::immediate(2));
        let err = complete(&p, ChatRequest::new("claude-nope", "", vec![ChatMessage::user("x")]), CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "That key was rejected by Anthropic. Check it and paste it again.");
        assert!(!err.to_string().contains("sk-ant"));
        let err = complete(&p, ChatRequest::new("claude-nope", "", vec![ChatMessage::user("x")]), CancellationToken::new())
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::ModelNotFound { ref model, .. } if model == "claude-nope"));
    }

    #[tokio::test]
    async fn drops_fallbacks_when_the_server_rejects_them() {
        let (url, seen) = serve(vec![
            Canned::json(400, r#"{"type":"error","error":{"type":"invalid_request_error","message":"fallbacks: not available"}}"#),
            Canned::sse(TEXT_STREAM),
        ])
        .await;
        let p = AnthropicProvider::new(key()).with_base_url(url).with_retry(RetryPolicy::immediate(0));
        let (text, _) = complete(&p, ChatRequest::new("claude-sonnet-5-5", "", vec![ChatMessage::user("x")]), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(text, "Hello from Cilica.");
        let seen = seen.lock().unwrap();
        assert!(seen[0].contains("server-side-fallback-2026-07-01"));
        assert!(!seen[1].contains("server-side-fallback"));
    }

    #[tokio::test]
    async fn missing_key_fails_fast() {
        let p = AnthropicProvider::new(Arc::new(|| None));
        let err = complete(&p, ChatRequest::new("m", "", vec![ChatMessage::user("x")]), CancellationToken::new()).await.unwrap_err();
        assert!(matches!(err, LlmError::MissingKey { .. }));
    }

    #[tokio::test]
    #[ignore = "needs ANTHROPIC_API_KEY; spends a few tokens"]
    async fn live_smoke() {
        let key = std::env::var("ANTHROPIC_API_KEY").ok();
        let p = AnthropicProvider::new(Arc::new(move || key.clone()));
        p.verify().await.unwrap();
        let mut req = ChatRequest::new("claude-haiku-5-5", "Reply with one word.", vec![ChatMessage::user("Say hello")]);
        req.max_tokens = 200;
        req.effort = Some(Effort::Low);
        let (text, _) = complete(&p, req, CancellationToken::new()).await.unwrap();
        assert!(!text.is_empty());
    }
}
