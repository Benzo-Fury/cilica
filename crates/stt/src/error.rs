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
    /// The provider has no key for us yet. Holds the provider's name.
    #[error("Add your {0} API key in Settings › Transcription, then start recording again.")]
    MissingKey(String),
    /// The provider rejected the key. Holds the provider's name.
    #[error("{0} did not accept the API key. Paste a valid key in Settings › Transcription.")]
    Auth(String),
    /// The provider took the key but it lacks a permission (for example live transcription).
    /// Holds a message fit for people saying what to change.
    #[error("{0}")]
    NotAllowed(String),
    /// The provider could not be reached. Holds the provider's name.
    #[error("Margin could not reach {0}. Check your connection; the audio is still being recorded.")]
    Network(String),
    /// The provider answered with an error. Holds a message fit for people (never the key).
    #[error("{0}")]
    Provider(String),
}

pub type SttResult<T> = Result<T, SttError>;

impl From<SttError> for AppError {
    fn from(e: SttError) -> Self {
        let code = match &e {
            SttError::ModelMissing(_) | SttError::EngineUnavailable(_) | SttError::MissingKey(_) => ErrorCode::Unavailable,
            SttError::Auth(_) | SttError::NotAllowed(_) => ErrorCode::PermissionDenied,
            SttError::Network(_) | SttError::Provider(_) => ErrorCode::Upstream,
            SttError::UnknownModel(_) => ErrorCode::NotFound,
            SttError::Download(_) | SttError::Checksum(_) => ErrorCode::Upstream,
            SttError::Cancelled => ErrorCode::Cancelled,
            _ => ErrorCode::Internal,
        };
        AppError::new(code, e.to_string())
    }
}
