//! Provider-neutral streaming chat with tool use, and the providers Margin supports.
//!
//! - [`Provider`] streams one model response: text deltas, tool calls and a final [`Completion`]
//!   (stop reason, usage and the full assistant content to store and replay).
//! - [`anthropic::AnthropicProvider`] (default), [`openai::OpenAiCompatProvider`] (OpenAI, OpenRouter,
//!   Groq, LM Studio and Ollama's OpenAI-compatible endpoint), [`claude_code`] (experimental bridge to
//!   the user's `claude` CLI) and [`mock::MockProvider`] for tests.
//!
//! API keys are never stored here: providers take a [`KeyLoader`] closure and call it per request.
//! Keys never appear in errors, logs or the returned data.

use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use margin_domain::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use tokio_util::sync::CancellationToken;

pub mod anthropic;
pub mod claude_code;
mod http;
pub mod mock;
pub mod ollama;
pub mod openai;

// ── Conversation ─────────────────────────────────────────────────────────

/// Loads an API key on demand (from the OS keychain in the app, from a string in tests).
pub type KeyLoader = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// One block of message content. Stored as-is in `messages.raw_json` and replayed on later turns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
    /// Anthropic thinking. Replayed unchanged (signature included) to the provider that made it.
    Thinking {
        thinking: String,
        signature: String,
    },
    RedactedThinking {
        data: String,
    },
    /// A provider-specific block kept verbatim (for example Anthropic's `fallback` block).
    /// Replayed only to the provider named here; other providers skip it.
    Opaque {
        provider: String,
        block: Value,
    },
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// True for blocks that belong to one provider's internals (thinking, opaque).
    pub fn is_provider_internal(&self) -> bool {
        matches!(self, Self::Thinking { .. } | Self::RedactedThinking { .. } | Self::Opaque { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: Vec<ContentBlock>,
}

impl ChatMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: ChatRole::User, content: vec![ContentBlock::text(text)] }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self { role: ChatRole::Assistant, content: vec![ContentBlock::text(text)] }
    }

    /// All text blocks joined.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Tool calls in this message, in order.
    pub fn tool_uses(&self) -> Vec<ToolCall> {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => {
                    Some(ToolCall { id: id.clone(), name: name.clone(), input: input.clone() })
                }
                _ => None,
            })
            .collect()
    }
}

/// A tool the model may call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema of the input object.
    pub input_schema: Value,
}

/// How hard the model should think. Maps to Anthropic's `output_config.effort`; ignored elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Low,
    Medium,
    High,
}

/// One request for one model response.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    /// Kept byte-identical across a thread so the provider's prompt cache holds.
    pub system: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
    pub effort: Option<Effort>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, system: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self { model: model.into(), system: system.into(), messages, tools: Vec::new(), max_tokens: 4096, effort: None }
    }
}

// ── Response stream ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    /// The model declined to answer.
    Refusal,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// The end of one model response.
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    pub stop_reason: StopReason,
    pub usage: Usage,
    /// The full assistant content (text, tool calls, thinking) to store and replay.
    pub content: Vec<ContentBlock>,
    /// Something the user should know about how this response was produced
    /// ("This model can't use tools…"). Plain text, for people.
    pub notice: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    TextDelta(String),
    /// A complete tool call. Always sent before [`StreamEvent::Done`].
    ToolCall(ToolCall),
    Done(Completion),
}

pub type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>;

/// A streaming chat model.
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Display name for copy ("Anthropic").
    fn display_name(&self) -> &str;

    /// Stream one response. Errors (including a missing key) arrive as stream items.
    /// Cancelling `cancel` ends the stream with [`LlmError::Cancelled`].
    fn stream(&self, request: ChatRequest, cancel: CancellationToken) -> ChatStream;

    /// A cheap call that checks the key / server without spending tokens where possible.
    async fn verify(&self) -> Result<(), LlmError>;
}

/// Run a request to completion. Returns the concatenated text and the completion.
pub async fn complete(
    provider: &dyn Provider,
    request: ChatRequest,
    cancel: CancellationToken,
) -> Result<(String, Completion), LlmError> {
    let mut stream = provider.stream(request, cancel);
    let mut text = String::new();
    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::TextDelta(delta) => text.push_str(&delta),
            StreamEvent::ToolCall(_) => {}
            StreamEvent::Done(completion) => return Ok((text, completion)),
        }
    }
    Err(LlmError::Protocol("the response ended early".into()))
}

/// Turn an mpsc receiver into a [`ChatStream`].
pub(crate) fn receiver_stream<T: Send + 'static>(
    rx: tokio::sync::mpsc::Receiver<T>,
) -> Pin<Box<dyn Stream<Item = T> + Send>> {
    Box::pin(futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) }))
}

// ── Errors ───────────────────────────────────────────────────────────────

/// What went wrong talking to a provider. `Display` is written for people and never contains a key.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LlmError {
    #[error("Add your {provider} API key in Settings › Margin to use Margin.")]
    MissingKey { provider: String },
    #[error("That key was rejected by {provider}. Check it and paste it again.")]
    Auth { provider: String },
    #[error("{provider} does not offer the model “{model}”. Pick another model in Settings › Margin.")]
    ModelNotFound { provider: String, model: String },
    #[error("{provider} is limiting requests right now. Wait a minute and try again.")]
    RateLimited { provider: String },
    #[error("{provider} is overloaded right now. Try again in a minute.")]
    Overloaded { provider: String },
    #[error("{provider} had a problem ({status}). Try again in a minute.")]
    Server { provider: String, status: u16 },
    #[error("Margin could not reach {provider}. Check your connection and try again.")]
    Network { provider: String },
    #[error("{provider} turned the request down: {message}")]
    BadRequest { provider: String, message: String },
    #[error("The request was cancelled.")]
    Cancelled,
    #[error("The model's response could not be read: {0}")]
    Protocol(String),
    #[error("{0}")]
    Unavailable(String),
}

impl LlmError {
    /// Worth retrying after a pause.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Overloaded { .. } | Self::Server { .. } | Self::Network { .. })
    }
}

impl From<LlmError> for AppError {
    fn from(e: LlmError) -> Self {
        let code = match &e {
            LlmError::MissingKey { .. } | LlmError::Unavailable(_) => ErrorCode::Unavailable,
            LlmError::Cancelled => ErrorCode::Cancelled,
            LlmError::Auth { .. } => ErrorCode::PermissionDenied,
            _ => ErrorCode::Upstream,
        };
        AppError::new(code, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_blocks_round_trip_as_tagged_json() {
        let msg = ChatMessage {
            role: ChatRole::Assistant,
            content: vec![
                ContentBlock::Thinking { thinking: String::new(), signature: "sig".into() },
                ContentBlock::text("Hi"),
                ContentBlock::ToolUse { id: "t1".into(), name: "get_note".into(), input: serde_json::json!({"note_id": "x"}) },
            ],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"tool_use\""));
        let back: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back, msg);
        assert_eq!(back.tool_uses().len(), 1);
        assert_eq!(back.text(), "Hi");
    }

    #[test]
    fn error_messages_are_for_people() {
        let e = LlmError::Auth { provider: "Anthropic".into() };
        assert_eq!(e.to_string(), "That key was rejected by Anthropic. Check it and paste it again.");
        let app: AppError = e.into();
        assert_eq!(app.code, ErrorCode::PermissionDenied);
    }
}
