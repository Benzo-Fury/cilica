//! Crash-safe WAV recording and reading.
//!
//! The writer appends 16-bit PCM and rewrites the RIFF sizes in the 44-byte header every second,
//! so after a crash the file is valid up to the last second; [`recover_wav`] fixes the header
//! from the file length to get the rest back.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::{AudioError, AudioResult, SAMPLE_RATE, resample_to_16k};

const HEADER_LEN: u64 = 44;
const HEADER_EVERY: Duration = Duration::from_secs(1);
const SYNC_EVERY: Duration = Duration::from_secs(5);

/// Writes interleaved 16-bit PCM, keeping the header valid as it goes.
pub struct WavWriter {
    out: BufWriter<File>,
    channels: u16,
    sample_rate: u32,
    data_bytes: u64,
    last_header: Instant,
    last_sync: Instant,
}

impl WavWriter {
    pub fn create(path: &Path, channels: u16, sample_rate: u32) -> AudioResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = OpenOptions::new().create(true).truncate(true).write(true).read(true).open(path)?;
        let mut w = Self {
            out: BufWriter::with_capacity(64 * 1024, file),
            channels,
            sample_rate,
            data_bytes: 0,
            last_header: Instant::now(),
            last_sync: Instant::now(),
        };
        w.out.write_all(&header(channels, sample_rate, 0))?;
        w.out.flush()?;
        Ok(w)
    }

    /// Append interleaved frames (`channels` samples per frame), clamped to -1..=1.
    pub fn write_interleaved(&mut self, samples: &[f32]) -> AudioResult<()> {
        let mut buf = [0u8; 2];
        for s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            buf.copy_from_slice(&v.to_le_bytes());
            self.out.write_all(&buf)?;
        }
        self.data_bytes += samples.len() as u64 * 2;
        if self.last_header.elapsed() >= HEADER_EVERY {
            self.update_header()?;
        }
        Ok(())
    }

    /// Frames written so far.
    pub fn frames(&self) -> u64 {
        self.data_bytes / (2 * self.channels as u64)
    }

    fn update_header(&mut self) -> AudioResult<()> {
        self.out.flush()?;
        let file = self.out.get_mut();
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&header(self.channels, self.sample_rate, self.data_bytes))?;
        file.seek(SeekFrom::End(0))?;
        self.last_header = Instant::now();
        if self.last_sync.elapsed() >= SYNC_EVERY {
            file.sync_data()?;
            self.last_sync = Instant::now();
        }
        Ok(())
    }

    /// Write the final header and close. Returns the frame count.
    pub fn finish(mut self) -> AudioResult<u64> {
        self.update_header()?;
        self.out.get_mut().sync_all()?;
        Ok(self.frames())
    }
}

fn header(channels: u16, sample_rate: u32, data_bytes: u64) -> [u8; 44] {
    let data = data_bytes.min(u32::MAX as u64 - 36) as u32;
    let block_align = channels * 2;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&(sample_rate * block_align as u32).to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data.to_le_bytes());
    h
}

/// Repair a WAV written by [`WavWriter`] that was not finished (the app quit or crashed):
/// the sizes are recomputed from the file length. Returns the number of frames recovered.
pub fn recover_wav(path: &Path) -> AudioResult<u64> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let len = file.metadata()?.len();
    let mut h = [0u8; 44];
    if len < HEADER_LEN || file.read_exact(&mut h).is_err() || &h[0..4] != b"RIFF" || &h[36..40] != b"data" {
        return Err(AudioError::Encode("the partial recording has no readable header".into()));
    }
    let channels = u16::from_le_bytes([h[22], h[23]]).max(1);
    let sample_rate = u32::from_le_bytes([h[24], h[25], h[26], h[27]]);
    let block = 2 * channels as u64;
    let data = (len - HEADER_LEN) / block * block;
    file.set_len(HEADER_LEN + data)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&header(channels, sample_rate, data))?;
    file.sync_all()?;
    Ok(data / block)
}

/// Read any PCM or float WAV as one `Vec<f32>` per channel, resampled to 16 kHz.
pub fn read_wav_16k(path: &Path) -> AudioResult<Vec<Vec<f32>>> {
    let reader = hound::WavReader::open(path).map_err(|e| AudioError::Encode(format!("cannot read {}: {e}", path.display())))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.into_samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader.into_samples::<i32>().filter_map(Result::ok).map(|s| s as f32 * scale).collect()
        }
    };
    let mut out = vec![Vec::with_capacity(interleaved.len() / channels); channels];
    for frame in interleaved.chunks_exact(channels) {
        for (c, s) in frame.iter().enumerate() {
            out[c].push(*s);
        }
    }
    if spec.sample_rate != SAMPLE_RATE {
        for ch in &mut out {
            *ch = resample_to_16k(ch, spec.sample_rate)?;
        }
    }
    Ok(out)
}

/// Read one channel of a WAV as 16 kHz `f32`, without holding the other channels in memory
/// (a one-hour stereo recording is 230 MB per channel as `f32`).
pub fn read_wav_channel_16k(path: &Path, channel: usize) -> AudioResult<Vec<f32>> {
    let reader = hound::WavReader::open(path).map_err(|e| AudioError::Encode(format!("cannot read {}: {e}", path.display())))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    if channel >= channels {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(reader.duration() as usize);
    match spec.sample_format {
        hound::SampleFormat::Float => {
            out.extend(reader.into_samples::<f32>().skip(channel).step_by(channels).filter_map(Result::ok));
        }
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            out.extend(reader.into_samples::<i32>().skip(channel).step_by(channels).filter_map(Result::ok).map(|s| s as f32 * scale));
        }
    }
    if spec.sample_rate != SAMPLE_RATE {
        out = resample_to_16k(&out, spec.sample_rate)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cilica-audio-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn write_read_roundtrip() {
        let path = tmp("roundtrip.wav");
        let mut w = WavWriter::create(&path, 2, SAMPLE_RATE).unwrap();
        let frames: Vec<f32> = (0..16_000).flat_map(|i| [i as f32 / 16_000.0, -0.5]).collect();
        w.write_interleaved(&frames).unwrap();
        assert_eq!(w.finish().unwrap(), 16_000);
        let ch = read_wav_16k(&path).unwrap();
        assert_eq!(ch.len(), 2);
        assert_eq!(ch[0].len(), 16_000);
        assert!((ch[0][8000] - 0.5).abs() < 1e-3);
        assert!((ch[1][3] + 0.5).abs() < 1e-3);
        let right = read_wav_channel_16k(&path, 1).unwrap();
        assert_eq!(right, ch[1]);
        assert!(read_wav_channel_16k(&path, 2).unwrap().is_empty());
    }

    #[test]
    fn unfinished_file_is_recovered() {
        let path = tmp("crash.wav");
        let mut w = WavWriter::create(&path, 2, SAMPLE_RATE).unwrap();
        w.write_interleaved(&vec![0.25; 2 * 8000]).unwrap();
        // Simulate a crash: flush the data but never write the final header.
        w.out.flush().unwrap();
        std::mem::forget(w);
        // Also a torn final frame.
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(&[1, 2, 3]).unwrap();
        drop(f);
        assert_eq!(recover_wav(&path).unwrap(), 8000);
        let ch = read_wav_16k(&path).unwrap();
        assert_eq!(ch[1].len(), 8000);
        assert!((ch[1][7999] - 0.25).abs() < 1e-3);
    }
}
