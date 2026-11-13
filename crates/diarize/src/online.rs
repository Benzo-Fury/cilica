//! Live diarization: who is speaking, decided a few seconds behind the audio, on this computer.
//!
//! Every [`OnlineConfig::step`] the last 10 s of audio go through pyannote segmentation (the same
//! model as [`crate::Diarizer`]). Each local speaker with enough clean speech in the window is
//! embedded with CAM++ and matched against the speakers found so far:
//!
//! - close to one speaker ([`OnlineConfig::match_sim`]) → that speaker, and their centroid learns
//!   from it;
//! - far from everyone ([`OnlineConfig::new_sim`]) with enough clean speech
//!   ([`OnlineConfig::min_new_secs`]) → a new speaker;
//! - anything in between, or too little speech to tell → [`Label::Pending`].
//!
//! A frame is seen by every window that covers it, so each window votes on it. Frames are only
//! committed once they are [`OnlineConfig::latency`] samples behind the newest audio, which
//! gives later windows (with more of the speaker's voice) a say before anything is shown.
//!
//! A speaker started from a few seconds of speech can turn out to be someone already known once
//! more of their voice is heard. When two speakers' centroids come within
//! [`OnlineConfig::merge_sim`], the later one is folded into the earlier one and reported by
//! [`OnlineDiarizer::take_merges`], so lines already given to it can move.

use std::collections::VecDeque;
use std::path::Path;

use crate::embedding::Embedder;
use crate::segmentation::{FRAME_STEP, LOCAL_SPEAKERS, Segmenter, WINDOW, frame_center};
use crate::{DiarizeError, DiarizeResult, SAMPLE_RATE, cosine, l2_normalize};

/// Tuning knobs for [`OnlineDiarizer`].
#[derive(Debug, Clone, Copy)]
pub struct OnlineConfig {
    /// Samples between two segmentation runs.
    pub step: usize,
    /// Samples a frame waits behind the newest audio before it is committed.
    pub latency: usize,
    /// Clean speech a local speaker needs in a window to be embedded.
    pub min_embed_secs: f32,
    /// Clean speech a local speaker needs in a window to start a new speaker.
    pub min_new_secs: f32,
    /// Cosine similarity at or above which a local speaker is a known speaker.
    pub match_sim: f32,
    /// Cosine similarity below which (to every known speaker) a new speaker may start.
    pub new_sim: f32,
    /// Cosine similarity at or above which two speakers' centroids are the same person.
    pub merge_sim: f32,
    pub max_speakers: usize,
}

impl Default for OnlineConfig {
    fn default() -> Self {
        Self {
            step: SAMPLE_RATE as usize,
            latency: SAMPLE_RATE as usize * 3,
            min_embed_secs: 1.0,
            min_new_secs: 2.0,
            match_sim: 0.5,
            new_sim: 0.35,
            merge_sim: 0.6,
            max_speakers: 8,
        }
    }
}

/// What a committed stretch of audio is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Silence,
    /// Someone is speaking but it is not yet clear who.
    Pending,
    Speaker(usize),
}

/// A committed stretch with one label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start_ms: u32,
    pub end_ms: u32,
    pub label: Label,
}

/// Running centroid of one speaker.
struct Centroid {
    sum: Vec<f32>,
    weight: f32,
    unit: Vec<f32>,
}

impl Centroid {
    fn add(&mut self, embedding: &[f32], weight: f32) {
        let sum: Vec<f32> = embedding.iter().map(|v| v * weight).collect();
        self.add_sum(&sum, weight);
    }

    fn add_sum(&mut self, sum: &[f32], weight: f32) {
        for (s, v) in self.sum.iter_mut().zip(sum) {
            *s += v;
        }
        self.weight += weight;
        self.unit = l2_normalize(&self.sum);
    }
}

/// Votes a frame has collected from the windows that covered it.
#[derive(Clone, Default)]
struct Votes {
    cover: u16,
    pending: u16,
    speakers: Vec<u16>,
}

/// Causal diarizer fed audio as it is captured.
pub struct OnlineDiarizer {
    seg: Segmenter,
    emb: Embedder,
    cfg: OnlineConfig,
    /// The last [`WINDOW`] samples (zero-padded at the start of the stream).
    buf: VecDeque<f32>,
    /// Samples pushed so far.
    pushed: usize,
    /// Samples since the last segmentation run.
    since_run: usize,
    /// Every speaker ever started, by index; `None` once folded into another.
    centroids: Vec<Option<Centroid>>,
    /// Where each speaker index now points (itself while it is live).
    alias: Vec<usize>,
    /// Folds not yet collected by [`OnlineDiarizer::take_merges`]: (from, into).
    merges: Vec<(usize, usize)>,
    /// Votes for frames from `first_frame` on.
    votes: VecDeque<Votes>,
    first_frame: usize,
}

impl OnlineDiarizer {
    pub fn load(segmentation: &Path, embedding: &Path, cfg: OnlineConfig) -> DiarizeResult<Self> {
        Ok(Self {
            seg: Segmenter::load(segmentation)?,
            emb: Embedder::load(embedding)?,
            cfg,
            buf: VecDeque::from(vec![0.0; WINDOW]),
            pushed: 0,
            since_run: 0,
            centroids: Vec::new(),
            alias: Vec::new(),
            merges: Vec::new(),
            votes: VecDeque::new(),
            first_frame: 0,
        })
    }

    /// Speakers found so far (not counting ones folded into another).
    pub fn speakers(&self) -> usize {
        self.centroids.iter().flatten().count()
    }

    /// Speakers folded into another since the last call: (from, into). Spans committed before a
    /// fold may still name `from`; spans committed after it never do.
    pub fn take_merges(&mut self) -> Vec<(usize, usize)> {
        std::mem::take(&mut self.merges)
    }

    /// A stretch of `samples` with no audio (a gap in capture): commit what is pending, then
    /// carry on after the gap as silence.
    pub fn skip(&mut self, samples: usize) -> Vec<Span> {
        let mut out = self.finish();
        if samples == 0 {
            return out;
        }
        let ms = |s: usize| (s as u64 * 1000 / SAMPLE_RATE as u64) as u32;
        let from = out.last().map(|s| s.end_ms).unwrap_or(ms(self.pushed));
        self.pushed += samples;
        self.since_run = 0;
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.votes.clear();
        self.first_frame = self.pushed / FRAME_STEP + 1;
        let to = ms(self.first_frame * FRAME_STEP);
        if to > from {
            out.push(Span { start_ms: from, end_ms: to, label: Label::Silence });
        }
        out
    }

    /// The live speaker an index now stands for.
    fn resolve(&self, mut c: usize) -> usize {
        while self.alias[c] != c {
            c = self.alias[c];
        }
        c
    }

    /// Feed 16 kHz mono audio; returns the stretches committed by it.
    pub fn push(&mut self, samples: &[f32]) -> DiarizeResult<Vec<Span>> {
        let mut out = Vec::new();
        let mut rest = samples;
        while !rest.is_empty() {
            let take = (self.cfg.step - self.since_run).min(rest.len());
            self.buf.extend(&rest[..take]);
            self.buf.drain(..take);
            self.pushed += take;
            self.since_run += take;
            rest = &rest[take..];
            if self.since_run >= self.cfg.step {
                self.since_run = 0;
                self.run_window()?;
                let upto = self.pushed.saturating_sub(self.cfg.latency) / FRAME_STEP;
                self.commit(upto, &mut out);
            }
        }
        Ok(out)
    }

    /// Commit everything left at the end of the stream.
    pub fn finish(&mut self) -> Vec<Span> {
        let mut out = Vec::new();
        self.commit(self.pushed / FRAME_STEP + 1, &mut out);
        out
    }

    /// Segment the current window, label its local speakers and vote on its frames.
    fn run_window(&mut self) -> DiarizeResult<()> {
        let window: Vec<f32> = self.buf.iter().copied().collect();
        // Timeline sample of window[0]; negative while the stream is shorter than a window.
        let origin = self.pushed as i64 - WINDOW as i64;
        let act = self.seg.run(&window)?;
        let valid = |k: usize| origin + frame_center(k) as i64 >= 0;
        let min_embed = (self.cfg.min_embed_secs * SAMPLE_RATE as f32 / FRAME_STEP as f32) as usize;

        // 1. Embed each local speaker's clean speech.
        let mut locals: Vec<(usize, Vec<f32>, f32)> = Vec::new();
        for s in 0..LOCAL_SPEAKERS {
            let clean: Vec<usize> =
                (0..act.len()).filter(|&k| valid(k) && act[k][s] && act[k].iter().filter(|a| **a).count() == 1).collect();
            if clean.len() < min_embed {
                continue;
            }
            let mut clip = Vec::with_capacity(clean.len() * FRAME_STEP);
            for &k in &clean {
                let lo = frame_center(k).saturating_sub(FRAME_STEP / 2);
                let hi = (lo + FRAME_STEP).min(WINDOW);
                clip.extend_from_slice(&window[lo..hi]);
            }
            match self.emb.embed(&clip) {
                Ok(e) => locals.push((s, e, clip.len() as f32 / SAMPLE_RATE as f32)),
                Err(DiarizeError::TooShort) => {}
                Err(e) => return Err(e),
            }
        }

        // 2. Match local speakers to known ones, best pairs first, one-to-one.
        let mut label: [Label; LOCAL_SPEAKERS] = [Label::Pending; LOCAL_SPEAKERS];
        let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
        for (i, (_, e, _)) in locals.iter().enumerate() {
            for (c, cen) in self.centroids.iter().enumerate() {
                if let Some(cen) = cen {
                    pairs.push((cosine(e, &cen.unit), i, c));
                }
            }
        }
        pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (mut used_local, mut used_known) = (vec![false; locals.len()], vec![false; self.centroids.len()]);
        for &(sim, i, c) in &pairs {
            if sim < self.cfg.match_sim || used_local[i] || used_known[c] {
                continue;
            }
            used_local[i] = true;
            used_known[c] = true;
            label[locals[i].0] = Label::Speaker(c);
            let (_, e, w) = &locals[i];
            if let Some(cen) = self.centroids[c].as_mut() {
                cen.add(e, *w);
            }
        }

        // 3. Unmatched, clearly new and long enough: a new speaker.
        for (i, (s, e, w)) in locals.iter().enumerate() {
            if used_local[i] || *w < self.cfg.min_new_secs || self.speakers() >= self.cfg.max_speakers {
                continue;
            }
            let nearest = self.centroids.iter().flatten().map(|c| cosine(e, &c.unit)).fold(f32::MIN, f32::max);
            if nearest < self.cfg.new_sim {
                label[*s] = Label::Speaker(self.centroids.len());
                self.alias.push(self.centroids.len());
                self.centroids.push(Some(Centroid { sum: e.iter().map(|v| v * w).collect(), weight: *w, unit: e.clone() }));
            }
        }
        self.fold_duplicates();

        // 4. Vote on the window's frames.
        let n = self.centroids.len();
        for (k, frame) in act.iter().enumerate() {
            let t = origin + frame_center(k) as i64;
            if t < 0 {
                continue;
            }
            let g = t as usize / FRAME_STEP;
            if g < self.first_frame {
                continue;
            }
            let idx = g - self.first_frame;
            if self.votes.len() <= idx {
                self.votes.resize(idx + 1, Votes::default());
            }
            let v = &mut self.votes[idx];
            v.cover += 1;
            if v.speakers.len() < n {
                v.speakers.resize(n, 0);
            }
            for s in 0..LOCAL_SPEAKERS {
                if frame[s] {
                    match label[s] {
                        Label::Speaker(c) => v.speakers[c] += 1,
                        _ => v.pending += 1,
                    }
                }
            }
        }
        Ok(())
    }

    /// Fold together speakers whose centroids sound like the same person: the later into the
    /// earlier, so the numbering people already see stays put.
    fn fold_duplicates(&mut self) {
        loop {
            let mut best: Option<(f32, usize, usize)> = None;
            for i in 0..self.centroids.len() {
                for j in i + 1..self.centroids.len() {
                    if let (Some(a), Some(b)) = (&self.centroids[i], &self.centroids[j]) {
                        let sim = cosine(&a.unit, &b.unit);
                        if sim >= self.cfg.merge_sim && best.is_none_or(|(s, _, _)| sim > s) {
                            best = Some((sim, i, j));
                        }
                    }
                }
            }
            let Some((_, into, from)) = best else { return };
            let Some(gone) = self.centroids[from].take() else { return };
            if let Some(keep) = self.centroids[into].as_mut() {
                keep.add_sum(&gone.sum, gone.weight);
            }
            self.alias[from] = into;
            self.merges.push((from, into));
        }
    }

    /// Commit frames before `upto`, merging equal neighbours into spans.
    fn commit(&mut self, upto: usize, out: &mut Vec<Span>) {
        let ms = |g: usize| ((g * FRAME_STEP) as u64 * 1000 / SAMPLE_RATE as u64) as u32;
        while self.first_frame < upto {
            let v = self.votes.pop_front().unwrap_or_default();
            let g = self.first_frame;
            self.first_frame += 1;
            // Votes cast for a speaker since folded count for the one it joined.
            let mut speakers = vec![0u16; v.speakers.len()];
            for (c, n) in v.speakers.iter().enumerate() {
                speakers[self.resolve(c)] += n;
            }
            let best = speakers.iter().enumerate().max_by_key(|(_, n)| **n).map(|(c, n)| (c, *n));
            let active = v.pending + best.map(|b| b.1).unwrap_or(0);
            let label = if v.cover == 0 || active * 2 < v.cover {
                Label::Silence
            } else {
                match best {
                    Some((c, n)) if n > 0 && n >= v.pending => Label::Speaker(c),
                    _ => Label::Pending,
                }
            };
            match out.last_mut() {
                Some(last) if last.label == label && last.end_ms == ms(g) => last.end_ms = ms(g + 1),
                _ => out.push(Span { start_ms: ms(g), end_ms: ms(g + 1), label }),
            }
        }
    }
}
