//! A [`CaptureSource`] that plays a fixed clip in real time from its own thread, as a device
//! would. For tests and demos of everything above capture: the recording pipeline uses it to run
//! whole sessions from fixture WAVs without a microphone.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::source::CallbackSink;
use crate::{AudioResult, CaptureSource, RawStream, SourceKind};

/// Plays `samples` (mono, at `sample_rate`) in real time, then silence for as long as it stays
/// open. Clones share playback state. Optionally "unplugs" itself once to exercise device loss.
#[derive(Clone)]
pub struct ClipSource {
    kind: SourceKind,
    sample_rate: u32,
    samples: Arc<Vec<f32>>,
    /// The first stream reports a failure this long after it opens.
    fail_after: Option<Duration>,
    opens: Arc<AtomicU32>,
    /// When playback started (the first open); a reopened stream resumes where real time is.
    started: Arc<Mutex<Option<Instant>>>,
}

impl ClipSource {
    pub fn new(kind: SourceKind, sample_rate: u32, samples: Vec<f32>) -> Self {
        Self {
            kind,
            sample_rate,
            samples: Arc::new(samples),
            fail_after: None,
            opens: Arc::new(AtomicU32::new(0)),
            started: Arc::new(Mutex::new(None)),
        }
    }

    /// Simulate the device being unplugged this long after it is first opened. Reopening works.
    pub fn failing_after(mut self, after: Duration) -> Self {
        self.fail_after = Some(after);
        self
    }

    /// How many times the source was opened (reopens after a failure count).
    pub fn opens(&self) -> u32 {
        self.opens.load(Ordering::SeqCst)
    }
}

/// Stops and joins the playback thread when the stream is dropped.
struct Run {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for Run {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl CaptureSource for ClipSource {
    fn kind(&self) -> SourceKind {
        self.kind
    }

    fn open(&self) -> AudioResult<RawStream> {
        let first = self.opens.fetch_add(1, Ordering::SeqCst) == 0;
        let opened = Instant::now();
        let started = *self.started.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert(opened);
        let fail_after = if first { self.fail_after } else { None };
        let (mut sink, consumer, status) = CallbackSink::new();
        let stop = Arc::new(AtomicBool::new(false));
        let (samples, rate, thread_stop, thread_status) = (self.samples.clone(), self.sample_rate, stop.clone(), status.clone());
        let handle = std::thread::Builder::new().name("margin-clip-source".into()).spawn(move || {
            let per_tick = (rate / 100) as usize;
            let due = || (started.elapsed().as_secs_f64() * rate as f64) as usize;
            // Resume on a 10 ms boundary of the shared timeline.
            let mut pos = due() / per_tick * per_tick;
            let mut frame = vec![0.0f32; per_tick];
            while !thread_stop.load(Ordering::SeqCst) {
                if fail_after.is_some_and(|d| opened.elapsed() >= d) {
                    thread_status.fail("the test device was unplugged");
                    return;
                }
                while pos + per_tick <= due() {
                    for (i, f) in frame.iter_mut().enumerate() {
                        *f = samples.get(pos + i).copied().unwrap_or(0.0);
                    }
                    sink.push_interleaved(&frame, 1, |v| v);
                    pos += per_tick;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        })?;
        Ok(RawStream {
            sample_rate: self.sample_rate,
            consumer,
            status,
            device_name: if self.kind == SourceKind::Mic { "Test microphone".into() } else { "Test system audio".into() },
            _guard: Box::new(Run { stop, handle: Some(handle) }),
            rate_probe: None,
        })
    }
}
