//! Voice activity detection and utterance chunking.
//!
//! [`SileroVad`] runs Silero VAD v5 (MIT, ONNX) through ONNX Runtime one 32 ms frame at a time,
//! keeping its recurrent state, so it suits live audio. [`EnergyVad`] is a fallback that needs
//! no model. [`Segmenter`] turns frame probabilities into utterances with pre-roll, hysteresis,
//! an end-of-speech pause and a hard length cap.

use std::collections::VecDeque;
use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

use crate::{SAMPLE_RATE, SttError, SttResult};

/// Silero's frame at 16 kHz.
pub const FRAME: usize = 512;
const CONTEXT: usize = 64;

/// Speech probability for consecutive 512-sample frames.
pub trait Vad: Send {
    fn prob(&mut self, frame: &[f32]) -> f32;
    fn reset(&mut self);
}

pub(crate) fn ort_env() -> SttResult<&'static ort::environment::Environment> {
    // Shared with speaker labelling: ONNX Runtime allows one environment per process.
    cilica_onnx::env().map_err(|e| SttError::ModelLoad(format!("ONNX Runtime: {e}")))
}

/// Silero VAD v5.
pub struct SileroVad {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
    input: Vec<f32>,
}

impl SileroVad {
    pub fn load(path: &Path) -> SttResult<Self> {
        let session = Session::builder(ort_env()?)
            .and_then(|b| b.with_intra_threads(1).map_err(Into::into))
            .and_then(|mut b| b.commit_from_file(path))
            .map_err(|e| SttError::ModelLoad(format!("voice activity model: {e}")))?;
        Ok(Self { session, state: vec![0.0; 2 * 128], context: vec![0.0; CONTEXT], input: vec![0.0; CONTEXT + FRAME] })
    }

    fn run(&mut self, frame: &[f32]) -> Result<f32, ort::Error> {
        self.input[..CONTEXT].copy_from_slice(&self.context);
        self.input[CONTEXT..].copy_from_slice(frame);
        let input = Tensor::from_array(([1usize, CONTEXT + FRAME], self.input.clone()))?;
        let state = Tensor::from_array(([2usize, 1, 128], self.state.clone()))?;
        let sr = Tensor::from_array(((), vec![SAMPLE_RATE as i64]))?;
        let outputs = self.session.run(ort::inputs![input, state, sr])?;
        let prob = outputs[0].try_extract_tensor::<f32>()?.1.first().copied().unwrap_or(0.0);
        let new_state = outputs[1].try_extract_tensor::<f32>()?.1;
        if new_state.len() == self.state.len() {
            self.state.copy_from_slice(new_state);
        }
        self.context.copy_from_slice(&frame[FRAME - CONTEXT..]);
        Ok(prob)
    }
}

impl Vad for SileroVad {
    fn prob(&mut self, frame: &[f32]) -> f32 {
        debug_assert_eq!(frame.len(), FRAME);
        match self.run(frame) {
            Ok(p) => p,
            Err(error) => {
                tracing::warn!(%error, "voice activity detection failed for a frame");
                0.0
            }
        }
    }
    fn reset(&mut self) {
        self.state.fill(0.0);
        self.context.fill(0.0);
    }
}

/// Level-based fallback: speech is energy well above a tracked noise floor.
pub struct EnergyVad {
    floor_db: f32,
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self { floor_db: -60.0 }
    }
}

impl Vad for EnergyVad {
    fn prob(&mut self, frame: &[f32]) -> f32 {
        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len().max(1) as f32).sqrt();
        let db = 20.0 * rms.max(1e-6).log10();
        // Floor follows quiet frames quickly and loud ones very slowly.
        let rate = if db < self.floor_db { 0.2 } else { 0.002 };
        self.floor_db += (db - self.floor_db) * rate;
        let snr = db - self.floor_db.max(-70.0);
        if db < -50.0 { 0.0 } else { 1.0 / (1.0 + (-(snr - 12.0) / 3.0).exp()) }
    }
    fn reset(&mut self) {
        self.floor_db = -60.0;
    }
}

/// How speech is cut into utterances.
#[derive(Debug, Clone, Copy)]
pub struct SegmenterConfig {
    pub start_threshold: f32,
    pub end_threshold: f32,
    /// Silence that ends an utterance.
    pub min_silence_ms: u32,
    /// Audio kept before the first speech frame.
    pub pre_roll_ms: u32,
    /// Silence kept after the last speech frame.
    pub post_roll_ms: u32,
    /// Past this length a shorter pause ([`Self::soft_silence_ms`]) ends the utterance, so long
    /// turns are cut at a breath instead of running on to the hard cap.
    pub soft_max_ms: u32,
    pub soft_silence_ms: u32,
    /// Utterances are cut at this length even without a pause.
    pub max_utterance_ms: u32,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            start_threshold: 0.5,
            end_threshold: 0.35,
            min_silence_ms: 700,
            pre_roll_ms: 300,
            post_roll_ms: 200,
            soft_max_ms: 6_000,
            soft_silence_ms: 250,
            max_utterance_ms: 12_000,
        }
    }
}

/// A finished utterance.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub start_sample: u64,
    pub samples: Vec<f32>,
    /// Samples judged to be speech (excludes pre/post roll and short pauses).
    pub speech_samples: usize,
}

struct Active {
    start: u64,
    audio: Vec<f32>,
    speech: usize,
    silence_run: usize,
}

/// Streaming utterance chunker.
pub struct Segmenter {
    vad: Box<dyn Vad>,
    cfg: SegmenterConfig,
    frame: Vec<f32>,
    /// Timeline index of the next sample expected.
    next: Option<u64>,
    preroll: VecDeque<f32>,
    active: Option<Active>,
}

fn ms(samples_ms: u32) -> usize {
    (samples_ms as u64 * SAMPLE_RATE as u64 / 1000) as usize
}

impl Segmenter {
    pub fn new(vad: Box<dyn Vad>, cfg: SegmenterConfig) -> Self {
        Self { vad, cfg, frame: Vec::with_capacity(FRAME), next: None, preroll: VecDeque::new(), active: None }
    }

    /// The utterance in progress: (start sample, audio so far, speech samples).
    pub fn current(&self) -> Option<(u64, &[f32], usize)> {
        self.active.as_ref().map(|a| (a.start, a.audio.as_slice(), a.speech))
    }

    /// Feed audio starting at `start`; returns utterances that ended.
    pub fn push(&mut self, start: u64, samples: &[f32]) -> Vec<Chunk> {
        let mut done = Vec::new();
        if let Some(next) = self.next
            && start != next
        {
            // A gap (or overlap) on the timeline: close what is open and start afresh.
            self.frame.clear();
            if let Some(c) = self.close(true) {
                done.push(c);
            }
            self.preroll.clear();
            self.vad.reset();
        }
        self.next = Some(start + samples.len() as u64);
        let mut pos = start;
        for s in samples {
            if self.frame.is_empty() {
                // Frame start index is implied by pos.
            }
            self.frame.push(*s);
            pos += 1;
            if self.frame.len() == FRAME {
                let frame = std::mem::take(&mut self.frame);
                self.on_frame(pos - FRAME as u64, &frame, &mut done);
                self.frame = frame;
                self.frame.clear();
            }
        }
        done
    }

    /// End of stream: close what is open (including a trailing partial frame).
    pub fn flush(&mut self) -> Option<Chunk> {
        if let Some(a) = self.active.as_mut() {
            a.audio.extend_from_slice(&self.frame);
        }
        self.frame.clear();
        self.next = None;
        self.close(true)
    }

    fn on_frame(&mut self, frame_start: u64, frame: &[f32], done: &mut Vec<Chunk>) {
        let p = self.vad.prob(frame);
        match self.active.as_mut() {
            None => {
                if p >= self.cfg.start_threshold {
                    let pre: Vec<f32> = self.preroll.drain(..).collect();
                    let start = frame_start - pre.len() as u64;
                    let mut audio = pre;
                    audio.extend_from_slice(frame);
                    self.active = Some(Active { start, audio, speech: FRAME, silence_run: 0 });
                } else {
                    self.preroll.extend(frame.iter().copied());
                    let keep = ms(self.cfg.pre_roll_ms);
                    while self.preroll.len() > keep {
                        self.preroll.pop_front();
                    }
                }
            }
            Some(a) => {
                a.audio.extend_from_slice(frame);
                if p >= self.cfg.end_threshold {
                    a.speech += FRAME + a.silence_run.min(ms(300));
                    a.silence_run = 0;
                } else {
                    a.silence_run += FRAME;
                }
                let pause = if a.audio.len() >= ms(self.cfg.soft_max_ms) { self.cfg.soft_silence_ms } else { self.cfg.min_silence_ms };
                if a.silence_run >= ms(pause) {
                    if let Some(c) = self.close(false) {
                        done.push(c);
                    }
                } else if a.audio.len() >= ms(self.cfg.max_utterance_ms)
                    && let Some(c) = self.close(false)
                {
                    done.push(c);
                }
            }
        }
    }

    fn close(&mut self, _end_of_stream: bool) -> Option<Chunk> {
        let mut a = self.active.take()?;
        let keep_tail = ms(self.cfg.post_roll_ms);
        if a.silence_run > keep_tail {
            let cut = a.silence_run - keep_tail;
            let len = a.audio.len().saturating_sub(cut);
            a.audio.truncate(len);
        }
        Some(Chunk { start_sample: a.start, samples: a.audio, speech_samples: a.speech })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake VAD: speech wherever the frame is loud.
    struct Loud;
    impl Vad for Loud {
        fn prob(&mut self, frame: &[f32]) -> f32 {
            if frame.iter().any(|s| s.abs() > 0.05) { 1.0 } else { 0.0 }
        }
        fn reset(&mut self) {}
    }

    fn tone(secs: f32) -> Vec<f32> {
        (0..(secs * 16_000.0) as usize).map(|i| 0.3 * (i as f32 * 0.1).sin()).collect()
    }

    #[test]
    fn cuts_on_pauses_with_pre_and_post_roll() {
        let mut audio = vec![0.0; 16_000];
        audio.extend(tone(1.0));
        audio.extend(vec![0.0; 16_000]);
        audio.extend(tone(2.0));
        audio.extend(vec![0.0; 16_000]);
        let mut seg = Segmenter::new(Box::new(Loud), SegmenterConfig::default());
        let mut chunks = Vec::new();
        for (i, c) in audio.chunks(1600).enumerate() {
            chunks.extend(seg.push(i as u64 * 1600, c));
        }
        chunks.extend(seg.flush());
        assert_eq!(chunks.len(), 2);
        // First utterance starts ~300 ms before the tone at 1 s, ends ~200 ms after it.
        let s0 = chunks[0].start_sample as i64;
        assert!((s0 - (16_000 - 4_800)).abs() <= 512, "{s0}");
        let len0 = chunks[0].samples.len() as i64;
        assert!((len0 - (4_800 + 16_000 + 3_200)).abs() <= 1024, "{len0}");
        assert!(chunks[1].speech_samples >= 31_000);
    }

    #[test]
    fn long_speech_is_capped_and_gaps_close_utterances() {
        let mut seg = Segmenter::new(Box::new(Loud), SegmenterConfig { max_utterance_ms: 5_000, ..Default::default() });
        let long = tone(12.0);
        let mut chunks = seg.push(0, &long);
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| c.samples.len() <= 5 * 16_000 + 512));
        // A jump on the timeline closes the open utterance.
        chunks = seg.push(1_000_000, &tone(0.5));
        assert_eq!(chunks.len(), 1);
        assert!(seg.current().is_some());
    }

    #[test]
    fn long_turns_are_cut_at_a_short_pause() {
        // 7 s of speech, a 300 ms breath, 3 s more: one cut at the breath (past the soft cap),
        // where a 300 ms pause early in an utterance would not cut.
        let mut audio = tone(1.0);
        audio.extend(vec![0.0; 4_800]);
        audio.extend(tone(6.0));
        audio.extend(vec![0.0; 4_800]);
        audio.extend(tone(3.0));
        let mut seg = Segmenter::new(Box::new(Loud), SegmenterConfig::default());
        let mut chunks = seg.push(0, &audio);
        chunks.extend(seg.flush());
        assert_eq!(chunks.len(), 2, "{:?}", chunks.iter().map(|c| c.samples.len()).collect::<Vec<_>>());
        assert!(chunks[0].samples.len() > 16_000 * 7);
    }

    #[test]
    fn energy_vad_separates_speech_from_silence() {
        let mut v = EnergyVad::default();
        let quiet = vec![0.0005; FRAME];
        for _ in 0..50 {
            v.prob(&quiet);
        }
        assert!(v.prob(&quiet) < 0.2);
        let loud: Vec<f32> = (0..FRAME).map(|i| 0.2 * (i as f32 * 0.2).sin()).collect();
        assert!(v.prob(&loud) > 0.8);
    }
}
