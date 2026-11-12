//! Speech-to-text for Margin.
//!
//! - [`Transcriber`] is the one engine trait. [`WhisperLocal`] (whisper.cpp through whisper-rs,
//!   Metal on macOS) is the default and keeps audio on the machine; cloud engines plug in behind
//!   the same trait ([`cloud`]) and currently report `Unavailable`.
//! - A [`TranscriptStream`] is fed one channel's 16 kHz audio as it is captured; Silero VAD
//!   ([`vad`]) cuts it into utterances, partial results arrive about every second while someone
//!   speaks, and a final result when they pause.
//! - [`models`] is the model catalogue and the resumable, verified downloader.

pub mod cloud;
mod error;
mod guards;
pub mod models;
pub mod vad;
mod whisper_local;

pub use error::{SttError, SttResult};
pub use models::{ModelFile, ModelManager, ModelPaths};
pub use whisper_local::{WhisperLocal, WhisperOptions};

use margin_domain::{AudioChannel, SttEngine};

/// Audio enters every engine at this rate, mono.
pub const SAMPLE_RATE: u32 = 16_000;

/// One recognised utterance on the recording timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    /// Stable within a stream: a partial and the final that replaces it share it.
    pub id: u64,
    pub start_ms: u32,
    pub end_ms: u32,
    /// Empty on a final means "nothing was said here" (drop the partial).
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SttEvent {
    /// Text so far for an utterance still in progress; replaced by later events with the same id.
    Partial(Utterance),
    Final(Utterance),
}

/// Per-stream settings.
#[derive(Debug, Clone, Default)]
pub struct StreamOptions {
    pub channel: Option<AudioChannel>,
    /// ISO 639-1, or `None` to detect (then locked after the first confident utterance).
    pub language: Option<String>,
}

/// A speech-to-text engine.
pub trait Transcriber: Send + Sync {
    fn engine(&self) -> SttEngine;
    /// Start a live stream for one channel.
    fn open_stream(&self, options: StreamOptions) -> SttResult<Box<dyn TranscriptStream>>;
    /// Transcribe a whole clip at once (16 kHz mono). Blocking.
    fn transcribe(&self, samples: &[f32], language: Option<&str>) -> SttResult<Vec<Utterance>> {
        let mut stream = self.open_stream(StreamOptions { channel: None, language: language.map(str::to_owned) })?;
        let mut events = stream.push(0, samples)?;
        events.extend(stream.finish()?);
        Ok(events
            .into_iter()
            .filter_map(|e| match e {
                SttEvent::Final(u) if !u.text.is_empty() => Some(u),
                _ => None,
            })
            .collect())
    }
}

/// One channel's live transcription. Calls may run the model and block for a while, so drive a
/// stream from its own thread (never an async runtime's core thread).
pub trait TranscriptStream: Send {
    /// Feed audio that starts at `start_sample` on the recording timeline (gaps are allowed).
    fn push(&mut self, start_sample: u64, samples: &[f32]) -> SttResult<Vec<SttEvent>>;
    /// The recording ended: finalise whatever is in progress.
    fn finish(&mut self) -> SttResult<Vec<SttEvent>>;
}

pub(crate) fn samples_to_ms(samples: u64) -> u32 {
    (samples * 1000 / SAMPLE_RATE as u64).min(u32::MAX as u64) as u32
}
