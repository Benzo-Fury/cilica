//! Speaker diarization for Margin, run once after a meeting.
//!
//! Pipeline: slide a 10 s window (2.5 s step) over the channel and run pyannote segmentation 3.0
//! to find up to three local speakers per window; embed each local speaker's clean (non-overlapped)
//! speech with 3D-Speaker CAM++; cluster the embeddings (average linkage, cosine distance) into
//! Person 1…n; rebuild who-spoke-when by voting across overlapping windows; then assign each
//! transcript segment to the speaker who talks most during it. Each speaker gets an
//! L2-normalised centroid embedding for voiceprint matching ([`suggest_people`]).
//!
//! Models are loaded from paths (downloaded by `margin-stt`'s model manager); this crate never
//! touches the network. See README.md for sources and licences.

mod cluster;
pub mod embedding;
pub mod fbank;
pub mod segmentation;

use std::path::Path;

use margin_domain::{AppError, ErrorCode, Id};

pub use cluster::agglomerative;
use embedding::Embedder;
use segmentation::{FRAME_STEP, LOCAL_SPEAKERS, Segmenter, WINDOW, frame_center};

pub const SAMPLE_RATE: u32 = 16_000;

/// Cosine similarity at or above which a cluster is suggested as a Saved person. Tuned on CAM++:
/// the same synthetic voice across turns scores 0.7–0.9, different voices below 0.4.
pub const SUGGEST_THRESHOLD: f32 = 0.6;

#[derive(Debug, thiserror::Error)]
pub enum DiarizeError {
    #[error("The speaker models could not be loaded: {0}. They download with the speech model; try downloading it again in Settings › Transcription.")]
    Model(String),
    #[error("Speaker detection failed: {0}")]
    Inference(String),
    #[error("Not enough speech to tell speakers apart.")]
    TooShort,
}

impl From<ort::Error> for DiarizeError {
    fn from(e: ort::Error) -> Self {
        DiarizeError::Inference(e.to_string())
    }
}

pub type DiarizeResult<T> = Result<T, DiarizeError>;

impl From<DiarizeError> for AppError {
    fn from(e: DiarizeError) -> Self {
        let code = match e {
            DiarizeError::Model(_) => ErrorCode::Unavailable,
            _ => ErrorCode::Internal,
        };
        AppError::new(code, e.to_string())
    }
}

pub(crate) fn ort_env() -> DiarizeResult<&'static ort::environment::Environment> {
    // Shared with voice-activity detection: ONNX Runtime allows one environment per process.
    margin_onnx::env().map_err(|e| DiarizeError::Model(format!("ONNX Runtime: {e}")))
}

pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n <= f32::EPSILON { v.to_vec() } else { v.iter().map(|x| x / n).collect() }
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut d, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        d += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= f32::EPSILON || nb <= f32::EPSILON { 0.0 } else { d / (na.sqrt() * nb.sqrt()) }
}

/// Tuning knobs.
#[derive(Debug, Clone, Copy)]
pub struct DiarizeConfig {
    pub step: usize,
    /// Clean speech a local speaker needs in a window to be embedded.
    pub min_embed_secs: f32,
    /// Cosine distance at which clusters stop merging.
    pub cluster_threshold: f32,
    pub max_speakers: usize,
    /// Clusters with less speech than this are folded into their nearest neighbour.
    pub min_speaker_secs: f32,
}

impl Default for DiarizeConfig {
    fn default() -> Self {
        Self { step: WINDOW / 4, min_embed_secs: 0.6, cluster_threshold: 0.6, max_speakers: 8, min_speaker_secs: 2.0 }
    }
}

/// A stretch of one speaker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn {
    pub start_ms: u32,
    pub end_ms: u32,
    pub speaker: usize,
}

/// One speaker found in the channel.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeakerProfile {
    pub index: usize,
    /// L2-normalised centroid embedding.
    pub embedding: Vec<f32>,
    pub speech_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Diarization {
    pub speakers: Vec<SpeakerProfile>,
    pub turns: Vec<Turn>,
}

/// The loaded models. Heavy: build one per diarization job, off the async runtime.
pub struct Diarizer {
    seg: Segmenter,
    emb: Embedder,
    cfg: DiarizeConfig,
}

impl Diarizer {
    pub fn load(segmentation: &Path, embedding: &Path) -> DiarizeResult<Self> {
        Ok(Self { seg: Segmenter::load(segmentation)?, emb: Embedder::load(embedding)?, cfg: DiarizeConfig::default() })
    }

    pub fn with_config(mut self, cfg: DiarizeConfig) -> Self {
        self.cfg = cfg;
        self
    }

    /// An embedding of a clip (for tests and for building voiceprints from a known stretch).
    pub fn embed(&mut self, samples: &[f32]) -> DiarizeResult<Vec<f32>> {
        self.emb.embed(samples)
    }

    /// Who spoke when in 16 kHz mono audio.
    pub fn diarize(&mut self, audio: &[f32]) -> DiarizeResult<Diarization> {
        if audio.len() < SAMPLE_RATE as usize {
            return Ok(Diarization::default());
        }
        let step = self.cfg.step.max(FRAME_STEP);
        let starts: Vec<usize> = {
            let mut v: Vec<usize> = (0..audio.len().saturating_sub(WINDOW).max(1)).step_by(step).collect();
            if audio.len() > WINDOW && v.last().is_some_and(|&l| l + WINDOW < audio.len()) {
                v.push(audio.len() - WINDOW);
            }
            v
        };
        let total_frames = audio.len() / FRAME_STEP + 1;
        let min_clean_frames = (self.cfg.min_embed_secs * SAMPLE_RATE as f32 / FRAME_STEP as f32) as usize;

        // 1. Segment every window; embed each local speaker's clean speech.
        struct Local {
            window: usize,
            local: usize,
            embedding: Vec<f32>,
            weight: f32,
        }
        let mut activities: Vec<Vec<[bool; LOCAL_SPEAKERS]>> = Vec::with_capacity(starts.len());
        let mut locals: Vec<Local> = Vec::new();
        let mut window = vec![0.0f32; WINDOW];
        for (w, &start) in starts.iter().enumerate() {
            let end = (start + WINDOW).min(audio.len());
            window[..end - start].copy_from_slice(&audio[start..end]);
            window[end - start..].fill(0.0);
            let act = self.seg.run(&window)?;
            for s in 0..LOCAL_SPEAKERS {
                let clean: Vec<usize> =
                    (0..act.len()).filter(|&k| act[k][s] && act[k].iter().filter(|a| **a).count() == 1).collect();
                if clean.len() < min_clean_frames {
                    continue;
                }
                let mut clip = Vec::with_capacity(clean.len() * FRAME_STEP);
                for &k in &clean {
                    let c = frame_center(k);
                    let lo = c.saturating_sub(FRAME_STEP / 2);
                    let hi = (lo + FRAME_STEP).min(end - start);
                    if lo < hi {
                        clip.extend_from_slice(&window[lo..hi]);
                    }
                }
                match self.emb.embed(&clip) {
                    Ok(e) => locals.push(Local {
                        window: w,
                        local: s,
                        embedding: e,
                        weight: clip.len() as f32 / SAMPLE_RATE as f32,
                    }),
                    Err(DiarizeError::TooShort) => {}
                    Err(e) => return Err(e),
                }
            }
            activities.push(act);
        }
        if locals.is_empty() {
            return Ok(Diarization::default());
        }

        // 2. Cluster.
        let items: Vec<Vec<f32>> = locals.iter().map(|l| l.embedding.clone()).collect();
        let weights: Vec<f32> = locals.iter().map(|l| l.weight).collect();
        let labels = agglomerative(&items, &weights, self.cfg.cluster_threshold, self.cfg.max_speakers, self.cfg.min_speaker_secs);
        let n_speakers = labels.iter().max().map(|m| m + 1).unwrap_or(0);

        // 3. Rebuild activity on the global frame grid by voting across windows.
        let mut votes = vec![vec![0u16; total_frames]; n_speakers];
        let mut cover = vec![0u16; total_frames];
        let map = |w: usize, s: usize| locals.iter().position(|l| l.window == w && l.local == s).map(|i| labels[i]);
        for (w, act) in activities.iter().enumerate() {
            let start = starts[w];
            let mapping: Vec<Option<usize>> = (0..LOCAL_SPEAKERS).map(|s| map(w, s)).collect();
            for (k, frame) in act.iter().enumerate() {
                let g = (start + frame_center(k)) / FRAME_STEP;
                if g >= total_frames || start + frame_center(k) >= audio.len() {
                    continue;
                }
                cover[g] += 1;
                for s in 0..LOCAL_SPEAKERS {
                    if frame[s]
                        && let Some(c) = mapping[s]
                    {
                        votes[c][g] += 1;
                    }
                }
            }
        }
        let frame_ms = |g: usize| ((g * FRAME_STEP) as u64 * 1000 / SAMPLE_RATE as u64) as u32;
        let mut turns = Vec::new();
        let mut speech_frames = vec![0usize; n_speakers];
        for (c, v) in votes.iter().enumerate() {
            let mut open: Option<usize> = None;
            for g in 0..=total_frames {
                let active = g < total_frames && cover[g] > 0 && v[g] * 2 >= cover[g] && v[g] > 0;
                if active {
                    speech_frames[c] += 1;
                }
                match (active, open) {
                    (true, None) => open = Some(g),
                    (false, Some(s)) => {
                        turns.push(Turn { start_ms: frame_ms(s), end_ms: frame_ms(g), speaker: c });
                        open = None;
                    }
                    _ => {}
                }
            }
        }
        // Bridge short gaps within a speaker and drop blips.
        turns.sort_by_key(|t| (t.speaker, t.start_ms));
        let mut merged: Vec<Turn> = Vec::new();
        for t in turns {
            match merged.last_mut() {
                Some(last) if last.speaker == t.speaker && t.start_ms <= last.end_ms + 300 => last.end_ms = last.end_ms.max(t.end_ms),
                _ => merged.push(t),
            }
        }
        merged.retain(|t| t.end_ms - t.start_ms >= 200);
        merged.sort_by_key(|t| (t.start_ms, t.speaker));

        // 4. Speakers numbered by first appearance, with centroids.
        let mut order: Vec<usize> = Vec::new();
        for t in &merged {
            if !order.contains(&t.speaker) {
                order.push(t.speaker);
            }
        }
        for c in 0..n_speakers {
            if !order.contains(&c) {
                order.push(c);
            }
        }
        let renumber = |c: usize| order.iter().position(|&o| o == c).unwrap_or(c);
        let speakers = order
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let mut sum = vec![0.0f32; items[0].len()];
                for (l, lab) in locals.iter().zip(&labels) {
                    if *lab == c {
                        for (s, v) in sum.iter_mut().zip(&l.embedding) {
                            *s += v * l.weight;
                        }
                    }
                }
                SpeakerProfile { index: i, embedding: l2_normalize(&sum), speech_ms: frame_ms(speech_frames[c]) }
            })
            .collect();
        let turns = merged.into_iter().map(|t| Turn { speaker: renumber(t.speaker), ..t }).collect();
        Ok(Diarization { speakers, turns })
    }
}

/// For each transcript segment `(id, start_ms, end_ms)`, the speaker who talks most during it,
/// or the nearest speaker within 2 s when nobody overlaps it.
pub fn assign_segments(turns: &[Turn], segments: &[(Id, u32, u32)]) -> Vec<(Id, usize)> {
    let mut out = Vec::with_capacity(segments.len());
    for &(id, s, e) in segments {
        let mut overlap: Vec<(usize, u32)> = Vec::new();
        for t in turns {
            let o = e.min(t.end_ms).saturating_sub(s.max(t.start_ms));
            if o > 0 {
                match overlap.iter_mut().find(|(sp, _)| *sp == t.speaker) {
                    Some(x) => x.1 += o,
                    None => overlap.push((t.speaker, o)),
                }
            }
        }
        if let Some((sp, _)) = overlap.iter().max_by_key(|(_, o)| *o) {
            out.push((id, *sp));
            continue;
        }
        let nearest = turns
            .iter()
            .map(|t| (t.speaker, if t.end_ms < s { s - t.end_ms } else { t.start_ms.saturating_sub(e) }))
            .min_by_key(|(_, d)| *d);
        if let Some((sp, d)) = nearest
            && d <= 2000
        {
            out.push((id, sp));
        }
    }
    out
}

/// A Saved person's voiceprint for matching.
#[derive(Debug, Clone)]
pub struct KnownVoice {
    pub person_id: Id,
    pub embedding: Vec<f32>,
}

/// Greedy one-to-one matches of speakers to Saved people with similarity ≥ `threshold`:
/// `(speaker index, person id, similarity)`, best first. Never applied automatically.
pub fn suggest_people(speakers: &[SpeakerProfile], known: &[KnownVoice], threshold: f32) -> Vec<(usize, Id, f32)> {
    let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
    for (si, s) in speakers.iter().enumerate() {
        for (ki, k) in known.iter().enumerate() {
            let sim = cosine(&s.embedding, &k.embedding);
            if sim >= threshold {
                pairs.push((sim, si, ki));
            }
        }
    }
    pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (mut us, mut uk) = (Vec::new(), Vec::new());
    let mut out = Vec::new();
    for (sim, si, ki) in pairs {
        if us.contains(&si) || uk.contains(&ki) {
            continue;
        }
        us.push(si);
        uk.push(ki);
        out.push((speakers[si].index, known[ki].person_id, sim));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigns_by_overlap_then_proximity() {
        let turns = vec![
            Turn { start_ms: 0, end_ms: 3000, speaker: 0 },
            Turn { start_ms: 3000, end_ms: 6000, speaker: 1 },
            Turn { start_ms: 9000, end_ms: 10_000, speaker: 0 },
        ];
        let ids: Vec<Id> = (0..4).map(|_| margin_domain::new_id()).collect();
        let segs = vec![(ids[0], 500, 2500), (ids[1], 2500, 5500), (ids[2], 6500, 7000), (ids[3], 20_000, 21_000)];
        let got = assign_segments(&turns, &segs);
        assert_eq!(got, vec![(ids[0], 0), (ids[1], 1), (ids[2], 1)]);
    }

    #[test]
    fn suggestions_are_one_to_one() {
        let a = SpeakerProfile { index: 0, embedding: l2_normalize(&[1.0, 0.1]), speech_ms: 1000 };
        let b = SpeakerProfile { index: 1, embedding: l2_normalize(&[0.9, 0.2]), speech_ms: 1000 };
        let p = margin_domain::new_id();
        let got = suggest_people(&[a, b], &[KnownVoice { person_id: p, embedding: vec![1.0, 0.0] }], 0.6);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].0, got[0].1), (0, p));
    }
}
