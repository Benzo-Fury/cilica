//! What a recording needs from the outside world, behind traits so the pipeline is testable
//! with fake audio, a fake engine and no models.

use std::path::PathBuf;
use std::sync::Arc;

use margin_audio::{CaptureSource, MicSource, SystemSource, permissions};
use margin_diarize::Diarizer;
use margin_domain::{AppError, AppResult, PermissionKind, PermissionStatus, Settings, SttEngine};
use margin_stt::cloud::{self, CloudConfig, CloudTranscriber};
use margin_stt::{ModelManager, Transcriber, WhisperLocal, WhisperOptions};

/// Audio sources and permissions.
pub trait AudioBackend: Send + Sync {
    fn mic(&self, device_id: Option<String>) -> Box<dyn CaptureSource>;
    fn system(&self) -> Box<dyn CaptureSource>;
    fn permission(&self, kind: PermissionKind) -> PermissionStatus;
    /// May block while the OS prompt is up.
    fn request_permission(&self, kind: PermissionKind) -> PermissionStatus;
}

/// The real devices.
pub struct DeviceAudio;

impl AudioBackend for DeviceAudio {
    fn mic(&self, device_id: Option<String>) -> Box<dyn CaptureSource> {
        Box::new(MicSource::new(device_id))
    }
    fn system(&self) -> Box<dyn CaptureSource> {
        Box::new(SystemSource)
    }
    fn permission(&self, kind: PermissionKind) -> PermissionStatus {
        permissions::status(kind)
    }
    fn request_permission(&self, kind: PermissionKind) -> PermissionStatus {
        permissions::request(kind)
    }
}

/// Speech and speaker models.
#[async_trait::async_trait]
pub trait SpeechBackend: Send + Sync {
    /// A ready engine for these settings (blocking: loads the model).
    fn transcriber(&self, settings: &Settings) -> AppResult<Arc<dyn Transcriber>>;
    /// Paths of the speaker models, fetching them if needed. `None` skips diarization.
    async fn speaker_models(&self) -> Option<(PathBuf, PathBuf)>;
    /// Load the diarizer (blocking).
    fn diarizer(&self, models: &(PathBuf, PathBuf)) -> AppResult<Diarizer> {
        Ok(Diarizer::load(&models.0, &models.1)?)
    }
    /// Fail fast before capture starts when the engine cannot run (model not downloaded).
    fn check(&self, settings: &Settings) -> AppResult<()>;
    /// The engine for the careful pass after the meeting, when it is turned on and its model is
    /// on this computer (blocking: loads the model). `None` keeps the live transcript.
    fn refiner(&self, _settings: &Settings) -> Option<Arc<dyn Transcriber>> {
        None
    }
}

/// The model the after-meeting pass uses.
pub const REFINE_MODEL_ID: &str = "large-v3-turbo-q5_0";

/// Reads a cloud provider's key from the OS keychain (the shell provides it; this crate never
/// touches the keychain).
pub type SttKeys = Arc<dyn Fn(SttEngine) -> Option<String> + Send + Sync>;

/// Local models from the model manager, and cloud engines with keys from the keychain.
pub struct LocalModels {
    pub models: Arc<ModelManager>,
    pub keys: SttKeys,
}

impl LocalModels {
    fn cloud_config(&self, settings: &Settings) -> CloudConfig {
        let engine = settings.transcription.engine;
        let keys = self.keys.clone();
        let compatible = engine == SttEngine::OpenAiCompatible;
        CloudConfig {
            engine,
            key: Arc::new(move || keys(engine)),
            base_url: settings.transcription.cloud.base_url.clone().filter(|_| compatible),
            model: settings.transcription.cloud.model.clone().filter(|_| compatible),
            speaker_labels: settings.transcription.live_speakers(),
        }
    }
}

#[async_trait::async_trait]
impl SpeechBackend for LocalModels {
    fn transcriber(&self, settings: &Settings) -> AppResult<Arc<dyn Transcriber>> {
        match settings.transcription.engine {
            SttEngine::WhisperLocal => {
                let paths = self.models.speech_paths(&settings.transcription.model_id)?;
                Ok(Arc::new(WhisperLocal::load(WhisperOptions::new(paths.whisper, paths.vad))?))
            }
            _ => Ok(Arc::new(CloudTranscriber::new(self.cloud_config(settings), self.models.vad_path()))),
        }
    }

    async fn speaker_models(&self) -> Option<(PathBuf, PathBuf)> {
        match self.models.ensure_speaker_models().await {
            Ok(p) => Some(p),
            Err(error) => {
                tracing::warn!(%error, "speaker models unavailable; skipping diarization");
                None
            }
        }
    }

    fn refiner(&self, settings: &Settings) -> Option<Arc<dyn Transcriber>> {
        if !settings.transcription.refine_after_meeting {
            return None;
        }
        let paths = self.models.speech_paths(REFINE_MODEL_ID).ok()?;
        match WhisperLocal::load(WhisperOptions::new(paths.whisper, paths.vad)) {
            Ok(engine) => Some(Arc::new(engine)),
            Err(error) => {
                tracing::warn!(%error, "the after-meeting model could not be loaded; keeping the live transcript");
                None
            }
        }
    }

    fn check(&self, settings: &Settings) -> AppResult<()> {
        match settings.transcription.engine {
            SttEngine::WhisperLocal => self.models.speech_paths(&settings.transcription.model_id).map(|_| ()).map_err(AppError::from),
            _ => cloud::check(&self.cloud_config(settings)).map_err(AppError::from),
        }
    }
}
