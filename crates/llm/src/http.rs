//! Shared HTTP plumbing: retries with backoff, status → error mapping, SSE decoding.

use std::time::Duration;

use bytes::Bytes;
use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};

use crate::{CancellationToken, LlmError};

/// How many times and how long to retry 429 / 5xx / overloaded responses.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self { max_retries: 3, base_delay: Duration::from_millis(800), max_delay: Duration::from_secs(20) }
    }
}

impl RetryPolicy {
    /// No waiting, for tests.
    pub fn immediate(max_retries: u32) -> Self {
        Self { max_retries, base_delay: Duration::from_millis(1), max_delay: Duration::from_millis(5) }
    }

    /// Delay before retry number `attempt` (0-based), honouring a server `retry-after` hint.
    pub fn delay(&self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        if let Some(hint) = retry_after {
            return hint.min(self.max_delay);
        }
        let exp = self.base_delay.saturating_mul(2u32.saturating_pow(attempt));
        // Cheap jitter without a rand dependency: 0–25% based on the clock.
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let jitter = exp.mul_f64(f64::from(nanos % 250) / 1000.0);
        (exp + jitter).min(self.max_delay)
    }
}

/// Sleep unless cancelled first. Returns false when cancelled.
pub async fn sleep_or_cancel(delay: Duration, cancel: &CancellationToken) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => true,
        _ = cancel.cancelled() => false,
    }
}

/// Parse a `retry-after` header in seconds.
pub fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s >= 0.0)
        .map(Duration::from_secs_f64)
}

/// Pull a human-readable message out of a provider error body (`{"error":{"message":…}}` or `{"error":"…"}`).
pub fn error_message(body: &str) -> String {
    let parsed: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let message = parsed.as_ref().and_then(|v| {
        v.pointer("/error/message")
            .and_then(|m| m.as_str())
            .or_else(|| v.get("error").and_then(|e| e.as_str()))
            .or_else(|| v.get("message").and_then(|m| m.as_str()))
            .map(str::to_string)
    });
    let message = message.unwrap_or_else(|| body.chars().take(300).collect());
    message.trim().to_string()
}

/// The provider's error type string (`overloaded_error`, `not_found_error`, …) when present.
pub fn error_type(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.pointer("/error/type").and_then(|t| t.as_str()).map(str::to_string)
}

/// Map an HTTP failure to an [`LlmError`].
pub fn status_error(provider: &str, model: &str, status: u16, body: &str) -> LlmError {
    let provider = provider.to_string();
    let kind = error_type(body).unwrap_or_default();
    let message = error_message(body);
    let lower = message.to_lowercase();
    match status {
        401 | 403 => LlmError::Auth { provider },
        404 => LlmError::ModelNotFound { provider, model: model.to_string() },
        429 => LlmError::RateLimited { provider },
        529 => LlmError::Overloaded { provider },
        400 if kind == "not_found_error" || (lower.contains("model") && (lower.contains("not found") || lower.contains("does not exist") || lower.contains("invalid model"))) => {
            LlmError::ModelNotFound { provider, model: model.to_string() }
        }
        _ if kind == "overloaded_error" => LlmError::Overloaded { provider },
        500..=599 => LlmError::Server { provider, status },
        _ => LlmError::BadRequest { provider, message },
    }
}

/// Map a transport error. Never includes headers (and so never the key).
pub fn transport_error(provider: &str, error: &reqwest::Error) -> LlmError {
    tracing::warn!(provider, kind = if error.is_timeout() { "timeout" } else if error.is_connect() { "connect" } else { "other" }, "provider request failed");
    LlmError::Network { provider: provider.to_string() }
}

/// One server-sent event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// Decode a byte stream into SSE events.
pub fn sse_events<S, E>(bytes: S) -> impl Stream<Item = Result<SseEvent, LlmError>> + Send
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    bytes.eventsource().map(|item| match item {
        Ok(ev) => Ok(SseEvent { event: ev.event, data: ev.data }),
        Err(e) => Err(LlmError::Protocol(format!("stream interrupted: {e}"))),
    })
}

/// Split a fixture or body into small byte chunks, for tests that exercise chunk boundaries.
#[cfg(test)]
pub fn chunked(body: &str, size: usize) -> impl Stream<Item = Result<Bytes, std::convert::Infallible>> + Send + 'static {
    let bytes = body.as_bytes().to_vec();
    let chunks: Vec<Result<Bytes, std::convert::Infallible>> =
        bytes.chunks(size.max(1)).map(|c| Ok(Bytes::copy_from_slice(c))).collect();
    futures::stream::iter(chunks)
}

#[cfg(test)]
pub mod test_server {
    //! A tiny HTTP/1.1 server that answers each connection with the next canned response.
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[derive(Clone)]
    pub struct Canned {
        pub status: u16,
        pub content_type: &'static str,
        pub body: String,
        pub headers: Vec<(String, String)>,
    }

    impl Canned {
        pub fn sse(body: &str) -> Self {
            Self { status: 200, content_type: "text/event-stream", body: body.into(), headers: vec![] }
        }
        pub fn json(status: u16, body: &str) -> Self {
            Self { status, content_type: "application/json", body: body.into(), headers: vec![] }
        }
    }

    /// Requests seen by the server (raw head + body).
    pub type Seen = Arc<Mutex<Vec<String>>>;

    pub async fn serve(responses: Vec<Canned>) -> (String, Seen) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: Seen = Arc::default();
        let seen2 = seen.clone();
        tokio::spawn(async move {
            for canned in responses {
                let Ok((mut sock, _)) = listener.accept().await else { return };
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                // Read the head, then the body by content-length.
                loop {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = find(&buf, b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                        let len = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                            .unwrap_or(0);
                        while buf.len() < pos + 4 + len {
                            let n = sock.read(&mut tmp).await.unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            buf.extend_from_slice(&tmp[..n]);
                        }
                        break;
                    }
                }
                seen2.lock().unwrap().push(String::from_utf8_lossy(&buf).to_string());
                let mut head = format!(
                    "HTTP/1.1 {} X\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n",
                    canned.status,
                    canned.content_type,
                    canned.body.len()
                );
                for (k, v) in &canned.headers {
                    head.push_str(&format!("{k}: {v}\r\n"));
                }
                head.push_str("\r\n");
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(canned.body.as_bytes()).await;
                let _ = sock.shutdown().await;
            }
        });
        (format!("http://{addr}"), seen)
    }

    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_statuses_to_people_errors() {
        assert!(matches!(status_error("Anthropic", "m", 401, "{}"), LlmError::Auth { .. }));
        assert!(matches!(status_error("Anthropic", "m", 529, "{}"), LlmError::Overloaded { .. }));
        assert!(matches!(
            status_error("Anthropic", "m", 400, r#"{"type":"error","error":{"type":"not_found_error","message":"model: x"}}"#),
            LlmError::ModelNotFound { .. }
        ));
        let bad = status_error("Anthropic", "m", 400, r#"{"error":{"type":"invalid_request_error","message":"messages: empty"}}"#);
        assert_eq!(bad.to_string(), "Anthropic turned the request down: messages: empty");
    }

    #[test]
    fn retry_delay_honours_hint_and_cap() {
        let p = RetryPolicy::default();
        assert_eq!(p.delay(0, Some(Duration::from_secs(2))), Duration::from_secs(2));
        assert_eq!(p.delay(0, Some(Duration::from_secs(600))), p.max_delay);
        assert!(p.delay(10, None) <= p.max_delay);
    }
}
