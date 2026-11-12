use margin_domain::{AppError, ErrorCode};

/// What can go wrong while capturing. Messages are written for people.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("No microphone was found. Plug one in or pick another input in Settings › Recording.")]
    NoInputDevice,
    #[error("The microphone \"{0}\" is not available. Plug it back in or pick another input in Settings › Recording.")]
    DeviceNotFound(String),
    #[error("Margin is not allowed to use the microphone. Allow it in System Settings › Privacy & Security › Microphone, then try again.")]
    MicPermissionDenied,
    #[error("Margin is not allowed to record system audio. Allow it in System Settings › Privacy & Security › Screen & System Audio Recording, then try again.")]
    SystemPermissionDenied,
    #[error("System audio capture is not supported here: {0}")]
    SystemUnsupported(String),
    #[error("The audio device stopped working: {0}. Check the device and try again.")]
    Device(String),
    #[error("The recording file could not be written: {0}. Check that the disk has space.")]
    Io(#[from] std::io::Error),
    #[error("The recording could not be encoded: {0}")]
    Encode(String),
    #[error("Audio could not be converted: {0}")]
    Resample(String),
}

pub type AudioResult<T> = Result<T, AudioError>;

impl From<AudioError> for AppError {
    fn from(e: AudioError) -> Self {
        let code = match &e {
            AudioError::NoInputDevice | AudioError::DeviceNotFound(_) | AudioError::SystemUnsupported(_) => {
                ErrorCode::Unavailable
            }
            AudioError::MicPermissionDenied | AudioError::SystemPermissionDenied => ErrorCode::PermissionDenied,
            _ => ErrorCode::Internal,
        };
        AppError::new(code, e.to_string())
    }
}
