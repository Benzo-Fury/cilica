//! A scriptable provider for tests: queue responses (text and tool calls), then inspect the requests.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    CancellationToken, ChatRequest, ChatStream, Completion, ContentBlock, LlmError, Provider, StopReason, StreamEvent,
    ToolCall, Usage,
};

/// One scripted model response.
#[derive(Debug, Clone)]
pub enum MockResponse {
    Reply { text: String, tool_calls: Vec<ToolCall>, stop: Option<StopReason> },
    Error(LlmError),
    /// Streams `text` and then waits until cancelled.
    Hang { text: String },
}

impl MockResponse {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Reply { text: text.into(), tool_calls: Vec::new(), stop: None }
    }

    /// A response that calls one tool.
    pub fn tool(name: &str, input: Value) -> Self {
        Self::tools(vec![(name, input)])
    }

    /// A response that calls several tools in parallel.
    pub fn tools(calls: Vec<(&str, Value)>) -> Self {
        let tool_calls = calls
            .into_iter()
            .enumerate()
            .map(|(i, (name, input))| ToolCall { id: format!("toolu_mock_{}_{i}", crate_unique()), name: name.to_string(), input })
            .collect();
        Self::Reply { text: String::new(), tool_calls, stop: None }
    }

    pub fn with_text(self, text: impl Into<String>) -> Self {
        match self {
            Self::Reply { tool_calls, stop, .. } => Self::Reply { text: text.into(), tool_calls, stop },
            other => other,
        }
    }
}

fn crate_unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Scripted provider. When the queue is empty it answers with `fallback` text (or an error if unset).
#[derive(Default)]
pub struct MockProvider {
    script: Mutex<VecDeque<MockResponse>>,
    requests: Mutex<Vec<ChatRequest>>,
    fallback: Mutex<Option<String>>,
}

impl MockProvider {
    pub fn new(script: Vec<MockResponse>) -> Self {
        Self { script: Mutex::new(script.into()), ..Default::default() }
    }

    /// Answer with this text once the script runs out.
    pub fn with_fallback(self, text: impl Into<String>) -> Self {
        *self.fallback.lock().unwrap() = Some(text.into());
        self
    }

    pub fn push(&self, response: MockResponse) {
        self.script.lock().unwrap().push_back(response);
    }

    /// Every request received, in order.
    pub fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub fn remaining(&self) -> usize {
        self.script.lock().unwrap().len()
    }
}

#[async_trait::async_trait]
impl Provider for MockProvider {
    fn display_name(&self) -> &str {
        "Mock"
    }

    fn stream(&self, request: ChatRequest, cancel: CancellationToken) -> ChatStream {
        self.requests.lock().unwrap().push(request);
        let next = self.script.lock().unwrap().pop_front();
        let response = match next {
            Some(r) => r,
            None => match self.fallback.lock().unwrap().clone() {
                Some(text) => MockResponse::text(text),
                None => MockResponse::Error(LlmError::Protocol("MockProvider has no scripted response left".into())),
            },
        };
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            match response {
                MockResponse::Error(e) => {
                    let _ = tx.send(Err(e)).await;
                }
                MockResponse::Hang { text } => {
                    let _ = tx.send(Ok(StreamEvent::TextDelta(text))).await;
                    cancel.cancelled().await;
                    let _ = tx.send(Err(LlmError::Cancelled)).await;
                }
                MockResponse::Reply { text, tool_calls, stop } => {
                    let mut content = Vec::new();
                    if !text.is_empty() {
                        // Stream word by word.
                        for piece in text.split_inclusive(' ') {
                            if cancel.is_cancelled() {
                                let _ = tx.send(Err(LlmError::Cancelled)).await;
                                return;
                            }
                            let _ = tx.send(Ok(StreamEvent::TextDelta(piece.to_string()))).await;
                        }
                        content.push(ContentBlock::Text { text });
                    }
                    for call in &tool_calls {
                        content.push(ContentBlock::ToolUse { id: call.id.clone(), name: call.name.clone(), input: call.input.clone() });
                        let _ = tx.send(Ok(StreamEvent::ToolCall(call.clone()))).await;
                    }
                    let stop_reason = stop.unwrap_or(if tool_calls.is_empty() { StopReason::EndTurn } else { StopReason::ToolUse });
                    let usage = Usage { input_tokens: 10, output_tokens: 5, ..Default::default() };
                    let _ = tx.send(Ok(StreamEvent::Done(Completion { stop_reason, usage, content, notice: None }))).await;
                }
            }
        });
        crate::receiver_stream(rx)
    }

    async fn verify(&self) -> Result<(), LlmError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChatMessage, complete};

    #[tokio::test]
    async fn plays_the_script_in_order() {
        let mock = MockProvider::new(vec![
            MockResponse::tool("search_notes", serde_json::json!({"query": "x"})),
            MockResponse::text("done here"),
        ]);
        let req = ChatRequest::new("m", "s", vec![ChatMessage::user("hi")]);
        let (_, first) = complete(&mock, req.clone(), CancellationToken::new()).await.unwrap();
        assert_eq!(first.stop_reason, StopReason::ToolUse);
        let (text, second) = complete(&mock, req, CancellationToken::new()).await.unwrap();
        assert_eq!(text, "done here");
        assert_eq!(second.stop_reason, StopReason::EndTurn);
        assert_eq!(mock.requests().len(), 2);
    }
}
