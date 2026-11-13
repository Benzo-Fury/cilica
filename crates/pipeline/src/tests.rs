//! Whole sessions without a window: fixture audio through `ClipSource`s, a scripted transcriber
//! (or the real models, in the ignored tests), an in-memory database and a recording event sink.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cilica_audio::{CaptureSource, ClipSource, SourceKind, WavWriter, read_wav_16k};
use cilica_domain::*;
use cilica_store::Db;
use cilica_stt::vad::{EnergyVad, Segmenter, SegmenterConfig};
use cilica_stt::{StreamOptions, SttEvent, SttResult, Transcriber, TranscriptStream, Utterance};

use crate::files::{RecordingFiles, Sidecar, Stage};
use crate::{AudioBackend, DetectEvent, RecordingService, ServiceConfig, SpeechBackend};

// ── Fixtures ──────────────────────────────────────────────────────────────

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn mono(name: &str) -> Vec<f32> {
    read_wav_16k(&fixtures().join(name)).unwrap().remove(0)
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Turn {
    channel: String,
    speaker: String,
    start_ms: u32,
    end_ms: u32,
    text: String,
}

impl Turn {
    fn channel(&self) -> AudioChannel {
        if self.channel == "mic" { AudioChannel::Mic } else { AudioChannel::System }
    }
}

fn truth() -> Vec<Turn> {
    serde_json::from_slice(&std::fs::read(fixtures().join("meeting.json")).unwrap()).unwrap()
}

// ── Fakes ─────────────────────────────────────────────────────────────────

struct FakeAudio {
    mic: ClipSource,
    system: ClipSource,
    system_permission: PermissionStatus,
}

impl FakeAudio {
    fn meeting(seconds: f32) -> Self {
        let cut = |mut v: Vec<f32>| {
            v.truncate((seconds * 16_000.0) as usize);
            v
        };
        Self {
            mic: ClipSource::new(SourceKind::Mic, 16_000, cut(mono("meeting_mic.wav"))),
            system: ClipSource::new(SourceKind::System, 16_000, cut(mono("meeting_system.wav"))),
            system_permission: PermissionStatus::Granted,
        }
    }
}

impl AudioBackend for FakeAudio {
    fn mic(&self, _device_id: Option<String>) -> Box<dyn CaptureSource> {
        Box::new(self.mic.clone())
    }
    fn system(&self) -> Box<dyn CaptureSource> {
        Box::new(self.system.clone())
    }
    fn permission(&self, kind: PermissionKind) -> PermissionStatus {
        if kind == PermissionKind::SystemAudio { self.system_permission } else { PermissionStatus::Granted }
    }
    fn request_permission(&self, kind: PermissionKind) -> PermissionStatus {
        self.permission(kind)
    }
}

/// Finds utterances with an energy VAD and "recognises" them from the ground truth by time, so
/// the pipeline is tested without a model. Times are absolute when `offset_ms` is right.
struct ScriptedTranscriber {
    truth: Arc<Vec<Turn>>,
}

struct ScriptedStream {
    seg: Segmenter,
    channel: Option<AudioChannel>,
    truth: Arc<Vec<Turn>>,
    next_id: u64,
    open: Option<u64>,
}

impl ScriptedStream {
    fn text(&self, start_ms: u32, end_ms: u32) -> String {
        self.truth
            .iter()
            .filter(|t| self.channel.is_none_or(|c| c == t.channel()))
            .filter(|t| end_ms.min(t.end_ms).saturating_sub(start_ms.max(t.start_ms)) * 2 > t.end_ms - t.start_ms)
            .map(|t| t.text.clone())
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn finals(&mut self, chunks: Vec<cilica_stt::vad::Chunk>) -> Vec<SttEvent> {
        chunks
            .into_iter()
            .map(|c| {
                let id = self.open.take().unwrap_or_else(|| {
                    self.next_id += 1;
                    self.next_id
                });
                let (s, e) = ((c.start_sample / 16) as u32, ((c.start_sample + c.samples.len() as u64) / 16) as u32);
                SttEvent::Final(Utterance { id, start_ms: s, end_ms: e, text: self.text(s, e), speaker: None })
            })
            .collect()
    }
}

impl TranscriptStream for ScriptedStream {
    fn push(&mut self, start_sample: u64, samples: &[f32]) -> SttResult<Vec<SttEvent>> {
        let chunks = self.seg.push(start_sample, samples);
        let mut out = self.finals(chunks);
        if let Some((s, audio, _)) = self.seg.current()
            && self.open.is_none()
            && audio.len() > 8_000
        {
            self.next_id += 1;
            self.open = Some(self.next_id);
            let (s_ms, e_ms) = ((s / 16) as u32, ((s + audio.len() as u64) / 16) as u32);
            out.push(SttEvent::Partial(Utterance { id: self.next_id, start_ms: s_ms, end_ms: e_ms, text: "…".into(), speaker: None }));
        }
        Ok(out)
    }
    fn finish(&mut self) -> SttResult<Vec<SttEvent>> {
        let chunk = self.seg.flush();
        Ok(self.finals(chunk.into_iter().collect()))
    }
}

impl Transcriber for ScriptedTranscriber {
    fn engine(&self) -> SttEngine {
        SttEngine::WhisperLocal
    }
    fn open_stream(&self, options: StreamOptions) -> SttResult<Box<dyn TranscriptStream>> {
        Ok(Box::new(ScriptedStream {
            seg: Segmenter::new(Box::new(EnergyVad::default()), SegmenterConfig::default()),
            channel: options.channel,
            truth: self.truth.clone(),
            next_id: 0,
            open: None,
        }))
    }
}

struct FakeSpeech {
    engine: Arc<dyn Transcriber>,
    speaker_models: Option<(PathBuf, PathBuf)>,
    refiner: Option<Arc<dyn Transcriber>>,
}

impl FakeSpeech {
    fn scripted() -> Self {
        Self { engine: Arc::new(ScriptedTranscriber { truth: Arc::new(truth()) }), speaker_models: None, refiner: None }
    }
}

/// The after-meeting engine: one utterance of fixed text for any channel with sound in it.
struct FixedRefiner {
    text: &'static str,
}

impl Transcriber for FixedRefiner {
    fn engine(&self) -> SttEngine {
        SttEngine::WhisperLocal
    }
    fn open_stream(&self, _options: StreamOptions) -> SttResult<Box<dyn TranscriptStream>> {
        Err(cilica_stt::SttError::EngineUnavailable("live transcription".into()))
    }
    fn transcribe_recording(&self, samples: &[f32], _language: Option<&str>) -> SttResult<Vec<Utterance>> {
        if samples.iter().all(|s| s.abs() < 1e-4) {
            return Ok(Vec::new());
        }
        let end_ms = (samples.len() / 16) as u32;
        Ok(vec![Utterance { id: 1, start_ms: 0, end_ms, text: self.text.into(), speaker: None }])
    }
}

/// A cloud engine whose service refuses every stretch of audio, as with a key it will not take.
struct RefusingTranscriber;

struct RefusingStream;

impl Transcriber for RefusingTranscriber {
    fn engine(&self) -> SttEngine {
        SttEngine::Soniox
    }
    fn open_stream(&self, _options: StreamOptions) -> SttResult<Box<dyn TranscriptStream>> {
        Ok(Box::new(RefusingStream))
    }
}

impl TranscriptStream for RefusingStream {
    fn push(&mut self, _start: u64, _samples: &[f32]) -> SttResult<Vec<SttEvent>> {
        Err(cilica_stt::SttError::Auth("Soniox".into()))
    }
    fn finish(&mut self) -> SttResult<Vec<SttEvent>> {
        Ok(Vec::new())
    }
}

/// A cloud engine that labels speakers on both channels: one batch of finals per channel.
struct LabellingTranscriber;

struct LabellingStream {
    channel: Option<AudioChannel>,
    sent: bool,
}

impl Transcriber for LabellingTranscriber {
    fn engine(&self) -> SttEngine {
        SttEngine::Soniox
    }
    fn open_stream(&self, options: StreamOptions) -> SttResult<Box<dyn TranscriptStream>> {
        Ok(Box::new(LabellingStream { channel: options.channel, sent: false }))
    }
}

impl TranscriptStream for LabellingStream {
    fn push(&mut self, _start: u64, _samples: &[f32]) -> SttResult<Vec<SttEvent>> {
        if std::mem::replace(&mut self.sent, true) {
            return Ok(Vec::new());
        }
        let say = |id: u64, text: &str, speaker: Option<&str>| {
            let start_ms = id as u32 * 1_000;
            SttEvent::Final(Utterance { id, start_ms, end_ms: start_ms + 900, text: text.into(), speaker: speaker.map(str::to_owned) })
        };
        Ok(match self.channel {
            Some(AudioChannel::System) => vec![say(1, "Hello everyone.", Some("1")), say(2, "Hi there.", Some("2")), say(3, "Mm.", None), say(4, "Shall we start?", Some("1"))],
            _ => vec![say(1, "Morning all.", Some("1")), say(2, "Morning! Coffee?", Some("2")), say(3, "Yes please.", Some("1"))],
        })
    }
    fn finish(&mut self) -> SttResult<Vec<SttEvent>> {
        Ok(Vec::new())
    }
}

#[async_trait::async_trait]
impl SpeechBackend for FakeSpeech {
    fn transcriber(&self, _settings: &Settings) -> AppResult<Arc<dyn Transcriber>> {
        Ok(self.engine.clone())
    }
    async fn speaker_models(&self) -> Option<(PathBuf, PathBuf)> {
        self.speaker_models.clone()
    }
    fn check(&self, _settings: &Settings) -> AppResult<()> {
        Ok(())
    }
    fn refiner(&self, settings: &Settings) -> Option<Arc<dyn Transcriber>> {
        self.refiner.clone().filter(|_| settings.transcription.refine_after_meeting)
    }
}

#[derive(Default)]
struct FakeSummarizer {
    calls: Mutex<Vec<Id>>,
}

#[async_trait::async_trait]
impl MeetingSummarizer for FakeSummarizer {
    async fn summarize_meeting(&self, note_id: Id) -> AppResult<()> {
        self.calls.lock().unwrap().push(note_id);
        Ok(())
    }
}

struct Harness {
    svc: RecordingService,
    db: Db,
    sink: Arc<RecordingSink>,
    summarizer: Arc<FakeSummarizer>,
    dir: tempfile::TempDir,
}

async fn harness(audio: FakeAudio, speech: FakeSpeech) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open_in_memory().await.unwrap().with_data_dir(dir.path());
    let sink = Arc::new(RecordingSink::default());
    let summarizer = Arc::new(FakeSummarizer::default());
    let s = summarizer.clone();
    let mut config = ServiceConfig::new(dir.path());
    config.auto_stop_delay = Duration::from_millis(800);
    let svc = RecordingService::new(
        db.clone(),
        sink.clone(),
        Arc::new(audio),
        Arc::new(speech),
        Arc::new(move || s.clone() as Arc<dyn MeetingSummarizer>),
        config,
        tokio::runtime::Handle::current(),
    );
    Harness { svc, db, sink, summarizer, dir }
}

impl Harness {
    fn events(&self) -> Vec<AppEvent> {
        self.sink.0.lock().unwrap().clone()
    }
    fn phases(&self) -> Vec<RecordingPhase> {
        let mut out: Vec<RecordingPhase> = Vec::new();
        for e in self.events() {
            if let AppEvent::RecordingStateChanged { state } = e
                && out.last() != Some(&state.phase)
            {
                out.push(state.phase);
            }
        }
        out
    }
    fn toasts(&self) -> Vec<ToastRequest> {
        self.events().into_iter().filter_map(|e| if let AppEvent::Toast { toast } = e { Some(toast) } else { None }).collect()
    }
    async fn wait_idle(&self, limit: Duration) {
        let until = Instant::now() + limit;
        while self.svc.is_busy() {
            assert!(Instant::now() < until, "still busy: {:?}", self.svc.state());
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

fn manual(note_id: Option<Id>, app: Option<&str>) -> StartRecordingOptions {
    StartRecordingOptions { note_id, meeting_app: app.map(String::from), trigger: RecordingTrigger::Manual }
}

// ── Tests ─────────────────────────────────────────────────────────────────

/// The full meeting: live segments per channel, levels, auto-stop after the meeting app lets go,
/// kept audio, phases, summary.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn whole_meeting_from_fixtures() {
    let h = harness(FakeAudio::meeting(40.0), FakeSpeech::scripted()).await;
    let state = h.svc.start(manual(None, Some("Zoom"))).await.unwrap();
    assert_eq!(state.phase, RecordingPhase::Recording);
    let note_id = state.note_id.unwrap();
    let note = h.db.get_note(note_id).await.unwrap();
    assert!(note.title.starts_with("Zoom meeting · "), "{}", note.title);
    assert_eq!(note.kind, NoteKind::Meeting);
    assert!(h.svc.is_busy());

    tokio::time::sleep(Duration::from_millis(32_600)).await;
    // Zoom lets go of the mic: auto-stop is scheduled, then happens.
    h.svc.handle_detect_event(DetectEvent::Ended { app_name: "Zoom".into() }).await;
    assert!(h.svc.state().auto_stop_at.is_some());
    h.wait_idle(Duration::from_secs(30)).await;

    // Phases in order.
    assert_eq!(
        h.phases(),
        vec![
            RecordingPhase::Starting,
            RecordingPhase::Recording,
            RecordingPhase::Stopping,
            RecordingPhase::Diarizing,
            RecordingPhase::Summarizing,
            RecordingPhase::Idle
        ]
    );
    // Segments: final only, on the right channel and speaker, near the truth.
    let transcript = h.db.get_transcript(note_id).await.unwrap();
    let you = transcript.speakers.iter().find(|s| s.is_self).unwrap();
    let others = transcript.speakers.iter().find(|s| s.label == "Others").unwrap();
    assert!(transcript.segments.iter().all(|s| s.is_final));
    for t in truth() {
        let seg = transcript
            .segments
            .iter()
            .find(|s| s.text.contains(&t.text))
            .unwrap_or_else(|| panic!("missing {:?} in {:#?}", t.text, transcript.segments));
        assert_eq!(seg.channel, t.channel(), "{}", t.text);
        assert_eq!(seg.speaker_id, Some(if t.channel() == AudioChannel::Mic { you.id } else { others.id }));
        assert!(seg.start_ms.abs_diff(t.start_ms) < 700, "{} starts at {} not {}", t.text, seg.start_ms, t.start_ms);
    }
    // Echo cancellation: the mic hears the call at -16 dB, yet no mic segment carries call text.
    assert_eq!(transcript.segments.iter().filter(|s| s.channel == AudioChannel::Mic).count(), 2, "{:#?}", transcript.segments);

    // Kept audio, relative path, duration, working files gone.
    let (path, duration) = h.db.get_note_audio(note_id).await.unwrap();
    assert_eq!(path.as_deref(), Some(format!("recordings/{note_id}.ogg").as_str()));
    assert!((32_500..34_500).contains(&duration.unwrap()), "{duration:?}");
    let entries: Vec<String> = std::fs::read_dir(h.dir.path().join("recordings")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(entries, vec![format!("{note_id}.ogg")]);
    assert!(transcript.has_audio);
    let note = h.db.get_note(note_id).await.unwrap();
    assert!(note.started_at.is_some() && note.ended_at.is_some());

    // Events.
    let events = h.events();
    let levels = events.iter().filter(|e| matches!(e, AppEvent::AudioLevels { .. })).count();
    assert!((550..800).contains(&levels), "about 20 level events a second: {levels}");
    assert!(events.iter().any(|e| matches!(e, AppEvent::AudioLevels { levels } if levels.system > 0.3)));
    let segs = events.iter().filter(|e| matches!(e, AppEvent::TranscriptSegment { .. })).count();
    assert!(segs >= 12, "partials and finals emitted: {segs}");
    assert!(events.iter().any(|e| matches!(e, AppEvent::MeetingEnded { app_name } if app_name == "Zoom")));
    assert!(events.iter().any(|e| matches!(e, AppEvent::DataChanged { scopes, .. } if scopes.contains(&DataScope::Transcript))));
    assert!(h.toasts().iter().any(|t| t.action == Some(ToastAction::KeepRecording)));
    assert_eq!(*h.summarizer.calls.lock().unwrap(), vec![note_id]);
    // Search sees the transcript.
    let hits = h.db.search_notes("brochures", 10).await.unwrap();
    assert_eq!(hits.first().map(|h| h.note.id), Some(note_id));
}

/// No system-audio permission and a microphone unplugged mid-recording: the recording carries on
/// with the mic only and says what happened.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mic_only_and_unplugged() {
    let mut audio = FakeAudio::meeting(6.0);
    audio.system_permission = PermissionStatus::Denied;
    audio.mic = audio.mic.clone().failing_after(Duration::from_millis(2_000));
    let mic = audio.mic.clone();
    let h = harness(audio, FakeSpeech::scripted()).await;
    let note_id = h.svc.start(manual(None, None)).await.unwrap().note_id.unwrap();
    assert!(h.db.get_note(note_id).await.unwrap().title.starts_with("Recording · "));
    tokio::time::sleep(Duration::from_millis(5_500)).await;
    h.svc.stop().await.unwrap();
    h.wait_idle(Duration::from_secs(20)).await;

    let titles: Vec<String> = h.toasts().into_iter().map(|t| t.title).collect();
    assert!(titles.contains(&"Recording your microphone only".to_string()), "{titles:?}");
    assert!(titles.contains(&"Microphone disconnected".to_string()), "{titles:?}");
    assert!(titles.contains(&"Microphone back".to_string()), "{titles:?}");
    assert!(mic.opens() >= 2);
    let (path, duration) = h.db.get_note_audio(note_id).await.unwrap();
    assert!(path.is_some() && duration.unwrap() > 5_000);
    let ch = cilica_audio::decode_ogg_opus(&h.dir.path().join(path.unwrap())).unwrap();
    // Opus codes the pair jointly, so a trace of the left channel shows on the right.
    let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
    assert!(rms(&ch[1]) < 0.1 * rms(&ch[0]), "no system audio recorded: {} vs {}", rms(&ch[1]), rms(&ch[0]));
}

/// The speech model is missing: nothing starts, no note is left behind, the error names the fix.
#[tokio::test]
async fn missing_model_is_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let models = cilica_stt::ModelManager::new(dir.path().join("models"), Arc::new(NullSink));
    let db = Db::open_in_memory().await.unwrap();
    let svc = RecordingService::new(
        db.clone(),
        Arc::new(NullSink),
        Arc::new(FakeAudio::meeting(1.0)),
        Arc::new(crate::LocalModels { models, keys: Arc::new(|_| None) }),
        Arc::new(|| Arc::new(FakeSummarizer::default()) as Arc<dyn MeetingSummarizer>),
        ServiceConfig::new(dir.path()),
        tokio::runtime::Handle::current(),
    );
    let err = svc.start(manual(None, None)).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Unavailable);
    assert!(err.message.contains("Settings › Transcription"), "{}", err.message);
    assert!(db.list_notes().await.unwrap().is_empty());
    assert_eq!(svc.state().phase, RecordingPhase::Idle);

    let mut settings = db.get_settings().await.unwrap();
    settings.transcription.engine = SttEngine::Deepgram;
    db.set_settings(&settings).await.unwrap();
    let err = svc.start(manual(None, None)).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Unavailable);
    assert!(err.message.contains("Deepgram"), "{}", err.message);
}

/// Both channels fail the same way when a service refuses the key: one warning, not one each.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_refused_key_is_reported_once_for_both_channels() {
    let speech = FakeSpeech { engine: Arc::new(RefusingTranscriber), ..FakeSpeech::scripted() };
    let h = harness(FakeAudio::meeting(4.0), speech).await;
    h.svc.start(manual(None, None)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    h.svc.stop().await.unwrap();
    h.wait_idle(Duration::from_secs(20)).await;
    let stopped: Vec<_> = h.toasts().into_iter().filter(|t| t.title == "Live transcript stopped").collect();
    assert_eq!(stopped.len(), 1, "{stopped:?}");
    assert!(stopped[0].description.as_deref().unwrap_or_default().contains("Soniox"));
}

/// A service that labels speakers: each label becomes a person, live; the first voice on the mic
/// is "You", unlabelled call audio stays "Others".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_speaker_labels_become_people() {
    let speech = FakeSpeech { engine: Arc::new(LabellingTranscriber), ..FakeSpeech::scripted() };
    let h = harness(FakeAudio::meeting(3.0), speech).await;
    let mut settings = h.db.get_settings().await.unwrap();
    settings.transcription.engine = SttEngine::Soniox;
    h.db.set_settings(&settings).await.unwrap();

    let state = h.svc.start(manual(None, None)).await.unwrap();
    assert!(state.live_speakers, "Soniox labels speakers live");
    let note_id = state.note_id.unwrap();
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    h.svc.stop().await.unwrap();
    h.wait_idle(Duration::from_secs(20)).await;

    let transcript = h.db.get_transcript(note_id).await.unwrap();
    let who = |text: &str| {
        let segment = transcript.segments.iter().find(|s| s.text == text).unwrap_or_else(|| panic!("no segment {text}"));
        transcript.speakers.iter().find(|p| Some(p.id) == segment.speaker_id).map(|p| p.label.clone()).unwrap_or_default()
    };
    // Numbers follow the order voices are first heard, across both channels.
    let (call1, call2, room) = (who("Hello everyone."), who("Hi there."), who("Morning! Coffee?"));
    let mut people = vec![call1.clone(), call2.clone(), room.clone()];
    people.sort();
    assert_eq!(people, vec!["Person 1", "Person 2", "Person 3"], "three different people");
    assert_eq!(who("Shall we start?"), call1, "the same label is the same person");
    assert_eq!(who("Mm."), "Others");
    // The first voice on the mic is you; labels are per channel, so mic "1" is not call "1".
    assert_eq!(who("Morning all."), "You");
    assert_eq!(who("Yes please."), "You");
}

/// Live labels off in Settings, or a service without them: speakers wait for the meeting's end.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_speakers_follow_the_setting_and_the_service() {
    let h = harness(FakeAudio::meeting(2.0), FakeSpeech { engine: Arc::new(LabellingTranscriber), ..FakeSpeech::scripted() }).await;
    let mut settings = h.db.get_settings().await.unwrap();
    for (engine, on, expected) in [(SttEngine::Soniox, false, false), (SttEngine::OpenAi, true, false), (SttEngine::WhisperLocal, true, false), (SttEngine::Deepgram, true, true)] {
        settings.transcription.engine = engine;
        settings.transcription.live_speaker_labels = on;
        assert_eq!(settings.transcription.live_speakers(), expected, "{engine:?}, setting {on}");
    }
    settings.transcription.engine = SttEngine::Soniox;
    settings.transcription.live_speaker_labels = false;
    h.db.set_settings(&settings).await.unwrap();
    let state = h.svc.start(manual(None, None)).await.unwrap();
    assert!(!state.live_speakers);
    h.svc.stop().await.unwrap();
    h.wait_idle(Duration::from_secs(20)).await;
}

/// With "Identify speakers after the meeting" off, the step is skipped altogether.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn identifying_speakers_after_the_meeting_can_be_turned_off() {
    for identify in [true, false] {
        let h = harness(FakeAudio::meeting(2.0), FakeSpeech::scripted()).await;
        let mut settings = h.db.get_settings().await.unwrap();
        settings.transcription.identify_speakers_after_meeting = identify;
        h.db.set_settings(&settings).await.unwrap();
        h.svc.start(manual(None, None)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1_000)).await;
        h.svc.stop().await.unwrap();
        h.wait_idle(Duration::from_secs(20)).await;
        let diarized = h.events().iter().any(|e| matches!(e, AppEvent::RecordingStateChanged { state } if state.phase == RecordingPhase::Diarizing));
        assert_eq!(diarized, identify, "identify after the meeting: {identify}");
    }
}

/// Two short recordings into one note: the second continues the timeline and the kept audio.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn second_recording_appends_to_the_note() {
    let h = harness(FakeAudio::meeting(6.0), FakeSpeech::scripted()).await;
    let note_id = h.svc.start(manual(None, None)).await.unwrap().note_id.unwrap();
    tokio::time::sleep(Duration::from_millis(3_000)).await;
    h.svc.stop().await.unwrap();
    h.wait_idle(Duration::from_secs(20)).await;
    let first = h.db.get_note_audio(note_id).await.unwrap().1.unwrap();

    let state = h.svc.start(manual(Some(note_id), None)).await.unwrap();
    assert_eq!(state.note_id, Some(note_id));
    tokio::time::sleep(Duration::from_millis(2_000)).await;
    h.svc.stop().await.unwrap();
    h.wait_idle(Duration::from_secs(20)).await;
    let (path, total) = h.db.get_note_audio(note_id).await.unwrap();
    assert!(total.unwrap().abs_diff(first + 2_000) < 400, "{first} + 2000 vs {total:?}");
    let ch = cilica_audio::decode_ogg_opus(&h.dir.path().join(path.unwrap())).unwrap();
    assert!((ch[0].len() as i64 - total.unwrap() as i64 * 16).abs() < 1_000);
    assert_eq!(h.db.list_notes().await.unwrap().len(), 1);
}

/// The app stopped mid-recording: at launch the WAV is repaired, the rest transcribed, the audio
/// kept and the summary written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn interrupted_recording_is_finished_at_launch() {
    let h = harness(FakeAudio::meeting(1.0), FakeSpeech::scripted()).await;
    let note = h.db.create_note(cilica_store::NewNote { title: "Standup".into(), kind: NoteKind::Meeting, ..Default::default() }).await.unwrap();
    let files = RecordingFiles::new(h.dir.path(), note.id);
    let sidecar = Sidecar { note_id: note.id, started_at: now(), offset_ms: 0, previous_audio: None, stage: Stage::Recording };
    files.write_sidecar(&sidecar).unwrap();
    // Twelve seconds of call audio, mic silent, written without its final header (a crash).
    let system = mono("meeting_system.wav");
    let mut w = WavWriter::create(&files.wav(), 2, 16_000).unwrap();
    let frames: Vec<f32> = system[..16_000 * 12].iter().flat_map(|s| [0.0, *s]).collect();
    w.write_interleaved(&frames).unwrap();
    drop(w);
    corrupt_header(&files.wav());

    h.svc.startup().await;
    assert!(!h.svc.is_busy());
    let transcript = h.db.get_transcript(note.id).await.unwrap();
    let texts: Vec<&str> = transcript.segments.iter().map(|s| s.text.as_str()).collect();
    assert!(texts.iter().any(|t| t.contains("third of March")), "{texts:?}");
    assert!(transcript.segments.iter().all(|s| s.channel == AudioChannel::System));
    let (path, duration) = h.db.get_note_audio(note.id).await.unwrap();
    assert_eq!(path.as_deref(), Some(format!("recordings/{}.ogg", note.id).as_str()));
    assert!(duration.unwrap().abs_diff(12_000) < 50);
    assert!(!files.sidecar().exists() && !files.wav().exists());
    assert_eq!(*h.summarizer.calls.lock().unwrap(), vec![note.id]);
    assert!(h.toasts().iter().any(|t| t.title == "Finishing an interrupted recording"));
    assert!(h.db.get_note(note.id).await.unwrap().ended_at.is_some());
}

/// A captured recording whose live transcript is done, finished at launch with an
/// after-meeting engine that returns `refined` for the call audio.
async fn finish_with_refiner(refined: &'static str, live: &str) -> (Harness, Id) {
    finish_with_refiner_on(SttEngine::WhisperLocal, refined, live).await
}

/// [`finish_with_refiner`], with `engine` as the live transcription engine in Settings.
async fn finish_with_refiner_on(engine: SttEngine, refined: &'static str, live: &str) -> (Harness, Id) {
    let speech = FakeSpeech { refiner: Some(Arc::new(FixedRefiner { text: refined })), ..FakeSpeech::scripted() };
    let h = harness(FakeAudio::meeting(1.0), speech).await;
    let mut settings = h.db.get_settings().await.unwrap();
    settings.transcription.engine = engine;
    h.db.set_settings(&settings).await.unwrap();
    let note = h.db.create_note(cilica_store::NewNote { title: "Call".into(), kind: NoteKind::Meeting, ..Default::default() }).await.unwrap();
    let files = RecordingFiles::new(h.dir.path(), note.id);
    files.write_sidecar(&Sidecar { note_id: note.id, started_at: now(), offset_ms: 0, previous_audio: None, stage: Stage::Captured }).unwrap();
    // Twelve seconds of call audio (the fixture opens quietly), mic silent.
    let system = mono("meeting_system.wav");
    let mut w = WavWriter::create(&files.wav(), 2, 16_000).unwrap();
    let frames: Vec<f32> = system[..16_000 * 12].iter().flat_map(|s| [0.0, *s]).collect();
    w.write_interleaved(&frames).unwrap();
    w.finish().unwrap();
    let (_, others) = h.db.ensure_channel_speakers(note.id).await.unwrap();
    let segment = Segment {
        id: new_id(),
        note_id: note.id,
        speaker_id: Some(others.id),
        channel: AudioChannel::System,
        start_ms: 100,
        end_ms: 11_900,
        text: live.into(),
        is_final: true,
    };
    h.db.upsert_segment(&segment).await.unwrap();
    h.svc.startup().await;
    (h, note.id)
}

/// The after-meeting pass replaces the live transcript of the channel it redid.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn after_meeting_pass_replaces_the_live_transcript() {
    let (h, note_id) = finish_with_refiner("The careful transcript of what was said on the call.", "the careless live text of the call").await;
    let segments = h.db.get_transcript(note_id).await.unwrap().segments;
    let texts: Vec<&str> = segments.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, vec!["The careful transcript of what was said on the call."]);
    assert_eq!(segments[0].channel, AudioChannel::System);
}

/// A pass that comes back with far less text than the live transcript went wrong: keep live.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn after_meeting_pass_that_loses_text_keeps_the_live_transcript() {
    let (h, note_id) = finish_with_refiner("Hm.", "one two three four five six seven eight nine ten").await;
    let texts: Vec<String> = h.db.get_transcript(note_id).await.unwrap().segments.into_iter().map(|s| s.text).collect();
    assert_eq!(texts, vec!["one two three four five six seven eight nine ten".to_string()]);
}

/// A meeting transcribed by a cloud service keeps that transcript: no second pass on this computer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn after_meeting_pass_is_skipped_for_cloud_engines() {
    let live = "what the cloud service heard on the call";
    let (h, note_id) = finish_with_refiner_on(SttEngine::Soniox, "The careful transcript of what was said on the call.", live).await;
    let texts: Vec<String> = h.db.get_transcript(note_id).await.unwrap().segments.into_iter().map(|s| s.text).collect();
    assert_eq!(texts, vec![live.to_string()]);
}

/// Zero the RIFF and data sizes, as a WAV looks when the app dies before the header is rewritten.
fn corrupt_header(path: &Path) {
    use std::io::{Seek, SeekFrom, Write};
    let mut f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    f.seek(SeekFrom::Start(4)).unwrap();
    f.write_all(&36u32.to_le_bytes()).unwrap();
    f.seek(SeekFrom::Start(40)).unwrap();
    f.write_all(&0u32.to_le_bytes()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retention_removes_old_audio_but_keeps_the_transcript() {
    let h = harness(FakeAudio::meeting(1.0), FakeSpeech::scripted()).await;
    let note = h.db.create_note(cilica_store::NewNote { title: "Old call".into(), ..Default::default() }).await.unwrap();
    let rel = format!("recordings/{}.ogg", note.id);
    let path = h.dir.path().join(&rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"OggS").unwrap();
    h.db.set_note_audio(note.id, Some(&rel), Some(61_000)).await.unwrap();
    assert_eq!(h.svc.apply_retention().await.unwrap(), 0);
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(std::time::SystemTime::now() - Duration::from_secs(31 * 86_400)).unwrap();
    let mut settings = h.db.get_settings().await.unwrap();
    settings.recording.audio_retention_days = 0;
    h.db.set_settings(&settings).await.unwrap();
    assert_eq!(h.svc.apply_retention().await.unwrap(), 0, "0 keeps for ever");
    settings.recording.audio_retention_days = 30;
    h.db.set_settings(&settings).await.unwrap();
    assert_eq!(h.svc.apply_retention().await.unwrap(), 1);
    assert!(!path.exists());
    assert_eq!(h.db.get_note_audio(note.id).await.unwrap(), (None, Some(61_000)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn detection_asks_first_or_starts() {
    let h = harness(FakeAudio::meeting(4.0), FakeSpeech::scripted()).await;
    let zoom = || DetectEvent::Detected(MeetingDetected { app_name: "Zoom".into(), app_id: Some("us.zoom.xos".into()) });
    h.svc.handle_detect_event(zoom()).await;
    let ask = h.toasts().pop().unwrap();
    assert_eq!(ask.action, Some(ToastAction::StartRecording { meeting_app: Some("Zoom".into()) }));
    assert!(!h.svc.is_capturing());
    assert!(h.events().iter().any(|e| matches!(e, AppEvent::MeetingDetected { .. })));

    let mut settings = h.db.get_settings().await.unwrap();
    settings.recording.detect_mode = DetectMode::AutoStart;
    h.db.set_settings(&settings).await.unwrap();
    h.svc.handle_detect_event(zoom()).await;
    assert!(h.svc.is_capturing());
    assert_eq!(h.svc.state().meeting_app.as_deref(), Some("Zoom"));
    assert!(h.toasts().iter().any(|t| t.action == Some(ToastAction::StopRecording)));

    // The meeting ends, then the person keeps recording: no auto-stop.
    h.svc.handle_detect_event(DetectEvent::Ended { app_name: "Zoom".into() }).await;
    assert!(h.svc.state().auto_stop_at.is_some());
    assert!(h.svc.keep_recording().auto_stop_at.is_none());
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    assert!(h.svc.is_capturing(), "kept recording");
    // A manual toggle stops it.
    h.svc.toggle(RecordingTrigger::Hotkey).await.unwrap();
    assert!(!h.svc.is_capturing());
    h.wait_idle(Duration::from_secs(20)).await;
}

// ── With the real models (ignored) ─────────────────────────────────────────

fn test_models() -> PathBuf {
    std::env::var_os("CILICA_TEST_MODELS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-models"))
}

/// Word error rate by edit distance, after lower-casing and dropping punctuation.
fn wer(reference: &str, hypothesis: &str) -> f32 {
    let norm = |s: &str| -> Vec<String> {
        s.split_whitespace().map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()).filter(|w| !w.is_empty()).collect()
    };
    let (r, h) = (norm(reference), norm(hypothesis));
    let mut d: Vec<Vec<usize>> = (0..=r.len()).map(|i| (0..=h.len()).map(|j| if i == 0 { j } else if j == 0 { i } else { 0 }).collect()).collect();
    for i in 1..=r.len() {
        for j in 1..=h.len() {
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + usize::from(r[i - 1] != h[j - 1]));
        }
    }
    d[r.len()][h.len()] as f32 / r.len().max(1) as f32
}

/// The same meeting through Whisper (default: tiny; `CILICA_TEST_WHISPER=ggml-large-v3-turbo-q5_0.bin`
/// for the default model), Silero VAD, pyannote segmentation and CAM++: accuracy, no echo
/// duplicates, two people found on the call, and a Saved person recognised (named or suggested).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs Whisper, Silero VAD and the speaker models in target/test-models"]
async fn whole_meeting_with_real_models() {
    let dir = test_models();
    let whisper = dir.join(std::env::var("CILICA_TEST_WHISPER").unwrap_or_else(|_| "ggml-tiny.bin".into()));
    let models = (dir.join("pyannote-segmentation-3.0.onnx"), dir.join("campplus-voxceleb.onnx"));
    let engine = cilica_stt::WhisperLocal::load(cilica_stt::WhisperOptions::new(whisper.clone(), Some(dir.join("silero_vad.onnx")))).unwrap();
    let h = harness(FakeAudio::meeting(40.0), FakeSpeech { engine: Arc::new(engine), speaker_models: Some(models.clone()), refiner: None }).await;

    // Karen is a Saved person from an earlier meeting (her first sentence).
    let system = mono("meeting_system.wav");
    let karen_turn = &truth()[2];
    let mut d = cilica_diarize::Diarizer::load(&models.0, &models.1).unwrap();
    let karen_print = d.embed(&system[karen_turn.start_ms as usize * 16..karen_turn.end_ms as usize * 16]).unwrap();
    let karen = h.db.create_person("Karen", Some(&karen_print)).await.unwrap();

    let started = Instant::now();
    let note_id = h.svc.start(manual(None, None)).await.unwrap().note_id.unwrap();
    // Track how far behind speech each final lands.
    let mut lag_ms = 0u64;
    let mut seen = std::collections::HashSet::new();
    while started.elapsed() < Duration::from_millis(32_500) {
        for e in h.events() {
            if let AppEvent::TranscriptSegment { segment } = e
                && segment.is_final
                && seen.insert(segment.id)
            {
                let lag = (started.elapsed().as_millis() as u64).saturating_sub(segment.end_ms as u64);
                println!("final {:?} at {} ms, {lag} ms after its speech ended", segment.channel, segment.end_ms);
                lag_ms = lag_ms.max(lag);
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    h.svc.stop().await.unwrap();
    h.wait_idle(Duration::from_secs(120)).await;

    let transcript = h.db.get_transcript(note_id).await.unwrap();
    for s in &transcript.segments {
        let who = transcript.speakers.iter().find(|p| Some(p.id) == s.speaker_id).map(|p| p.label.as_str()).unwrap_or("?");
        println!("[{:>6}-{:>6}] {:?} {who}: {}", s.start_ms, s.end_ms, s.channel, s.text);
    }
    let reference: Vec<String> = truth().iter().map(|t| t.text.clone()).collect();
    let hypothesis: Vec<String> = transcript.segments.iter().map(|s| s.text.clone()).collect();
    let w = wer(&reference.join(" "), &hypothesis.join(" "));
    println!("{}: WER {:.1}% over both channels, worst live lag {lag_ms} ms", whisper.file_name().unwrap().to_string_lossy(), w * 100.0);
    assert!(w < 0.15, "WER {w}");
    assert!(lag_ms < 3_000, "a final landed {lag_ms} ms after its speech");

    // No call audio transcribed on the mic.
    let mic: Vec<&Segment> = transcript.segments.iter().filter(|s| s.channel == AudioChannel::Mic).collect();
    assert_eq!(mic.len(), 2, "{mic:#?}");

    // Two people on the call, each turn given to the right one.
    let call: Vec<&Speaker> = transcript.speakers.iter().filter(|s| s.label.starts_with("Person")).collect();
    assert_eq!(call.len(), 2, "{:#?}", transcript.speakers);
    let speaker_of = |t: &Turn| {
        transcript.segments.iter().filter(|s| s.channel == AudioChannel::System && s.start_ms < t.end_ms && s.end_ms > t.start_ms).map(|s| s.speaker_id).collect::<Vec<_>>()
    };
    let turns = truth();
    let daniel: Vec<_> = turns.iter().filter(|t| t.speaker == "Daniel").flat_map(speaker_of).collect();
    let karen_ids: Vec<_> = turns.iter().filter(|t| t.speaker == "Karen").flat_map(speaker_of).collect();
    assert!(daniel.iter().all(|s| *s == daniel[0]) && karen_ids.iter().all(|s| *s == karen_ids[0]) && daniel[0] != karen_ids[0]);

    // Karen's voice is known: her speaker is named Karen when Cilica is sure, suggested when it
    // is less sure, never both. Learning waits for a confirmation either way.
    let suggestions = h.db.speaker_suggestions(note_id, cilica_diarize::SUGGEST_THRESHOLD).await.unwrap();
    println!("suggestions: {suggestions:?}");
    let person_of = |id: Option<Id>| transcript.speakers.iter().find(|s| Some(s.id) == id).and_then(|s| s.person_id);
    let named = person_of(karen_ids[0]) == Some(karen.id);
    let suggested = suggestions.iter().any(|s| Some(s.speaker_id) == karen_ids[0] && s.person_id == karen.id);
    assert!(named != suggested, "named {named}, suggested {suggested}");
    assert_eq!(h.db.get_person(karen.id).await.unwrap().sample_count, 1);
    // Daniel is nobody Cilica knows.
    assert_eq!(person_of(daniel[0]), None);
}
