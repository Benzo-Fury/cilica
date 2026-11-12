//! Recording, permissions, speech models, transcripts, speakers, people. Owner: Audio agent.
//!
//! Every command here is part of the IPC contract in `src/ipc/bindings.ts`. Keep names, arguments and
//! return types as they are unless the orchestrator agrees to a change.
//!
//! The work lives in `margin-pipeline` ([`RecordingService`]); this module builds it at startup,
//! keeps the meeting detector in step with Settings, and maps commands onto it and the store.

use std::sync::Arc;

use margin_domain::*;
use margin_pipeline::{DeviceAudio, LocalModels, RecordingService, ServiceConfig, SummarizerProvider};
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
    let summarizer_app = app.clone();
    let summarizer: SummarizerProvider = Arc::new(move || super::assistant::summarizer(&summarizer_app));
    let service = RecordingService::new(
        core.db.clone(),
        events,
        Arc::new(DeviceAudio),
        Arc::new(LocalModels { models: models.clone() }),
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

/// Fold `source` into `target` (voiceprints averaged, speakers relinked).
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
