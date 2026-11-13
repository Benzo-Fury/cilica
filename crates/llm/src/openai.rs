//! OpenAI-compatible chat completions with streaming and tool calls: OpenAI, OpenRouter, Groq,
//! LM Studio, and Ollama (through its OpenAI-compatible `/v1` endpoint, no key).
//!
//! When a model or server does not support tools, the request is retried once without them and the
//! completion carries a `notice` so the UI can say why Cilica could not look anything up.

use std::collections::BTreeMap;

use futures::StreamExt;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::http::{self, RetryPolicy, SseEvent};
use crate::{
    CancellationToken, ChatRequest, ChatRole, ChatStream, Completion, ContentBlock, KeyLoader, LlmError, Provider,
    StopReason, StreamEvent, ToolCall, Usage,
};

pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434/v1";
const NO_TOOLS_NOTICE: &str = "This model can't use tools, so Cilica answered without reading your notes. Pick a model with tool support in Settings › Cilica.";

pub struct OpenAiCompatProvider {
    http: reqwest::Client,
    name: String,
    base_url: String,
    /// `None` for servers without auth (Ollama, LM Studio).
    key: Option<KeyLoader>,
    retry: RetryPolicy,
}

impl OpenAiCompatProvider {
    /// An OpenAI-compatible server. `key` is required unless the server is local.
    pub fn new(name: impl Into<String>, base_url: impl Into<String>, key: Option<KeyLoader>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            name: name.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            key,
            retry: RetryPolicy::default(),
        }
    }

    /// Local Ollama (no key).
    pub fn ollama(base_url: Option<String>) -> Self {
        Self::new("Ollama", base_url.unwrap_or_else(|| OLLAMA_BASE_URL.into()), None)
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    fn clone_handle(&self) -> Self {
        Self { http: self.http.clone(), name: self.name.clone(), base_url: self.base_url.clone(), key: self.key.clone(), retry: self.retry }
    }

    /// The request body. Contains no key.
    pub fn build_body(&self, req: &ChatRequest, with_tools: bool) -> Value {
        let mut body = json!({
            "model": req.model,
            "stream": true,
            "stream_options": {"include_usage": true},
            "messages": to_wire_messages(req),
        });
        // OpenAI's own API wants max_completion_tokens; most compatible servers still read max_tokens.
        if self.base_url.contains("api.openai.com") {
            body["max_completion_tokens"] = json!(req.max_tokens);
        } else {
            body["max_tokens"] = json!(req.max_tokens);
        }
        if with_tools && !req.tools.is_empty() {
            body["tools"] = Value::Array(
                req.tools
                    .iter()
                    .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.input_schema}}))
                    .collect(),
            );
        }
        body
    }

    fn api_key(&self) -> Result<Option<String>, LlmError> {
        match &self.key {
            None => Ok(None),
            Some(load) => match load().filter(|k| !k.trim().is_empty()) {
                Some(k) => Ok(Some(k)),
                None => Err(LlmError::MissingKey { provider: self.name.clone() }),
            },
        }
    }

    async fn run_inner(
        &self,
        req: &ChatRequest,
        cancel: &CancellationToken,
        tx: &mpsc::Sender<Result<StreamEvent, LlmError>>,
    ) -> Result<(), LlmError> {
        let key = self.api_key()?;
        let mut with_tools = true;
        let mut notice = None;
        let mut attempt = 0u32;
        loop {
            let body = self.build_body(req, with_tools);
            let mut builder = self.http.post(format!("{}/chat/completions", self.base_url)).json(&body);
            if let Some(k) = &key {
                builder = builder.bearer_auth(k);
            }
            let sent = tokio::select! {
                r = builder.send() => r,
                _ = cancel.cancelled() => return Err(LlmError::Cancelled),
            };
            let (error, hint) = match sent {
                Err(e) => (http::transport_error(&self.name, &e), None),
                Ok(resp) if resp.status().is_success() => {
                    return parse_stream(&self.name, http::sse_events(resp.bytes_stream()), notice, cancel, tx).await;
                }
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let hint = http::retry_after(resp.headers());
                    let text = resp.text().await.unwrap_or_default();
                    if with_tools && !req.tools.is_empty() && (400..500).contains(&status) && mentions_no_tools(&text) {
                        tracing::info!(provider = %self.name, "model does not support tools; retrying without them");
                        with_tools = false;
                        notice = Some(NO_TOOLS_NOTICE.to_string());
                        continue;
                    }
                    (http::status_error(&self.name, &req.model, status, &text), hint)
                }
            };
            if !error.is_retryable() || attempt >= self.retry.max_retries {
                return Err(error);
            }
            if !http::sleep_or_cancel(self.retry.delay(attempt, hint), cancel).await {
                return Err(LlmError::Cancelled);
            }
            attempt += 1;
        }
    }
}

fn mentions_no_tools(body: &str) -> bool {
    let lower = http::error_message(body).to_lowercase();
    lower.contains("tool") && (lower.contains("not support") || lower.contains("unsupported") || lower.contains("does not support"))
}

#[async_trait::async_trait]
impl Provider for OpenAiCompatProvider {
    fn display_name(&self) -> &str {
        &self.name
    }

    fn stream(&self, request: ChatRequest, cancel: CancellationToken) -> ChatStream {
        let (tx, rx) = mpsc::channel(64);
        let this = self.clone_handle();
        tokio::spawn(async move {
            if let Err(e) = this.run_inner(&request, &cancel, &tx).await {
                let _ = tx.send(Err(e)).await;
            }
        });
        crate::receiver_stream(rx)
    }

    async fn verify(&self) -> Result<(), LlmError> {
        let key = self.api_key()?;
        let mut builder = self.http.get(format!("{}/models", self.base_url));
        if let Some(k) = key {
            builder = builder.bearer_auth(k);
        }
        let resp = builder.send().await.map_err(|e| http::transport_error(&self.name, &e))?;
        if resp.status().is_success() {
            return Ok(());
        }
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        Err(http::status_error(&self.name, "", status, &text))
    }
}

// ── Request conversion ───────────────────────────────────────────────────

fn to_wire_messages(req: &ChatRequest) -> Value {
    let mut out = Vec::new();
    if !req.system.is_empty() {
        out.push(json!({"role": "system", "content": req.system}));
    }
    for msg in &req.messages {
        match msg.role {
            ChatRole::User => {
                // Tool results become `tool` messages, then any text as a user message.
                let mut text = String::new();
                for block in &msg.content {
                    match block {
                        ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                            let content = if *is_error { format!("Error: {content}") } else { content.clone() };
                            out.push(json!({"role": "tool", "tool_call_id": tool_use_id, "content": content}));
                        }
                        ContentBlock::Text { text: t } => {
                            if !text.is_empty() {
                                text.push_str("\n\n");
                            }
                            text.push_str(t);
                        }
                        _ => {}
                    }
                }
                if !text.is_empty() {
                    out.push(json!({"role": "user", "content": text}));
                }
            }
            ChatRole::Assistant => {
                let text = msg.text();
                let calls: Vec<Value> = msg
                    .tool_uses()
                    .into_iter()
                    .map(|c| json!({"id": c.id, "type": "function", "function": {"name": c.name, "arguments": c.input.to_string()}}))
                    .collect();
                if text.is_empty() && calls.is_empty() {
                    continue;
                }
                let mut m = json!({"role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) }});
                if !calls.is_empty() {
                    m["tool_calls"] = Value::Array(calls);
                }
                out.push(m);
            }
        }
    }
    Value::Array(out)
}

// ── Stream parsing ───────────────────────────────────────────────────────

#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
}

async fn parse_stream<S>(
    provider: &str,
    events: S,
    notice: Option<String>,
    cancel: &CancellationToken,
    tx: &mpsc::Sender<Result<StreamEvent, LlmError>>,
) -> Result<(), LlmError>
where
    S: futures::Stream<Item = Result<SseEvent, LlmError>> + Send,
{
    futures::pin_mut!(events);
    let mut text = String::new();
    let mut calls: BTreeMap<u64, PartialCall> = BTreeMap::new();
    let mut finish: Option<String> = None;
    let mut usage = Usage::default();
    loop {
        let next = tokio::select! {
            n = events.next() => n,
            _ = cancel.cancelled() => return Err(LlmError::Cancelled),
        };
        let Some(item) = next else { break };
        let ev = item?;
        let data = ev.data.trim();
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            break;
        }
        let v: Value = serde_json::from_str(data).map_err(|e| LlmError::Protocol(format!("bad chunk: {e}")))?;
        if let Some(err) = v.get("error") {
            let message = err.get("message").and_then(Value::as_str).unwrap_or("unknown error").to_string();
            return Err(LlmError::BadRequest { provider: provider.to_string(), message });
        }
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            let get = |k: &str| u.get(k).and_then(Value::as_u64).map(|n| n.min(u64::from(u32::MAX)) as u32).unwrap_or(0);
            usage.input_tokens = get("prompt_tokens");
            usage.output_tokens = get("completion_tokens");
            usage.cache_read_tokens = u.pointer("/prompt_tokens_details/cached_tokens").and_then(Value::as_u64).unwrap_or(0) as u32;
        }
        let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else { continue };
        let delta = &choice["delta"];
        if let Some(piece) = delta.get("content").and_then(Value::as_str)
            && !piece.is_empty()
        {
            text.push_str(piece);
            let _ = tx.send(Ok(StreamEvent::TextDelta(piece.to_string()))).await;
        }
        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for (pos, tc) in tool_calls.iter().enumerate() {
                let index = tc.get("index").and_then(Value::as_u64).unwrap_or(pos as u64);
                let entry = calls.entry(index).or_default();
                if let Some(id) = tc.get("id").and_then(Value::as_str) {
                    entry.id = id.to_string();
                }
                if let Some(name) = tc.pointer("/function/name").and_then(Value::as_str) {
                    entry.name.push_str(name);
                }
                if let Some(args) = tc.pointer("/function/arguments").and_then(Value::as_str) {
                    entry.arguments.push_str(args);
                }
            }
        }
        if let Some(f) = choice.get("finish_reason").and_then(Value::as_str) {
            finish = Some(f.to_string());
        }
    }

    let mut content = Vec::new();
    if !text.is_empty() {
        content.push(ContentBlock::Text { text });
    }
    let mut tool_calls = Vec::new();
    for (index, call) in calls {
        let input = if call.arguments.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&call.arguments)
                .map_err(|e| LlmError::Protocol(format!("tool input for {} is not valid JSON: {e}", call.name)))?
        };
        let id = if call.id.is_empty() { format!("call_{index}") } else { call.id };
        content.push(ContentBlock::ToolUse { id: id.clone(), name: call.name.clone(), input: input.clone() });
        tool_calls.push(ToolCall { id, name: call.name, input });
    }
    let stop_reason = match finish.as_deref() {
        Some("tool_calls") | Some("function_call") => StopReason::ToolUse,
        _ if !tool_calls.is_empty() => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        Some("content_filter") => StopReason::Refusal,
        Some("stop") | None => StopReason::EndTurn,
        Some(other) => StopReason::Other(other.to_string()),
    };
    for call in tool_calls {
        let _ = tx.send(Ok(StreamEvent::ToolCall(call))).await;
    }
    let _ = tx.send(Ok(StreamEvent::Done(Completion { stop_reason, usage, content, notice }))).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::http::test_server::{Canned, serve};
    use crate::{ChatMessage, ToolSpec, complete};

    const TOOL_STREAM: &str = include_str!("../tests/fixtures/openai_tool_calls.sse");
    const TEXT_STREAM: &str = include_str!("../tests/fixtures/openai_text.sse");

    fn req_with_tools() -> ChatRequest {
        let mut req = ChatRequest::new("gpt-test", "Be brief.", vec![ChatMessage::user("find acme")]);
        req.tools = vec![ToolSpec { name: "search_notes".into(), description: "Search".into(), input_schema: json!({"type": "object"}) }];
        req
    }

    #[tokio::test]
    async fn parses_parallel_tool_calls_with_partial_arguments() {
        let (tx, mut rx) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        parse_stream("OpenAI", http::sse_events(http::chunked(TOOL_STREAM, 5)), None, &cancel, &tx).await.unwrap();
        drop(tx);
        let mut events = Vec::new();
        while let Some(Ok(e)) = rx.recv().await {
            events.push(e);
        }
        let calls: Vec<_> = events.iter().filter_map(|e| if let StreamEvent::ToolCall(c) = e { Some(c.clone()) } else { None }).collect();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].input, json!({"query": "Acme kickoff"}));
        assert_eq!(calls[1].name, "list_categories");
        let StreamEvent::Done(done) = events.last().unwrap() else { panic!() };
        assert_eq!(done.stop_reason, StopReason::ToolUse);
        assert_eq!(done.usage.input_tokens, 120);
    }

    #[test]
    fn converts_tool_history_to_openai_messages() {
        let mut req = req_with_tools();
        req.messages.push(ChatMessage {
            role: ChatRole::Assistant,
            content: vec![
                ContentBlock::Thinking { thinking: "x".into(), signature: "s".into() },
                ContentBlock::ToolUse { id: "c1".into(), name: "search_notes".into(), input: json!({"query": "a"}) },
            ],
        });
        req.messages.push(ChatMessage {
            role: ChatRole::User,
            content: vec![ContentBlock::ToolResult { tool_use_id: "c1".into(), content: "3 hits".into(), is_error: false }],
        });
        let p = OpenAiCompatProvider::new("OpenAI", OPENAI_BASE_URL, None);
        let body = p.build_body(&req, true);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[2]["tool_calls"][0]["function"]["arguments"], "{\"query\":\"a\"}");
        assert_eq!(msgs[3]["role"], "tool");
        assert!(body.get("max_completion_tokens").is_some());
    }

    #[tokio::test]
    async fn retries_without_tools_when_the_model_lacks_them() {
        let (url, seen) = serve(vec![
            Canned::json(400, r#"{"error":{"message":"registry.ollama.ai/library/gemma:2b does not support tools"}}"#),
            Canned::sse(TEXT_STREAM),
        ])
        .await;
        let p = OpenAiCompatProvider::ollama(Some(url)).with_retry(RetryPolicy::immediate(0));
        let (text, done) = complete(&p, req_with_tools(), CancellationToken::new()).await.unwrap();
        assert_eq!(text, "Hi there.");
        assert!(done.notice.unwrap().contains("can't use tools"));
        let seen = seen.lock().unwrap();
        assert!(seen[0].contains("\"tools\""));
        assert!(!seen[1].contains("\"tools\""));
        assert!(!seen[0].to_lowercase().contains("authorization"));
    }

    #[tokio::test]
    async fn sends_bearer_key_and_maps_401() {
        let (url, seen) = serve(vec![Canned::json(401, r#"{"error":{"message":"Incorrect API key provided"}}"#)]).await;
        let key: KeyLoader = Arc::new(|| Some("sk-test".into()));
        let p = OpenAiCompatProvider::new("OpenRouter", url, Some(key)).with_retry(RetryPolicy::immediate(0));
        let err = complete(&p, req_with_tools(), CancellationToken::new()).await.unwrap_err();
        assert_eq!(err.to_string(), "That key was rejected by OpenRouter. Check it and paste it again.");
        assert!(seen.lock().unwrap()[0].to_lowercase().contains("authorization: bearer sk-test"));
    }
}
