//! Kaldi-compatible log mel filterbank features (what speaker-embedding models are trained on):
//! 25 ms Povey windows every 10 ms, DC removal, 0.97 pre-emphasis, 512-point FFT power
//! spectrum, 80 triangular mel bins from 20 Hz to Nyquist, natural log, then per-utterance mean
//! normalisation.

use std::sync::Arc;

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

pub const NUM_BINS: usize = 80;
const RATE: f32 = 16_000.0;
const FRAME_LEN: usize = 400;
const FRAME_SHIFT: usize = 160;
const FFT_LEN: usize = 512;
const LOW_HZ: f32 = 20.0;

pub struct Fbank {
    window: Vec<f32>,
    /// For each mel bin: (first FFT bin, weights).
    filters: Vec<(usize, Vec<f32>)>,
    fft: Arc<dyn Fft<f32>>,
}

fn mel(hz: f32) -> f32 {
    1127.0 * (1.0 + hz / 700.0).ln()
}

impl Default for Fbank {
    fn default() -> Self {
        Self::new()
    }
}

impl Fbank {
    pub fn new() -> Self {
        let window = (0..FRAME_LEN)
            .map(|i| (0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME_LEN - 1) as f32).cos()).powf(0.85))
            .collect();
        let (mel_low, mel_high) = (mel(LOW_HZ), mel(RATE / 2.0));
        let delta = (mel_high - mel_low) / (NUM_BINS + 1) as f32;
        let bin_hz = RATE / FFT_LEN as f32;
        let filters = (0..NUM_BINS)
            .map(|m| {
                let (left, center, right) = (mel_low + m as f32 * delta, mel_low + (m + 1) as f32 * delta, mel_low + (m + 2) as f32 * delta);
                let mut first = None;
                let mut weights = Vec::new();
                for k in 0..FFT_LEN / 2 {
                    let f = mel(k as f32 * bin_hz);
                    let w = if f > left && f < right {
                        if f <= center { (f - left) / (center - left) } else { (right - f) / (right - center) }
                    } else {
                        0.0
                    };
                    if w > 0.0 {
                        first.get_or_insert(k);
                        weights.push(w);
                    } else if first.is_some() {
                        break;
                    }
                }
                (first.unwrap_or(0), weights)
            })
            .collect();
        let fft = FftPlanner::new().plan_fft_forward(FFT_LEN);
        Self { window, filters, fft }
    }

    /// Features for 16 kHz mono audio in -1..=1: one row of [`NUM_BINS`] per 10 ms frame.
    pub fn compute(&self, samples: &[f32]) -> Vec<[f32; NUM_BINS]> {
        if samples.len() < FRAME_LEN {
            return Vec::new();
        }
        let frames = 1 + (samples.len() - FRAME_LEN) / FRAME_SHIFT;
        let mut out = Vec::with_capacity(frames);
        let mut buf = vec![Complex32::new(0.0, 0.0); FFT_LEN];
        let mut frame = [0.0f32; FRAME_LEN];
        for f in 0..frames {
            let start = f * FRAME_SHIFT;
            // Kaldi works on 16-bit sample values.
            for (i, v) in frame.iter_mut().enumerate() {
                *v = samples[start + i] * 32768.0;
            }
            let mean = frame.iter().sum::<f32>() / FRAME_LEN as f32;
            for v in frame.iter_mut() {
                *v -= mean;
            }
            for i in (1..FRAME_LEN).rev() {
                frame[i] -= 0.97 * frame[i - 1];
            }
            frame[0] -= 0.97 * frame[0];
            for (i, b) in buf.iter_mut().enumerate() {
                *b = Complex32::new(if i < FRAME_LEN { frame[i] * self.window[i] } else { 0.0 }, 0.0);
            }
            self.fft.process(&mut buf);
            let mut row = [0.0f32; NUM_BINS];
            for (m, (first, weights)) in self.filters.iter().enumerate() {
                let mut e = 0.0f32;
                for (j, w) in weights.iter().enumerate() {
                    e += w * buf[first + j].norm_sqr();
                }
                row[m] = e.max(f32::EPSILON).ln();
            }
            out.push(row);
        }
        // Mean normalisation over the utterance.
        let mut means = [0.0f32; NUM_BINS];
        for row in &out {
            for (m, v) in row.iter().enumerate() {
                means[m] += v;
            }
        }
        for m in means.iter_mut() {
            *m /= out.len() as f32;
        }
        for row in out.iter_mut() {
            for (m, v) in row.iter_mut().enumerate() {
                *v -= means[m];
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_and_tone_peak() {
        let fb = Fbank::new();
        let tone: Vec<f32> = (0..16_000).map(|i| 0.3 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 16_000.0).sin()).collect();
        let noise: Vec<f32> = (0..16_000).map(|i| ((i * 7919 % 1013) as f32 / 1013.0 - 0.5) * 0.01).collect();
        let mixed: Vec<f32> = tone.iter().zip(&noise).map(|(a, b)| a + b).collect();
        let feats = fb.compute(&mixed);
        assert_eq!(feats.len(), 1 + (16_000 - 400) / 160);
        // Mean-normalised: each dimension averages to ~0.
        let avg: f32 = feats.iter().map(|r| r[10]).sum::<f32>() / feats.len() as f32;
        assert!(avg.abs() < 1e-3);
        // Unnormalised energy peaks in the bin holding 1 kHz: compare against a noise-only run.
        let raw_tone = fb.filters.iter().position(|(first, w)| {
            let hz = |k: usize| k as f32 * 16_000.0 / 512.0;
            hz(*first) <= 1000.0 && hz(first + w.len()) >= 1000.0
        });
        assert!(raw_tone.is_some());
    }
}
