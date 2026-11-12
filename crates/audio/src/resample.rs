//! Streaming conversion of mono audio to 16 kHz.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler as _};

use crate::{AudioError, AudioResult, SAMPLE_RATE};

/// Input frames per resampler call.
const CHUNK: usize = 1024;

/// Mono streaming resampler to [`SAMPLE_RATE`]. Pass-through when the input is already 16 kHz.
/// Allocation happens only at construction and when output buffers grow.
pub struct Resampler {
    from: u32,
    inner: Option<Fft<f32>>,
    pending: Vec<f32>,
    out: Vec<f32>,
    /// Output frames still to drop at the start (the resampler's delay), so timing stays exact.
    skip: usize,
}

impl Resampler {
    pub fn new(from: u32) -> AudioResult<Self> {
        if from == SAMPLE_RATE {
            return Ok(Self { from, inner: None, pending: Vec::new(), out: Vec::new(), skip: 0 });
        }
        let inner = Fft::<f32>::new(from as usize, SAMPLE_RATE as usize, CHUNK, 1, FixedSync::Input)
            .map_err(|e| AudioError::Resample(e.to_string()))?;
        let skip = inner.output_delay();
        let out = vec![0.0; inner.output_frames_max()];
        Ok(Self { from, inner: Some(inner), pending: Vec::with_capacity(CHUNK * 2), out, skip })
    }

    pub fn input_rate(&self) -> u32 {
        self.from
    }

    /// Feed input samples; resampled output is appended to `dst`.
    pub fn push(&mut self, input: &[f32], dst: &mut Vec<f32>) -> AudioResult<()> {
        let Some(inner) = self.inner.as_mut() else {
            dst.extend_from_slice(input);
            return Ok(());
        };
        self.pending.extend_from_slice(input);
        let mut offset = 0;
        while self.pending.len() - offset >= inner.input_frames_next() {
            let need = inner.input_frames_next();
            let input = InterleavedSlice::new(&self.pending[offset..offset + need], 1, need)
                .map_err(|e| AudioError::Resample(e.to_string()))?;
            let frames_out = inner.output_frames_next();
            if self.out.len() < frames_out {
                self.out.resize(frames_out, 0.0);
            }
            let mut output = InterleavedSlice::new_mut(&mut self.out[..frames_out], 1, frames_out)
                .map_err(|e| AudioError::Resample(e.to_string()))?;
            let (_, written) = inner
                .process_into_buffer(&input, &mut output, None)
                .map_err(|e| AudioError::Resample(e.to_string()))?;
            offset += need;
            let skip = self.skip.min(written);
            self.skip -= skip;
            dst.extend_from_slice(&self.out[skip..written]);
        }
        self.pending.drain(..offset);
        Ok(())
    }

    /// Flush what is buffered (pads with silence), for the end of a stream.
    pub fn flush(&mut self, dst: &mut Vec<f32>) -> AudioResult<()> {
        let Some(inner) = self.inner.as_ref() else { return Ok(()) };
        let need = inner.input_frames_next();
        let pad = need - self.pending.len().min(need) + need;
        let zeros = vec![0.0; pad];
        self.push(&zeros, dst)
    }
}

/// Resample a whole clip to 16 kHz (for fixtures and files).
pub fn resample_to_16k(samples: &[f32], from: u32) -> AudioResult<Vec<f32>> {
    if from == SAMPLE_RATE {
        return Ok(samples.to_vec());
    }
    let mut r = Resampler::new(from)?;
    let expected = (samples.len() as u64 * SAMPLE_RATE as u64 / from as u64) as usize;
    let mut out = Vec::with_capacity(expected + CHUNK);
    r.push(samples, &mut out)?;
    r.flush(&mut out)?;
    out.truncate(expected);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, freq: f32, secs: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize).map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin() * 0.5).collect()
    }

    /// Dominant frequency by counting zero crossings.
    fn freq_of(x: &[f32], rate: u32) -> f32 {
        let crossings = x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        crossings as f32 * rate as f32 / x.len() as f32
    }

    #[test]
    fn rates_convert_with_exact_length_and_pitch() {
        for rate in [48_000, 44_100, 22_050, 96_000, 16_000] {
            let input = sine(rate, 440.0, 2.0);
            let out = resample_to_16k(&input, rate).unwrap();
            assert_eq!(out.len(), 32_000, "rate {rate}");
            let f = freq_of(&out[1600..30_000], SAMPLE_RATE);
            assert!((f - 440.0).abs() < 3.0, "rate {rate}: {f} Hz");
            let peak = out[1600..30_000].iter().fold(0.0f32, |a, b| a.max(b.abs()));
            assert!((peak - 0.5).abs() < 0.03, "rate {rate}: peak {peak}");
        }
    }

    #[test]
    fn streaming_in_odd_chunks_matches_length() {
        let input = sine(48_000, 300.0, 1.0);
        let mut r = Resampler::new(48_000).unwrap();
        let mut out = Vec::new();
        for chunk in input.chunks(137) {
            r.push(chunk, &mut out).unwrap();
        }
        // Everything but the last partial chunk is out, with the delay already removed.
        assert!(out.len() <= 16_000 && out.len() > 16_000 - 400, "{}", out.len());
        // First samples line up with the input (no leading silence): sin starts near zero and rises.
        assert!(out[10] > 0.05, "delay trimmed: {}", out[10]);
    }
}
