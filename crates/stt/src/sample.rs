//! A short, known clip for checking that an engine works end to end: Settings › Transcription
//! streams it as a meeting would (in real time, in 100 ms pieces) and shows what comes back.
//!
//! The clip is John F. Kennedy's 1961 inaugural address, a US government recording in the public
//! domain (`assets/sample_speech.wav`, from whisper.cpp's `samples/jfk.wav`): 11 s, 16 kHz mono.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::{SAMPLE_RATE, SttError, SttEvent, SttResult, Transcriber};

/// What the clip says.
pub const SAMPLE_TEXT: &str = "And so, my fellow Americans, ask not what your country can do for you, ask what you can do for your country.";

const SAMPLE_WAV: &[u8] = include_bytes!("../assets/sample_speech.wav");

/// Audio is sent in pieces of this size, as live capture delivers it.
const STEP: usize = SAMPLE_RATE as usize / 10;

/// The clip as 16 kHz mono samples.
pub fn sample_audio() -> Vec<f32> {
    pcm_data(SAMPLE_WAV).chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / i16::MAX as f32).collect()
}

/// The `data` chunk of a 16-bit PCM WAV file.
fn pcm_data(wav: &[u8]) -> &[u8] {
    let mut at = 12; // "RIFF", size, "WAVE"
    while at + 8 <= wav.len() {
        let id = &wav[at..at + 4];
        let size = u32::from_le_bytes([wav[at + 4], wav[at + 5], wav[at + 6], wav[at + 7]]) as usize;
        let body = at + 8;
        if id == b"data" {
            return &wav[body..(body + size).min(wav.len())];
        }
        at = body + size + (size & 1);
    }
    &[]
}

/// What came back from one run of the clip.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleRun {
    pub text: String,
    /// From the first audio sent to the first words received.
    pub first_words: Option<Duration>,
    pub took: Duration,
}

/// Stream the clip through `engine` and collect the text. With `realtime`, each 100 ms piece is
/// sent when it would have been spoken, so the run shows how quickly words come back during a
/// meeting; without it the clip is sent as fast as the engine takes it. `on_text` gets the whole
/// text heard so far each time it changes. Blocking: call it from a blocking thread.
pub fn stream_sample(engine: &dyn Transcriber, realtime: bool, on_text: &mut dyn FnMut(&str)) -> SttResult<SampleRun> {
    let audio = sample_audio();
    let mut stream = engine.open_stream(crate::StreamOptions { channel: None, language: None })?;
    let started = Instant::now();
    let mut heard = Heard::default();
    for (i, piece) in audio.chunks(STEP).enumerate() {
        if realtime {
            let due = started + Duration::from_millis(i as u64 * 100);
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
        }
        let events = stream.push((i * STEP) as u64, piece)?;
        heard.take(events, started, on_text);
    }
    let events = stream.finish()?;
    heard.take(events, started, on_text);
    let text = heard.text();
    if text.is_empty() {
        return Err(SttError::Provider(format!(
            "{} sent nothing back for the sample. Check your account with the service, then try again.",
            crate::cloud::display_name(engine.engine())
        )));
    }
    Ok(SampleRun { text, first_words: heard.first_words, took: started.elapsed() })
}

/// The utterances heard so far, by id.
#[derive(Default)]
struct Heard {
    utterances: BTreeMap<u64, String>,
    first_words: Option<Duration>,
}

impl Heard {
    fn take(&mut self, events: Vec<SttEvent>, started: Instant, on_text: &mut dyn FnMut(&str)) {
        if events.is_empty() {
            return;
        }
        let before = self.text();
        for event in events {
            let (SttEvent::Partial(u) | SttEvent::Final(u)) = event;
            if u.text.trim().is_empty() {
                self.utterances.remove(&u.id);
            } else {
                self.utterances.insert(u.id, u.text.trim().to_string());
            }
        }
        let now = self.text();
        if now != before {
            if !now.is_empty() && self.first_words.is_none() {
                self.first_words = Some(started.elapsed());
            }
            on_text(&now);
        }
    }

    fn text(&self) -> String {
        self.utterances.values().map(String::as_str).collect::<Vec<_>>().join(" ")
    }
}

#[cfg(test)]
mod tests {
    use cilica_domain::SttEngine;

    use super::*;
    use crate::{StreamOptions, TranscriptStream, Utterance};

    #[test]
    fn the_clip_is_eleven_seconds_of_speech() {
        let audio = sample_audio();
        assert_eq!(audio.len(), 11 * SAMPLE_RATE as usize);
        let peak = audio.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.1, "the clip should not be silent");
    }

    /// Says one word per second of audio as a partial, and the whole sentence at the end.
    struct Counting {
        reply: &'static str,
    }

    struct CountingStream {
        reply: &'static str,
        samples: u64,
    }

    impl Transcriber for Counting {
        fn engine(&self) -> SttEngine {
            SttEngine::Soniox
        }
        fn open_stream(&self, _: StreamOptions) -> SttResult<Box<dyn TranscriptStream>> {
            Ok(Box::new(CountingStream { reply: self.reply, samples: 0 }))
        }
    }

    impl TranscriptStream for CountingStream {
        fn push(&mut self, _start: u64, samples: &[f32]) -> SttResult<Vec<SttEvent>> {
            self.samples += samples.len() as u64;
            let words = (self.samples / SAMPLE_RATE as u64) as usize;
            if words == 0 || self.reply.is_empty() {
                return Ok(Vec::new());
            }
            let text: Vec<&str> = self.reply.split(' ').take(words).collect();
            Ok(vec![SttEvent::Partial(Utterance { id: 1, start_ms: 0, end_ms: 0, text: text.join(" "), speaker: None })])
        }
        fn finish(&mut self) -> SttResult<Vec<SttEvent>> {
            Ok(vec![SttEvent::Final(Utterance { id: 1, start_ms: 0, end_ms: 0, text: self.reply.into(), speaker: None })])
        }
    }

    #[test]
    fn collects_the_text_and_reports_progress() {
        let engine = Counting { reply: SAMPLE_TEXT };
        let mut seen = Vec::new();
        let run = stream_sample(&engine, false, &mut |t| seen.push(t.to_string())).unwrap();
        assert_eq!(run.text, SAMPLE_TEXT);
        assert!(run.first_words.is_some());
        assert_eq!(seen.first().map(String::as_str), Some("And"));
        assert_eq!(seen.last().map(String::as_str), Some(SAMPLE_TEXT));
        assert!(seen.len() > 5, "partials arrive as the clip plays: {seen:?}");
    }

    #[test]
    fn nothing_back_is_an_error() {
        let engine = Counting { reply: "" };
        let err = stream_sample(&engine, false, &mut |_| {}).unwrap_err();
        assert!(matches!(err, SttError::Provider(ref m) if m.contains("Soniox sent nothing back")), "{err:?}");
    }
}
