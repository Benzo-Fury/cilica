//! Soniox real-time (`stt-rt-v6`, 60+ languages). Configuration goes in the first text frame;
//! results are tokens, final ones sent once and non-final ones re-sent until they settle; with
//! endpoint detection an `<end>` token closes each utterance. An empty text frame ends the audio.
//! With speaker diarization each token carries a `speaker` label ("1", "2"); a change of speaker
//! closes the utterance too, so each line has one speaker.
//! Docs: <https://docs.soniox.com/stt/api-reference/websocket-api>.

use serde::Deserialize;
use tokio_tungstenite::tungstenite::Message;

use super::link::{Audio, Emitter};
use super::ws::Protocol;
use crate::{SttError, SttResult};

pub(crate) const URL: &str = "wss://stt-rt.soniox.com/transcribe-websocket";
pub(crate) const VERIFY_URL: &str = "https://api.soniox.com/v1/models";
const MODEL: &str = "stt-rt-v6";
/// A key without real-time speech-to-text access (Soniox keys have per-feature permissions).
const NOT_REALTIME: &str = "This Soniox key is not allowed to transcribe live. In the Soniox console, edit the key and set \u{201c}Speech-to-text, real-time\u{201d} to Access, then try again.";

pub(crate) struct Soniox {
    pub url: String,
    pub key: String,
    pub language: Option<String>,
    /// Ask Soniox to label speakers.
    pub speakers: bool,
    next_id: u64,
    /// Final text of the utterance in progress, its span and its speaker.
    settled: String,
    span: Option<(u32, u32)>,
    speaker: Option<String>,
    /// Non-final tail from the last message, and the speaker of its first token.
    tail: String,
    tail_speaker: Option<String>,
}

impl Soniox {
    pub fn new(key: String, language: Option<String>, speakers: bool) -> Self {
        Self {
            url: URL.into(),
            key,
            language,
            speakers,
            next_id: 1,
            settled: String::new(),
            span: None,
            speaker: None,
            tail: String::new(),
            tail_speaker: None,
        }
    }

    fn close_utterance(&mut self, out: &Emitter) {
        if !self.settled.trim().is_empty() {
            let (s, e) = self.span.unwrap_or_default();
            out.final_(self.next_id, &self.settled, s, e, self.speaker.as_deref());
            self.next_id += 1;
        }
        self.settled.clear();
        self.span = None;
        self.speaker = None;
    }

    fn extend_span(&mut self, start_ms: Option<u32>, end_ms: Option<u32>, offset_ms: u32) {
        if let (Some(s), Some(e)) = (start_ms, end_ms) {
            let (s, e) = (s + offset_ms, e + offset_ms);
            self.span = Some(self.span.map_or((s, e), |(a, _)| (a, e)));
        }
    }
}

#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    tokens: Vec<Token>,
    #[serde(default)]
    finished: bool,
    error_code: Option<u32>,
    error_type: Option<String>,
    error_message: Option<String>,
}

#[derive(Deserialize)]
struct Token {
    text: String,
    start_ms: Option<u32>,
    end_ms: Option<u32>,
    #[serde(default)]
    is_final: bool,
    speaker: Option<String>,
}

/// Open a live stream with the key and end it at once, with no audio: Soniox checks the key and
/// its real-time permission when the stream starts. Only a refusal counts against the key; any
/// other outcome (the network, a slow reply) leaves it to the model listing that already passed.
pub(crate) async fn verify_realtime(key: &str) -> SttResult<()> {
    verify_realtime_at(URL, key, super::ws::native_tls()?).await
}

pub(crate) async fn verify_realtime_at(url: &str, key: &str, tls: std::sync::Arc<rustls::ClientConfig>) -> SttResult<()> {
    let mut protocol = Soniox::new(key.to_string(), None, false);
    protocol.url = url.to_string();
    let (audio, rx) = tokio::sync::mpsc::unbounded_channel();
    let _ = audio.send(Audio::End);
    match super::ws::run_with(protocol, rx, Emitter::discard(), tls).await {
        Err(e @ (SttError::Auth(_) | SttError::NotAllowed(_))) => Err(e),
        Err(error) => {
            tracing::info!(%error, "could not confirm Soniox real-time access; keeping the key");
            Ok(())
        }
        Ok(()) => Ok(()),
    }
}

impl Protocol for Soniox {
    fn name(&self) -> &str {
        "Soniox"
    }

    fn request(&self) -> SttResult<(String, Vec<(&'static str, String)>)> {
        Ok((self.url.clone(), vec![("Authorization", format!("Bearer {}", self.key))]))
    }

    fn start(&self) -> Vec<Message> {
        let mut config = serde_json::json!({
            "model": MODEL,
            "audio_format": "pcm_s16le",
            "sample_rate": 16000,
            "num_channels": 1,
            "enable_endpoint_detection": true,
        });
        if self.speakers {
            config["enable_speaker_diarization"] = serde_json::json!(true);
        }
        if let Some(l) = &self.language {
            config["language_hints"] = serde_json::json!([l]);
        }
        vec![Message::Text(config.to_string().into())]
    }

    fn end(&mut self) -> Vec<Message> {
        vec![Message::Text(String::new().into())]
    }

    fn keepalive(&self) -> Option<Message> {
        Some(Message::Text(r#"{"type":"keepalive"}"#.into()))
    }

    fn on_text(&mut self, text: &str, offset_ms: u32, out: &Emitter) -> SttResult<bool> {
        let reply: Reply = serde_json::from_str(text).map_err(|_| SttError::Provider("Soniox sent a reply Margin could not read.".into()))?;
        if let Some(code) = reply.error_code {
            if code == 403 || reply.error_type.as_deref() == Some("permission_denied") {
                return Err(SttError::NotAllowed(NOT_REALTIME.into()));
            }
            return Err(match code {
                401 => SttError::Auth("Soniox".into()),
                402 => SttError::Provider("Soniox says the account is out of credit. Top it up, then start recording again.".into()),
                _ => SttError::Provider(format!("Soniox: {}", reply.error_message.unwrap_or_else(|| format!("error {code}")))),
            });
        }
        self.tail.clear();
        self.tail_speaker = None;
        for t in reply.tokens {
            match (t.text.as_str(), t.is_final) {
                ("<end>", true) => self.close_utterance(out),
                ("<fin>", _) => {}
                (_, true) => {
                    // A new speaker starts a new line.
                    if t.speaker.is_some() && self.speaker.is_some() && t.speaker != self.speaker && !self.settled.trim().is_empty() {
                        self.close_utterance(out);
                    }
                    if self.speaker.is_none() {
                        self.speaker = t.speaker.clone();
                    }
                    self.settled.push_str(&t.text);
                    self.extend_span(t.start_ms, t.end_ms, offset_ms);
                }
                (_, false) => {
                    if self.tail.is_empty() {
                        self.tail_speaker = t.speaker.clone();
                    }
                    self.tail.push_str(&t.text);
                    self.extend_span(t.start_ms, t.end_ms, offset_ms);
                }
            }
        }
        let shown = format!("{}{}", self.settled, self.tail);
        if !shown.trim().is_empty() {
            let (s, e) = self.span.unwrap_or_default();
            let speaker = self.speaker.as_deref().or(self.tail_speaker.as_deref());
            out.partial(self.next_id, &shown, s, e, speaker);
        }
        if reply.finished {
            self.close_utterance(out);
            return Ok(true);
        }
        Ok(false)
    }

    fn flush(&mut self, out: &Emitter) {
        // A non-final tail never settled: keep it rather than lose the words.
        let tail = std::mem::take(&mut self.tail);
        if self.speaker.is_none() {
            self.speaker = self.tail_speaker.take();
        }
        self.settled.push_str(&tail);
        self.close_utterance(out);
    }
}
