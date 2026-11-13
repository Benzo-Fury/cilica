//! What a recording needs from the outside world, behind traits so the pipeline is testable
//! with fake audio, a fake engine and no models.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cilica_audio::{CaptureSource, MicSource, SystemSource, permissions};
use cilica_diarize::embedding::Embedder;
use cilica_diarize::online::{OnlineConfig, OnlineDiarizer};
use cilica_domain::{AppError, AppResult, PermissionKind, PermissionStatus, Settings, SpeakerSource, SttEngine};
use cilica_stt::cloud::{self, CloudConfig, CloudTranscriber};
use cilica_stt::{ModelManager, Transcriber, WhisperLocal, WhisperOptions};

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
    /// Paths of the speaker models (segmentation, and the CAM++ voice model that voiceprints of
    /// Saved people are made with), fetching them if needed. `None` when they cannot be had.
    async fn speaker_models(&self) -> Option<(PathBuf, PathBuf)>;
    /// The models for telling speakers apart live with the voice model the person picked (the
    /// default one while theirs is not downloaded). `None` when they cannot be had.
    async fn live_speaker_models(&self, _model_id: &str) -> Option<LiveSpeakerModels> {
        let (segmentation, embedding) = self.speaker_models().await?;
        Some(LiveSpeakerModels { segmentation, embedding, config: OnlineConfig::default() })
    }
    /// Load the voice model that recognises Saved people (blocking).
    fn voice_embedder(&self, embedding: &Path) -> AppResult<Embedder> {
        Ok(Embedder::load(embedding)?)
    }
    /// Load the live speaker detection for one channel (blocking).
    fn online_diarizer(&self, models: &LiveSpeakerModels) -> AppResult<OnlineDiarizer> {
        Ok(OnlineDiarizer::load(&models.segmentation, &models.embedding, models.config)?)
    }
    /// Fail fast before capture starts when the engine cannot run (model not downloaded).
    fn check(&self, settings: &Settings) -> AppResult<()>;
    /// The engine for the careful pass after the meeting, when it is turned on and its model is
    /// on this computer (blocking: loads the model). `None` keeps the live transcript.
    fn refiner(&self, _settings: &Settings) -> Option<Arc<dyn Transcriber>> {
        None
    }
}

/// What live speaker detection runs with.
#[derive(Debug, Clone)]
pub struct LiveSpeakerModels {
    pub segmentation: PathBuf,
    pub embedding: PathBuf,
    /// Thresholds tuned for this voice model.
    pub config: OnlineConfig,
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
            speaker_labels: settings.transcription.speaker_labels() == SpeakerSource::Service,
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

    async fn live_speaker_models(&self, model_id: &str) -> Option<LiveSpeakerModels> {
        let (segmentation, fallback) = self.speaker_models().await?;
        let (embedding, config) = match self.models.speaker_model_path(model_id) {
            Some((path, model)) => (path, live_config(model)),
            None => {
                tracing::info!(model_id, "the chosen voice model is not downloaded; using the default one");
                let default = cilica_stt::models::speaker_model(cilica_domain::DEFAULT_SPEAKER_MODEL)?;
                (fallback, live_config(default))
            }
        };
        Some(LiveSpeakerModels { segmentation, embedding, config })
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

/// Live speaker detection tuned for a voice model from the catalogue.
fn live_config(model: &cilica_stt::models::SpeakerModelInfo) -> OnlineConfig {
    OnlineConfig { match_sim: model.match_sim, new_sim: model.new_sim, merge_sim: model.merge_sim, ..OnlineConfig::default() }
}
