//! Provider-neutral streaming chat with tool use, and the providers Cilica supports.
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
use cilica_domain::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use tokio_util::sync::CancellationToken;

pub mod anthropic;
pub mod claude_code;
mod http;
pub mod mock;
pub mod ollama;
pub mod openai;
pub mod vision;

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
        /// Images that go with the result (from `view_images`). Only references are stored; the
        /// bytes are loaded just before each request. Rows written before images existed have none.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<ToolImage>,
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

    /// A tool result without images.
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self::ToolResult { tool_use_id: tool_use_id.into(), content: content.into(), is_error, images: Vec::new() }
    }

    /// True for blocks that belong to one provider's internals (thinking, opaque).
    pub fn is_provider_internal(&self) -> bool {
        matches!(self, Self::Thinking { .. } | Self::RedactedThinking { .. } | Self::Opaque { .. })
    }
}

/// An image attached to a tool result.
///
/// History stores `path` and `media_type` only. `data` (base64 of the bytes as sent: downscaled and
/// re-encoded) is filled in just before a request and never serialised, so stored rows stay small
/// and the request bytes stay the same from turn to turn (the provider's prompt cache holds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolImage {
    /// The note's path for it: `attachments/<hash>.<ext>`.
    pub path: String,
    /// What `data` is: `image/jpeg` or `image/png`.
    pub media_type: String,
    #[serde(skip)]
    pub data: Option<Arc<str>>,
}

/// Text sent in place of an image whose bytes could not be loaded for this request.
pub const IMAGE_NOT_LOADED: &str = "[image not loaded]";

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

    /// Whether `model` on this provider can see images in tool results. Must be stable for a given
    /// model, so the tool list (and the prompt cache) does not change between requests.
    async fn sees_images(&self, _model: &str) -> bool {
        false
    }
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
    #[error("Add your {provider} API key in Settings › Cilica to use Cilica.")]
    MissingKey { provider: String },
    #[error("That key was rejected by {provider}. Check it and paste it again.")]
    Auth { provider: String },
    #[error("{provider} does not offer the model “{model}”. Pick another model in Settings › Cilica.")]
    ModelNotFound { provider: String, model: String },
    #[error("{provider} is limiting requests right now. Wait a minute and try again.")]
    RateLimited { provider: String },
    #[error("{provider} is overloaded right now. Try again in a minute.")]
    Overloaded { provider: String },
    #[error("{provider} had a problem ({status}). Try again in a minute.")]
    Server { provider: String, status: u16 },
    #[error("Cilica could not reach {provider}. Check your connection and try again.")]
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
    fn tool_result_images_store_references_only_and_old_rows_still_read() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "t1".into(),
            content: "1 image".into(),
            is_error: false,
            images: vec![ToolImage { path: "attachments/0123456789abcdef0123456789abcdef.png".into(), media_type: "image/png".into(), data: Some("QUJD".into()) }],
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"path\":\"attachments/0123456789abcdef0123456789abcdef.png\""));
        assert!(!json.contains("QUJD"), "image bytes are never stored");
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        let ContentBlock::ToolResult { images, .. } = &back else { panic!() };
        assert_eq!(images[0].data, None);
        assert_eq!(images[0].media_type, "image/png");

        // A row from before images: no field at all, and none written for a plain result
        let old: ContentBlock = serde_json::from_str(r#"{"type":"tool_result","tool_use_id":"t0","content":"ok"}"#).unwrap();
        assert_eq!(old, ContentBlock::tool_result("t0", "ok", false));
        assert!(!serde_json::to_string(&old).unwrap().contains("images"));
    }

    #[test]
    fn error_messages_are_for_people() {
        let e = LlmError::Auth { provider: "Anthropic".into() };
        assert_eq!(e.to_string(), "That key was rejected by Anthropic. Check it and paste it again.");
        let app: AppError = e.into();
        assert_eq!(app.code, ErrorCode::PermissionDenied);
    }
}
