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
    /// The engine labels who is speaking as the meeting goes. When false, speakers are only
    /// known once the meeting is finished, so the live transcript does not show them.
    #[serde(default)]
    pub live_speakers: bool,
}

impl RecordingState {
    pub fn idle() -> Self {
        Self { phase: RecordingPhase::Idle, note_id: None, meeting_app: None, started_at: None, auto_stop_at: None, live_speakers: false }
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

/// Whether a cloud speech service is set up. Never carries the key itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SttProviderStatus {
    pub engine: crate::SttEngine,
    /// A key is stored in the OS keychain.
    pub has_key: bool,
    /// What is wrong, written for people (for example the key could not be checked).
    pub problem: Option<String>,
}

/// What came back when the sample clip was streamed to the transcription engine (Settings ›
/// Transcription › Test). The clip is public-domain speech, never the person's own audio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct TranscriptionTest {
    pub engine: crate::SttEngine,
    /// What the engine heard.
    pub text: String,
    /// What the clip says, to compare.
    pub expected: String,
    /// From the start of the clip to the first words coming back.
    pub first_words_ms: Option<u32>,
    /// The whole test; the clip plays in real time, as a meeting would.
    pub took_ms: u32,
}

/// What this computer can run, for recommending a speech model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct HardwareProfile {
    /// Logical CPU cores.
    pub cores: u32,
    /// Installed memory, whole gigabytes.
    pub ram_gb: u32,
    pub apple_silicon: bool,
    /// Too slow for good local transcription: suggest a cloud service.
    pub suggest_cloud: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub downloaded_bytes: f64,
    pub total_bytes: f64,
    pub done: bool,
    pub error: Option<String>,
}
