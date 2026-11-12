//! The [`CaptureSource`] trait and the lock-free handoff between an OS audio callback and the
//! capture thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use ringbuf::traits::{Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

use crate::AudioResult;

/// Which stream a source captures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    Mic,
    System,
}

/// Shared between an OS callback and the capture thread. Only atomics are touched on the audio
/// thread; the message mutex is only taken on the (rare) error path.
#[derive(Debug, Default)]
pub struct SourceStatus {
    failed: AtomicBool,
    message: Mutex<Option<String>>,
    dropped: AtomicU64,
}

impl SourceStatus {
    /// Mark the stream as broken (device unplugged, server gone). Called from error callbacks.
    pub fn fail(&self, message: impl Into<String>) {
        if let Ok(mut m) = self.message.try_lock() {
            *m = Some(message.into());
        }
        self.failed.store(true, Ordering::Release);
    }
    pub fn is_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }
    pub fn message(&self) -> Option<String> {
        self.message.lock().ok().and_then(|m| m.clone())
    }
    /// Samples lost because the capture thread fell behind.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// A running capture: mono samples at `sample_rate` arrive in `consumer`. Dropping it stops the
/// OS stream. Not `Send`: it is opened and dropped on the capture thread.
pub struct RawStream {
    pub sample_rate: u32,
    pub consumer: HeapCons<f32>,
    pub status: Arc<SourceStatus>,
    /// A human-readable name of what is being captured ("MacBook Pro Microphone").
    pub device_name: String,
    /// Keeps the platform stream alive.
    pub(crate) _guard: Box<dyn std::any::Any>,
    /// Re-reads the stream's current native rate, for sources whose format can change under a
    /// running stream (the macOS tap follows the output device). A change reopens the stream.
    pub(crate) rate_probe: Option<Box<dyn Fn() -> Option<u32>>>,
}

/// One kind of audio input on one platform. Cheap configuration only; [`open`](Self::open)
/// starts the OS stream on the calling thread.
pub trait CaptureSource: Send {
    fn kind(&self) -> SourceKind;
    /// Start capturing. Errors say what the person should do.
    fn open(&self) -> AudioResult<RawStream>;
}

/// Ring buffer size: two seconds at 48 kHz, so a stalled consumer never blocks the callback.
pub(crate) const RING_CAPACITY: usize = 48_000 * 2;

/// The producer half handed to an OS callback, with the downmix scratch it needs so the callback
/// never allocates.
pub(crate) struct CallbackSink {
    producer: HeapProd<f32>,
    scratch: Vec<f32>,
    pub(crate) status: Arc<SourceStatus>,
}

impl CallbackSink {
    pub(crate) fn new() -> (Self, HeapCons<f32>, Arc<SourceStatus>) {
        let (producer, consumer) = HeapRb::<f32>::new(RING_CAPACITY).split();
        let status = Arc::new(SourceStatus::default());
        (Self { producer, scratch: vec![0.0; 8192], status: status.clone() }, consumer, status)
    }

    /// Push interleaved samples of `channels` channels, downmixed to mono. Real-time safe.
    pub(crate) fn push_interleaved<T: Copy>(&mut self, data: &[T], channels: usize, to_f32: impl Fn(T) -> f32) {
        let channels = channels.max(1);
        let frames = data.len() / channels;
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(self.scratch.len());
            for i in 0..n {
                let base = (done + i) * channels;
                let mut acc = 0.0f32;
                for c in 0..channels {
                    acc += to_f32(data[base + c]);
                }
                self.scratch[i] = acc / channels as f32;
            }
            let pushed = self.producer.push_slice(&self.scratch[..n]);
            if pushed < n {
                self.status.dropped.fetch_add((n - pushed) as u64, Ordering::Relaxed);
            }
            done += n;
        }
    }
}
