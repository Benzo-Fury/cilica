use margin_domain::{AppError, ErrorCode};

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("The speech model \"{0}\" is not downloaded. Download it in Settings › Transcription, then start recording again.")]
    ModelMissing(String),
    #[error("There is no speech model called \"{0}\". Pick one in Settings › Transcription.")]
    UnknownModel(String),
    #[error("The speech model could not be loaded: {0}. Delete it in Settings › Transcription and download it again.")]
    ModelLoad(String),
    #[error("Transcription failed: {0}")]
    Inference(String),
    #[error("{0} is not available yet. Choose Local Whisper in Settings › Transcription.")]
    EngineUnavailable(String),
    #[error("The download failed: {0}. Check your connection and try again.")]
    Download(String),
    #[error("The downloaded file is damaged ({0}). Try the download again.")]
    Checksum(String),
    #[error("The download was cancelled.")]
    Cancelled,
    #[error("A file could not be written: {0}. Check that the disk has space.")]
    Io(#[from] std::io::Error),
}

pub type SttResult<T> = Result<T, SttError>;

impl From<SttError> for AppError {
    fn from(e: SttError) -> Self {
        let code = match &e {
            SttError::ModelMissing(_) | SttError::EngineUnavailable(_) => ErrorCode::Unavailable,
            SttError::UnknownModel(_) => ErrorCode::NotFound,
            SttError::Download(_) | SttError::Checksum(_) => ErrorCode::Upstream,
            SttError::Cancelled => ErrorCode::Cancelled,
            _ => ErrorCode::Internal,
        };
        AppError::new(code, e.to_string())
    }
}
