//! AssemblyAI streaming (v3). Each `Turn` message replaces the previous one for the same
//! `turn_order`; a turn is done when `end_of_turn` and `turn_is_formatted` are both true.
//! `Terminate` ends the audio and the service answers with its last turns and `Termination`.
//! Billing runs while the connection is open, so it is closed as soon as the audio ends.
//! With `speaker_labels` each turn carries a `speaker_label` ("A", "B"; "PENDING" while too short
//! to tell). Later `SpeakerRevision` corrections are not applied: the check on this computer after
//! the meeting is more accurate.
//! Docs: <https://www.assemblyai.com/docs/api-reference/streaming-api/streaming-api>.

use serde::Deserialize;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

use super::link::Emitter;
use super::ws::Protocol;
use crate::{SttError, SttResult};

pub(crate) const URL: &str = "wss://streaming.assemblyai.com/v3/ws";
pub(crate) const VERIFY_URL: &str = "https://api.assemblyai.com/v2/transcript?limit=1";

pub(crate) struct AssemblyAi {
    pub url: String,
    pub key: String,
    pub language: Option<String>,
    /// Ask AssemblyAI to label speakers.
    pub speakers: bool,
    /// Utterance id of `turn_order` 0 on the current connection.
    base_id: u64,
    next_id: u64,
    /// The turn in progress: (id, text, start, end, speaker).
    open: Option<(u64, String, u32, u32, Option<String>)>,
}

impl AssemblyAi {
    pub fn new(key: String, language: Option<String>, speakers: bool) -> Self {
        Self { url: URL.into(), key, language, speakers, base_id: 1, next_id: 1, open: None }
    }

    /// English-only streaming is cheapest; anything else (or detecting) needs the multilingual model.
    fn model(&self) -> &'static str {
        if self.language.as_deref() == Some("en") { "universal-streaming-english" } else { "universal-3-6-pro" }
    }
}

#[derive(Deserialize)]
struct Reply {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    turn_order: u64,
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    end_of_turn: bool,
    #[serde(default)]
    turn_is_formatted: bool,
    #[serde(default)]
    words: Vec<Word>,
    speaker_label: Option<String>,
    error_code: Option<u32>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct Word {
    start: u32,
    end: u32,
}

fn auth_or_provider(code: u32, message: &str) -> SttError {
    let m = message.to_lowercase();
    if code == 1008 && (m.contains("insufficient") || m.contains("funds")) {
        SttError::Provider("AssemblyAI says the account is out of credit. Top it up, then start recording again.".into())
    } else if code == 1008 && m.contains("concurrent") {
        SttError::Provider("AssemblyAI is limiting new sessions right now. Try again in a minute.".into())
    } else if code == 1008 || code == 1011 {
        SttError::Auth("AssemblyAI".into())
    } else {
        SttError::Provider(format!("AssemblyAI: {message}"))
    }
}

impl Protocol for AssemblyAi {
    fn name(&self) -> &str {
        "AssemblyAI"
    }

    fn request(&self) -> SttResult<(String, Vec<(&'static str, String)>)> {
        let mut url = format!("{}?sample_rate=16000&encoding=pcm_s16le&speech_model={}&format_turns=true", self.url, self.model());
        if self.language.is_none() {
            url.push_str("&language_detection=true");
        }
        if self.speakers {
            url.push_str("&speaker_labels=true");
        }
        Ok((url, vec![("Authorization", self.key.clone())]))
    }

    fn end(&mut self) -> Vec<Message> {
        vec![Message::Text(r#"{"type":"Terminate"}"#.into())]
    }

    fn on_text(&mut self, text: &str, offset_ms: u32, out: &Emitter) -> SttResult<bool> {
        let reply: Reply = serde_json::from_str(text).map_err(|_| SttError::Provider("AssemblyAI sent a reply Margin could not read.".into()))?;
        match reply.kind.as_str() {
            "Begin" => {
                // Turn numbers restart on each connection.
                self.base_id = self.next_id;
                Ok(false)
            }
            "Turn" => {
                let id = self.base_id + reply.turn_order;
                self.next_id = self.next_id.max(id + 1);
                let start = reply.words.first().map(|w| w.start + offset_ms).unwrap_or(offset_ms);
                let end = reply.words.last().map(|w| w.end + offset_ms).unwrap_or(start);
                let speaker = reply.speaker_label.filter(|l| !l.is_empty() && l != "PENDING");
                if reply.end_of_turn && reply.turn_is_formatted {
                    out.final_(id, &reply.transcript, start, end, speaker.as_deref());
                    self.open = None;
                } else if !reply.transcript.trim().is_empty() {
                    out.partial(id, &reply.transcript, start, end, speaker.as_deref());
                    self.open = Some((id, reply.transcript, start, end, speaker));
                }
                Ok(false)
            }
            "Termination" => {
                self.flush(out);
                Ok(true)
            }
            "Error" => Err(auth_or_provider(reply.error_code.unwrap_or(0), reply.error.as_deref().unwrap_or("the stream failed"))),
            _ => Ok(false),
        }
    }

    fn flush(&mut self, out: &Emitter) {
        if let Some((id, text, s, e, speaker)) = self.open.take() {
            out.final_(id, &text, s, e, speaker.as_deref());
        }
    }

    fn on_close(&self, code: CloseCode, reason: &str) -> Option<SttError> {
        let code: u16 = code.into();
        match code {
            1008 | 1011 | 3005..=3008 => Some(auth_or_provider(code as u32, reason)),
            _ => None,
        }
    }
}
