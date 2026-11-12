use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{Id, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecordingPhase {
    Idle,
    Starting,
    Recording,
    /// Capture has stopped; the last audio is being transcribed.
    Stopping,
    /// Diarizing speakers after the meeting.
    Diarizing,
    /// The summary is being written into the note.
    Summarizing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct RecordingState {
    pub phase: RecordingPhase,
    pub note_id: Option<Id>,
    pub meeting_app: Option<String>,
    pub started_at: Option<Timestamp>,
    /// Set while a detected meeting has ended and recording will stop unless the user keeps it.
    pub auto_stop_at: Option<Timestamp>,
}

impl RecordingState {
    pub fn idle() -> Self {
        Self { phase: RecordingPhase::Idle, note_id: None, meeting_app: None, started_at: None, auto_stop_at: None }
    }
}

/// Input levels, 0..=1 (RMS mapped to a perceptual scale). Emitted about 20 times a second while recording.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct AudioLevels {
    pub mic: f32,
    pub system: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecordingTrigger {
    Manual,
    Hotkey,
    Tray,
    Detected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct StartRecordingOptions {
    /// Record into an existing note; a new meeting note is created when `None`.
    pub note_id: Option<Id>,
    pub meeting_app: Option<String>,
    pub trigger: RecordingTrigger,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    Microphone,
    SystemAudio,
    /// Optional: only used to spot Meet/Teams running in a browser.
    Accessibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Granted,
    Denied,
    NotDetermined,
    /// The OS has no such permission (for example system audio on Windows).
    NotNeeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Permissions {
    pub microphone: PermissionStatus,
    pub system_audio: PermissionStatus,
    pub accessibility: PermissionStatus,
}

/// A meeting app was seen holding the microphone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct MeetingDetected {
    /// Display name: "Zoom", "Google Meet".
    pub app_name: String,
    /// Bundle id / process name, when known.
    pub app_id: Option<String>,
}

/// A local speech model that can be downloaded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SttModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_mb: u32,
    pub multilingual: bool,
    pub recommended: bool,
    pub downloaded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub downloaded_bytes: f64,
    pub total_bytes: f64,
    pub done: bool,
    pub error: Option<String>,
}
