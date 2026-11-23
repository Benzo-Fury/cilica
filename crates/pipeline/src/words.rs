//! Voiceprints of words someone moved to who actually said them, after the meeting (see
//! `cilica_store`'s `words` module). The audio of the moved lines is read back from the kept
//! recording and voiceprinted with the same voice model that recognises Saved people.

use cilica_audio::{AudioSpan, SAMPLE_RATE, read_spans_16k};
use cilica_domain::{AudioChannel, Id, RecordingPhase, Segment};

use crate::RecordingService;

/// The least speech worth learning a voice from: shorter stretches say more about the words than
/// about the voice.
pub const MIN_WORDS_SPEECH_MS: u32 = 2_000;

impl RecordingService {
    /// The note is being recorded or its recording is still being processed, so its lines may
    /// still change under the person.
    pub fn is_working_on(&self, note_id: Id) -> bool {
        let state = self.state();
        state.note_id == Some(note_id) && state.phase != RecordingPhase::Idle
    }

    /// A voiceprint of `lines` from the note's kept recording, and how much speech it holds.
    /// `None` when there is no recording any more, too little speech, or no voice model.
    pub async fn voiceprint_of_lines(&self, note_id: Id, lines: &[Segment]) -> Option<(Vec<f32>, u32)> {
        let speech_ms: u32 = lines.iter().map(|s| s.end_ms.saturating_sub(s.start_ms)).sum();
        if speech_ms < MIN_WORDS_SPEECH_MS {
            return None;
        }
        let (rel, _) = self.inner.db.get_note_audio(note_id).await.ok()?;
        let path = self.inner.config.data_dir.join(rel?);
        if !path.exists() {
            return None;
        }
        let mut spans: Vec<AudioSpan> = lines
            .iter()
            .map(|s| AudioSpan { channel: if s.channel == AudioChannel::Mic { 0 } else { 1 }, from_ms: s.start_ms, to_ms: s.end_ms })
            .collect();
        spans.sort_by_key(|s| s.from_ms);
        let clip = match tokio::task::spawn_blocking(move || read_spans_16k(&path, &spans)).await {
            Ok(Ok(clip)) => clip,
            Ok(Err(error)) => {
                tracing::warn!(%error, "could not read the audio of moved words");
                return None;
            }
            Err(_) => return None,
        };
        let heard_ms = (clip.len() * 1000 / SAMPLE_RATE as usize) as u32;
        if heard_ms < MIN_WORDS_SPEECH_MS {
            return None;
        }
        let (_, model) = self.inner.speech.speaker_models().await?;
        let speech = self.inner.speech.clone();
        match tokio::task::spawn_blocking(move || speech.voice_embedder(&model).and_then(|mut e| e.embed(&clip).map_err(Into::into))).await {
            Ok(Ok(embedding)) => Some((embedding, heard_ms)),
            Ok(Err(error)) => {
                tracing::warn!(message = %error.message, "could not voiceprint moved words");
                None
            }
            Err(_) => None,
        }
    }
}
