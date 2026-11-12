//! Cloud engines (Deepgram, Soniox, AssemblyAI) plug in here, behind [`Transcriber`].
//!
//! Each will stream 16 kHz PCM over the provider's WebSocket with the person's own key (read
//! from the OS keychain by the shell and passed in, never stored here), map interim results to
//! [`SttEvent::Partial`](crate::SttEvent) and finals to `Final`. Until then they report
//! `Unavailable` so the app falls back to Local Whisper with a clear message.

use margin_domain::SttEngine;

use crate::{StreamOptions, SttError, SttResult, Transcriber, TranscriptStream};

/// A cloud engine that is not implemented yet.
pub struct CloudTranscriber {
    engine: SttEngine,
}

impl CloudTranscriber {
    pub fn new(engine: SttEngine) -> Self {
        Self { engine }
    }

    fn name(&self) -> &'static str {
        match self.engine {
            SttEngine::Deepgram => "Deepgram",
            SttEngine::Soniox => "Soniox",
            SttEngine::AssemblyAi => "AssemblyAI",
            SttEngine::WhisperLocal => "Local Whisper",
        }
    }
}

impl Transcriber for CloudTranscriber {
    fn engine(&self) -> SttEngine {
        self.engine
    }

    fn open_stream(&self, _options: StreamOptions) -> SttResult<Box<dyn TranscriptStream>> {
        Err(SttError::EngineUnavailable(format!("{} transcription", self.name())))
    }
}
