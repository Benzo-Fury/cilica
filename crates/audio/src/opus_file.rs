//! The kept recording: Ogg Opus (RFC 7845), stereo (mic left, system right), 16 kHz input,
//! 32 kbit/s VBR in VoIP mode. About 14 MB per hour.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use ogg::writing::PacketWriteEndInfo;
use opus::{Application, Bitrate, Channels};

use crate::{AudioError, AudioResult, SAMPLE_RATE, read_wav_16k};

/// 20 ms frames.
const FRAME: usize = (SAMPLE_RATE / 50) as usize;
/// Ogg Opus granule positions always count 48 kHz samples.
const GRANULE_SCALE: u64 = 48_000 / SAMPLE_RATE as u64;
const BITRATE: i32 = 32_000;

fn enc_err(e: impl std::fmt::Display) -> AudioError {
    AudioError::Encode(e.to_string())
}

/// Frames of a WAV, 20 ms at a time, as 16 kHz float, without loading the whole file: 16 kHz
/// files (what the capture session writes) stream from disk; other rates are resampled in memory.
struct FrameReader {
    channels: usize,
    frames: usize,
    source: FrameSource,
}

enum FrameSource {
    Stream(hound::WavIntoSamples<BufReader<File>, i32>, f32),
    Memory(Vec<Vec<f32>>, usize),
}

impl FrameReader {
    fn open(wav: &Path) -> AudioResult<Self> {
        let reader = hound::WavReader::open(wav).map_err(|e| AudioError::Encode(format!("cannot read {}: {e}", wav.display())))?;
        let spec = reader.spec();
        let channels = spec.channels.max(1) as usize;
        if spec.sample_rate == SAMPLE_RATE && spec.sample_format == hound::SampleFormat::Int {
            let frames = reader.duration() as usize;
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            return Ok(Self { channels, frames, source: FrameSource::Stream(reader.into_samples::<i32>(), scale) });
        }
        drop(reader);
        let data = read_wav_16k(wav)?;
        let frames = data.first().map(Vec::len).unwrap_or(0);
        Ok(Self { channels: data.len().max(1), frames, source: FrameSource::Memory(data, 0) })
    }

    /// Fill `out` (interleaved, `n_ch` channels) with the next frames, zero-padded at the end.
    /// Channels beyond `n_ch` are dropped.
    fn next(&mut self, out: &mut [f32], n_ch: usize) {
        let frames = out.len() / n_ch;
        for i in 0..frames {
            for c in 0..self.channels {
                let v = match &mut self.source {
                    FrameSource::Stream(it, scale) => it.next().and_then(Result::ok).map(|s| s as f32 * *scale).unwrap_or(0.0),
                    FrameSource::Memory(data, pos) => data[c].get(*pos + i).copied().unwrap_or(0.0),
                };
                if c < n_ch {
                    out[i * n_ch + c] = v;
                }
            }
        }
        if let FrameSource::Memory(_, pos) = &mut self.source {
            *pos += frames;
        }
    }
}

/// Encode a WAV (any rate, one or two channels) to an Ogg Opus file, streaming. Returns the
/// duration in ms. Writes to a temporary file and renames, so `out` is never left half-written.
pub fn encode_ogg_opus(wav: &Path, out: &Path) -> AudioResult<u32> {
    let mut input = FrameReader::open(wav)?;
    let n_ch = input.channels.clamp(1, 2);
    let frames = input.frames;
    let mut enc = opus::Encoder::new(SAMPLE_RATE, if n_ch == 2 { Channels::Stereo } else { Channels::Mono }, Application::Voip)
        .map_err(enc_err)?;
    enc.set_bitrate(Bitrate::Bits(BITRATE)).map_err(enc_err)?;
    enc.set_vbr(true).map_err(enc_err)?;
    let lookahead = enc.get_lookahead().map_err(enc_err)?.max(0) as u64;
    let pre_skip = (lookahead * GRANULE_SCALE) as u16;

    let tmp = out.with_extension("ogg.tmp");
    let file = BufWriter::new(File::create(&tmp)?);
    let mut writer = ogg::writing::PacketWriter::new(file);
    let serial: u32 = 0x4e4f5441; // "CILICA"

    // Identification header.
    let mut head = Vec::with_capacity(19);
    head.extend_from_slice(b"OpusHead");
    head.push(1);
    head.push(n_ch as u8);
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    writer.write_packet(head, serial, PacketWriteEndInfo::EndPage, 0)?;
    // Comment header.
    let vendor = b"Cilica";
    let mut tags = Vec::new();
    tags.extend_from_slice(b"OpusTags");
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes());
    writer.write_packet(tags, serial, PacketWriteEndInfo::EndPage, 0)?;

    let mut pcm = vec![0.0f32; FRAME * n_ch];
    let mut packet = vec![0u8; 4000];
    let total_frames = frames.div_ceil(FRAME).max(1);
    for f in 0..total_frames {
        let start = f * FRAME;
        input.next(&mut pcm, n_ch);
        let len = enc.encode_float(&pcm, &mut packet).map_err(enc_err)?;
        let last = f + 1 == total_frames;
        // The final granule marks the real end so decoders trim the padding.
        let granule = if last {
            (frames as u64 + lookahead) * GRANULE_SCALE
        } else {
            ((start + FRAME) as u64) * GRANULE_SCALE
        };
        let info = if last {
            PacketWriteEndInfo::EndStream
        } else if (f + 1) % 50 == 0 {
            PacketWriteEndInfo::EndPage
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        writer.write_packet(packet[..len].to_vec(), serial, info, granule)?;
    }
    let mut file = writer.into_inner();
    std::io::Write::flush(&mut file)?;
    file.into_inner().map_err(|e| AudioError::Io(e.into_error()))?.sync_all()?;
    std::fs::rename(&tmp, out)?;
    Ok((frames as u64 * 1000 / SAMPLE_RATE as u64) as u32)
}

/// Decode an Ogg Opus file written by [`encode_ogg_opus`] to one `Vec<f32>` per channel at 16 kHz.
pub fn decode_ogg_opus(path: &Path) -> AudioResult<Vec<Vec<f32>>> {
    let mut reader = ogg::reading::PacketReader::new(BufReader::new(File::open(path)?));
    let head = reader.read_packet().map_err(enc_err)?.ok_or_else(|| enc_err("empty file"))?;
    if head.data.len() < 19 || &head.data[0..8] != b"OpusHead" {
        return Err(enc_err("not an Ogg Opus file"));
    }
    let n_ch = head.data[9].clamp(1, 2) as usize;
    let pre_skip = u16::from_le_bytes([head.data[10], head.data[11]]) as u64 / GRANULE_SCALE;
    let _tags = reader.read_packet().map_err(enc_err)?;
    let mut dec = opus::Decoder::new(SAMPLE_RATE, if n_ch == 2 { Channels::Stereo } else { Channels::Mono }).map_err(enc_err)?;
    let mut out = vec![Vec::new(); n_ch];
    let mut buf = vec![0.0f32; 5760 * n_ch];
    let mut last_granule = 0u64;
    while let Some(p) = reader.read_packet().map_err(enc_err)? {
        let n = dec.decode_float(&p.data, &mut buf, false).map_err(enc_err)?;
        for i in 0..n {
            for c in 0..n_ch {
                out[c].push(buf[i * n_ch + c]);
            }
        }
        last_granule = p.absgp_page();
    }
    let end = (last_granule / GRANULE_SCALE) as usize;
    for ch in &mut out {
        ch.truncate(end.min(ch.len()));
        ch.drain(..(pre_skip as usize).min(ch.len()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WavWriter;

    #[test]
    fn encode_and_decode_stereo() {
        let dir = std::env::temp_dir().join(format!("cilica-opus-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("in.wav");
        let ogg = dir.join("out.ogg");
        let mut w = WavWriter::create(&wav, 2, SAMPLE_RATE).unwrap();
        let n = SAMPLE_RATE as usize * 3;
        let frames: Vec<f32> = (0..n)
            .flat_map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                [0.3 * (2.0 * std::f32::consts::PI * 440.0 * t).sin(), 0.3 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin()]
            })
            .collect();
        w.write_interleaved(&frames).unwrap();
        w.finish().unwrap();
        let ms = encode_ogg_opus(&wav, &ogg).unwrap();
        assert_eq!(ms, 3000);
        let size = std::fs::metadata(&ogg).unwrap().len();
        assert!(size < 20_000, "compact: {size} bytes for 3 s");
        let ch = decode_ogg_opus(&ogg).unwrap();
        assert_eq!(ch.len(), 2);
        assert!((ch[0].len() as i64 - n as i64).abs() <= 320, "{} vs {n}", ch[0].len());
        // Channels kept apart: left is 440 Hz, right 1 kHz (count zero crossings).
        let zc = |x: &[f32]| x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count() as f32 / (x.len() as f32 / SAMPLE_RATE as f32);
        let mid = 8000..40_000;
        assert!((zc(&ch[0][mid.clone()]) - 440.0).abs() < 10.0);
        assert!((zc(&ch[1][mid]) - 1000.0).abs() < 15.0);
    }
}
