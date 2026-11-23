//! Stretches of a kept recording (Ogg Opus or WAV), read without holding the whole file in
//! memory: for voiceprinting a few lines someone pointed at after the meeting.

use std::path::Path;

use crate::{AudioError, AudioResult, SAMPLE_RATE, read_wav_16k};

/// A stretch of one channel of a recording, in milliseconds on its timeline (channel 0 is the
/// mic, 1 the system audio).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSpan {
    pub channel: usize,
    pub from_ms: u32,
    pub to_ms: u32,
}

/// Gathers the samples of each span from frames handed over in timeline order.
struct Collector {
    /// (channel, first sample, end sample) per span.
    spans: Vec<(usize, usize, usize)>,
    out: Vec<Vec<f32>>,
}

impl Collector {
    fn new(spans: &[AudioSpan]) -> Self {
        let at = |ms: u32| ms as usize * SAMPLE_RATE as usize / 1000;
        Self { spans: spans.iter().map(|s| (s.channel, at(s.from_ms), at(s.to_ms.max(s.from_ms)))).collect(), out: vec![Vec::new(); spans.len()] }
    }
    fn first(&self) -> usize {
        self.spans.iter().map(|s| s.1).min().unwrap_or(0)
    }
    fn end(&self) -> usize {
        self.spans.iter().map(|s| s.2).max().unwrap_or(0)
    }
    /// One frame (a sample per channel) at timeline sample `index`.
    fn push(&mut self, index: usize, frame: &[f32]) {
        for (k, &(c, a, b)) in self.spans.iter().enumerate() {
            if index >= a && index < b {
                if let Some(v) = frame.get(c) {
                    self.out[k].push(*v);
                }
            }
        }
    }
    fn finish(self) -> Vec<f32> {
        self.out.concat()
    }
}

/// The audio of `spans`, one after another, at 16 kHz.
pub fn read_spans_16k(path: &Path, spans: &[AudioSpan]) -> AudioResult<Vec<f32>> {
    if spans.is_empty() {
        return Ok(Vec::new());
    }
    if path.extension().and_then(|e| e.to_str()) == Some("ogg") {
        return ogg_spans(path, spans);
    }
    wav_spans(path, spans)
}

fn wav_spans(path: &Path, spans: &[AudioSpan]) -> AudioResult<Vec<f32>> {
    let mut collector = Collector::new(spans);
    let mut reader = hound::WavReader::open(path).map_err(|e| AudioError::Encode(format!("cannot read {}: {e}", path.display())))?;
    let spec = reader.spec();
    if spec.sample_rate != SAMPLE_RATE {
        // Not written by the capture session: resample it all.
        let data = read_wav_16k(path)?;
        let len = data.first().map(Vec::len).unwrap_or(0);
        let mut frame = vec![0.0; data.len()];
        for i in collector.first().min(len)..collector.end().min(len) {
            for (c, ch) in data.iter().enumerate() {
                frame[c] = ch[i];
            }
            collector.push(i, &frame);
        }
        return Ok(collector.finish());
    }
    let channels = spec.channels.max(1) as usize;
    let first = collector.first().min(reader.duration() as usize);
    reader.seek(first as u32).map_err(|e| AudioError::Encode(e.to_string()))?;
    let wanted = (collector.end().saturating_sub(first)) * channels;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().take(wanted).filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().take(wanted).filter_map(Result::ok).map(|s| s as f32 * scale).collect()
        }
    };
    for (i, frame) in samples.chunks_exact(channels).enumerate() {
        collector.push(first + i, frame);
    }
    Ok(collector.finish())
}

#[cfg(feature = "opus")]
fn ogg_spans(path: &Path, spans: &[AudioSpan]) -> AudioResult<Vec<f32>> {
    let mut collector = Collector::new(spans);
    let end = collector.end();
    crate::opus_file::decode_ogg_opus_frames(path, |index, frame| {
        collector.push(index, frame);
        index + 1 < end
    })?;
    Ok(collector.finish())
}

#[cfg(not(feature = "opus"))]
fn ogg_spans(_path: &Path, _spans: &[AudioSpan]) -> AudioResult<Vec<f32>> {
    Err(AudioError::Encode("this build cannot read Ogg Opus".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WavWriter;

    #[test]
    fn reads_spans_of_a_wav() {
        let dir = std::env::temp_dir().join(format!("cilica-span-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("in.wav");
        let mut w = WavWriter::create(&wav, 2, SAMPLE_RATE).unwrap();
        // Left counts up in 1/1000ths per millisecond; right is silent.
        let frames: Vec<f32> = (0..SAMPLE_RATE as usize).flat_map(|i| [(i / 16) as f32 / 1000.0, 0.0]).collect();
        w.write_interleaved(&frames).unwrap();
        w.finish().unwrap();

        let out = read_spans_16k(&wav, &[AudioSpan { channel: 0, from_ms: 500, to_ms: 510 }, AudioSpan { channel: 1, from_ms: 0, to_ms: 5 }]).unwrap();
        assert_eq!(out.len(), 10 * 16 + 5 * 16);
        assert!((out[0] - 0.5).abs() < 0.001);
        assert!(out[160..].iter().all(|v| *v == 0.0));

        // The same stretch of the kept Ogg Opus file is as long, and stops early.
        #[cfg(feature = "opus")]
        {
            let ogg = dir.join("out.ogg");
            crate::encode_ogg_opus(&wav, &ogg).unwrap();
            let out = read_spans_16k(&ogg, &[AudioSpan { channel: 1, from_ms: 200, to_ms: 450 }]).unwrap();
            assert_eq!(out.len(), 250 * 16);
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
