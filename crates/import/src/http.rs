//! HTTP for account imports: one client, polite pacing per provider, and errors written for people.

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::ImportError;

/// Reports a wait ("Waiting for Fathom's rate limit") so the UI can say why nothing moves.
pub type WaitNotice = Arc<dyn Fn(Option<String>) + Send + Sync>;

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    /// Provider name for messages ("Fireflies didn't accept that key").
    name: &'static str,
    /// At least this long between requests.
    gap: Duration,
    last: Arc<Mutex<Option<Instant>>>,
    headers: HeaderMap,
    notice: Option<WaitNotice>,
}

/// One response: the JSON body and the `Link: <…>; rel="next"` URL, if any.
pub struct Reply {
    pub body: Value,
    pub next_link: Option<String>,
}

impl Http {
    pub fn new(name: &'static str, gap: Duration) -> Self {
        let client = reqwest::Client::builder()
            // tl;dv refuses library user agents; every provider gets an honest, descriptive one.
            .user_agent(concat!("Cilica-notes-importer/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self { client, name, gap, last: Arc::new(Mutex::new(None)), headers: HeaderMap::new(), notice: None }
    }

    pub fn header(mut self, name: &'static str, value: &str) -> Self {
        if let Ok(v) = HeaderValue::from_str(value) {
            self.headers.insert(HeaderName::from_static(name), v);
        }
        self
    }

    pub fn bearer(self, key: &str) -> Self {
        self.header("authorization", &format!("Bearer {}", key.trim()))
    }

    pub fn with_notice(mut self, notice: Option<WaitNotice>) -> Self {
        self.notice = notice;
        self
    }

    pub async fn get(&self, url: &str) -> Result<Reply, ImportError> {
        self.send(|| self.client.get(url)).await
    }

    pub async fn post_json(&self, url: &str, body: &Value) -> Result<Reply, ImportError> {
        self.send(|| self.client.post(url).json(body)).await
    }

    async fn send(&self, build: impl Fn() -> reqwest::RequestBuilder) -> Result<Reply, ImportError> {
        let mut attempt = 0u32;
        loop {
            self.pace().await;
            let response = build().headers(self.headers.clone()).send().await.map_err(|e| {
                if e.is_timeout() || e.is_connect() {
                    ImportError::upstream(format!("Cilica could not reach {}. Check your internet connection and try again.", self.name))
                } else {
                    ImportError::upstream(format!("{} did not answer properly. Try again in a minute.", self.name))
                }
            })?;
            let status = response.status();
            if status.as_u16() == 429 || status.is_server_error() {
                attempt += 1;
                if attempt > 6 {
                    return Err(if status.as_u16() == 429 {
                        ImportError::upstream(format!("{} is limiting how fast Cilica can read. Try again later; notes already imported stay.", self.name))
                    } else {
                        ImportError::upstream(format!("{} is having trouble right now. Try again later.", self.name))
                    });
                }
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or(Duration::from_secs(2u64.pow(attempt.min(5))))
                    .min(Duration::from_secs(120));
                if let Some(n) = &self.notice {
                    n(Some(format!("Waiting for {}'s rate limit", self.name)));
                }
                tokio::time::sleep(wait).await;
                if let Some(n) = &self.notice {
                    n(None);
                }
                continue;
            }
            match status.as_u16() {
                401 => return Err(ImportError::invalid(format!("{} didn't accept the API key. Copy it again from {}'s settings and paste it here.", self.name, self.name))),
                402 | 403 => {
                    return Err(ImportError::invalid(format!(
                        "{} refused access. API access may need a different plan, or an admin may have turned it off. Check {}'s settings.",
                        self.name, self.name
                    )));
                }
                404 => return Err(ImportError::not_found(format!("{} could not find that item.", self.name))),
                413 => return Err(ImportError::too_large()),
                s if !(200..300).contains(&s) => return Err(ImportError::upstream(format!("{} answered with an error ({s}). Try again later.", self.name))),
                _ => {}
            }
            let next_link = response.headers().get("link").and_then(|v| v.to_str().ok()).and_then(next_from_link);
            let text = response.text().await.map_err(|_| ImportError::upstream(format!("{} stopped answering halfway. Try again.", self.name)))?;
            let body = if text.trim().is_empty() { Value::Null } else { serde_json::from_str(&text).map_err(|_| ImportError::upstream(format!("{} answered in a way Cilica did not understand.", self.name)))? };
            return Ok(Reply { body, next_link });
        }
    }

    async fn pace(&self) {
        let mut last = self.last.lock().await;
        if let Some(at) = *last {
            let since = at.elapsed();
            if since < self.gap {
                tokio::time::sleep(self.gap - since).await;
            }
        }
        *last = Some(Instant::now());
    }
}

/// `<https://x/api/meetings?cursor=abc>; rel="next"` → the URL.
fn next_from_link(header: &str) -> Option<String> {
    header.split(',').find(|part| part.contains("rel=\"next\"") || part.contains("rel=next")).and_then(|part| {
        let start = part.find('<')? + 1;
        let end = part.find('>')?;
        Some(part[start..end].to_string())
    })
}

/// Percent-encode a query value.
pub fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_headers() {
        assert_eq!(next_from_link(r#"</api/meetings?cursor=abc>; rel="next""#).as_deref(), Some("/api/meetings?cursor=abc"));
        assert_eq!(next_from_link(r#"<a>; rel="prev", <b>; rel="next""#).as_deref(), Some("b"));
        assert_eq!(next_from_link(r#"<a>; rel="prev""#), None);
        assert_eq!(encode(r#"{"a":1}"#), "%7B%22a%22%3A1%7D");
    }
}
