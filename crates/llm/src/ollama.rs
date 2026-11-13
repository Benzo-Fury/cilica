//! Ollama model management over its native HTTP API (`/api/version`, `/api/tags`, `/api/pull`,
//! `/api/delete`). Chat itself goes through [`crate::openai::OpenAiCompatProvider::ollama`].
//!
//! Pull progress is reported as `AppEvent::ModelDownload` with `model_id` = [`progress_id`], so the
//! UI reuses the speech-model download plumbing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use cilica_domain::{AppError, AppEvent, AppResult, Events, LocalLlmModel, ModelDownloadProgress};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

/// The Ollama server root when settings give none.
pub const OLLAMA_ROOT: &str = "http://localhost:11434";

/// Models Cilica offers. All support tool calling, which the assistant needs.
const CATALOG: &[(&str, &str, &str, u32, u32, bool)] = &[
    ("qwen3:4b", "Qwen3 4B", "Fast and light. Fine for titles and short summaries.", 2600, 8, false),
    ("qwen3:8b", "Qwen3 8B", "The best balance for most Macs.", 5200, 16, true),
    ("qwen3:14b", "Qwen3 14B", "Sharper summaries and edits. Slower.", 9300, 24, false),
    ("gpt-oss:20b", "gpt-oss 20B", "Strongest reasoning here. Needs a lot of memory.", 14000, 32, false),
];

/// Progress id suffix for installing Ollama itself (events carry `ollama:app`).
pub const APP_ID: &str = "app";

/// The model a fresh setup pulls.
pub fn recommended_model() -> &'static str {
    CATALOG.iter().find(|m| m.5).map(|m| m.0).unwrap_or("qwen3:8b")
}

/// The `model_id` on download events for an Ollama model.
pub fn progress_id(model: &str) -> String {
    format!("ollama:{model}")
}

/// `http://host:11434/v1/` → `http://host:11434`.
pub fn root_url(base_url: Option<&str>) -> String {
    let base = base_url.map(str::trim).filter(|u| !u.is_empty()).unwrap_or(OLLAMA_ROOT);
    let base = base.trim_end_matches('/');
    base.strip_suffix("/v1").unwrap_or(base).to_string()
}

/// Ollama on this machine, which Cilica may install and start.
pub fn is_local_root(root: &str) -> bool {
    let u = root.to_lowercase();
    u.contains("localhost") || u.contains("127.0.0.1") || u.contains("[::1]")
}

/// `qwen3:8b` matches `qwen3:8b`; `llama3` matches `llama3:latest`.
fn same_tag(a: &str, b: &str) -> bool {
    let norm = |s: &str| if s.contains(':') { s.to_string() } else { format!("{s}:latest") };
    norm(a) == norm(b)
}

#[derive(Deserialize)]
struct Tags {
    #[serde(default)]
    models: Vec<Tag>,
}
#[derive(Deserialize)]
struct Tag {
    name: String,
}
#[derive(Deserialize)]
struct Version {
    version: String,
}
#[derive(Deserialize, Default)]
struct PullLine {
    #[serde(default)]
    status: String,
    digest: Option<String>,
    total: Option<f64>,
    completed: Option<f64>,
    error: Option<String>,
}

/// Talks to a local Ollama server and tracks pulls in flight.
#[derive(Clone)]
pub struct OllamaManager {
    http: reqwest::Client,
    events: Events,
    pulls: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl OllamaManager {
    pub fn new(events: Events) -> Self {
        Self { http: reqwest::Client::new(), events, pulls: Arc::default() }
    }

    /// The server's version, or `None` when it does not answer.
    pub async fn version(&self, root: &str) -> Option<String> {
        let resp = self.http.get(format!("{root}/api/version")).timeout(Duration::from_secs(2)).send().await.ok()?;
        resp.json::<Version>().await.ok().map(|v| v.version)
    }

    /// Tags already pulled into Ollama.
    pub async fn installed(&self, root: &str) -> AppResult<Vec<String>> {
        let resp = self
            .http
            .get(format!("{root}/api/tags"))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|_| unreachable_err())?;
        let tags: Tags = resp.json().await.map_err(|_| unreachable_err())?;
        Ok(tags.models.into_iter().map(|t| t.name).collect())
    }

    /// The catalog, marked with what is already pulled. Models the person pulled themselves are
    /// listed after it so they can be chosen too.
    pub async fn list(&self, root: &str) -> Vec<LocalLlmModel> {
        let installed = self.installed(root).await.unwrap_or_default();
        let mut out: Vec<LocalLlmModel> = CATALOG
            .iter()
            .map(|&(id, name, description, size_mb, min_ram_gb, recommended)| LocalLlmModel {
                id: id.into(),
                name: name.into(),
                description: description.into(),
                size_mb,
                min_ram_gb,
                recommended,
                downloaded: installed.iter().any(|t| same_tag(t, id)),
            })
            .collect();
        for tag in installed {
            if !CATALOG.iter().any(|m| same_tag(m.0, &tag)) {
                out.push(LocalLlmModel {
                    id: tag.clone(),
                    name: tag,
                    description: "Already in Ollama. It may not support Cilica's tools.".into(),
                    size_mb: 0,
                    min_ram_gb: 0,
                    recommended: false,
                    downloaded: true,
                });
            }
        }
        out
    }

    pub fn is_pulling(&self, model: &str) -> bool {
        self.pulls.lock().unwrap().contains_key(model)
    }

    /// Start pulling `model` in the background. Progress and the outcome arrive as events.
    pub fn start_pull(&self, root: String, model: String) -> AppResult<()> {
        let token = {
            let mut pulls = self.pulls.lock().unwrap();
            if pulls.contains_key(&model) {
                return Ok(());
            }
            let token = CancellationToken::new();
            pulls.insert(model.clone(), token.clone());
            token
        };
        let this = self.clone();
        tokio::spawn(async move {
            let result = tokio::select! {
                r = this.pull(&root, &model) => r,
                _ = token.cancelled() => Err("Download cancelled.".to_string()),
            };
            this.pulls.lock().unwrap().remove(&model);
            let (done, error) = match result {
                Ok(()) => (true, None),
                Err(e) => (false, Some(e)),
            };
            this.emit(&model, 0.0, 0.0, done, error);
        });
        Ok(())
    }

    pub fn cancel(&self, model: &str) {
        if let Some(token) = self.pulls.lock().unwrap().remove(model) {
            token.cancel();
        }
    }

    pub async fn delete(&self, root: &str, model: &str) -> AppResult<()> {
        let resp = self
            .http
            .delete(format!("{root}/api/delete"))
            .json(&serde_json::json!({ "model": model }))
            .send()
            .await
            .map_err(|_| unreachable_err())?;
        if resp.status().is_success() || resp.status().as_u16() == 404 {
            Ok(())
        } else {
            Err(AppError::unavailable("Ollama did not delete the model. Try again."))
        }
    }

    /// Streams `/api/pull`. Ollama reports progress per layer; the sum over layers is the total.
    async fn pull(&self, root: &str, model: &str) -> Result<(), String> {
        let resp = self
            .http
            .post(format!("{root}/api/pull"))
            .json(&serde_json::json!({ "model": model, "stream": true }))
            .send()
            .await
            .map_err(|_| "Cilica could not reach Ollama.".to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Ollama refused the download ({}).", resp.status().as_u16()));
        }
        let mut layers: HashMap<String, (f64, f64)> = HashMap::new();
        let mut buf = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "The connection to Ollama dropped.".to_string())?;
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let Ok(line) = serde_json::from_slice::<PullLine>(&line) else { continue };
                if let Some(error) = line.error {
                    return Err(format!("Ollama said: {error}."));
                }
                if line.status == "success" {
                    return Ok(());
                }
                if let (Some(digest), Some(total)) = (line.digest, line.total) {
                    layers.insert(digest, (line.completed.unwrap_or(0.0), total));
                    let (done, total) = layers.values().fold((0.0, 0.0), |a, l| (a.0 + l.0, a.1 + l.1));
                    self.emit(model, done, total, false, None);
                }
            }
        }
        Err("Ollama stopped before the model finished downloading.".into())
    }

    /// Download Ollama's installer to `dest`, reporting progress under `progress_id(APP_ID)`.
    /// Unpacking and launching it is the shell's job (it is platform-specific).
    pub async fn download_installer(&self, url: &str, dest: &std::path::Path) -> AppResult<()> {
        let fail = |m: &str| AppError::unavailable(format!("{m} Check your connection, then try again."));
        let resp = self.http.get(url).send().await.map_err(|_| fail("Ollama did not download."))?;
        if !resp.status().is_success() {
            return Err(fail("Ollama did not download."));
        }
        let total = resp.content_length().unwrap_or(0) as f64;
        let mut file = tokio::fs::File::create(dest).await.map_err(|_| fail("Cilica could not save Ollama."))?;
        let (mut done, mut last) = (0.0, 0.0);
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| fail("The download of Ollama dropped."))?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await.map_err(|_| fail("Cilica could not save Ollama."))?;
            done += chunk.len() as f64;
            if done - last > 1_000_000.0 {
                last = done;
                self.emit(APP_ID, done, total, false, None);
            }
        }
        Ok(())
    }

    /// Announce the end of the Ollama install (or why it failed).
    pub fn finish_install(&self, error: Option<String>) {
        self.emit(APP_ID, 0.0, 0.0, error.is_none(), error);
    }

    fn emit(&self, model: &str, downloaded: f64, total: f64, done: bool, error: Option<String>) {
        self.events.emit(AppEvent::ModelDownload {
            progress: ModelDownloadProgress {
                model_id: progress_id(model),
                downloaded_bytes: downloaded,
                total_bytes: total,
                done,
                error,
            },
        });
    }
}

fn unreachable_err() -> AppError {
    AppError::unavailable("Cilica could not reach Ollama. Set it up in Settings › Cilica, then try again.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_strips_v1() {
        assert_eq!(root_url(None), OLLAMA_ROOT);
        assert_eq!(root_url(Some("http://localhost:11434/v1/")), "http://localhost:11434");
        assert_eq!(root_url(Some("http://box:11434")), "http://box:11434");
    }

    #[test]
    fn tags_match_latest() {
        assert!(same_tag("llama3", "llama3:latest"));
        assert!(same_tag("qwen3:8b", "qwen3:8b"));
        assert!(!same_tag("qwen3:8b", "qwen3:4b"));
    }

    #[test]
    fn one_recommended() {
        assert_eq!(CATALOG.iter().filter(|m| m.5).count(), 1);
        assert_eq!(recommended_model(), "qwen3:8b");
    }
}
