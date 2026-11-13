//! The capturing half of a session: the capture thread (cilica-audio), a router thread that turns
//! capture events into app events and feeds each channel to its transcription thread, the
//! transcription threads (blocking Whisper calls, never on the async runtime), and an async
//! task that stores segments and emits them.
//!
//! ```text
//! capture thread ──CaptureEvent──▶ router ──levels, toasts──▶ Events
//!                                     ├──mic audio──▶ stt (mic) ──────┐
//!                                     ├──sys audio──▶ stt (sys) ──────┤
//!                                     ├──mic audio──▶ speakers (mic) ─┤  (who is speaking, on
//!                                     └──sys audio──▶ speakers (sys) ─┴─Msg─▶ persister ─▶ Db + Events
//! ```
//!
//! Speakers are told apart on this computer by default ([`SpeakerSource::Local`]): a line is
//! stored without a speaker ("Pending" in the transcript) until the speaker detection has heard
//! enough of it, a few seconds behind the words, and then given to that person.
//!
//! Stopping the capture closes the chain from the left, so every stage drains and finishes on
//! its own: no audio captured before the stop is lost.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use cilica_audio::{CaptureConfig, CaptureEvent, CaptureSession, CaptureSummary, SourceKind};
use cilica_diarize::online::{Label, Span};
use cilica_domain::{
    AppError, AppEvent, AppResult, AudioChannel, DataScope, Events, Id, Segment, Settings, SpeakerSource, ToastAction, ToastKind,
    ToastRequest, new_id,
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

/// What the persister hears: transcription results, and who is speaking.
enum Msg {
    Stt { channel: AudioChannel, event: SttEvent },
    Speakers { channel: AudioChannel, update: SpeakerUpdate },
}

/// News from one channel's live speaker detection.
enum SpeakerUpdate {
    /// Newly decided stretches, in order, on the capture timeline.
    Spans(Vec<Span>),
    /// Speakers found to be someone already heard: (from, into).
    Merged(Vec<(usize, usize)>),
    /// It could not run: lines fall back to "You" (mic) and "Others" (call).
    Unavailable,
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
    speakers: Option<JoinHandle<()>>,
    persister: Option<tokio::task::JoinHandle<()>>,
}

impl Draining {
    /// Wait until every captured sample is transcribed and stored.
    pub async fn finish(mut self) {
        let threads: Vec<JoinHandle<()>> = [self.router.take(), self.stt.take(), self.speakers.take()].into_iter().flatten().collect();
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
        let (msg_tx, msg_rx) = mpsc::unbounded_channel::<Msg>();
        let stopping = Arc::new(AtomicBool::new(false));

        let t = &p.settings.transcription;
        let labels = t.speaker_labels();
        // Saved voices are recognised live for every speaker someone labelled.
        let tap = (t.recognise_voices && labels != SpeakerSource::Off).then(|| Arc::new(VoiceTap::default()));
        let recogniser = tap.clone().map(|tap| Recogniser::start(p.db.clone(), p.events.clone(), p.speech.clone(), p.note_id, tap, &rt));
        let persister = rt.spawn(persist(Persist {
            db: p.db.clone(),
            events: p.events.clone(),
            note_id: p.note_id,
            you: p.you,
            others: p.others,
            offset_ms: p.offset_ms,
            labels,
            recogniser,
            rx: msg_rx,
        }));
        let (mut mic_out, mut sys_out) = (vec![mic_tx], vec![sys_tx]);
        let speakers = if labels == SpeakerSource::Local {
            let (mic_tx, mic_rx) = crossbeam_channel::unbounded::<Chunk>();
            let (sys_tx, sys_rx) = crossbeam_channel::unbounded::<Chunk>();
            mic_out.push(mic_tx);
            sys_out.push(sys_tx);
            Some(spawn_speakers(p.speech.clone(), t.speaker_model.clone(), p.events.clone(), rt.clone(), mic_rx, sys_rx, msg_tx.clone())?)
        } else {
            None
        };
        let stt = spawn_stt(p.speech.clone(), p.settings.clone(), p.events.clone(), mic_rx, sys_rx, msg_tx, tap)?;
        let router = {
            let (events, stopping, on_failed) = (p.events.clone(), stopping.clone(), p.on_capture_failed);
            std::thread::Builder::new()
                .name("cilica-router".into())
                .spawn(move || route(capture_rx, mic_out, sys_out, events, stopping, on_failed))
                .map_err(|e| AppError::internal(format!("Cilica could not start recording: {e}")))?
        };
        Ok(Self { capture, stopping, draining: Draining { router: Some(router), stt: Some(stt), speakers, persister: Some(persister) } })
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

/// Hand a chunk to every consumer of its channel (transcription, then speaker detection).
fn fan_out(to: &[Sender<Chunk>], start: u64, samples: Vec<f32>) {
    if let Some((last, rest)) = to.split_last() {
        for tx in rest {
            let _ = tx.send((start, samples.clone()));
        }
        let _ = last.send((start, samples));
    }
}

/// The router thread: levels and device changes become app events; audio goes to transcription
/// and speaker detection.
fn route(
    rx: Receiver<CaptureEvent>,
    mic: Vec<Sender<Chunk>>,
    sys: Vec<Sender<Chunk>>,
    events: Events,
    stopping: Arc<AtomicBool>,
    on_failed: Box<dyn FnOnce(String) + Send>,
) {
    for event in rx.iter() {
        match event {
            CaptureEvent::Audio { kind: SourceKind::Mic, start, samples } => fan_out(&mic, start, samples),
            CaptureEvent::Audio { kind: SourceKind::System, start, samples } => fan_out(&sys, start, samples),
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
    out: mpsc::UnboundedSender<Msg>,
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
    out: mpsc::UnboundedSender<Msg>,
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
            let _ = out.send(Msg::Stt { channel, event });
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

// ── Speakers ─────────────────────────────────────────────────────────────

/// Load the voice models (fetching the default ones on first use), then tell speakers apart on
/// each channel on its own thread. The returned thread ends when both channels have drained.
#[allow(clippy::too_many_arguments)]
fn spawn_speakers(
    speech: Arc<dyn SpeechBackend>,
    model_id: String,
    events: Events,
    rt: tokio::runtime::Handle,
    mic: Receiver<Chunk>,
    sys: Receiver<Chunk>,
    out: mpsc::UnboundedSender<Msg>,
) -> AppResult<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("cilica-speakers".into())
        .spawn(move || {
            let models = rt.block_on(speech.live_speaker_models(&model_id));
            let workers: Vec<JoinHandle<()>> = [(AudioChannel::Mic, mic), (AudioChannel::System, sys)]
                .into_iter()
                .filter_map(|(channel, rx)| {
                    let diarizer = match &models {
                        Some(m) => speech.online_diarizer(m).map_err(|e| e.message),
                        None => Err("Cilica's voice model could not be downloaded; check your connection.".to_string()),
                    };
                    let out = out.clone();
                    let diarizer = match diarizer {
                        Ok(d) => d,
                        Err(message) => {
                            tracing::warn!(%message, ?channel, "live speaker detection is off");
                            if channel == AudioChannel::Mic {
                                toast(
                                    &events,
                                    ToastKind::Error,
                                    "Speakers not labelled",
                                    format!("{message} This meeting's transcript shows You and Others instead."),
                                    None,
                                );
                            }
                            let _ = out.send(Msg::Speakers { channel, update: SpeakerUpdate::Unavailable });
                            // Keep the queue empty until capture ends.
                            return std::thread::Builder::new().name("cilica-speakers-idle".into()).spawn(move || for _ in rx.iter() {}).ok();
                        }
                    };
                    std::thread::Builder::new()
                        .name(format!("cilica-speakers-{channel:?}").to_lowercase())
                        .spawn(move || label_channel(diarizer, channel, rx, out))
                        .ok()
                })
                .collect();
            drop(out);
            for w in workers {
                let _ = w.join();
            }
        })
        .map_err(|e| AppError::internal(format!("Cilica could not start telling speakers apart: {e}")))
}

/// One channel's speaker detection: feed whatever audio is queued, forward what it decides.
fn label_channel(mut d: cilica_diarize::online::OnlineDiarizer, channel: AudioChannel, rx: Receiver<Chunk>, out: mpsc::UnboundedSender<Msg>) {
    let send = |update| {
        let _ = out.send(Msg::Speakers { channel, update });
    };
    // The next sample the detection expects on the capture timeline.
    let mut next = 0u64;
    while let Ok((start, samples)) = rx.recv() {
        let mut spans = Vec::new();
        if start > next {
            spans.extend(d.skip((start - next) as usize));
            next = start;
        }
        let from = ((next - start) as usize).min(samples.len());
        match d.push(&samples[from..]) {
            Ok(s) => spans.extend(s),
            Err(error) => {
                tracing::warn!(%error, ?channel, "live speaker detection failed; lines fall back to You and Others");
                send(SpeakerUpdate::Spans(spans));
                send(SpeakerUpdate::Unavailable);
                for _ in rx.iter() {}
                return;
            }
        }
        next = start + samples.len() as u64;
        let merges = d.take_merges();
        if !merges.is_empty() {
            send(SpeakerUpdate::Merged(merges));
        }
        if !spans.is_empty() {
            send(SpeakerUpdate::Spans(spans));
        }
    }
    send(SpeakerUpdate::Spans(d.finish()));
}

/// The people told apart on each channel, by channel and label ("1" from a service, "local 0"
/// from Cilica), as the note's speakers. Labels are per stream, so "1" on the mic is not "1" on
/// the call. The first voice on the microphone is "You"; everyone else is "Person N", numbered
/// after the ones the note has.
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
        // from another device).
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
                tracing::warn!(%error, "could not add a speaker");
                None
            }
        }
    }

    /// Cilica found that `from` is someone already heard as `into`: their lines move to one
    /// speaker ("You" or a named one is kept over a "Person N").
    async fn merge(&mut self, db: &Db, events: &Events, note_id: Id, channel: AudioChannel, from: &str, into: &str) {
        let (from_key, into_key) = ((channel, from.to_string()), (channel, into.to_string()));
        let Some(gone) = self.by_label.remove(&from_key) else { return };
        let Some(&kept) = self.by_label.get(&into_key) else {
            // Nothing was given to `into` yet: it simply carries on as `from`'s speaker.
            self.by_label.insert(into_key, gone);
            return;
        };
        if gone == kept {
            return;
        }
        match db.merge_speakers(gone, kept).await {
            Ok(survivor) => {
                for id in self.by_label.values_mut() {
                    if *id == gone || *id == kept {
                        *id = survivor;
                    }
                }
                events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Transcript], note_id: Some(note_id) });
            }
            Err(error) => {
                tracing::warn!(%error, "could not join two speakers");
                self.by_label.insert(from_key, gone);
            }
        }
    }
}

/// Within this long of a line, the speaker heard just before or after it is taken as theirs when
/// Cilica could not tell who said the line itself.
const NEIGHBOUR_MS: u32 = 2_000;
/// A line still undecided once Cilica has heard this much after it goes to the nearest speaker.
const GIVE_UP_MS: u32 = 10_000;

/// What speaker detection decided for one channel so far.
#[derive(Default)]
struct Track {
    /// Decided stretches in time order, on the capture timeline.
    spans: Vec<Span>,
    /// Everything before this is decided.
    horizon_ms: u32,
    /// Speakers folded into another: from → into.
    alias: HashMap<usize, usize>,
    unavailable: bool,
}

/// Who said a line, as far as speaker detection can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Speaker(usize),
    /// Nobody can tell: "You" on the mic, "Others" on the call.
    Fallback,
    /// Not decided yet: "Pending".
    Wait,
}

impl Track {
    fn resolve(&self, mut c: usize) -> usize {
        while let Some(&next) = self.alias.get(&c) {
            if next == c {
                break;
            }
            c = next;
        }
        c
    }

    fn add(&mut self, spans: Vec<Span>) {
        // Only speakers are kept; silence and pending stretches just say how far detection got.
        for span in spans {
            self.horizon_ms = self.horizon_ms.max(span.end_ms);
            if matches!(span.label, Label::Speaker(_)) {
                self.spans.push(span);
            }
        }
    }

    /// The speaker of a line between these capture-timeline times. `last` is the end of the
    /// meeting: nothing more will be decided, so the nearest speaker wins.
    fn decide(&self, start_ms: u32, end_ms: u32, last: bool) -> Decision {
        if self.unavailable {
            return Decision::Fallback;
        }
        if !last && self.horizon_ms < end_ms {
            return Decision::Wait;
        }
        // The speaker who talks most during the line.
        let first = self.spans.partition_point(|s| s.end_ms <= start_ms);
        let mut talk: Vec<(usize, u32)> = Vec::new();
        for span in self.spans[first..].iter().take_while(|s| s.start_ms < end_ms) {
            let Label::Speaker(c) = span.label else { continue };
            let c = self.resolve(c);
            let overlap = end_ms.min(span.end_ms).saturating_sub(start_ms.max(span.start_ms));
            match talk.iter_mut().find(|(sp, _)| *sp == c) {
                Some(t) => t.1 += overlap,
                None => talk.push((c, overlap)),
            }
        }
        if let Some(&(c, _)) = talk.iter().filter(|(_, ms)| *ms > 0).max_by_key(|(_, ms)| *ms) {
            return Decision::Speaker(c);
        }
        // Nobody clearly speaking during it: whoever spoke right before or after.
        let before = self.spans[..first].last().map(|s| (start_ms.saturating_sub(s.end_ms), s.label));
        let after = self.spans.get(first).map(|s| (s.start_ms.saturating_sub(end_ms), s.label));
        let nearest = [before, after].into_iter().flatten().min_by_key(|(gap, _)| *gap);
        let settled = last || self.horizon_ms >= end_ms.saturating_add(GIVE_UP_MS);
        match nearest {
            Some((gap, Label::Speaker(c))) if gap <= NEIGHBOUR_MS || settled => Decision::Speaker(self.resolve(c)),
            _ if settled => Decision::Fallback,
            // Someone may yet be decided just after it.
            _ => Decision::Wait,
        }
    }
}

/// Everything the persister needs.
struct Persist {
    db: Db,
    events: Events,
    note_id: Id,
    you: Id,
    others: Id,
    offset_ms: u32,
    labels: SpeakerSource,
    recogniser: Option<Recogniser>,
    rx: mpsc::UnboundedReceiver<Msg>,
}

/// A finished line that still waits for its speaker.
struct Waiting {
    segment: Segment,
    /// Capture-timeline times (the segment's own are on the note's timeline).
    start_ms: u32,
    end_ms: u32,
}

/// Store and emit segments as they arrive. Partials are upserted in place by id; a final
/// replaces its partial; an empty final or echo retracts it. Each line goes to the person who
/// said it: as labelled by the service, or as Cilica tells from the voices (stored without a
/// speaker, "Pending", until it is sure); otherwise "You" (mic) or "Others" (call).
async fn persist(p: Persist) {
    let Persist { db, events, note_id, you, others, offset_ms, labels, mut recogniser, mut rx } = p;
    let mut live_speakers = LiveSpeakers::new(&db, note_id).await;
    let mut tracks: HashMap<AudioChannel, Track> = HashMap::new();
    let mut waiting: Vec<Waiting> = Vec::new();
    let mut ids: HashMap<(AudioChannel, u64), Id> = HashMap::new();
    let mut echo = EchoFilter::default();
    let fallback = |channel: AudioChannel| if channel == AudioChannel::Mic { you } else { others };
    let retract = async |db: &Db, id: Id| {
        if let Err(error) = db.delete_segment(id).await {
            tracing::warn!(%error, "could not remove a transcript segment");
        }
        events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Transcript], note_id: Some(note_id) });
    };
    let store = async |db: &Db, segment: Segment| {
        match db.upsert_segment(&segment).await {
            Ok(()) => events.emit(AppEvent::TranscriptSegment { segment }),
            Err(error) => tracing::warn!(%error, "could not store a transcript segment"),
        }
    };
    while let Some(msg) = rx.recv().await {
        let (channel, event) = match msg {
            Msg::Stt { channel, event } => (channel, event),
            Msg::Speakers { channel, update } => {
                let track = tracks.entry(channel).or_default();
                match update {
                    SpeakerUpdate::Spans(spans) => track.add(spans),
                    SpeakerUpdate::Unavailable => track.unavailable = true,
                    SpeakerUpdate::Merged(merges) => {
                        for (from, into) in merges {
                            track.alias.insert(from, into);
                            live_speakers.merge(&db, &events, note_id, channel, &local_label(from), &local_label(into)).await;
                        }
                    }
                }
                // Lines this news settles get their speaker.
                let mut still = Vec::with_capacity(waiting.len());
                for w in std::mem::take(&mut waiting) {
                    if w.segment.channel != channel {
                        still.push(w);
                        continue;
                    }
                    let decision = tracks.get(&channel).map(|t| t.decide(w.start_ms, w.end_ms, false)).unwrap_or(Decision::Wait);
                    match settle(decision, &mut live_speakers, &db, &events, note_id, channel, you).await {
                        Some(speaker) => {
                            let segment = Segment { speaker_id: Some(speaker.unwrap_or(fallback(channel))), ..w.segment };
                            if let (Some(r), Some(id)) = (recogniser.as_mut(), speaker)
                                && id != you
                            {
                                r.heard(id, channel, w.start_ms, w.end_ms);
                            }
                            store(&db, segment).await;
                        }
                        None => still.push(w),
                    }
                }
                waiting = still;
                continue;
            }
        };
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
                    waiting.retain(|w| w.segment.id != drop);
                    retract(&db, drop).await;
                }
                None => {}
            }
        }
        // Who said it: `Some(None)` is the channel's fallback, `None` is not known yet.
        let speaker: Option<Option<Id>> = match labels {
            SpeakerSource::Off => Some(None),
            SpeakerSource::Service => match u.speaker.as_deref() {
                None => Some(None),
                Some(label) => Some(live_speakers.speaker(&db, &events, note_id, channel, label, you).await),
            },
            SpeakerSource::Local => {
                let decision = tracks.get(&channel).map(|t| t.decide(u.start_ms, u.end_ms, false)).unwrap_or(Decision::Wait);
                settle(decision, &mut live_speakers, &db, &events, note_id, channel, you).await
            }
        };
        let segment = Segment {
            id,
            note_id,
            speaker_id: speaker.map(|s| s.unwrap_or(fallback(channel))),
            channel,
            start_ms,
            end_ms,
            text: u.text.trim().to_string(),
            is_final,
        };
        if is_final && speaker.is_none() {
            waiting.push(Waiting { segment: segment.clone(), start_ms: u.start_ms, end_ms: u.end_ms });
        }
        store(&db, segment).await;
        // A finished line by someone told apart: listen for a voice Cilica knows.
        if is_final
            && let Some(Some(speaker_id)) = speaker
            && speaker_id != you
            && let Some(r) = recogniser.as_mut()
        {
            r.heard(speaker_id, channel, u.start_ms, u.end_ms);
        }
    }
    // The meeting is over: every line still waiting goes to the nearest speaker.
    for w in waiting {
        let channel = w.segment.channel;
        let decision = tracks.get(&channel).map(|t| t.decide(w.start_ms, w.end_ms, true)).unwrap_or(Decision::Fallback);
        let speaker = settle(decision, &mut live_speakers, &db, &events, note_id, channel, you).await.flatten();
        store(&db, Segment { speaker_id: Some(speaker.unwrap_or(fallback(channel))), ..w.segment }).await;
    }
    if let Some(r) = recogniser {
        r.finish().await;
    }
}

/// How Cilica's own speaker numbers are keyed among the note's labels.
fn local_label(c: usize) -> String {
    format!("local {c}")
}

/// Turn a decision into a speaker: `Some(Some(id))` a person, `Some(None)` the channel's
/// fallback, `None` still pending.
async fn settle(decision: Decision, live: &mut LiveSpeakers, db: &Db, events: &Events, note_id: Id, channel: AudioChannel, you: Id) -> Option<Option<Id>> {
    match decision {
        Decision::Wait => None,
        Decision::Fallback => Some(None),
        Decision::Speaker(c) => Some(live.speaker(db, events, note_id, channel, &local_label(c), you).await),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start_ms: u32, end_ms: u32, label: Label) -> Span {
        Span { start_ms, end_ms, label }
    }

    fn track(spans: Vec<Span>) -> Track {
        let mut t = Track::default();
        t.add(spans);
        t
    }

    #[test]
    fn a_line_waits_until_detection_has_heard_it() {
        let t = track(vec![span(0, 2_000, Label::Speaker(0))]);
        assert_eq!(t.decide(500, 1_500, false), Decision::Speaker(0));
        assert_eq!(t.decide(1_500, 2_500, false), Decision::Wait, "the end of the line is not decided yet");
        assert_eq!(t.decide(1_500, 2_500, true), Decision::Speaker(0), "at the end of the meeting it is");
    }

    #[test]
    fn the_person_who_talks_most_during_a_line_said_it() {
        let t = track(vec![span(0, 1_000, Label::Speaker(0)), span(1_000, 4_000, Label::Speaker(1)), span(4_000, 5_000, Label::Pending)]);
        assert_eq!(t.decide(500, 3_000, false), Decision::Speaker(1));
        assert_eq!(t.decide(0, 1_200, false), Decision::Speaker(0));
    }

    #[test]
    fn an_unclear_line_takes_its_neighbour_or_waits() {
        let mut t = track(vec![span(0, 1_000, Label::Speaker(2)), span(1_000, 3_000, Label::Pending), span(3_000, 4_000, Label::Silence)]);
        assert_eq!(t.decide(1_500, 2_500, false), Decision::Speaker(2), "the speaker just before it, within 2 s");
        t.add(vec![span(4_000, 20_000, Label::Silence)]);
        assert_eq!(t.decide(15_000, 16_000, false), Decision::Wait, "nobody near it and not settled");
        t.add(vec![span(20_000, 30_000, Label::Silence)]);
        assert_eq!(t.decide(15_000, 16_000, false), Decision::Speaker(2), "long after, the nearest speaker");
        assert_eq!(Track::default().decide(0, 1_000, true), Decision::Fallback, "nobody at all");
    }

    #[test]
    fn folded_speakers_count_as_the_one_they_joined() {
        let mut t = track(vec![span(0, 1_000, Label::Speaker(3)), span(1_000, 1_500, Label::Speaker(0))]);
        t.alias.insert(3, 0);
        assert_eq!(t.decide(0, 1_500, false), Decision::Speaker(0));
    }

    #[test]
    fn without_detection_lines_fall_back_at_once() {
        let t = Track { unavailable: true, ..Track::default() };
        assert_eq!(t.decide(0, 1_000, false), Decision::Fallback);
    }
}
