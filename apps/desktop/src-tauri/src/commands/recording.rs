//! Recording, permissions, speech models, transcripts, speakers, people. Owner: Audio agent.
//!
//! Every command here is part of the IPC contract in `src/ipc/bindings.ts`. Keep names, arguments and
//! return types as they are unless the orchestrator agrees to a change.
//!
//! The work lives in `margin-pipeline` ([`RecordingService`]); this module builds it at startup,
//! keeps the meeting detector in step with Settings, and maps commands onto it and the store.

use std::sync::Arc;

use margin_domain::*;
use margin_pipeline::{DeviceAudio, LocalModels, RecordingService, ServiceConfig, SpeechBackend, SttKeys, SummarizerProvider};
use margin_stt::ModelManager;
use tauri::{AppHandle, Manager, State};

use crate::core::Core;
use crate::shell::Shell;

/// Managed state for this area.
pub struct RecordingArea {
    pub service: RecordingService,
    pub models: Arc<ModelManager>,
}

/// Routes toasts through the shell, which holds them until the UI is listening (an interrupted
/// recording is finished at launch, before the window has painted). Everything else goes
/// straight to the UI.
struct AreaSink {
    inner: Events,
    app: AppHandle,
}

impl EventSink for AreaSink {
    fn emit(&self, event: AppEvent) {
        match event {
            AppEvent::Toast { toast } => match self.app.try_state::<Shell>() {
                Some(shell) => shell.toast(toast),
                None => self.inner.emit(AppEvent::Toast { toast }),
            },
            other => self.inner.emit(other),
        }
    }
}

fn area(app: &AppHandle) -> AppResult<tauri::State<'_, RecordingArea>> {
    app.try_state::<RecordingArea>().ok_or_else(|| AppError::unavailable("Recording is still starting up. Try again in a moment."))
}

/// Start a recording if idle, stop it if recording. The global hotkey and the tray menu (Core) call this.
pub async fn toggle_recording(app: &AppHandle, trigger: RecordingTrigger) -> AppResult<RecordingState> {
    let service = area(app)?.service.clone();
    service.toggle(trigger).await
}

/// True from the moment a recording starts until its summary is written. The scheduler (AI) checks
/// this so tasks never run during a meeting.
pub fn is_recording(app: &AppHandle) -> bool {
    app.try_state::<RecordingArea>().is_some_and(|a| a.service.is_busy())
}

/// Called once at startup, after the database is open. Builds the recording service, starts the
/// meeting detector when "Listening for meetings" is on (and follows Settings), and, in the
/// background, finishes any recording interrupted by a quit or crash and applies audio retention.
pub fn setup(app: &AppHandle, core: &Core) -> AppResult<()> {
    let events: Events = Arc::new(AreaSink { inner: core.events.clone(), app: app.clone() });
    let models = ModelManager::new(core.data_dir.join("models"), events.clone());
    models.set_hardware(hardware_profile());
    let summarizer_app = app.clone();
    let summarizer: SummarizerProvider = Arc::new(move || super::assistant::summarizer(&summarizer_app));
    let service = RecordingService::new(
        core.db.clone(),
        events,
        Arc::new(DeviceAudio),
        Arc::new(LocalModels { models: models.clone(), keys: stt_keys() }),
        summarizer,
        ServiceConfig::new(&core.data_dir),
        tauri::async_runtime::handle().inner().clone(),
    );
    app.manage(RecordingArea { service: service.clone(), models });

    let settings = tauri::async_runtime::block_on(core.db.get_settings())?;
    service.apply_settings(&settings);
    let listener = service.clone();
    core.on_event(move |event| {
        if let AppEvent::SettingsChanged { settings } = event {
            listener.apply_settings(settings);
        }
    });
    tauri::async_runtime::spawn(async move { service.startup().await });
    Ok(())
}

fn people_changed(core: &Core) {
    core.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::People, DataScope::Transcript], note_id: None });
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| AppError::internal(format!("Margin could not finish that: {e}")))
}

#[tauri::command]
#[specta::specta]
pub async fn get_recording_state(area: State<'_, RecordingArea>) -> AppResult<RecordingState> {
    Ok(area.service.state())
}

#[tauri::command]
#[specta::specta]
pub async fn start_recording(area: State<'_, RecordingArea>, options: StartRecordingOptions) -> AppResult<RecordingState> {
    area.service.start(options).await
}

/// Stops capture and starts finalising (diarize, then summarise).
#[tauri::command]
#[specta::specta]
pub async fn stop_recording(area: State<'_, RecordingArea>) -> AppResult<RecordingState> {
    area.service.stop().await
}

/// Cancel a pending auto-stop after the meeting app released the mic.
#[tauri::command]
#[specta::specta]
pub async fn keep_recording(area: State<'_, RecordingArea>) -> AppResult<RecordingState> {
    Ok(area.service.keep_recording())
}

#[tauri::command]
#[specta::specta]
pub async fn list_input_devices() -> AppResult<Vec<AudioDevice>> {
    Ok(blocking(margin_audio::list_input_devices).await??)
}

#[tauri::command]
#[specta::specta]
pub async fn get_permissions() -> AppResult<Permissions> {
    blocking(margin_audio::permissions::all).await
}

/// Trigger the OS prompt where one exists.
#[tauri::command]
#[specta::specta]
pub async fn request_permission(kind: PermissionKind) -> AppResult<PermissionStatus> {
    blocking(move || margin_audio::permissions::request(kind)).await
}

/// Open the matching pane of the OS privacy settings.
#[tauri::command]
#[specta::specta]
pub async fn open_permission_settings(kind: PermissionKind) -> AppResult<()> {
    Ok(blocking(move || margin_audio::permissions::open_settings(kind)).await??)
}

#[tauri::command]
#[specta::specta]
pub async fn list_stt_models(area: State<'_, RecordingArea>) -> AppResult<Vec<SttModel>> {
    Ok(area.models.list())
}

/// Starts the download; progress arrives as ModelDownload events.
#[tauri::command]
#[specta::specta]
pub async fn download_stt_model(area: State<'_, RecordingArea>, model_id: String) -> AppResult<()> {
    Ok(area.models.start_download(&model_id)?)
}

/// Download a speech model if needed, then make it the one Margin transcribes with. Returns at
/// once; progress arrives as ModelDownload events and Settings changes when the model is ready.
#[tauri::command]
#[specta::specta]
pub async fn download_and_use_stt_model(core: State<'_, Core>, area: State<'_, RecordingArea>, model_id: String) -> AppResult<()> {
    let ready = area.models.download_and_wait(&model_id)?;
    let core = core.inner().clone();
    tauri::async_runtime::spawn(async move {
        // A failed or cancelled download is already reported through its progress event.
        if let Ok(Ok(())) = ready.await
            && let Err(error) = use_stt_model(&core, &model_id).await
        {
            tracing::warn!(message = %error.message, "could not switch to the downloaded speech model");
        }
    });
    Ok(())
}

/// Point Settings at a local Whisper model.
async fn use_stt_model(core: &Core, model_id: &str) -> AppResult<()> {
    let mut settings = core.db.get_settings().await?;
    if settings.transcription.engine == SttEngine::WhisperLocal && settings.transcription.model_id == model_id {
        return Ok(());
    }
    settings.transcription.engine = SttEngine::WhisperLocal;
    settings.transcription.model_id = model_id.to_string();
    core.db.set_settings(&settings).await?;
    let saved = core.db.get_settings().await?;
    core.events.emit(AppEvent::SettingsChanged { settings: saved });
    Ok(())
}

// ── Cloud speech keys ────────────────────────────────────────────────────

const KEYCHAIN_SERVICE: &str = "app.margin.desktop";

/// Keychain account for a cloud speech provider: "stt.soniox", "stt.open_ai_compatible".
fn stt_account(engine: SttEngine) -> String {
    let name = serde_json::to_value(engine).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
    format!("stt.{name}")
}

fn read_stt_key(engine: SttEngine) -> Option<String> {
    if !engine.is_cloud() {
        return None;
    }
    keyring::Entry::new(KEYCHAIN_SERVICE, &stt_account(engine)).ok()?.get_password().ok().filter(|k| !k.trim().is_empty())
}

/// How the pipeline reads keys: from the keychain, each time a stream opens.
fn stt_keys() -> SttKeys {
    Arc::new(read_stt_key)
}

fn stt_status(engine: SttEngine, problem: Option<String>) -> SttProviderStatus {
    SttProviderStatus { engine, has_key: read_stt_key(engine).is_some(), problem }
}

#[tauri::command]
#[specta::specta]
pub async fn get_stt_provider_status(engine: SttEngine) -> AppResult<SttProviderStatus> {
    blocking(move || stt_status(engine, None)).await
}

/// Check the key with the provider, then save it to the keychain. A rejected key is not kept;
/// one that could not be checked (offline) is kept with a note.
#[tauri::command]
#[specta::specta]
pub async fn set_stt_provider_key(core: State<'_, Core>, engine: SttEngine, key: String) -> AppResult<SttProviderStatus> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(AppError::invalid("Paste a key first."));
    }
    if !engine.is_cloud() {
        return Err(AppError::invalid("Local Whisper does not need a key."));
    }
    let settings = core.db.get_settings().await?;
    let base_url = settings.transcription.cloud.base_url.clone();
    let checked = tokio::time::timeout(std::time::Duration::from_secs(15), margin_stt::cloud::verify_key(engine, &key, base_url.as_deref())).await;
    let problem = match checked {
        Ok(Ok(())) => None,
        Ok(Err(error @ (margin_stt::SttError::Auth(_) | margin_stt::SttError::NotAllowed(_) | margin_stt::SttError::Provider(_)))) => {
            return Err(error.into());
        }
        Ok(Err(_)) | Err(_) => Some("Saved. Margin could not check the key just now.".to_string()),
    };
    blocking(move || {
        keyring::Entry::new(KEYCHAIN_SERVICE, &stt_account(engine))
            .and_then(|e| e.set_password(&key))
            .map_err(|_| AppError::unavailable("Margin could not save the key in your keychain. Check that the keychain is unlocked and try again."))?;
        Ok(stt_status(engine, problem))
    })
    .await?
}

#[tauri::command]
#[specta::specta]
pub async fn clear_stt_provider_key(engine: SttEngine) -> AppResult<SttProviderStatus> {
    blocking(move || {
        match keyring::Entry::new(KEYCHAIN_SERVICE, &stt_account(engine)).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(stt_status(engine, None)),
            Err(_) => Err(AppError::unavailable("Margin could not remove the key from your keychain. Check that it is unlocked and try again.")),
        }
    })
    .await?
}

/// Stream a short public-domain speech clip to the transcription engine in Settings, in real
/// time as a meeting would, and report what came back. Progress arrives as
/// `transcription_test_progress` events. Takes about as long as the clip (11 s).
#[tauri::command]
#[specta::specta]
pub async fn test_transcription(core: State<'_, Core>, area: State<'_, RecordingArea>) -> AppResult<TranscriptionTest> {
    if area.service.is_busy() {
        return Err(AppError::invalid("Finish the recording first, then run the test."));
    }
    let settings = core.db.get_settings().await?;
    let engine = settings.transcription.engine;
    let speech = LocalModels { models: area.models.clone(), keys: stt_keys() };
    let events = core.events.clone();
    blocking(move || {
        let transcriber = speech.transcriber(&settings)?;
        let run = margin_stt::sample::stream_sample(transcriber.as_ref(), true, &mut |text| {
            events.emit(AppEvent::TranscriptionTestProgress { text: text.to_string() });
        })?;
        Ok(TranscriptionTest {
            engine,
            text: run.text,
            expected: margin_stt::sample::SAMPLE_TEXT.to_string(),
            first_words_ms: run.first_words.map(|d| d.as_millis() as u32),
            took_ms: run.took.as_millis() as u32,
        })
    })
    .await?
}

/// What this computer can run, and whether a cloud service would serve it better.
#[tauri::command]
#[specta::specta]
pub async fn get_hardware_profile(area: State<'_, RecordingArea>) -> AppResult<HardwareProfile> {
    Ok(area.models.hardware().unwrap_or_else(hardware_profile))
}

/// Cores, memory and chip, read once at startup.
fn hardware_profile() -> HardwareProfile {
    let cores = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(4);
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let ram_gb = (system.total_memory() as f64 / 1_073_741_824.0).round() as u32;
    let apple_silicon = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    margin_stt::recommend::profile(cores, ram_gb, apple_silicon)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_stt_model_download(area: State<'_, RecordingArea>, model_id: String) -> AppResult<()> {
    area.models.cancel(&model_id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_stt_model(area: State<'_, RecordingArea>, model_id: String) -> AppResult<()> {
    Ok(area.models.delete(&model_id)?)
}

#[tauri::command]
#[specta::specta]
pub async fn get_transcript(core: State<'_, Core>, note_id: Id) -> AppResult<Transcript> {
    Ok(core.db.get_transcript(note_id).await?)
}

/// Absolute path of the recording for playback, if it is still kept.
#[tauri::command]
#[specta::specta]
pub async fn get_audio_path(core: State<'_, Core>, note_id: Id) -> AppResult<Option<String>> {
    let (rel, _) = core.db.get_note_audio(note_id).await?;
    Ok(rel.map(|r| core.data_dir.join(r)).filter(|p| p.exists()).map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
#[specta::specta]
pub async fn rename_speaker(core: State<'_, Core>, speaker_id: Id, label: String) -> AppResult<Speaker> {
    Ok(core.db.rename_speaker(speaker_id, &label).await?)
}

/// Link a speaker to a Saved person (adds this meeting's voice to their voiceprint), or unlink with `None`.
#[tauri::command]
#[specta::specta]
pub async fn link_speaker(core: State<'_, Core>, speaker_id: Id, person_id: Option<Id>) -> AppResult<Speaker> {
    let speaker = core.db.link_speaker(speaker_id, person_id).await?;
    people_changed(&core);
    Ok(speaker)
}

#[tauri::command]
#[specta::specta]
pub async fn save_speaker_as_person(core: State<'_, Core>, speaker_id: Id, name: String) -> AppResult<Person> {
    let person = core.db.save_speaker_as_person(speaker_id, &name).await?;
    people_changed(&core);
    Ok(person)
}

#[tauri::command]
#[specta::specta]
pub async fn list_speaker_suggestions(core: State<'_, Core>, note_id: Id) -> AppResult<Vec<SpeakerSuggestion>> {
    Ok(core.db.speaker_suggestions(note_id, margin_diarize::SUGGEST_THRESHOLD).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn list_people(core: State<'_, Core>) -> AppResult<Vec<Person>> {
    // "You" is always among them.
    core.db.ensure_self_person().await?;
    Ok(core.db.list_people().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn rename_person(core: State<'_, Core>, person_id: Id, name: String) -> AppResult<Person> {
    let person = core.db.rename_person(person_id, &name).await?;
    people_changed(&core);
    Ok(person)
}

#[tauri::command]
#[specta::specta]
pub async fn set_person_color(core: State<'_, Core>, person_id: Id, color_index: u8) -> AppResult<Person> {
    let person = core.db.set_person_color(person_id, color_index).await?;
    people_changed(&core);
    Ok(person)
}

/// Forget how a person sounds (their voice samples); the person and their meetings stay.
#[tauri::command]
#[specta::specta]
pub async fn forget_person_voice(core: State<'_, Core>, person_id: Id) -> AppResult<Person> {
    let person = core.db.forget_voice(person_id).await?;
    people_changed(&core);
    Ok(person)
}

/// Fold `source` into `target` (voice samples move, speakers relinked).
#[tauri::command]
#[specta::specta]
pub async fn merge_people(core: State<'_, Core>, source_id: Id, target_id: Id) -> AppResult<Person> {
    let person = core.db.merge_people(source_id, target_id).await?;
    people_changed(&core);
    Ok(person)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_person(core: State<'_, Core>, person_id: Id) -> AppResult<()> {
    core.db.delete_person(person_id).await?;
    people_changed(&core);
    Ok(())
}

/// Write the meeting summary again from the transcript.
#[tauri::command]
#[specta::specta]
pub async fn regenerate_summary(area: State<'_, RecordingArea>, note_id: Id) -> AppResult<()> {
    area.service.regenerate_summary(note_id).await
}
