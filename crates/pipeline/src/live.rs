//! The capturing half of a session: the capture thread (cilica-audio), a router thread that turns
//! capture events into app events and feeds each channel to its transcription thread, the
//! transcription threads (blocking Whisper calls, never on the async runtime), and an async
//! task that stores segments and emits them.
//!
//! ```text
//! capture thread ──CaptureEvent──▶ router ──levels, toasts──▶ Events
//!                                     ├──mic audio──▶ stt (mic) ──┐
//!                                     └──sys audio──▶ stt (sys) ──┴─SttMsg─▶ persister ─▶ Db + Events
//! ```
//!
//! Stopping the capture closes the chain from the left, so every stage drains and finishes on
//! its own: no audio captured before the stop is lost.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use cilica_audio::{CaptureConfig, CaptureEvent, CaptureSession, CaptureSummary, SourceKind};
use cilica_domain::{
    AppError, AppEvent, AppResult, AudioChannel, DataScope, Events, Id, Segment, Settings, ToastAction, ToastKind, ToastRequest,
    new_id,
};
use cilica_store::{Db, NewSpeaker, OTHERS_LABEL};
use cilica_stt::{StreamOptions, SttError, SttEvent, Transcriber};
use tokio::sync::mpsc;

use crate::backend::{AudioBackend, SpeechBackend};
use crate::echo::EchoFilter;
use crate::files::RecordingFiles;
use crate::recognise::{Recogniser, VoiceTap};

/// One channel's audio on the recording timeline: (first sample index, 16 kHz mono samples).
type Chunk = (u64, Vec<f32>);

/// A transcription result on its way to the store.
struct SttMsg {
    channel: AudioChannel,
    event: SttEvent,
}

/// Everything a live session needs to start.
pub(crate) struct LiveParams {
    pub db: Db,
    pub events: Events,
    pub audio: Arc<dyn AudioBackend>,
    pub speech: Arc<dyn SpeechBackend>,
    pub settings: Settings,
    pub note_id: Id,
    pub you: Id,
    pub others: Id,
    pub offset_ms: u32,
    pub files: RecordingFiles,
    pub mute_start_ms: u32,
    pub echo_cancel: bool,
    /// Capture system audio (false when its permission is denied).
    pub with_system: bool,
    /// Called (from the router thread) if capture ends without being asked to, for example when
    /// the disk fills up.
    pub on_capture_failed: Box<dyn FnOnce(String) + Send>,
}

/// A running capture.
pub(crate) struct LiveSession {
    capture: CaptureSession,
    stopping: Arc<AtomicBool>,
    draining: Draining,
}

/// The stages that keep running after capture stops, until the transcript is complete.
pub(crate) struct Draining {
    router: Option<JoinHandle<()>>,
    stt: Option<JoinHandle<()>>,
    persister: Option<tokio::task::JoinHandle<()>>,
}

impl Draining {
    /// Wait until every captured sample is transcribed and stored.
    pub async fn finish(mut self) {
        let threads: Vec<JoinHandle<()>> = [self.router.take(), self.stt.take()].into_iter().flatten().collect();
        let _ = tokio::task::spawn_blocking(move || {
            for t in threads {
                let _ = t.join();
            }
        })
        .await;
        if let Some(p) = self.persister.take() {
            let _ = p.await;
        }
    }
}

impl LiveSession {
    /// Open the devices and start every stage. Blocking (opening devices takes a moment): call
    /// it from a blocking thread. Must be called inside a Tokio runtime.
    pub fn start(p: LiveParams, rt: tokio::runtime::Handle) -> AppResult<Self> {
        let (capture_tx, capture_rx) = crossbeam_channel::unbounded::<CaptureEvent>();
        let config = CaptureConfig {
            mic: p.audio.mic(p.settings.recording.input_device_id.clone()),
            system: p.with_system.then(|| p.audio.system()),
            wav_path: p.files.wav(),
            echo_cancel: p.echo_cancel,
            follow_default_input: p.settings.recording.input_device_id.is_none(),
            mute_start_ms: p.mute_start_ms,
        };
        let capture = CaptureSession::start(config, capture_tx).map_err(AppError::from)?;

        let (mic_tx, mic_rx) = crossbeam_channel::unbounded::<Chunk>();
        let (sys_tx, sys_rx) = crossbeam_channel::unbounded::<Chunk>();
        let (stt_tx, stt_rx) = mpsc::unbounded_channel::<SttMsg>();
        let stopping = Arc::new(AtomicBool::new(false));

        // Saved voices are recognised live only for speakers the service labels.
        let t = &p.settings.transcription;
        let tap = (t.recognise_voices && t.live_speakers()).then(|| Arc::new(VoiceTap::default()));
        let recogniser = tap.clone().map(|tap| Recogniser::start(p.db.clone(), p.events.clone(), p.speech.clone(), p.note_id, tap, &rt));
        let persister = rt.spawn(persist(p.db.clone(), p.events.clone(), p.note_id, p.you, p.others, p.offset_ms, recogniser, stt_rx));
        let stt = spawn_stt(p.speech.clone(), p.settings.clone(), p.events.clone(), mic_rx, sys_rx, stt_tx, tap)?;
        let router = {
            let (events, stopping, on_failed) = (p.events.clone(), stopping.clone(), p.on_capture_failed);
            std::thread::Builder::new()
                .name("cilica-router".into())
                .spawn(move || route(capture_rx, mic_tx, sys_tx, events, stopping, on_failed))
                .map_err(|e| AppError::internal(format!("Cilica could not start recording: {e}")))?
        };
        Ok(Self { capture, stopping, draining: Draining { router: Some(router), stt: Some(stt), persister: Some(persister) } })
    }

    /// Stop capturing now and finish the audio file. Blocking (joins the capture thread). The
    /// returned [`Draining`] completes once the transcript has caught up.
    pub fn stop(self) -> (Result<CaptureSummary, String>, Draining) {
        self.stopping.store(true, Ordering::SeqCst);
        let summary = self.capture.stop().map_err(|e| e.to_string());
        (summary, self.draining)
    }
}

fn toast(events: &Events, kind: ToastKind, title: &str, description: impl Into<String>, action: Option<(&str, ToastAction)>) {
    let (action_label, action) = match action {
        Some((label, a)) => (Some(label.to_string()), Some(a)),
        None => (None, None),
    };
    events.emit(AppEvent::Toast {
        toast: ToastRequest { kind, title: title.into(), description: Some(description.into()), action_label, action },
    });
}

/// The router thread: levels and device changes become app events; audio goes to transcription.
fn route(
    rx: Receiver<CaptureEvent>,
    mic: Sender<Chunk>,
    sys: Sender<Chunk>,
    events: Events,
    stopping: Arc<AtomicBool>,
    on_failed: Box<dyn FnOnce(String) + Send>,
) {
    for event in rx.iter() {
        match event {
            CaptureEvent::Audio { kind: SourceKind::Mic, start, samples } => {
                let _ = mic.send((start, samples));
            }
            CaptureEvent::Audio { kind: SourceKind::System, start, samples } => {
                let _ = sys.send((start, samples));
            }
            CaptureEvent::Levels(levels) => events.emit(AppEvent::AudioLevels { levels }),
            CaptureEvent::SourceLost { kind: SourceKind::Mic, message } => {
                tracing::warn!(%message, "microphone lost");
                toast(&events, ToastKind::Error, "Microphone disconnected", "Cilica keeps recording the call audio and picks the microphone up again as soon as it is back.", None);
            }
            CaptureEvent::SourceLost { kind: SourceKind::System, message } => {
                tracing::warn!(%message, "system audio lost");
                toast(&events, ToastKind::Error, "System audio stopped", "Cilica keeps recording your microphone and reconnects to the call audio as soon as it can.", None);
            }
            CaptureEvent::SourceRestored { kind, device_name } => {
                let title = if kind == SourceKind::Mic { "Microphone back" } else { "System audio back" };
                toast(&events, ToastKind::Info, title, format!("Recording from {device_name} again."), None);
            }
            CaptureEvent::SourceUnavailable { kind: SourceKind::System, message, permission_denied } => {
                tracing::warn!(%message, permission_denied, "recording without system audio");
                system_audio_missing_toast(&events, permission_denied, &message);
            }
            CaptureEvent::SourceUnavailable { kind: SourceKind::Mic, message, .. } => {
                toast(&events, ToastKind::Error, "Microphone unavailable", message, None);
            }
        }
    }
    // The capture thread is gone. If nobody asked it to stop, it failed.
    if !stopping.load(Ordering::SeqCst) {
        on_failed("the audio file could not be written".into());
    }
}

/// "Recording your microphone only", with what to do about it.
pub(crate) fn system_audio_missing_toast(events: &Events, permission_denied: bool, detail: &str) {
    if permission_denied {
        toast(
            events,
            ToastKind::Error,
            "Recording your microphone only",
            "Cilica can't hear the other people on the call. Allow Cilica in System Settings › Privacy & Security › Screen & System Audio Recording, then start a new recording.",
            Some(("Open settings", ToastAction::OpenSettings { page: "recording".into() })),
        );
    } else {
        toast(events, ToastKind::Error, "Recording your microphone only", format!("Cilica can't capture the call audio: {detail}"), None);
    }
}

/// Warnings shown at most once per recording: both channels usually hit the same problem (a
/// refused key fails the mic and the call audio alike).
#[derive(Default)]
struct Notices(Mutex<HashSet<&'static str>>);

impl Notices {
    /// True the first time `title` is asked for.
    fn first(&self, title: &'static str) -> bool {
        self.0.lock().map(|mut shown| shown.insert(title)).unwrap_or(false)
    }
}

/// Load the engine (on its own thread, so capture starts at once and audio queues meanwhile),
/// then run one transcription thread per channel. The returned thread ends when both have
/// drained.
fn spawn_stt(
    speech: Arc<dyn SpeechBackend>,
    settings: Settings,
    events: Events,
    mic: Receiver<Chunk>,
    sys: Receiver<Chunk>,
    out: mpsc::UnboundedSender<SttMsg>,
    tap: Option<Arc<VoiceTap>>,
) -> AppResult<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("cilica-stt".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let engine = match speech.transcriber(&settings) {
                Ok(e) => {
                    tracing::info!(took = ?started.elapsed(), "speech engine ready");
                    Some(e)
                }
                Err(error) => {
                    tracing::warn!(message = %error.message, "speech engine unavailable; recording audio only");
                    toast(&events, ToastKind::Error, "Live transcript is off", error.message, None);
                    None
                }
            };
            let language = settings.transcription.language.clone();
            let notices = Arc::new(Notices::default());
            let workers: Vec<JoinHandle<()>> = [(AudioChannel::Mic, mic), (AudioChannel::System, sys)]
                .into_iter()
                .filter_map(|(channel, rx)| {
                    let (engine, out, events, language, notices, tap) = (engine.clone(), out.clone(), events.clone(), language.clone(), notices.clone(), tap.clone());
                    std::thread::Builder::new()
                        .name(format!("cilica-stt-{channel:?}").to_lowercase())
                        .spawn(move || transcribe_channel(engine, channel, language, rx, out, events, &notices, tap.as_deref()))
                        .ok()
                })
                .collect();
            drop(out);
            for w in workers {
                let _ = w.join();
            }
        })
        .map_err(|e| AppError::internal(format!("Cilica could not start transcription: {e}")))
}

/// One channel: gather whatever audio is queued (so a backlog is decoded in fewer, larger
/// steps), push it through the stream, forward results; finish when the channel closes.
fn transcribe_channel(
    engine: Option<Arc<dyn Transcriber>>,
    channel: AudioChannel,
    language: Option<String>,
    rx: Receiver<Chunk>,
    out: mpsc::UnboundedSender<SttMsg>,
    events: Events,
    notices: &Notices,
    tap: Option<&VoiceTap>,
) {
    let Some(engine) = engine else {
        // No engine: keep the queue empty until capture ends.
        for _ in rx.iter() {}
        return;
    };
    let mut stream = match engine.open_stream(StreamOptions { channel: Some(channel), language }) {
        Ok(s) => s,
        Err(error) => {
            if notices.first("Live transcript is off") {
                toast(&events, ToastKind::Error, "Live transcript is off", AppError::from(error).message, None);
            }
            for _ in rx.iter() {}
            return;
        }
    };
    let send = |evs: Vec<SttEvent>| {
        for event in evs {
            let _ = out.send(SttMsg { channel, event });
        }
    };
    let mut failures = 0u32;
    while let Ok((mut start, mut buf)) = rx.recv() {
        // Merge contiguous queued chunks; a gap on the timeline flushes what we have.
        while let Ok((s, samples)) = rx.try_recv() {
            if s == start + buf.len() as u64 {
                buf.extend_from_slice(&samples);
            } else {
                if let Some(tap) = tap {
                    tap.push(channel, start, &buf);
                }
                match stream.push(start, &buf) {
                    Ok(evs) => send(evs),
                    Err(error) => tracing::warn!(%error, ?channel, "transcription failed for a stretch of audio"),
                }
                (start, buf) = (s, samples);
            }
        }
        if let Some(tap) = tap {
            tap.push(channel, start, &buf);
        }
        match stream.push(start, &buf) {
            Ok(evs) => send(evs),
            Err(error) => {
                failures += 1;
                tracing::warn!(%error, ?channel, "transcription failed for a stretch of audio");
                // A cloud service that rejects the key or cannot be reached: say so at once.
                let cloud = matches!(error, SttError::Auth(_) | SttError::NotAllowed(_) | SttError::MissingKey(_) | SttError::Network(_) | SttError::Provider(_));
                if failures == 1 && cloud {
                    if notices.first("Live transcript stopped") {
                        toast(
                            &events,
                            ToastKind::Error,
                            "Live transcript stopped",
                            AppError::from(error).message,
                            Some(("Open settings", ToastAction::OpenSettings { page: "transcription".into() })),
                        );
                    }
                } else if failures == 3 && notices.first("Transcription is struggling") {
                    toast(&events, ToastKind::Error, "Transcription is struggling", "Some of this meeting may be missing from the live transcript. The audio is still being recorded.", None);
                }
            }
        }
    }
    match stream.finish() {
        Ok(evs) => send(evs),
        Err(error) => tracing::warn!(%error, ?channel, "could not finish the transcript"),
    }
}

/// The people a cloud service told apart, by channel and its label ("1", "A"), as the note's
/// speakers. Labels are per stream, so "1" on the mic is not "1" on the call. The first voice on
/// the microphone is "You"; everyone else is "Person N", numbered after the ones the note has.
struct LiveSpeakers {
    by_label: HashMap<(AudioChannel, String), Id>,
    next: usize,
    mic_has_you: bool,
}

impl LiveSpeakers {
    async fn new(db: &Db, note_id: Id) -> Self {
        let existing = db.list_speakers(note_id).await.map(|all| all.iter().filter(|s| !s.is_self && s.label != OTHERS_LABEL).count()).unwrap_or(0);
        Self { by_label: HashMap::new(), next: existing + 1, mic_has_you: false }
    }

    /// The note's speaker for this label, created the first time it is heard.
    async fn speaker(&mut self, db: &Db, events: &Events, note_id: Id, channel: AudioChannel, label: &str, you: Id) -> Option<Id> {
        let key = (channel, label.to_string());
        if let Some(id) = self.by_label.get(&key) {
            return Some(*id);
        }
        // The first voice on the microphone is you; any other is someone in the room (or audio
        // from another device). The check after the meeting can still change who is who.
        if channel == AudioChannel::Mic && !self.mic_has_you {
            self.mic_has_you = true;
            self.by_label.insert(key, you);
            return Some(you);
        }
        let n = self.next;
        let new = NewSpeaker { label: format!("Person {n}"), color_index: crate::finalize::person_color(n), is_self: false, embedding: None };
        match db.create_speaker(note_id, new).await {
            Ok(speaker) => {
                self.next += 1;
                self.by_label.insert(key, speaker.id);
                events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Transcript], note_id: Some(note_id) });
                Some(speaker.id)
            }
            Err(error) => {
                tracing::warn!(%error, "could not add a speaker the service heard");
                None
            }
        }
    }
}

/// Store and emit segments as they arrive. Partials are upserted in place by id; a final
/// replaces its partial; an empty final or echo retracts it. Audio the service labelled by speaker
/// goes to that person; the rest is "You" (mic) or "Others" (call).
#[allow(clippy::too_many_arguments)]
async fn persist(
    db: Db,
    events: Events,
    note_id: Id,
    you: Id,
    others: Id,
    offset_ms: u32,
    mut recogniser: Option<Recogniser>,
    mut rx: mpsc::UnboundedReceiver<SttMsg>,
) {
    let mut live_speakers = LiveSpeakers::new(&db, note_id).await;
    let mut ids: HashMap<(AudioChannel, u64), Id> = HashMap::new();
    let mut echo = EchoFilter::default();
    let retract = async |db: &Db, id: Id| {
        if let Err(error) = db.delete_segment(id).await {
            tracing::warn!(%error, "could not remove a transcript segment");
        }
        events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Transcript], note_id: Some(note_id) });
    };
    while let Some(SttMsg { channel, event }) = rx.recv().await {
        let (u, is_final) = match event {
            SttEvent::Partial(u) => (u, false),
            SttEvent::Final(u) => (u, true),
        };
        let key = (channel, u.id);
        let known = ids.get(&key).copied();
        let id = if is_final { ids.remove(&key) } else { known }.unwrap_or_else(new_id);
        if !is_final {
            ids.insert(key, id);
        }
        let (start_ms, end_ms) = (u.start_ms.saturating_add(offset_ms), u.end_ms.saturating_add(offset_ms));
        if is_final {
            if u.text.trim().is_empty() {
                if known.is_some() {
                    retract(&db, id).await;
                }
                continue;
            }
            match echo.on_final(channel, id, start_ms, end_ms, &u.text) {
                Some(drop) if drop == id => {
                    tracing::debug!(text = %u.text, "dropped mic echo of the call audio");
                    if known.is_some() {
                        retract(&db, id).await;
                    }
                    continue;
                }
                Some(drop) => {
                    tracing::debug!("dropped an earlier mic segment that echoed the call audio");
                    retract(&db, drop).await;
                }
                None => {}
            }
        }
        let fallback = if channel == AudioChannel::Mic { you } else { others };
        let speaker_id = match u.speaker.as_deref() {
            None => fallback,
            Some(label) => live_speakers.speaker(&db, &events, note_id, channel, label, you).await.unwrap_or(fallback),
        };
        let segment = Segment {
            id,
            note_id,
            speaker_id: Some(speaker_id),
            channel,
            start_ms,
            end_ms,
            text: u.text.trim().to_string(),
            is_final,
        };
        match db.upsert_segment(&segment).await {
            Ok(()) => events.emit(AppEvent::TranscriptSegment { segment }),
            Err(error) => tracing::warn!(%error, "could not store a transcript segment"),
        }
        // A finished line by someone the service labelled: listen for a voice Cilica knows.
        if is_final
            && u.speaker.is_some()
            && speaker_id != you
            && let Some(r) = recogniser.as_mut()
        {
            r.heard(speaker_id, channel, u.start_ms, u.end_ms);
        }
    }
    if let Some(r) = recogniser {
        r.finish().await;
    }
}
