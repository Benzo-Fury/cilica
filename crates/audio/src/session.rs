//! One recording's capture thread: both sources, 16 kHz, echo cancellation, the crash-safe file,
//! levels and frames for transcription.
//!
//! The timeline is anchored to the wall clock: every 10 ms frame index is "time since start".
//! Each source fills its own FIFO; frames are cut 100 ms behind real time so ordinary jitter is
//! absorbed, a source that stalls or is unplugged is padded with silence (and reopened), and a
//! source that runs fast (clock drift) is trimmed. So mic and system stay aligned in the stereo
//! file and in segment timestamps, whatever happens to either device.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use margin_domain::AudioLevels;
use ringbuf::traits::{Consumer, Observer};

use crate::{
    AudioError, AudioResult, CaptureSource, EchoCanceller, FRAME_SAMPLES, LevelMeter, RawStream, Resampler, SAMPLE_RATE,
    SourceKind, WavWriter, mic::default_input_id, samples_to_ms,
};

/// How far behind real time frames are cut.
const LATENCY: u64 = SAMPLE_RATE as u64 / 10;
/// More than this buffered means the source runs fast; trim back to half.
const MAX_FIFO: usize = SAMPLE_RATE as usize;
/// Audio is handed to the caller in chunks of this many 10 ms frames.
const CHUNK_FRAMES: usize = 10;
const LEVEL_EVERY: Duration = Duration::from_millis(50);
const RETRY_EVERY: Duration = Duration::from_secs(2);
const PROBE_EVERY: Duration = Duration::from_secs(2);
const TICK: Duration = Duration::from_millis(10);

/// What to capture and where to write it.
pub struct CaptureConfig {
    pub mic: Box<dyn CaptureSource>,
    /// `None` records the mic only (no permission, or the person turned it off).
    pub system: Option<Box<dyn CaptureSource>>,
    /// Stereo 16-bit WAV, mic left, system right, written as audio arrives.
    pub wav_path: PathBuf,
    /// Cancel system audio out of the mic.
    pub echo_cancel: bool,
    /// Reopen the mic when the system default input changes (when no device was chosen).
    pub follow_default_input: bool,
    /// Silence this much audio at the start (the start chime plays while capture spins up).
    pub mute_start_ms: u32,
}

/// What the capture thread reports.
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureEvent {
    /// 16 kHz mono audio of one channel. `start` is the sample index on the shared timeline.
    Audio { kind: SourceKind, start: u64, samples: Vec<f32> },
    Levels(AudioLevels),
    /// A source stopped (unplugged, server gone). Silence is recorded until it comes back.
    SourceLost { kind: SourceKind, message: String },
    SourceRestored { kind: SourceKind, device_name: String },
    /// A source could not be opened at the start; the recording continues without it.
    SourceUnavailable { kind: SourceKind, message: String, permission_denied: bool },
}

/// The result of a finished capture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureSummary {
    pub frames: u64,
    pub duration_ms: u32,
}

/// A running capture. [`stop`](Self::stop) finishes the file; dropping it also stops.
pub struct CaptureSession {
    stop: Arc<AtomicBool>,
    stop_at: Arc<AtomicU64>,
    started: Instant,
    handle: Option<JoinHandle<AudioResult<CaptureSummary>>>,
}

impl CaptureSession {
    /// Open the sources and start the capture thread. Fails only when the microphone cannot be
    /// opened or the file cannot be created; a missing system source is reported as an event.
    pub fn start(config: CaptureConfig, events: Sender<CaptureEvent>) -> AudioResult<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_at = Arc::new(AtomicU64::new(u64::MAX));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<AudioResult<Instant>>();
        let thread_stop = stop.clone();
        let thread_stop_at = stop_at.clone();
        let handle = std::thread::Builder::new()
            .name("margin-capture".into())
            .spawn(move || {
                let mut worker = match Worker::open(config, events) {
                    Ok(w) => w,
                    Err(e) => {
                        let msg = e.to_string();
                        let _ = ready_tx.send(Err(e));
                        return Err(AudioError::Device(msg));
                    }
                };
                let _ = ready_tx.send(Ok(worker.started));
                worker.run(&thread_stop, &thread_stop_at)
            })
            .map_err(AudioError::Io)?;
        match ready_rx.recv() {
            Ok(Ok(started)) => Ok(Self { stop, stop_at, started, handle: Some(handle) }),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => Err(AudioError::Device("the capture thread stopped unexpectedly".into())),
        }
    }

    /// Milliseconds since capture started.
    pub fn elapsed_ms(&self) -> u32 {
        self.started.elapsed().as_millis().min(u32::MAX as u128) as u32
    }

    /// Stop capturing now: audio after this instant is not kept (the stop chime plays next),
    /// buffered audio before it is written, and the file is finalised.
    pub fn stop(mut self) -> AudioResult<CaptureSummary> {
        self.signal_stop();
        match self.handle.take() {
            Some(h) => h.join().map_err(|_| AudioError::Device("the capture thread panicked".into()))?,
            None => Err(AudioError::Device("already stopped".into())),
        }
    }

    fn signal_stop(&self) {
        let at = (self.started.elapsed().as_secs_f64() * SAMPLE_RATE as f64) as u64;
        let _ = self.stop_at.compare_exchange(u64::MAX, at, Ordering::SeqCst, Ordering::SeqCst);
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            self.signal_stop();
            let _ = h.join();
        }
    }
}

/// One source on the capture thread.
struct Lane {
    kind: SourceKind,
    source: Option<Box<dyn CaptureSource>>,
    stream: Option<RawStream>,
    resampler: Option<Resampler>,
    fifo: VecDeque<f32>,
    raw: Vec<f32>,
    resampled: Vec<f32>,
    retry_at: Option<Instant>,
    /// For the mic when following the default input: the id it was opened on.
    opened_default: Option<String>,
    meter: LevelMeter,
    chunk: Vec<f32>,
    chunk_start: u64,
    /// The lane delivers audio to the caller (it has been open at least once).
    active: bool,
}

impl Lane {
    fn new(kind: SourceKind, source: Option<Box<dyn CaptureSource>>) -> Self {
        Self {
            kind,
            source,
            stream: None,
            resampler: None,
            fifo: VecDeque::with_capacity(MAX_FIFO * 2),
            raw: vec![0.0; 4096],
            resampled: Vec::with_capacity(4096),
            retry_at: None,
            opened_default: None,
            meter: LevelMeter::new(),
            chunk: Vec::with_capacity(FRAME_SAMPLES * CHUNK_FRAMES),
            chunk_start: 0,
            active: false,
        }
    }

    fn open(&mut self) -> AudioResult<String> {
        let Some(source) = self.source.as_ref() else {
            return Err(AudioError::Device("no source".into()));
        };
        let stream = source.open()?;
        self.resampler = Some(Resampler::new(stream.sample_rate)?);
        let name = stream.device_name.clone();
        self.stream = Some(stream);
        self.retry_at = None;
        self.active = true;
        Ok(name)
    }

    fn close(&mut self) {
        self.stream = None;
        self.resampler = None;
    }

    /// Move everything the callback produced into the 16 kHz FIFO.
    fn pump(&mut self) -> AudioResult<()> {
        let (Some(stream), Some(rs)) = (self.stream.as_mut(), self.resampler.as_mut()) else { return Ok(()) };
        loop {
            let n = stream.consumer.pop_slice(&mut self.raw);
            if n == 0 {
                break;
            }
            self.resampled.clear();
            rs.push(&self.raw[..n], &mut self.resampled)?;
            self.fifo.extend(self.resampled.iter().copied());
            if stream.consumer.is_empty() {
                break;
            }
        }
        if self.fifo.len() > MAX_FIFO {
            let excess = self.fifo.len() - MAX_FIFO / 2;
            self.fifo.drain(..excess);
            tracing::debug!(kind = ?self.kind, excess, "capture source runs fast; trimmed");
        }
        Ok(())
    }

    /// Take one 10 ms frame, padding with silence if the source is behind or gone.
    fn take_frame(&mut self, out: &mut [f32]) {
        let n = self.fifo.len().min(FRAME_SAMPLES);
        for (o, s) in out.iter_mut().zip(self.fifo.drain(..n)) {
            *o = s;
        }
        for o in &mut out[n..] {
            *o = 0.0;
        }
    }
}

struct Worker {
    mic: Lane,
    sys: Lane,
    /// `None` only after the file has been finished.
    writer: Option<WavWriter>,
    aec: EchoCanceller,
    events: Sender<CaptureEvent>,
    started: Instant,
    follow_default: bool,
    mute_until: u64,
}

impl Worker {
    fn open(config: CaptureConfig, events: Sender<CaptureEvent>) -> AudioResult<Self> {
        let mut mic = Lane::new(SourceKind::Mic, Some(config.mic));
        let mut sys = Lane::new(SourceKind::System, config.system);
        if config.follow_default_input {
            mic.opened_default = default_input_id();
        }
        mic.open()?;
        if sys.source.is_some()
            && let Err(e) = sys.open()
        {
            tracing::warn!(error = %e, "system audio unavailable; recording the microphone only");
            let _ = events.send(CaptureEvent::SourceUnavailable {
                kind: SourceKind::System,
                permission_denied: matches!(e, AudioError::SystemPermissionDenied),
                message: e.to_string(),
            });
            // Do not keep retrying a source that was never available.
            sys.source = None;
        }
        let writer = WavWriter::create(&config.wav_path, 2, SAMPLE_RATE)?;
        let aec = if config.echo_cancel && sys.stream.is_some() { EchoCanceller::new() } else { EchoCanceller::disabled() };
        Ok(Self {
            mic,
            sys,
            writer: Some(writer),
            aec,
            events,
            started: Instant::now(),
            follow_default: config.follow_default_input,
            mute_until: config.mute_start_ms as u64 * SAMPLE_RATE as u64 / 1000,
        })
    }

    fn run(&mut self, stop: &AtomicBool, stop_at: &AtomicU64) -> AudioResult<CaptureSummary> {
        let mut emitted: u64 = 0;
        let mut last_level = Instant::now();
        let mut last_probe = Instant::now();
        let mut mic_frame = vec![0.0f32; FRAME_SAMPLES];
        let mut sys_frame = vec![0.0f32; FRAME_SAMPLES];
        let mut stereo = vec![0.0f32; FRAME_SAMPLES * 2];
        loop {
            let stopping = stop.load(Ordering::SeqCst);
            for lane in [&mut self.mic, &mut self.sys] {
                if let Err(e) = lane.pump() {
                    tracing::warn!(error = %e, "resampling failed");
                }
            }
            if !stopping {
                self.supervise(&mut last_probe);
            }

            // Cut frames up to (real time - latency), or on stop up to the stop instant using
            // whatever is buffered.
            let wall = (self.started.elapsed().as_secs_f64() * SAMPLE_RATE as f64) as u64;
            let target = if stopping {
                let at = stop_at.load(Ordering::SeqCst).min(wall);
                // Only as far as real audio reaches; never pad the tail with invented silence.
                let buffered = self.mic.fifo.len().max(self.sys.fifo.len()) as u64;
                at.min(emitted + buffered)
            } else {
                wall.saturating_sub(LATENCY)
            };
            while emitted + FRAME_SAMPLES as u64 <= target {
                self.mic.take_frame(&mut mic_frame);
                self.sys.take_frame(&mut sys_frame);
                if self.sys.stream.is_some() {
                    self.aec.process(&sys_frame, &mut mic_frame);
                }
                if emitted < self.mute_until {
                    mic_frame.fill(0.0);
                    sys_frame.fill(0.0);
                }
                for i in 0..FRAME_SAMPLES {
                    stereo[2 * i] = mic_frame[i];
                    stereo[2 * i + 1] = sys_frame[i];
                }
                if let Some(w) = self.writer.as_mut() {
                    w.write_interleaved(&stereo)?;
                }
                self.mic.meter.push(&mic_frame);
                self.sys.meter.push(&sys_frame);
                for (lane, frame) in [(&mut self.mic, &mic_frame), (&mut self.sys, &sys_frame)] {
                    if !lane.active {
                        continue;
                    }
                    if lane.chunk.is_empty() {
                        lane.chunk_start = emitted;
                    }
                    lane.chunk.extend_from_slice(frame);
                    if lane.chunk.len() >= FRAME_SAMPLES * CHUNK_FRAMES {
                        let samples = std::mem::replace(&mut lane.chunk, Vec::with_capacity(FRAME_SAMPLES * CHUNK_FRAMES));
                        let _ = self.events.send(CaptureEvent::Audio { kind: lane.kind, start: lane.chunk_start, samples });
                    }
                }
                emitted += FRAME_SAMPLES as u64;
            }

            // A fixed cadence (not "50 ms after the last one"), so sleep overshoot does not
            // slow the meter below 20 Hz; after a stall, skip ahead rather than burst.
            if last_level.elapsed() >= LEVEL_EVERY {
                last_level += LEVEL_EVERY;
                if last_level.elapsed() >= LEVEL_EVERY {
                    last_level = Instant::now();
                }
                let levels = AudioLevels { mic: self.mic.meter.take(), system: self.sys.meter.take() };
                let _ = self.events.send(CaptureEvent::Levels(levels));
            }

            if stopping {
                break;
            }
            std::thread::sleep(TICK);
        }

        // Hand over the last partial chunks.
        for lane in [&mut self.mic, &mut self.sys] {
            if lane.active && !lane.chunk.is_empty() {
                let samples = std::mem::take(&mut lane.chunk);
                let _ = self.events.send(CaptureEvent::Audio { kind: lane.kind, start: lane.chunk_start, samples });
            }
            lane.close();
        }
        let _ = self.events.send(CaptureEvent::Levels(AudioLevels { mic: 0.0, system: 0.0 }));
        let frames = match self.writer.take() {
            Some(w) => w.finish()?,
            None => 0,
        };
        Ok(CaptureSummary { frames, duration_ms: samples_to_ms(frames) })
    }

    /// Notice lost sources, reopen them, follow the default input and format changes.
    fn supervise(&mut self, last_probe: &mut Instant) {
        let now = Instant::now();
        let probe = now.duration_since(*last_probe) >= PROBE_EVERY;
        if probe {
            *last_probe = now;
        }
        let follow_default = self.follow_default;
        for lane in [&mut self.mic, &mut self.sys] {
            if lane.source.is_none() {
                continue;
            }
            // Lost?
            let mut reason = None;
            if let Some(stream) = lane.stream.as_ref() {
                if stream.status.is_failed() {
                    reason = Some(stream.status.message().unwrap_or_else(|| "the device stopped".into()));
                } else if probe
                    && let (Some(p), Some(rs)) = (stream.rate_probe.as_ref(), lane.resampler.as_ref())
                    && let Some(rate) = p()
                    && rate != rs.input_rate()
                {
                    tracing::info!(kind = ?lane.kind, rate, "source format changed; reopening");
                    lane.close();
                    lane.retry_at = Some(now);
                } else if probe && lane.kind == SourceKind::Mic && follow_default {
                    let current = default_input_id();
                    if current.is_some() && current != lane.opened_default {
                        tracing::info!(?current, "default input changed; switching");
                        lane.opened_default = current;
                        lane.close();
                        lane.retry_at = Some(now);
                    }
                }
            }
            if let Some(message) = reason {
                tracing::warn!(kind = ?lane.kind, %message, "capture source lost");
                lane.close();
                lane.retry_at = Some(now + RETRY_EVERY);
                let _ = self.events.send(CaptureEvent::SourceLost { kind: lane.kind, message });
            }
            // Reopen?
            if lane.stream.is_none() && lane.retry_at.is_some_and(|t| now >= t) {
                if lane.kind == SourceKind::Mic && follow_default {
                    lane.opened_default = default_input_id();
                }
                match lane.open() {
                    Ok(device_name) => {
                        let _ = self.events.send(CaptureEvent::SourceRestored { kind: lane.kind, device_name });
                    }
                    Err(_) => lane.retry_at = Some(now + RETRY_EVERY),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::CallbackSink;
    use std::sync::Mutex;

    /// A source that plays a fixed clip in real time from its own thread.
    struct FakeSource {
        kind: SourceKind,
        rate: u32,
        clip: Arc<Vec<f32>>,
        fail_after: Option<Duration>,
        opens: Arc<Mutex<u32>>,
    }

    struct FakeRun(Arc<AtomicBool>, Option<JoinHandle<()>>);
    impl Drop for FakeRun {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
            if let Some(h) = self.1.take() {
                let _ = h.join();
            }
        }
    }

    impl CaptureSource for FakeSource {
        fn kind(&self) -> SourceKind {
            self.kind
        }
        fn open(&self) -> AudioResult<RawStream> {
            *self.opens.lock().unwrap() += 1;
            let (mut sink, consumer, status) = CallbackSink::new();
            let stop = Arc::new(AtomicBool::new(false));
            let (clip, rate, fail_after, st) = (self.clip.clone(), self.rate, self.fail_after, stop.clone());
            let status2 = status.clone();
            let h = std::thread::spawn(move || {
                let started = Instant::now();
                let per_tick = rate as usize / 100;
                let mut pos = 0;
                while !st.load(Ordering::SeqCst) {
                    if fail_after.is_some_and(|d| started.elapsed() >= d) {
                        status2.fail("unplugged");
                        return;
                    }
                    let due = (started.elapsed().as_secs_f64() * rate as f64) as usize;
                    while pos + per_tick <= due {
                        let end = (pos + per_tick).min(clip.len());
                        let frame: Vec<f32> = if pos < clip.len() { clip[pos..end].to_vec() } else { vec![0.0; per_tick] };
                        sink.push_interleaved(&frame, 1, |v| v);
                        pos += per_tick;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            });
            Ok(RawStream {
                sample_rate: self.rate,
                consumer,
                status,
                device_name: format!("fake {:?}", self.kind),
                _guard: Box::new(FakeRun(stop, Some(h))),
                rate_probe: None,
            })
        }
    }

    fn tone(rate: u32, freq: f32, secs: f32) -> Arc<Vec<f32>> {
        Arc::new((0..(rate as f32 * secs) as usize).map(|i| 0.3 * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin()).collect())
    }

    #[test]
    fn records_both_channels_aligned_and_recovers_from_unplug() {
        let dir = std::env::temp_dir().join(format!("margin-session-test-{}", std::process::id()));
        let wav = dir.join("rec.wav");
        let mic_opens = Arc::new(Mutex::new(0));
        let config = CaptureConfig {
            mic: Box::new(FakeSource {
                kind: SourceKind::Mic,
                rate: 48_000,
                clip: tone(48_000, 300.0, 5.0),
                fail_after: Some(Duration::from_millis(600)),
                opens: mic_opens.clone(),
            }),
            system: Some(Box::new(FakeSource {
                kind: SourceKind::System,
                rate: 44_100,
                clip: tone(44_100, 700.0, 5.0),
                fail_after: None,
                opens: Arc::new(Mutex::new(0)),
            })),
            wav_path: wav.clone(),
            echo_cancel: false,
            follow_default_input: false,
            mute_start_ms: 100,
        };
        let (tx, rx) = crossbeam_channel::unbounded();
        let session = CaptureSession::start(config, tx).unwrap();
        std::thread::sleep(Duration::from_millis(3500));
        let summary = session.stop().unwrap();
        assert!((3200..=3600).contains(&summary.duration_ms), "duration {}", summary.duration_ms);

        let events: Vec<CaptureEvent> = rx.try_iter().collect();
        assert!(events.iter().any(|e| matches!(e, CaptureEvent::SourceLost { kind: SourceKind::Mic, .. })));
        assert!(events.iter().any(|e| matches!(e, CaptureEvent::SourceRestored { kind: SourceKind::Mic, .. })));
        assert!(*mic_opens.lock().unwrap() >= 2);
        assert!(events.iter().any(|e| matches!(e, CaptureEvent::Levels(l) if l.system > 0.5)));

        // Audio chunks tile the timeline per channel with no gaps.
        for kind in [SourceKind::Mic, SourceKind::System] {
            let mut next = 0;
            for e in &events {
                if let CaptureEvent::Audio { kind: k, start, samples } = e
                    && *k == kind
                {
                    assert_eq!(*start, next, "{kind:?} chunk starts where the last ended");
                    next += samples.len() as u64;
                }
            }
            assert_eq!(next, summary.frames, "{kind:?} delivered every frame");
        }

        let ch = crate::read_wav_16k(&wav).unwrap();
        assert_eq!(ch[0].len() as u64, summary.frames);
        // Muted start.
        assert!(ch[0][..1500].iter().all(|v| *v == 0.0) && ch[1][..1500].iter().all(|v| *v == 0.0));
        // System channel carries 700 Hz throughout; the mic has a silent gap while unplugged.
        let zc = |x: &[f32]| x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count() as f32 / (x.len() as f32 / 16_000.0);
        assert!((zc(&ch[1][4000..44_000]) - 700.0).abs() < 15.0);
        assert!((zc(&ch[0][2000..8000]) - 300.0).abs() < 15.0);
        let silent_run = ch[0].windows(8000).any(|w| w.iter().all(|v| *v == 0.0));
        assert!(silent_run, "the unplugged stretch is recorded as silence");
    }
}
