//! Deepgram live streaming (`/v1/listen`, Nova-3). Interim results arrive as `Results` with
//! `is_final: false`; `is_final: true` settles a piece of text, and `speech_final` (or an
//! `UtteranceEnd`) means the speaker paused, which closes the utterance. `CloseStream` ends the
//! audio; Deepgram answers with its last results and a `Metadata` summary. With diarization each
//! word carries a `speaker` number; settled words are split into lines at each change of speaker.
//! Docs: <https://developers.deepgram.com/reference/speech-to-text/listen-streaming>.

use serde::Deserialize;
use tokio_tungstenite::tungstenite::Message;

use super::link::Emitter;
use super::ws::{Protocol, secs_ms};
use crate::{SttError, SttResult};

pub(crate) const URL: &str = "wss://api.deepgram.com/v1/listen";
pub(crate) const VERIFY_URL: &str = "https://api.deepgram.com/v1/projects";

pub(crate) struct Deepgram {
    pub url: String,
    pub key: String,
    pub language: Option<String>,
    /// Ask Deepgram to label speakers.
    pub speakers: bool,
    next_id: u64,
    settled: String,
    span: Option<(u32, u32)>,
    /// Speaker of the settled text.
    speaker: Option<String>,
    /// The latest interim text after the settled part, and its first word's speaker.
    interim: String,
    interim_speaker: Option<String>,
    /// `CloseStream` was sent: the next `Metadata` means everything has arrived.
    closing: bool,
}

impl Deepgram {
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
            interim: String::new(),
            interim_speaker: None,
            closing: false,
        }
    }

    fn close_utterance(&mut self, out: &Emitter) {
        if !self.settled.trim().is_empty() {
            let (s, e) = self.span.unwrap_or_default();
            out.final_(self.next_id, &self.settled, s, e, self.speaker.as_deref());
            self.next_id += 1;
        }
        self.settled.clear();
        self.interim.clear();
        self.interim_speaker = None;
        self.span = None;
        self.speaker = None;
    }

    fn extend_span(&mut self, s: u32, e: u32) {
        self.span = Some(self.span.map_or((s, e), |(a, _)| (a, e)));
    }

    /// Settle words that carry speakers: one line per run of the same speaker.
    fn settle_words(&mut self, words: &[Word], offset_ms: u32, out: &Emitter) {
        let mut i = 0;
        while i < words.len() {
            let speaker = words[i].speaker.map(|n| n.to_string());
            let run = words[i..].iter().take_while(|w| w.speaker.map(|n| n.to_string()) == speaker).count();
            let run_words = &words[i..i + run];
            if speaker.is_some() && self.speaker.is_some() && speaker != self.speaker && !self.settled.trim().is_empty() {
                self.close_utterance(out);
            }
            if self.speaker.is_none() {
                self.speaker = speaker;
            }
            let text: Vec<&str> = run_words.iter().map(Word::text).collect();
            self.settled = join(&self.settled, &text.join(" "));
            if let (Some(first), Some(last)) = (run_words.first(), run_words.last()) {
                self.extend_span(secs_ms(first.start) + offset_ms, secs_ms(last.end) + offset_ms);
            }
            i += run;
        }
    }
}

#[derive(Deserialize)]
struct Reply {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    speech_final: bool,
    channel: Option<Channel>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct Channel {
    alternatives: Vec<Alternative>,
}

#[derive(Deserialize)]
struct Alternative {
    transcript: String,
    #[serde(default)]
    words: Vec<Word>,
}

#[derive(Deserialize)]
struct Word {
    word: String,
    punctuated_word: Option<String>,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    end: f64,
    speaker: Option<u32>,
}

impl Word {
    fn text(&self) -> &str {
        self.punctuated_word.as_deref().unwrap_or(&self.word)
    }
}

fn join(a: &str, b: &str) -> String {
    match (a.trim().is_empty(), b.trim().is_empty()) {
        (true, _) => b.trim().to_string(),
        (_, true) => a.trim().to_string(),
        _ => format!("{} {}", a.trim(), b.trim()),
    }
}

impl Protocol for Deepgram {
    fn name(&self) -> &str {
        "Deepgram"
    }

    fn request(&self) -> SttResult<(String, Vec<(&'static str, String)>)> {
        let language = self.language.as_deref().unwrap_or("multi");
        let url = format!(
            "{}?model=nova-3&encoding=linear16&sample_rate=16000&channels=1&language={language}\
             &interim_results=true&endpointing=300&utterance_end_ms=1000&smart_format=true&punctuate=true{}",
            self.url,
            if self.speakers { "&diarize=true" } else { "" }
        );
        Ok((url, vec![("Authorization", format!("Token {}", self.key))]))
    }

    fn end(&mut self) -> Vec<Message> {
        self.closing = true;
        vec![Message::Text(r#"{"type":"CloseStream"}"#.into())]
    }

    fn keepalive(&self) -> Option<Message> {
        Some(Message::Text(r#"{"type":"KeepAlive"}"#.into()))
    }

    fn on_text(&mut self, text: &str, offset_ms: u32, out: &Emitter) -> SttResult<bool> {
        let reply: Reply = serde_json::from_str(text).map_err(|_| SttError::Provider("Deepgram sent a reply Margin could not read.".into()))?;
        match reply.kind.as_str() {
            "Results" => {
                let (transcript, words) = reply.channel.and_then(|c| c.alternatives.into_iter().next()).map(|a| (a.transcript, a.words)).unwrap_or_default();
                let labelled = self.speakers && words.iter().any(|w| w.speaker.is_some());
                if reply.is_final {
                    if labelled {
                        self.settle_words(&words, offset_ms, out);
                    } else {
                        if !transcript.trim().is_empty() {
                            self.extend_span(secs_ms(reply.start) + offset_ms, secs_ms(reply.start + reply.duration) + offset_ms);
                        }
                        self.settled = join(&self.settled, &transcript);
                    }
                    self.interim.clear();
                    self.interim_speaker = None;
                    if reply.speech_final {
                        self.close_utterance(out);
                        return Ok(false);
                    }
                } else {
                    if !transcript.trim().is_empty() {
                        self.extend_span(secs_ms(reply.start) + offset_ms, secs_ms(reply.start + reply.duration) + offset_ms);
                    }
                    self.interim = transcript;
                    self.interim_speaker = words.first().and_then(|w| w.speaker).map(|n| n.to_string());
                }
                let shown = join(&self.settled, &self.interim);
                if !shown.is_empty() {
                    let (s, e) = self.span.unwrap_or_default();
                    let speaker = self.speaker.as_deref().or(self.interim_speaker.as_deref());
                    out.partial(self.next_id, &shown, s, e, speaker);
                }
                Ok(false)
            }
            "UtteranceEnd" => {
                self.close_utterance(out);
                Ok(false)
            }
            "Metadata" if self.closing => {
                self.close_utterance(out);
                Ok(true)
            }
            "Error" => Err(SttError::Provider(format!("Deepgram: {}", reply.description.unwrap_or_else(|| "the stream failed".into())))),
            _ => Ok(false),
        }
    }

    fn flush(&mut self, out: &Emitter) {
        self.settled = join(&self.settled, &self.interim);
        if self.speaker.is_none() {
            self.speaker = self.interim_speaker.take();
        }
        self.close_utterance(out);
    }
}
