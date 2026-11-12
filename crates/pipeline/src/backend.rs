//! What a recording needs from the outside world, behind traits so the pipeline is testable
//! with fake audio, a fake engine and no models.

use std::path::PathBuf;
use std::sync::Arc;

use margin_audio::{CaptureSource, MicSource, SystemSource, permissions};
use margin_diarize::Diarizer;
use margin_domain::{AppError, AppResult, PermissionKind, PermissionStatus, Settings, SttEngine};
use margin_stt::{ModelManager, Transcriber, WhisperLocal, WhisperOptions, cloud::CloudTranscriber};

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
}

/// Local models from the model manager.
pub struct LocalModels {
    pub models: Arc<ModelManager>,
}

#[async_trait::async_trait]
impl SpeechBackend for LocalModels {
    fn transcriber(&self, settings: &Settings) -> AppResult<Arc<dyn Transcriber>> {
        match settings.transcription.engine {
            SttEngine::WhisperLocal => {
                let paths = self.models.speech_paths(&settings.transcription.model_id)?;
                Ok(Arc::new(WhisperLocal::load(WhisperOptions::new(paths.whisper, paths.vad))?))
            }
            other => Ok(Arc::new(CloudTranscriber::new(other))),
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

    fn check(&self, settings: &Settings) -> AppResult<()> {
        match settings.transcription.engine {
            SttEngine::WhisperLocal => self.models.speech_paths(&settings.transcription.model_id).map(|_| ()).map_err(AppError::from),
            other => {
                // Cloud engines are not wired yet: say so before anything starts.
                let t = CloudTranscriber::new(other);
                t.open_stream(Default::default()).map(|_| ()).map_err(AppError::from)
            }
        }
    }
}
