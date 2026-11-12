//! Acoustic echo cancellation on the microphone, with system audio as the reference.
//!
//! Uses the `aec3` crate, a pure-Rust port of WebRTC's AEC3 (plus its high-pass filter and noise
//! suppressor), so it builds identically on every platform. Remote voices that leak from the
//! speakers into the mic are removed before transcription, so they are not transcribed twice.
//! If the pipeline cannot be built the canceller passes audio through unchanged and says so.

use aec3::nodes::audio::AudioFormat;
use aec3::pipelines::linear::{self, LinearPipeline};

use crate::SAMPLE_RATE;

/// 10 ms at 16 kHz: the unit every stage of the capture thread works in.
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE / 100) as usize;

pub struct EchoCanceller {
    pipeline: Option<LinearPipeline>,
    out: Vec<f32>,
    /// Frames of output the pipeline has not produced yet (start-up latency); filled from input.
    passthrough_frames: u64,
    gate: ResidualGate,
}

/// Mutes what is left of the far end after cancellation.
///
/// AEC3 removes most of the echo, but the residue (quiet, smeared speech) is still enough for a
/// speech model to "hear" words in, and it writes nonsense lines under "You". While the speakers
/// are playing and cancellation removed at least half of the mic's energy, the mic at that moment
/// is mostly echo, so the remainder is turned right down. When the person at the mic talks over
/// the call their voice dominates, cancellation removes little, and the gate stays open.
struct ResidualGate {
    /// 10 ms frames since the speakers last played something audible.
    since_render: u32,
    /// Smoothed mic energy before and after cancellation.
    before: f32,
    after: f32,
    /// Current gain, moved gradually to avoid clicks.
    gain: f32,
}

impl ResidualGate {
    /// About -70 dBFS: below this the speakers are silent.
    const RENDER_FLOOR: f32 = 1e-7;
    /// Echo keeps arriving for a while after the speakers stop (delay plus room tail).
    const HANG_FRAMES: u32 = 40;
    /// Cancellation removed at least half the energy: what is left is residue, not a voice.
    const MOSTLY_ECHO: f32 = 0.5;
    const CLOSED_GAIN: f32 = 0.03;

    fn new() -> Self {
        Self { since_render: u32::MAX, before: 0.0, after: 0.0, gain: 1.0 }
    }

    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32
    }

    /// `before` is the raw mic frame's energy; `mic` is the cancelled frame, scaled in place.
    fn apply(&mut self, render: &[f32], before: f32, mic: &mut [f32]) {
        self.since_render = if Self::energy(render) > Self::RENDER_FLOOR { 0 } else { self.since_render.saturating_add(1) };
        // ~80 ms smoothing.
        self.before += 0.12 * (before - self.before);
        self.after += 0.12 * (Self::energy(mic) - self.after);
        let playing = self.since_render <= Self::HANG_FRAMES;
        let mostly_echo = self.after < self.before * Self::MOSTLY_ECHO;
        let target = if playing && mostly_echo { Self::CLOSED_GAIN } else { 1.0 };
        // Close quickly, open quickly too: the first syllable of a reply must not be lost.
        let step = if target < self.gain { 0.35 } else { 0.5 };
        for v in mic.iter_mut() {
            *v *= self.gain;
        }
        self.gain += step * (target - self.gain);
    }
}

impl Default for EchoCanceller {
    fn default() -> Self {
        Self::new()
    }
}

impl EchoCanceller {
    pub fn new() -> Self {
        let format = AudioFormat::ten_ms(SAMPLE_RATE, 1);
        let pipeline = linear::builder(format, format)
            // Automatic gain would make the stored recording pump; Whisper copes with level.
            .enable_gain_controller2(false)
            .enable_post_filter(false)
            .enable_noise_suppression(true)
            .enable_high_pass_filter(true)
            .build();
        let pipeline = match pipeline {
            Ok(p) => Some(p),
            Err(error) => {
                tracing::warn!(?error, "echo cancellation unavailable; the mic is passed through");
                None
            }
        };
        Self { pipeline, out: vec![0.0; FRAME_SAMPLES], passthrough_frames: 0, gate: ResidualGate::new() }
    }

    /// A canceller that never changes the mic (for "no system audio" sessions and tests).
    pub fn disabled() -> Self {
        Self { pipeline: None, out: vec![0.0; FRAME_SAMPLES], passthrough_frames: 0, gate: ResidualGate::new() }
    }

    pub fn is_active(&self) -> bool {
        self.pipeline.is_some()
    }

    /// Process one 10 ms frame: `render` is what the speakers played (system audio), `mic` is
    /// overwritten with the echo-cancelled microphone.
    pub fn process(&mut self, render: &[f32], mic: &mut [f32]) {
        debug_assert_eq!(render.len(), FRAME_SAMPLES);
        debug_assert_eq!(mic.len(), FRAME_SAMPLES);
        let Some(p) = self.pipeline.as_mut() else { return };
        let before = ResidualGate::energy(mic);
        let result = p.handle_render_frame(render).and_then(|_| p.process_capture_frame(mic, &mut self.out));
        match result {
            Ok(true) => {
                mic.copy_from_slice(&self.out);
                self.gate.apply(render, before, mic);
            }
            Ok(false) => self.passthrough_frames += 1,
            Err(error) => {
                tracing::warn!(?error, "echo cancellation failed; passing the mic through from now on");
                self.pipeline = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic noise-like "speech" (sum of drifting tones with an envelope).
    fn voice(len: usize, seed: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let env = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t + seed).sin();
                let f0 = 140.0 + 40.0 * (t * 1.7 + seed).sin();
                let mut s = 0.0;
                for h in 1..8 {
                    s += (2.0 * std::f32::consts::PI * f0 * h as f32 * t).sin() / h as f32;
                }
                0.15 * env * s
            })
            .collect()
    }

    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32
    }

    #[test]
    fn removes_echo_of_far_end() {
        let n = SAMPLE_RATE as usize * 8;
        let far = voice(n, 0.0);
        // Echo path: 40 ms delay, -6 dB, plus a little room tail.
        let delay = 640;
        let mut mic = vec![0.0f32; n];
        for i in delay..n {
            mic[i] = 0.5 * far[i - delay] + 0.1 * far[i - delay - 37.min(i - delay)];
        }
        let mut aec = EchoCanceller::new();
        assert!(aec.is_active());
        let mut out = mic.clone();
        for (r, m) in far.chunks(FRAME_SAMPLES).zip(out.chunks_mut(FRAME_SAMPLES)) {
            aec.process(r, m);
        }
        // Compare the last 3 s, after the filter has converged.
        let tail = n - 3 * SAMPLE_RATE as usize;
        let erle_db = 10.0 * (energy(&mic[tail..]) / energy(&out[tail..]).max(1e-12)).log10();
        assert!(erle_db > 10.0, "echo return loss enhancement only {erle_db:.1} dB");
    }

    #[test]
    fn residue_is_muted_but_a_reply_over_the_call_is_kept() {
        let n = SAMPLE_RATE as usize * 10;
        let far = voice(n, 0.0);
        let near = voice(n, 2.1);
        let delay = 640;
        let half = n / 2;
        // First half: only the call plays. Second half: the person at the mic talks over it, louder.
        let mut mic = vec![0.0f32; n];
        for i in delay..n {
            mic[i] = 0.4 * far[i - delay];
            if i >= half {
                mic[i] += 1.5 * near[i];
            }
        }
        let mut aec = EchoCanceller::new();
        let mut out = mic.clone();
        for (r, m) in far.chunks(FRAME_SAMPLES).zip(out.chunks_mut(FRAME_SAMPLES)) {
            aec.process(r, m);
        }
        let sr = SAMPLE_RATE as usize;
        // Echo only, after convergence: at least 30 dB quieter than the raw mic.
        let (a, b) = (half - 2 * sr, half - sr / 2);
        let drop_db = 10.0 * (energy(&mic[a..b]) / energy(&out[a..b]).max(1e-14)).log10();
        assert!(drop_db > 30.0, "echo residue only {drop_db:.1} dB down");
        // Talking over the call: most of the near voice survives.
        let (a, b) = (half + sr, n - sr / 2);
        let near_scaled: Vec<f32> = near[a..b].iter().map(|v| 1.5 * v).collect();
        let kept = energy(&out[a..b]) / energy(&near_scaled);
        assert!(kept > 0.5, "near-end voice kept: {kept:.2}");
    }

    #[test]
    fn keeps_near_end_speech() {
        let n = SAMPLE_RATE as usize * 6;
        let near = voice(n, 1.3);
        let silence = vec![0.0f32; n];
        let mut aec = EchoCanceller::new();
        let mut out = near.clone();
        for (r, m) in silence.chunks(FRAME_SAMPLES).zip(out.chunks_mut(FRAME_SAMPLES)) {
            aec.process(r, m);
        }
        let tail = n - 3 * SAMPLE_RATE as usize;
        let ratio = energy(&out[tail..]) / energy(&near[tail..]);
        assert!(ratio > 0.3, "near-end speech kept: {ratio:.2}");
    }
}
