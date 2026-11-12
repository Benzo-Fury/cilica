use serde::{Deserialize, Serialize};
use specta::Type;

/// Machine-readable error class. The UI switches on this; `message` is for people.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    Invalid,
    Conflict,
    PermissionDenied,
    /// Something outside the app is missing: no mic, no model downloaded, no API key.
    Unavailable,
    /// A provider or network call failed.
    Upstream,
    Cancelled,
    NotImplemented,
    Internal,
}

/// The one error type returned by every IPC command.
/// `message` follows the voice rules: say what went wrong and what to do next.
#[derive(Debug, Clone, Serialize, Deserialize, Type, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
    pub fn not_found(what: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, what)
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Invalid, message)
    }
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unavailable, message)
    }
    pub fn upstream(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Upstream, message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
    pub fn not_implemented(command: &str) -> Self {
        Self::new(ErrorCode::NotImplemented, format!("{command} is not implemented yet."))
    }
}

pub type AppResult<T> = Result<T, AppError>;
