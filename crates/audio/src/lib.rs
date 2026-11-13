//! Audio capture for Cilica: the microphone and the system output, each delivered as timestamped
//! 16 kHz mono `f32` frames, echo-cancelled, metered and written to disk as they arrive.
//!
//! - [`CaptureSource`] is the one trait every platform implements: [`MicSource`] (cpal, all
//!   platforms) and [`SystemSource`] (Core Audio process tap on macOS, WASAPI loopback on
//!   Windows, PipeWire sink monitor on Linux).
//! - [`CaptureSession`] runs both on one dedicated thread: resamples to 16 kHz, cancels the
//!   system audio out of the mic ([`EchoCanceller`]), writes a crash-safe stereo WAV
//!   ([`WavWriter`]; mic left, system right), meters levels and hands frames to the caller.
//! - [`encode_ogg_opus`] turns the finished WAV into the compact file that is kept.
//! - [`permissions`] and [`list_input_devices`] back the onboarding and Settings screens.
//! - [`ClipSource`] plays a clip in real time as if it were a device, for tests.
//!
//! Platform `cfg` lives only in this crate (and `cilica-detect`).

mod aec;
mod clip;
mod error;
mod level;
mod mic;
#[cfg(feature = "opus")]
mod opus_file;
pub mod permissions;
mod platform;
mod resample;
mod session;
mod source;
mod wav;

pub use aec::{EchoCanceller, FRAME_SAMPLES};
pub use clip::ClipSource;
pub use error::{AudioError, AudioResult};
pub use level::LevelMeter;
pub use mic::{MicSource, list_input_devices};
#[cfg(feature = "opus")]
pub use opus_file::{decode_ogg_opus, encode_ogg_opus};

/// Without the `opus` feature there is no encoder; callers keep the WAV.
#[cfg(not(feature = "opus"))]
pub fn encode_ogg_opus(_wav: &std::path::Path, _out: &std::path::Path) -> AudioResult<u32> {
    Err(AudioError::Encode("this build has no Opus encoder".into()))
}
pub use platform::SystemSource;
pub use resample::{Resampler, resample_to_16k};
pub use session::{CaptureConfig, CaptureEvent, CaptureSession, CaptureSummary};
pub use source::{CaptureSource, RawStream, SourceKind, SourceStatus};
pub use wav::{WavWriter, read_wav_16k, read_wav_channel_16k, recover_wav};

/// Every stream leaves this crate at 16 kHz mono.
pub const SAMPLE_RATE: u32 = 16_000;

/// Convert a sample count at [`SAMPLE_RATE`] to milliseconds.
pub fn samples_to_ms(samples: u64) -> u32 {
    (samples * 1000 / SAMPLE_RATE as u64).min(u32::MAX as u64) as u32
}
