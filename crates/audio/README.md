# cilica-audio

Audio capture for Cilica: the microphone and the system output, each delivered as 16 kHz mono `f32`
on one shared timeline, echo-cancelled, metered and written to disk as it arrives. The only crate
(with `cilica-detect`) allowed platform `cfg`.

## Public API

- `CaptureSource` (trait): `open() -> RawStream`. Implementations: `MicSource` (cpal, every OS),
  `SystemSource` (macOS: Core Audio process tap excluding Cilica's own process, in a private
  aggregate device, via cidre; Windows: WASAPI loopback of the default render device; Linux:
  PipeWire monitor of the default sink), `ClipSource` (plays a clip in real time, for tests).
- `CaptureSession::start(CaptureConfig, Sender<CaptureEvent>)`, `.stop() -> CaptureSummary`.
  One thread: resamples each source to 16 kHz (`Resampler`, rubato), cuts 10 ms frames 100 ms
  behind the wall clock (a stalled or unplugged source is padded with silence and reopened every
  2 s; a fast one is trimmed), runs `EchoCanceller` on the mic with system audio as reference,
  silences `mute_start_ms` at the start (the start chime), writes a stereo WAV (mic left, system
  right) with `WavWriter`, and sends `CaptureEvent::{Audio, Levels (20 Hz), SourceLost,
  SourceRestored, SourceUnavailable}`. Audio after the stop instant is not kept (the stop chime).
- `WavWriter` (header rewritten every second, fsync every 5 s: crash-safe), `recover_wav`
  (repairs an unfinished file), `read_wav_16k`, `read_wav_channel_16k`.
- `encode_ogg_opus` / `decode_ogg_opus` (feature `opus`, default): the kept file, Ogg Opus,
  stereo, 32 kbit/s VBR VoIP, about 14 MB an hour, streamed from the WAV.
- `EchoCanceller`: WebRTC AEC3 (pure-Rust port, crate `aec3`) with its high-pass filter and
  noise suppressor; automatic gain off so the stored recording does not pump.
- `LevelMeter`, `permissions::{status, all, request, open_settings}`, `list_input_devices`.

## Must not depend on

`tauri`, `cilica-store`, or any app crate except `cilica-domain`. Capture callbacks never allocate or
take a lock (a lock-free ring per source; the error message mutex is `try_lock` on the rare
failure path).

## Tests

- `cargo test -p cilica-audio`: resampling, WAV round trip and crash recovery, Opus round trip,
  level meter, AEC on synthetic and on speech fixtures (`tests/echo.rs`, files in
  `tests/fixtures`, made with `say` + `afconvert`), and a full capture session from fake
  sources at 48 and 44.1 kHz with a mic unplugged mid-way.
- Live: `cargo run -p cilica-audio --example capture -- --seconds 5 [--out target/capture]
  [--no-system]` records the real mic and system audio, prints levels and writes WAV + Ogg. On
  macOS the first run asks for Microphone and System Audio Recording for the app that launched
  it (your terminal).
- Windows and Linux: type-checked only (`cargo check -p cilica-audio --target
  x86_64-pc-windows-msvc --no-default-features`); never run.
