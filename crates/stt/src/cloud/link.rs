//! A cloud stream behind the blocking [`TranscriptStream`] contract.
//!
//! Each stream runs its connection on a thread of its own with a small Tokio runtime, so
//! `push` never waits for the network: it hands the audio over, then collects whatever results
//! have arrived since. `finish` signals the end of the audio and waits (bounded) for the last
//! results. Providers report times relative to the first audio they were sent; this layer puts
//! them on the recording timeline. A connection thread that dies without saying so (a panic)
//! counts as a failure, so the person is told rather than left with an empty transcript.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{SAMPLE_RATE, SttError, SttEvent, SttResult, TranscriptStream, Utterance};

/// Longest gap on the timeline filled with silence; longer gaps are filled too, but capped so a
/// clock jump cannot make us send minutes of zeros.
const MAX_GAP_FILL: u64 = SAMPLE_RATE as u64 * 30;
/// How long `finish` waits for the provider's last results.
const FINISH_TIMEOUT: Duration = Duration::from_secs(10);

/// Audio for the provider: 16 kHz mono 16-bit PCM, in order and without gaps.
pub(crate) enum Audio {
    Pcm(Vec<i16>),
    End,
}

/// From the connection thread to the stream.
enum Msg {
    Event(SttEvent),
    Failed(SttError),
    Done,
}

/// Lets a provider report results. Times are milliseconds from the first audio of the stream.
#[derive(Clone)]
pub(crate) struct Emitter {
    tx: mpsc::Sender<Msg>,
}

impl Emitter {
    /// An emitter whose results go nowhere (checking a key over a stream with no audio).
    pub fn discard() -> Self {
        let (tx, _rx) = mpsc::channel();
        Self { tx }
    }

    /// Text so far for an utterance in progress. `speaker` is the provider's label, if any.
    pub fn partial(&self, id: u64, text: &str, start_ms: u32, end_ms: u32, speaker: Option<&str>) {
        let _ = self.tx.send(Msg::Event(SttEvent::Partial(utterance(id, text, start_ms, end_ms, speaker))));
    }

    /// A final; empty text retracts the partial with the same id.
    pub fn final_(&self, id: u64, text: &str, start_ms: u32, end_ms: u32, speaker: Option<&str>) {
        let _ = self.tx.send(Msg::Event(SttEvent::Final(utterance(id, text, start_ms, end_ms, speaker))));
    }
}

fn utterance(id: u64, text: &str, start_ms: u32, end_ms: u32, speaker: Option<&str>) -> Utterance {
    Utterance { id, start_ms, end_ms, text: text.trim().to_string(), speaker: speaker.map(str::to_owned) }
}

pub(crate) struct LinkStream {
    audio: UnboundedSender<Audio>,
    rx: mpsc::Receiver<Msg>,
    /// Timeline sample of the first audio (set by the first push).
    start: Option<u64>,
    /// Timeline sample the next push should start at.
    next: u64,
    /// A failure not yet returned from `push`.
    failure: Option<SttError>,
    failed: bool,
    done: bool,
    thread: Option<JoinHandle<()>>,
}

impl LinkStream {
    /// Start the connection thread. `run` gets the audio and an emitter and returns when the
    /// audio has ended and the provider has sent its last results (or on a failure).
    pub fn spawn<F, Fut>(name: &str, run: F) -> SttResult<Self>
    where
        F: FnOnce(UnboundedReceiver<Audio>, Emitter) -> Fut + Send + 'static,
        Fut: Future<Output = SttResult<()>>,
    {
        let (audio, audio_rx) = unbounded_channel();
        let (tx, rx) = mpsc::channel();
        let emitter = Emitter { tx: tx.clone() };
        let thread = std::thread::Builder::new()
            .name(format!("cilica-stt-{name}"))
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(Msg::Failed(SttError::Provider(format!("Cilica could not start transcription: {e}"))));
                        let _ = tx.send(Msg::Done);
                        return;
                    }
                };
                match catch_unwind(AssertUnwindSafe(|| runtime.block_on(run(audio_rx, emitter)))) {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        let _ = tx.send(Msg::Failed(error));
                    }
                    Err(panic) => {
                        let detail = panic
                            .downcast_ref::<&str>()
                            .map(|s| s.to_string())
                            .or_else(|| panic.downcast_ref::<String>().cloned())
                            .unwrap_or_default();
                        tracing::error!(%detail, "the transcription connection crashed");
                        let _ = tx.send(Msg::Failed(crashed()));
                    }
                }
                let _ = tx.send(Msg::Done);
            })
            .map_err(|e| SttError::Provider(format!("Cilica could not start transcription: {e}")))?;
        Ok(Self { audio, rx, start: None, next: 0, failure: None, failed: false, done: false, thread: Some(thread) })
    }

    /// Collect what has arrived; with a deadline, wait for the end of the stream.
    fn drain(&mut self, until: Option<Instant>) -> Vec<SttEvent> {
        let offset = self.start.map(|s| (s * 1000 / SAMPLE_RATE as u64) as u32).unwrap_or(0);
        let mut out = Vec::new();
        loop {
            let msg = match until {
                None => match self.rx.try_recv() {
                    Ok(m) => m,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.lost();
                        break;
                    }
                },
                Some(deadline) => match self.rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(m) => m,
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        self.lost();
                        break;
                    }
                },
            };
            match msg {
                Msg::Event(e) => out.push(shift(e, offset)),
                Msg::Failed(e) => {
                    self.failed = true;
                    self.failure = Some(e);
                }
                Msg::Done => {
                    self.done = true;
                    break;
                }
            }
        }
        out
    }
}

impl LinkStream {
    /// The connection thread is gone without saying it was done.
    fn lost(&mut self) {
        if !self.done && !self.failed {
            self.failed = true;
            self.failure = Some(crashed());
        }
        self.done = true;
    }
}

/// What the person is told when the connection thread stops unexpectedly.
fn crashed() -> SttError {
    SttError::Provider("The connection to the transcription service stopped unexpectedly. The audio is still being recorded.".into())
}

fn shift(event: SttEvent, offset_ms: u32) -> SttEvent {
    let fix = |mut u: Utterance| {
        u.start_ms = u.start_ms.saturating_add(offset_ms);
        u.end_ms = u.end_ms.saturating_add(offset_ms);
        u
    };
    match event {
        SttEvent::Partial(u) => SttEvent::Partial(fix(u)),
        SttEvent::Final(u) => SttEvent::Final(fix(u)),
    }
}

/// f32 in [-1, 1] to 16-bit PCM.
pub(crate) fn to_pcm16(samples: &[f32]) -> Vec<i16> {
    samples.iter().map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).collect()
}

/// Little-endian bytes of 16-bit PCM.
pub(crate) fn pcm_bytes(pcm: &[i16]) -> Vec<u8> {
    pcm.iter().flat_map(|s| s.to_le_bytes()).collect()
}

impl TranscriptStream for LinkStream {
    fn push(&mut self, start_sample: u64, samples: &[f32]) -> SttResult<Vec<SttEvent>> {
        if self.start.is_none() {
            self.start = Some(start_sample);
            self.next = start_sample;
        }
        if !self.failed && !self.done {
            // Keep the provider's clock in step with ours: fill a gap with silence.
            if start_sample > self.next {
                let gap = (start_sample - self.next).min(MAX_GAP_FILL) as usize;
                let _ = self.audio.send(Audio::Pcm(vec![0; gap]));
            }
            let _ = self.audio.send(Audio::Pcm(to_pcm16(samples)));
        }
        self.next = self.next.max(start_sample + samples.len() as u64);
        let events = self.drain(None);
        match self.failure.take() {
            // Report a failure once; the pipeline keeps recording and stores what arrived.
            Some(error) if events.is_empty() => Err(error),
            Some(error) => {
                self.failure = Some(error);
                Ok(events)
            }
            None => Ok(events),
        }
    }

    fn finish(&mut self) -> SttResult<Vec<SttEvent>> {
        let _ = self.audio.send(Audio::End);
        let events = if self.done { self.drain(None) } else { self.drain(Some(Instant::now() + FINISH_TIMEOUT)) };
        if self.done
            && let Some(t) = self.thread.take()
        {
            let _ = t.join();
        }
        match self.failure.take() {
            Some(error) if events.is_empty() => Err(error),
            _ => Ok(events),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_land_on_the_recording_timeline_and_gaps_are_filled() {
        let mut stream = LinkStream::spawn("test", |mut audio, emitter| async move {
            let mut samples = 0usize;
            while let Some(a) = audio.recv().await {
                match a {
                    Audio::Pcm(pcm) => samples += pcm.len(),
                    Audio::End => break,
                }
            }
            // 1 s of audio, a 1 s gap, 1 s more: the provider heard 3 s.
            assert_eq!(samples, 48_000);
            emitter.final_(1, " hello ", 0, 3_000, None);
            Ok(())
        })
        .unwrap();
        let start = 160_000; // the stream started 10 s into the recording
        stream.push(start, &[0.1; 16_000]).unwrap();
        stream.push(start + 32_000, &[0.1; 16_000]).unwrap();
        let events = stream.finish().unwrap();
        assert_eq!(events, vec![SttEvent::Final(Utterance { id: 1, start_ms: 10_000, end_ms: 13_000, text: "hello".into(), speaker: None })]);
    }

    #[test]
    fn a_failure_is_reported_once() {
        let mut stream = LinkStream::spawn("test", |_audio, _emitter| async move { Err(SttError::Auth("Soniox".into())) }).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let first = stream.push(0, &[0.0; 1_600]);
        assert!(matches!(first, Err(SttError::Auth(_))));
        assert!(stream.push(1_600, &[0.0; 1_600]).unwrap().is_empty());
        assert!(stream.finish().unwrap().is_empty());
    }

    #[test]
    fn a_crashed_connection_is_reported() {
        let mut stream = LinkStream::spawn("test", |_audio, _emitter| async move { panic!("no crypto provider") }).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let first = stream.push(0, &[0.0; 1_600]);
        assert!(matches!(first, Err(SttError::Provider(ref m)) if m.contains("stopped unexpectedly")), "{first:?}");
        assert!(stream.push(1_600, &[0.0; 1_600]).unwrap().is_empty());
        assert!(stream.finish().unwrap().is_empty());
    }

    #[test]
    fn a_thread_that_vanishes_is_reported() {
        let mut stream = LinkStream::spawn("test", |_audio, _emitter| async move { Ok(()) }).unwrap();
        // A thread that ended without sending Done: its channel is simply closed.
        let (tx, rx) = mpsc::channel();
        drop(tx);
        stream.rx = rx;
        assert!(matches!(stream.push(0, &[0.0; 1_600]), Err(SttError::Provider(_))));
    }

    #[test]
    fn pcm_conversion_clamps() {
        assert_eq!(to_pcm16(&[0.0, 1.0, -1.0, 2.0]), vec![0, i16::MAX, -i16::MAX, i16::MAX]);
        assert_eq!(pcm_bytes(&[1, -2]), vec![1, 0, 254, 255]);
    }
}
