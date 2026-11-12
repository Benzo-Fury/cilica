//! Local Whisper (whisper.cpp through whisper-rs; Metal on macOS). Audio never leaves the machine.
//!
//! Live strategy: Silero VAD cuts each channel into utterances. While an utterance is open, the
//! audio so far is decoded about once a second with a shortened encoder window (fast, a little
//! less accurate) and sent as a partial; when the speaker pauses (700 ms) or 20 s pass, the
//! utterance is decoded again and sent as the final. Each decode is primed with the stream's
//! recent text, so names and the thread of the conversation carry across utterances. Both
//! channels share the loaded model but each has its own decoder state, so one never waits for
//! the other's state.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use margin_domain::SttEngine;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

use crate::guards;
use crate::vad::{EnergyVad, FRAME, Segmenter, SegmenterConfig, SileroVad, Vad};
use crate::{SAMPLE_RATE, StreamOptions, SttError, SttEvent, SttResult, Transcriber, TranscriptStream, Utterance, samples_to_ms};

/// Partial results at most this often (in audio).
const PARTIAL_EVERY: usize = SAMPLE_RATE as usize;
/// No partials for utterances shorter than this.
const PARTIAL_MIN: usize = SAMPLE_RATE as usize * 3 / 4;
/// Recent final text kept to prime the next decode (characters).
const CONTEXT_CHARS: usize = 200;

#[derive(Debug, Clone)]
pub struct WhisperOptions {
    pub model_path: PathBuf,
    /// Silero VAD model; `None` falls back to the energy detector.
    pub vad_model_path: Option<PathBuf>,
    pub threads: usize,
    pub use_gpu: bool,
    /// Decode finals with the full 30 s encoder window (slower, more accurate) instead of one
    /// sized to the utterance.
    pub final_full_window: bool,
    /// How speech is cut into utterances.
    pub segmenter: SegmenterConfig,
}

impl WhisperOptions {
    pub fn new(model_path: PathBuf, vad_model_path: Option<PathBuf>) -> Self {
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
        Self {
            model_path,
            vad_model_path,
            threads,
            use_gpu: true,
            final_full_window: false,
            segmenter: SegmenterConfig::default(),
        }
    }
}

struct Engine {
    ctx: WhisperContext,
    threads: usize,
    multilingual: bool,
    final_full_window: bool,
}

/// How carefully to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// Text so far of an utterance in progress.
    Partial,
    /// A final, with a shortened encoder window.
    Fast,
    /// A final that looked wrong after the fast pass: full window, temperature fallback.
    Full,
}

/// What one decode produced.
struct Decoded {
    text: String,
    no_speech: f32,
    language: Option<String>,
}

impl Engine {
    fn new_state(&self) -> SttResult<WhisperState> {
        self.ctx.create_state().map_err(|e| SttError::ModelLoad(e.to_string()))
    }

    /// Decode one clip. `prompt` is recent text from the same stream (may be empty).
    fn decode(&self, state: &mut WhisperState, audio: &[f32], language: Option<&str>, pass: Pass, prompt: &str) -> SttResult<Decoded> {
        let pass = if pass == Pass::Fast && self.final_full_window { Pass::Full } else { pass };
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.threads as i32);
        params.set_translate(false);
        params.set_no_context(true);
        if !prompt.is_empty() {
            params.set_initial_prompt(prompt);
        }
        params.set_single_segment(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_no_speech_thold(0.6);
        params.set_logprob_thold(-1.0);
        params.set_entropy_thold(2.4);
        match pass {
            // Encode only as much context as the audio needs, plus a margin, instead of the full
            // 30 s window: the encoder is most of the cost (large-v3-turbo on an M4: 2 s for the
            // full window, 0.3–0.4 s for a 4 s utterance), and the live budget is under 3 s.
            // A shortened window can make the decoder "hear" the utterance again in the
            // padding: segments past the end are dropped below, a whole repeat inside one
            // segment is collapsed by the guards, and a final with more words than fit the
            // time is decoded again with the full window. No temperature fallback here:
            // the repeats it reacts to come from the window, not from sampling.
            Pass::Partial | Pass::Fast => {
                params.set_audio_ctx(audio_ctx(audio.len()));
                params.set_temperature(0.0);
                params.set_temperature_inc(0.0);
            }
            // The careful pass: full window, at most three temperatures (0.0, 0.4, 0.8).
            Pass::Full => {
                params.set_temperature(0.0);
                params.set_temperature_inc(0.4);
            }
        }
        let lang = match language {
            Some(l) => Some(l),
            None if self.multilingual => Some("auto"),
            None => Some("en"),
        };
        params.set_language(lang);
        // Whisper needs at least one second; pad short clips with silence.
        let mut padded;
        let input = if audio.len() < SAMPLE_RATE as usize + SAMPLE_RATE as usize / 10 {
            padded = audio.to_vec();
            padded.resize(SAMPLE_RATE as usize + SAMPLE_RATE as usize / 10, 0.0);
            &padded[..]
        } else {
            audio
        };

        state.full(params, input).map_err(|e| SttError::Inference(e.to_string()))?;
        let mut text = String::new();
        let mut no_speech: f32 = 0.0;
        let n = state.full_n_segments();
        // Timestamps are in 10 ms units.
        let audio_cs = (input.len() as u64 * 100 / SAMPLE_RATE as u64) as i64;
        let mut kept = 0;
        for i in 0..n {
            if let Some(seg) = state.get_segment(i) {
                if i > 0 && phantom_segment(seg.start_timestamp(), seg.end_timestamp(), audio_cs) {
                    tracing::debug!(t0 = seg.start_timestamp(), t1 = seg.end_timestamp(), audio_cs, "dropped a segment past the end of the audio");
                    continue;
                }
                kept += 1;
                no_speech = no_speech.max(seg.no_speech_probability());
                if let Ok(t) = seg.to_str_lossy() {
                    text.push_str(&t);
                }
            }
        }
        let n = kept;
        let language = if language.is_none() && self.multilingual {
            whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(str::to_owned)
        } else {
            None
        };
        // Silence leaves no segments at all: treat as certainly no speech.
        if n == 0 {
            no_speech = 1.0;
        }
        Ok(Decoded { text, no_speech, language })
    }
}

/// Encoder frames (50 per second, 1500 = the full 30 s window) for `samples` of audio plus a
/// 64-frame margin, rounded up to a multiple of 64. Long clips use the full window.
fn audio_ctx(samples: usize) -> i32 {
    let frames = (samples as f32 / SAMPLE_RATE as f32 * 50.0).ceil() as i32 + 64;
    let rounded = (frames + 63) / 64 * 64;
    if rounded >= 1200 { 1500 } else { rounded.max(256) }
}

/// A later segment that starts at the very end of the audio or runs well past it describes
/// audio that is not there: with a shortened encoder window Whisper tends to repeat the whole
/// utterance into the padding ("[0-3.4 s] Hello. [3.4-19 s] Hello.").
fn phantom_segment(t0_cs: i64, t1_cs: i64, audio_cs: i64) -> bool {
    t0_cs >= audio_cs - 20 || t1_cs > audio_cs + 100
}

/// The local Whisper engine. Loading takes a second or two; keep one per recording.
pub struct WhisperLocal {
    engine: Arc<Engine>,
    vad_model_path: Option<PathBuf>,
    segmenter: SegmenterConfig,
}

impl WhisperLocal {
    pub fn load(options: WhisperOptions) -> SttResult<Self> {
        if !options.model_path.exists() {
            return Err(SttError::ModelMissing(
                options.model_path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(),
            ));
        }
        whisper_rs::install_logging_hooks();
        let mut cparams = WhisperContextParameters::default();
        cparams.use_gpu(options.use_gpu);
        // Flash attention garbles output when the encoder context is shortened (measured with
        // large-v3-turbo on Metal), and we always shorten it. Leave it off.
        cparams.flash_attn(false);
        let path = options.model_path.to_string_lossy().into_owned();
        let ctx = WhisperContext::new_with_params(&path, cparams).map_err(|e| SttError::ModelLoad(e.to_string()))?;
        let multilingual = ctx.is_multilingual();
        Ok(Self {
            engine: Arc::new(Engine {
                ctx,
                threads: options.threads,
                multilingual,
                final_full_window: options.final_full_window,
            }),
            vad_model_path: options.vad_model_path,
            segmenter: options.segmenter,
        })
    }

    fn make_vad(&self) -> Box<dyn Vad> {
        if let Some(path) = self.vad_model_path.as_ref() {
            match SileroVad::load(path) {
                Ok(v) => return Box::new(v),
                Err(error) => tracing::warn!(%error, "Silero VAD unavailable; using the energy detector"),
            }
        }
        Box::new(EnergyVad::default())
    }

    /// The careful pass over a whole recording channel: beam search, the full 30 s window,
    /// context carried from window to window, temperature fallback. Segments that fall where
    /// voice activity detection heard no speech are dropped (Whisper invents text for silence).
    fn careful(&self, samples: &[f32], language: Option<&str>) -> SttResult<Vec<Utterance>> {
        if samples.len() < SAMPLE_RATE as usize / 2 {
            return Ok(Vec::new());
        }
        let mut state = self.engine.new_state()?;
        let mut params = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 });
        params.set_n_threads(self.engine.threads as i32);
        params.set_translate(false);
        params.set_no_context(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_no_speech_thold(0.6);
        params.set_logprob_thold(-1.0);
        params.set_entropy_thold(2.4);
        params.set_temperature(0.0);
        params.set_temperature_inc(0.2);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        let lang = language.filter(|l| !l.is_empty() && *l != "auto");
        params.set_language(Some(match lang {
            Some(l) => l,
            None if self.engine.multilingual => "auto",
            None => "en",
        }));
        state.full(params, samples).map_err(|e| SttError::Inference(e.to_string()))?;

        let speech = speech_frames(self.make_vad(), samples);
        let mut out = Vec::new();
        for i in 0..state.full_n_segments() {
            let Some(seg) = state.get_segment(i) else { continue };
            let (t0, t1) = (seg.start_timestamp().max(0) as u64, seg.end_timestamp().max(0) as u64);
            if t1 <= t0 || speech_share(&speech, t0, t1) < MIN_SPEECH_SHARE {
                continue;
            }
            let Ok(text) = seg.to_str_lossy() else { continue };
            let secs = (t1 - t0) as f32 / 100.0;
            if let Some(text) = guards::clean(&text, seg.no_speech_probability(), secs) {
                out.push(Utterance { id: out.len() as u64 + 1, start_ms: (t0 * 10) as u32, end_ms: (t1 * 10) as u32, text, speaker: None });
            }
        }
        Ok(out)
    }
}

/// A careful-pass segment needs at least this share of its time to be speech.
const MIN_SPEECH_SHARE: f32 = 0.2;

/// Speech or not for each VAD frame of `samples`.
fn speech_frames(mut vad: Box<dyn Vad>, samples: &[f32]) -> Vec<bool> {
    samples.chunks_exact(FRAME).map(|frame| vad.prob(frame) >= 0.5).collect()
}

/// Share of speech frames between two Whisper timestamps (10 ms units).
fn speech_share(speech: &[bool], t0_cs: u64, t1_cs: u64) -> f32 {
    let frame_cs = FRAME as f64 * 100.0 / SAMPLE_RATE as f64;
    let a = ((t0_cs as f64 / frame_cs) as usize).min(speech.len());
    let b = ((t1_cs as f64 / frame_cs).ceil() as usize).clamp(a, speech.len());
    if b == a {
        return 0.0;
    }
    speech[a..b].iter().filter(|s| **s).count() as f32 / (b - a) as f32
}

impl Transcriber for WhisperLocal {
    fn engine(&self) -> SttEngine {
        SttEngine::WhisperLocal
    }

    fn transcribe_recording(&self, samples: &[f32], language: Option<&str>) -> SttResult<Vec<Utterance>> {
        self.careful(samples, language)
    }

    fn open_stream(&self, options: StreamOptions) -> SttResult<Box<dyn TranscriptStream>> {
        Ok(Box::new(WhisperStream {
            state: self.engine.new_state()?,
            engine: self.engine.clone(),
            context: String::new(),
            segmenter: Segmenter::new(self.make_vad(), self.segmenter),
            language: options.language.filter(|l| !l.is_empty() && l != "auto"),
            language_locked: false,
            next_id: 1,
            current_id: None,
            last_partial_len: 0,
            partial_sent: false,
        }))
    }
}

struct WhisperStream {
    engine: Arc<Engine>,
    /// This stream's own decoder state (the model weights are shared).
    state: WhisperState,
    /// Recent final text, to prime the next decode.
    context: String,
    segmenter: Segmenter,
    language: Option<String>,
    language_locked: bool,
    next_id: u64,
    current_id: Option<u64>,
    last_partial_len: usize,
    partial_sent: bool,
}

impl WhisperStream {
    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn decode(&mut self, audio: &[f32], pass: Pass) -> SttResult<Decoded> {
        self.engine.decode(&mut self.state, audio, self.language.as_deref(), pass, &self.context)
    }

    /// Remember a final's text as context, keeping the last [`CONTEXT_CHARS`] at a word boundary.
    fn remember(&mut self, text: &str) {
        if !self.context.is_empty() {
            self.context.push(' ');
        }
        self.context.push_str(text);
        let n = self.context.chars().count();
        if n > CONTEXT_CHARS {
            let skip = self.context.char_indices().nth(n - CONTEXT_CHARS).map(|(i, _)| i).unwrap_or(0);
            let tail = &self.context[skip..];
            let cut = tail.find(' ').map(|i| i + 1).unwrap_or(0);
            self.context = tail[cut..].to_string();
        }
    }

    fn finalize(&mut self, chunk: crate::vad::Chunk, out: &mut Vec<SttEvent>) -> SttResult<()> {
        let id = match self.current_id.take() {
            Some(id) => id,
            None => self.take_id(),
        };
        let had_partial = std::mem::replace(&mut self.partial_sent, false);
        self.last_partial_len = 0;
        let start_ms = samples_to_ms(chunk.start_sample);
        let end_ms = samples_to_ms(chunk.start_sample + chunk.samples.len() as u64);
        let secs = chunk.samples.len() as f32 / SAMPLE_RATE as f32;
        let text = if guards::has_enough_speech(&chunk.samples, chunk.speech_samples) {
            let started = Instant::now();
            let mut d = self.decode(&chunk.samples, Pass::Fast)?;
            if guards::implausible(&d.text, secs) {
                tracing::debug!(text = %d.text, "fast decode went wrong; decoding again with the full window");
                d = self.decode(&chunk.samples, Pass::Full)?;
            }
            tracing::debug!(secs, took = ?started.elapsed(), "final decode");
            if !self.language_locked
                && self.language.is_none()
                && secs >= 2.0
                && d.no_speech < 0.5
                && let Some(l) = d.language.clone()
            {
                tracing::info!(language = %l, "language detected; locking it for this stream");
                self.language = Some(l);
                self.language_locked = true;
            }
            guards::clean(&d.text, d.no_speech, secs)
        } else {
            None
        };
        match text {
            Some(text) => {
                self.remember(&text);
                out.push(SttEvent::Final(Utterance { id, start_ms, end_ms, text, speaker: None }))
            }
            // Retract a partial that turned out to be nothing.
            None if had_partial => out.push(SttEvent::Final(Utterance { id, start_ms, end_ms, text: String::new(), speaker: None })),
            None => {}
        }
        Ok(())
    }
}

impl TranscriptStream for WhisperStream {
    fn push(&mut self, start_sample: u64, samples: &[f32]) -> SttResult<Vec<SttEvent>> {
        let mut out = Vec::new();
        for chunk in self.segmenter.push(start_sample, samples) {
            self.finalize(chunk, &mut out)?;
        }
        // Partial for the utterance still open.
        let partial = match self.segmenter.current() {
            Some((start, audio, speech))
                if audio.len() >= PARTIAL_MIN
                    && audio.len() >= self.last_partial_len + PARTIAL_EVERY
                    && guards::has_enough_speech(audio, speech) =>
            {
                Some((start, audio.to_vec()))
            }
            _ => None,
        };
        if self.segmenter.current().is_some() && self.current_id.is_none() {
            self.current_id = Some(self.take_id());
        }
        if let Some((start, audio)) = partial {
            self.last_partial_len = audio.len();
            let d = self.decode(&audio, Pass::Partial)?;
            let secs = audio.len() as f32 / SAMPLE_RATE as f32;
            if let (Some(text), Some(id)) = (guards::clean(&d.text, d.no_speech, secs.max(3.0)), self.current_id) {
                self.partial_sent = true;
                out.push(SttEvent::Partial(Utterance {
                    id,
                    start_ms: samples_to_ms(start),
                    end_ms: samples_to_ms(start + audio.len() as u64),
                    text,
                    speaker: None,
                }));
            }
        }
        Ok(out)
    }

    fn finish(&mut self) -> SttResult<Vec<SttEvent>> {
        let mut out = Vec::new();
        if let Some(chunk) = self.segmenter.flush() {
            self.finalize(chunk, &mut out)?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_share_counts_vad_frames() {
        // 32 ms frames: frames 0-9 silent, 10-19 speech.
        let speech: Vec<bool> = (0..20).map(|i| i >= 10).collect();
        assert_eq!(speech_share(&speech, 0, 32), 0.0);
        assert!(speech_share(&speech, 32, 64) > 0.99);
        let half = speech_share(&speech, 16, 48);
        assert!((0.4..=0.6).contains(&half), "{half}");
        assert_eq!(speech_share(&speech, 100, 100), 0.0);
    }

    #[test]
    fn encoder_window_and_phantoms() {
        assert_eq!(audio_ctx(16_000 * 4), 320);
        assert_eq!(audio_ctx(8_000), 256);
        assert_eq!(audio_ctx(16_000 * 25), 1500);
        // Measured: a 3.72 s utterance followed by its repeat at 3.44–19.36 s.
        assert!(!phantom_segment(0, 344, 372));
        assert!(phantom_segment(344, 1936, 372));
        assert!(phantom_segment(370, 372, 372));
        assert!(!phantom_segment(200, 380, 372));
    }
}
