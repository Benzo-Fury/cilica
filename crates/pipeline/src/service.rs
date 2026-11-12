//! [`RecordingService`]: the one recording session at a time, its phases, auto-stop, meeting
//! detection, recovery at launch and audio retention.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use margin_detect::{DetectEvent, MeetingDetector};
use margin_domain::chrono::{self, Local};
use margin_domain::{
    AppError, AppEvent, AppResult, DataScope, DetectMode, ErrorCode, Events, Id, MeetingSummarizer, NoteKind, PermissionKind,
    PermissionStatus, RecordingPhase, RecordingState, RecordingTrigger, Settings, StartRecordingOptions, ToastAction, ToastKind,
    ToastRequest, now,
};
use margin_store::{Db, NewNote, StoreError};

use crate::backend::{AudioBackend, SpeechBackend};
use crate::files::{RecordingFiles, Sidecar, Stage, expired, unfinished};
use crate::finalize::{Job, finalize};
use crate::live::{LiveParams, LiveSession, system_audio_missing_toast};

/// Hands out the end-of-meeting summariser. Looked up each time it is needed, because the
/// assistant area may be set up after recording.
pub type SummarizerProvider = Arc<dyn Fn() -> Arc<dyn MeetingSummarizer> + Send + Sync>;

/// Tunables. [`ServiceConfig::new`] has the product values.
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// The app data directory; recordings go in `recordings/` inside it.
    pub data_dir: PathBuf,
    /// Audio silenced at the start of capture, so the start chime is not recorded.
    pub mute_start_ms: u32,
    /// How long after the meeting app lets go of the mic recording stops (unless kept).
    pub auto_stop_delay: Duration,
    /// Cancel system audio out of the mic.
    pub echo_cancel: bool,
}

impl ServiceConfig {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self { data_dir: data_dir.into(), mute_start_ms: 700, auto_stop_delay: Duration::from_secs(10), echo_cancel: true }
    }
}

/// Starts, stops and finishes recordings. Cheap to clone. Needs a Tokio runtime for the async
/// methods; the handle given to [`RecordingService::new`] runs background work.
#[derive(Clone)]
pub struct RecordingService {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub db: Db,
    pub events: Events,
    pub audio: Arc<dyn AudioBackend>,
    pub speech: Arc<dyn SpeechBackend>,
    pub summarizer: SummarizerProvider,
    pub config: ServiceConfig,
    rt: tokio::runtime::Handle,
    shared: Mutex<Shared>,
    /// Keeps state events in the order the state changed.
    emit_lock: Mutex<()>,
    /// Serialises start and stop.
    op: tokio::sync::Mutex<()>,
    capturing: AtomicBool,
    /// Recordings being finished (transcript catch-up, speakers, summary).
    finalizing: AtomicUsize,
    detector: Mutex<Option<MeetingDetector>>,
    /// Meeting apps the detector currently sees holding the mic.
    meetings: Mutex<Vec<String>>,
}

struct Shared {
    state: RecordingState,
    /// Bumped by every session; background work only changes the state of its own session.
    generation: u64,
    live: Option<Live>,
    /// Bumped whenever a pending auto-stop is scheduled or cancelled.
    auto_stop_seq: u64,
}

struct Live {
    session: LiveSession,
    files: RecordingFiles,
    sidecar: Sidecar,
    /// Meeting apps whose end stops this recording.
    apps: Vec<String>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// "Zoom meeting · 14:05", "Google Meet · 14:05", "Recording · 14:05".
pub fn default_title(app: Option<&str>, at: chrono::DateTime<Local>) -> String {
    let time = at.format("%H:%M");
    match app.map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) if a.ends_with("Meet") || a.to_lowercase().ends_with("meeting") => format!("{a} · {time}"),
        Some(a) => format!("{a} meeting · {time}"),
        None => format!("Recording · {time}"),
    }
}

impl Inner {
    pub fn state(&self) -> RecordingState {
        lock(&self.shared).state.clone()
    }

    pub fn emit_state(&self) {
        let _order = lock(&self.emit_lock);
        let state = self.state();
        self.events.emit(AppEvent::RecordingStateChanged { state });
    }

    /// Move session `epoch` to `phase` (ignored if a newer session has started).
    pub fn set_phase(&self, epoch: u64, phase: RecordingPhase) {
        {
            let mut sh = lock(&self.shared);
            if sh.generation != epoch || sh.live.is_some() {
                return;
            }
            if phase == RecordingPhase::Idle {
                sh.state = RecordingState::idle();
            } else {
                sh.state.phase = phase;
                sh.state.auto_stop_at = None;
            }
        }
        self.emit_state();
    }

    pub fn data_changed(&self, scopes: &[DataScope], note_id: Id) {
        self.events.emit(AppEvent::DataChanged { scopes: scopes.to_vec(), note_id: Some(note_id) });
    }

    pub fn emit_toast(&self, toast: ToastRequest) {
        self.events.emit(AppEvent::Toast { toast });
    }

    pub fn toast(&self, kind: ToastKind, title: &str, description: impl Into<String>, action: Option<(&str, ToastAction)>) {
        let (action_label, action) = match action {
            Some((l, a)) => (Some(l.to_string()), Some(a)),
            None => (None, None),
        };
        self.emit_toast(ToastRequest { kind, title: title.into(), description: Some(description.into()), action_label, action });
    }

    /// A finished session's background work is done.
    fn finished(&self, epoch: u64) {
        self.finalizing.fetch_sub(1, Ordering::SeqCst);
        self.set_phase(epoch, RecordingPhase::Idle);
    }
}

impl RecordingService {
    pub fn new(
        db: Db,
        events: Events,
        audio: Arc<dyn AudioBackend>,
        speech: Arc<dyn SpeechBackend>,
        summarizer: SummarizerProvider,
        config: ServiceConfig,
        rt: tokio::runtime::Handle,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                events,
                audio,
                speech,
                summarizer,
                config,
                rt,
                shared: Mutex::new(Shared { state: RecordingState::idle(), generation: 0, live: None, auto_stop_seq: 0 }),
                emit_lock: Mutex::new(()),
                op: tokio::sync::Mutex::new(()),
                capturing: AtomicBool::new(false),
                finalizing: AtomicUsize::new(0),
                detector: Mutex::new(None),
                meetings: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn state(&self) -> RecordingState {
        self.inner.state()
    }

    /// Audio is being captured.
    pub fn is_capturing(&self) -> bool {
        self.inner.capturing.load(Ordering::SeqCst)
    }

    /// True from the start of a recording until its summary is written (and while an
    /// interrupted recording is being finished). The scheduler waits while this is true.
    pub fn is_busy(&self) -> bool {
        self.is_capturing() || self.inner.finalizing.load(Ordering::SeqCst) > 0 || self.state().phase != RecordingPhase::Idle
    }

    // ── Start and stop ───────────────────────────────────────────────────

    /// Start recording into a new meeting note, or into `options.note_id`.
    pub async fn start(&self, options: StartRecordingOptions) -> AppResult<RecordingState> {
        let inner = &self.inner;
        let _op = inner.op.lock().await;
        if self.is_capturing() {
            return Err(AppError::new(ErrorCode::Conflict, "Margin is already recording. Stop that recording first."));
        }
        let settings = inner.db.get_settings().await?;
        inner.speech.check(&settings)?;
        self.ensure_microphone().await?;

        let meeting_app = options.meeting_app.clone().filter(|a| !a.trim().is_empty()).or_else(|| {
            let m = lock(&inner.meetings);
            (m.len() == 1).then(|| m[0].clone())
        });
        let started_at = now();
        let (note, created) = match options.note_id {
            Some(id) => {
                let note = inner.db.get_note(id).await?;
                if note.deleted_at.is_some() {
                    return Err(AppError::invalid("That note is in Trash. Restore it, then record again."));
                }
                (note, false)
            }
            None => {
                let new = NewNote {
                    title: default_title(meeting_app.as_deref(), started_at.with_timezone(&Local)),
                    kind: NoteKind::Meeting,
                    meeting_app: meeting_app.clone(),
                    started_at: Some(started_at),
                    ..NewNote::default()
                };
                (inner.db.create_note(new).await?, true)
            }
        };
        match self.start_capture(&settings, note.id, created, meeting_app.clone(), started_at, options.trigger).await {
            Ok(state) => Ok(state),
            Err(error) => {
                if created {
                    let _ = inner.db.delete_note_forever(note.id).await;
                }
                inner.data_changed(&[DataScope::Notes], note.id);
                Err(error)
            }
        }
    }

    /// Ask for the microphone if it was never asked for; refuse if it is denied.
    async fn ensure_microphone(&self) -> AppResult<()> {
        let audio = self.inner.audio.clone();
        let status = match audio.permission(PermissionKind::Microphone) {
            PermissionStatus::NotDetermined => {
                tokio::task::spawn_blocking(move || audio.request_permission(PermissionKind::Microphone)).await.unwrap_or(PermissionStatus::Denied)
            }
            s => s,
        };
        if status == PermissionStatus::Denied {
            return Err(margin_audio::AudioError::MicPermissionDenied.into());
        }
        Ok(())
    }

    async fn start_capture(
        &self,
        settings: &Settings,
        note_id: Id,
        created: bool,
        meeting_app: Option<String>,
        started_at: margin_domain::Timestamp,
        trigger: RecordingTrigger,
    ) -> AppResult<RecordingState> {
        let inner = &self.inner;
        // Where this recording sits on the note's timeline.
        let (previous_audio, previous_ms) = inner.db.get_note_audio(note_id).await?;
        let offset_ms = if created { 0 } else { inner.db.get_transcript(note_id).await?.duration_ms.unwrap_or(0).max(previous_ms.unwrap_or(0)) };
        let previous_audio = previous_audio.filter(|rel| inner.config.data_dir.join(rel).exists());
        let (you, others) = inner.db.ensure_channel_speakers(note_id).await?;

        let epoch = {
            let mut sh = lock(&inner.shared);
            sh.generation += 1;
            sh.state = RecordingState {
                phase: RecordingPhase::Starting,
                note_id: Some(note_id),
                meeting_app: meeting_app.clone(),
                started_at: Some(started_at),
                auto_stop_at: None,
            };
            sh.generation
        };
        inner.emit_state();
        if created {
            inner.data_changed(&[DataScope::Notes], note_id);
        }

        let files = RecordingFiles::new(&inner.config.data_dir, note_id);
        let sidecar = Sidecar { note_id, started_at, offset_ms, previous_audio, stage: Stage::Recording };
        let result = async {
            files.write_sidecar(&sidecar).map_err(|e| AppError::internal(format!("Margin could not create the recording file: {e}. Check that the disk has space.")))?;
            let with_system = inner.audio.permission(PermissionKind::SystemAudio) != PermissionStatus::Denied;
            if !with_system {
                system_audio_missing_toast(&inner.events, true, "");
            }
            let weak = Arc::downgrade(&self.inner);
            let rt = inner.rt.clone();
            let params = LiveParams {
                db: inner.db.clone(),
                events: inner.events.clone(),
                audio: inner.audio.clone(),
                speech: inner.speech.clone(),
                settings: settings.clone(),
                note_id,
                you: you.id,
                others: others.id,
                offset_ms,
                files: files.clone(),
                mute_start_ms: inner.config.mute_start_ms,
                echo_cancel: inner.config.echo_cancel,
                with_system,
                on_capture_failed: Box::new(move |message| {
                    rt.spawn(async move {
                        if let Some(inner) = weak.upgrade() {
                            RecordingService { inner }.capture_failed(epoch, message).await;
                        }
                    });
                }),
            };
            let rt = inner.rt.clone();
            let session = tokio::task::spawn_blocking(move || LiveSession::start(params, rt))
                .await
                .map_err(|_| AppError::internal("Starting the recording stopped unexpectedly. Try again."))??;
            Ok::<_, AppError>((session, with_system))
        }
        .await;

        let (session, with_system) = match result {
            Ok(r) => r,
            Err(error) => {
                files.remove_working_files();
                {
                    let mut sh = lock(&inner.shared);
                    if sh.generation == epoch {
                        sh.state = RecordingState::idle();
                    }
                }
                inner.emit_state();
                return Err(error);
            }
        };

        let mut apps: Vec<String> = lock(&inner.meetings).clone();
        if let Some(a) = meeting_app.as_ref().filter(|a| !apps.contains(a)) {
            apps.push(a.clone());
        }
        inner.capturing.store(true, Ordering::SeqCst);
        {
            let mut sh = lock(&inner.shared);
            sh.live = Some(Live { session, files, sidecar, apps });
            sh.state.phase = RecordingPhase::Recording;
        }
        inner.emit_state();
        tracing::info!(%note_id, ?trigger, with_system, "recording");

        if trigger != RecordingTrigger::Manual {
            let title = inner.db.get_note_summary(note_id).await.map(|n| n.title).unwrap_or_default();
            inner.toast(ToastKind::Action, "Recording", title, Some(("Stop", ToastAction::StopRecording)));
        }
        if with_system {
            // The system audio prompt may be answered "Don't Allow" after capture started; the
            // tap then delivers silence. Say so once.
            let svc = self.clone();
            inner.rt.spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let still = lock(&svc.inner.shared).generation == epoch && svc.is_capturing();
                if still && svc.inner.audio.permission(PermissionKind::SystemAudio) == PermissionStatus::Denied {
                    system_audio_missing_toast(&svc.inner.events, true, "");
                }
            });
        }
        Ok(self.state())
    }

    /// Stop capturing. Returns once the audio file is closed; the transcript, speakers and
    /// summary are finished in the background (phases Stopping → Diarizing → Summarizing → Idle).
    pub async fn stop(&self) -> AppResult<RecordingState> {
        let inner = &self.inner;
        let _op = inner.op.lock().await;
        let (live, epoch) = {
            let mut sh = lock(&inner.shared);
            let Some(live) = sh.live.take() else { return Ok(sh.state.clone()) };
            sh.state.phase = RecordingPhase::Stopping;
            sh.state.auto_stop_at = None;
            sh.auto_stop_seq += 1;
            (live, sh.generation)
        };
        inner.finalizing.fetch_add(1, Ordering::SeqCst);
        inner.capturing.store(false, Ordering::SeqCst);
        inner.emit_state();

        let Live { session, files, sidecar, .. } = live;
        let wav = files.wav();
        let job = match tokio::task::spawn_blocking(move || session.stop()).await {
            Ok((summary, draining)) => {
                match summary {
                    Ok(s) => tracing::info!(duration_ms = s.duration_ms, "capture stopped"),
                    Err(message) => {
                        tracing::warn!(%message, "capture ended with an error; repairing the file");
                        let _ = tokio::task::spawn_blocking(move || margin_audio::recover_wav(&wav)).await;
                    }
                }
                Job { files, sidecar, draining: Some(draining), ended_at: now(), recovered: false }
            }
            // The capture thread panicked: finish the note as if the app had crashed.
            Err(_) => Job { files, sidecar, draining: None, ended_at: now(), recovered: true },
        };
        let svc = self.clone();
        inner.rt.spawn(async move {
            finalize(&svc.inner, epoch, job).await;
            svc.inner.finished(epoch);
        });
        Ok(self.state())
    }

    /// Start if idle, stop if recording (hotkey, tray, quit).
    pub async fn toggle(&self, trigger: RecordingTrigger) -> AppResult<RecordingState> {
        if self.is_capturing() {
            self.stop().await
        } else {
            self.start(StartRecordingOptions { note_id: None, meeting_app: None, trigger }).await
        }
    }

    /// Capture ended on its own (disk full, capture thread died): finish what we have.
    async fn capture_failed(&self, epoch: u64, message: String) {
        if lock(&self.inner.shared).generation != epoch || !self.is_capturing() {
            return;
        }
        tracing::error!(%message, "capture failed");
        self.inner.toast(ToastKind::Error, "Recording stopped", format!("Margin stopped recording because {message}. Check that the disk has space; what was recorded so far is kept."), None);
        let _ = self.stop().await;
    }

    /// Cancel a pending auto-stop.
    pub fn keep_recording(&self) -> RecordingState {
        let changed = {
            let mut sh = lock(&self.inner.shared);
            sh.auto_stop_seq += 1;
            sh.state.auto_stop_at.take().is_some()
        };
        if changed {
            self.inner.emit_state();
        }
        self.state()
    }

    /// Write the meeting summary again.
    pub async fn regenerate_summary(&self, note_id: Id) -> AppResult<()> {
        (self.inner.summarizer)().summarize_meeting(note_id).await?;
        self.inner.data_changed(&[DataScope::Notes, DataScope::Versions], note_id);
        Ok(())
    }

    // ── Launch ───────────────────────────────────────────────────────────

    /// Finish recordings interrupted by a quit or crash, then apply audio retention.
    pub async fn startup(&self) {
        let inner = &self.inner;
        for (files, sidecar) in unfinished(&inner.config.data_dir) {
            let note = match inner.db.get_note(sidecar.note_id).await {
                Ok(n) => n,
                Err(StoreError::NotFound(_)) => {
                    files.remove_working_files();
                    continue;
                }
                Err(error) => {
                    tracing::warn!(%error, "could not look up an interrupted recording's note");
                    continue;
                }
            };
            tracing::info!(note_id = %note.id, stage = ?sidecar.stage, "finishing an interrupted recording");
            inner.toast(ToastKind::Info, "Finishing an interrupted recording", note.title.clone(), Some(("Open", ToastAction::OpenNote { note_id: note.id })));
            let ended_at = std::fs::metadata(files.wav())
                .and_then(|m| m.modified())
                .map(chrono::DateTime::<chrono::Utc>::from)
                .unwrap_or_else(|_| now());
            let epoch = {
                let mut sh = lock(&inner.shared);
                if sh.live.is_some() {
                    u64::MAX
                } else {
                    sh.generation += 1;
                    sh.state = RecordingState {
                        phase: RecordingPhase::Stopping,
                        note_id: Some(note.id),
                        meeting_app: note.meeting_app.clone(),
                        started_at: Some(sidecar.started_at),
                        auto_stop_at: None,
                    };
                    sh.generation
                }
            };
            inner.finalizing.fetch_add(1, Ordering::SeqCst);
            inner.emit_state();
            finalize(inner, epoch, Job { files, sidecar, draining: None, ended_at, recovered: true }).await;
            inner.finished(epoch);
        }
        if let Err(error) = self.apply_retention().await {
            tracing::warn!(message = %error.message, "audio retention failed");
        }
    }

    /// Remove kept recordings older than `settings.recording.audio_retention_days` (0 keeps them
    /// for ever). Transcripts stay. Returns how many files were removed.
    pub async fn apply_retention(&self) -> AppResult<u32> {
        let inner = &self.inner;
        let days = inner.db.get_settings().await?.recording.audio_retention_days;
        let live_note = lock(&inner.shared).state.note_id;
        let mut removed = 0;
        for (note_id, path) in expired(&inner.config.data_dir, days) {
            if Some(note_id) == live_note {
                continue;
            }
            let owner = match inner.db.get_note_audio(note_id).await {
                Ok((Some(rel), duration)) if inner.config.data_dir.join(&rel) == path => Some(duration),
                Ok(_) | Err(StoreError::NotFound(_)) => None,
                Err(error) => {
                    tracing::warn!(%error, "retention: could not look up a recording");
                    continue;
                }
            };
            if let Err(error) = std::fs::remove_file(&path) {
                tracing::warn!(%error, path = %path.display(), "retention: could not remove a recording");
                continue;
            }
            if let Some(duration) = owner {
                inner.db.set_note_audio(note_id, None, duration).await?;
                inner.data_changed(&[DataScope::Notes, DataScope::Transcript], note_id);
            }
            removed += 1;
        }
        if removed > 0 {
            tracing::info!(removed, days, "removed old recordings");
        }
        Ok(removed)
    }

    // ── Meeting detection ────────────────────────────────────────────────

    /// Start, stop or update the meeting detector to match the settings. Call at launch and on
    /// every settings change.
    pub fn apply_settings(&self, settings: &Settings) {
        let r = &settings.recording;
        let mut detector = lock(&self.inner.detector);
        if r.auto_detect {
            match detector.as_ref() {
                Some(d) => d.set_allow_list(r.meeting_apps.clone()),
                None => {
                    let weak: Weak<Inner> = Arc::downgrade(&self.inner);
                    let rt = self.inner.rt.clone();
                    *detector = Some(MeetingDetector::start(r.meeting_apps.clone(), move |event| {
                        if let Some(inner) = weak.upgrade() {
                            rt.spawn(async move { RecordingService { inner }.handle_detect_event(event).await });
                        }
                    }));
                    tracing::info!("listening for meetings");
                }
            }
        } else if let Some(d) = detector.take() {
            drop(detector);
            d.stop();
            lock(&self.inner.meetings).clear();
            tracing::info!("stopped listening for meetings");
        }
    }

    /// React to the detector: offer or start a recording, or schedule the auto-stop.
    pub async fn handle_detect_event(&self, event: DetectEvent) {
        let inner = &self.inner;
        match event {
            DetectEvent::Detected(meeting) => {
                let name = meeting.app_name.clone();
                {
                    let mut m = lock(&inner.meetings);
                    if !m.contains(&name) {
                        m.push(name.clone());
                    }
                }
                inner.events.emit(AppEvent::MeetingDetected { meeting });
                if self.is_capturing() {
                    // The meeting came back (or another one joined in): it now ends this recording
                    // too, and a pending auto-stop for it is called off.
                    let cancelled = {
                        let mut sh = lock(&inner.shared);
                        if let Some(live) = sh.live.as_mut()
                            && !live.apps.contains(&name)
                        {
                            live.apps.push(name.clone());
                        }
                        let pending = sh.state.auto_stop_at.is_some();
                        if pending {
                            sh.auto_stop_seq += 1;
                            sh.state.auto_stop_at = None;
                        }
                        pending
                    };
                    if cancelled {
                        inner.emit_state();
                    }
                    return;
                }
                let Ok(settings) = inner.db.get_settings().await else { return };
                if !settings.recording.auto_detect {
                    return;
                }
                match settings.recording.detect_mode {
                    DetectMode::AskFirst => inner.toast(
                        ToastKind::Action,
                        &format!("{name} meeting detected"),
                        "Record it to get a transcript and meeting notes.",
                        Some(("Record", ToastAction::StartRecording { meeting_app: Some(name) })),
                    ),
                    DetectMode::AutoStart => {
                        let options = StartRecordingOptions { note_id: None, meeting_app: Some(name), trigger: RecordingTrigger::Detected };
                        if let Err(error) = self.start(options).await {
                            inner.toast(ToastKind::Error, "Recording did not start", error.message, None);
                        }
                    }
                }
            }
            DetectEvent::Ended { app_name } => {
                lock(&inner.meetings).retain(|m| *m != app_name);
                inner.events.emit(AppEvent::MeetingEnded { app_name: app_name.clone() });
                let delay = inner.config.auto_stop_delay;
                let scheduled = {
                    let mut sh = lock(&inner.shared);
                    match sh.live.as_ref() {
                        Some(live) if live.apps.contains(&app_name) => {
                            sh.auto_stop_seq += 1;
                            sh.state.auto_stop_at = Some(now() + chrono::Duration::from_std(delay).unwrap_or_default());
                            Some(sh.auto_stop_seq)
                        }
                        _ => None,
                    }
                };
                let Some(seq) = scheduled else { return };
                inner.emit_state();
                inner.toast(
                    ToastKind::Action,
                    &format!("{app_name} meeting ended"),
                    format!("Recording stops in {} seconds.", delay.as_secs().max(1)),
                    Some(("Keep recording", ToastAction::KeepRecording)),
                );
                let svc = self.clone();
                inner.rt.spawn(async move {
                    tokio::time::sleep(delay).await;
                    let due = {
                        let sh = lock(&svc.inner.shared);
                        sh.auto_stop_seq == seq && sh.state.auto_stop_at.is_some() && sh.live.is_some()
                    };
                    if due && let Err(error) = svc.stop().await {
                        tracing::warn!(message = %error.message, "auto-stop failed");
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn titles() {
        let at = Local.with_ymd_and_hms(2026, 11, 11, 14, 5, 0).unwrap();
        assert_eq!(default_title(Some("Zoom"), at), "Zoom meeting · 14:05");
        assert_eq!(default_title(Some("Google Meet"), at), "Google Meet · 14:05");
        assert_eq!(default_title(None, at), "Recording · 14:05");
        assert_eq!(default_title(Some(" "), at), "Recording · 14:05");
    }
}
