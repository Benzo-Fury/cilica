use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{AssistantEvent, AudioLevels, Id, ImportProgress, MeetingDetected, ModelDownloadProgress, RecordingState, Segment, Settings, TaskRun};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ToastKind {
    Info,
    Success,
    Error,
    /// Carries an action button.
    Action,
    /// Carries Undo.
    Undo,
}

/// What a toast's button does. The UI maps each to a command call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToastAction {
    StartRecording {
        meeting_app: Option<String>,
    },
    StopRecording,
    KeepRecording,
    OpenNote {
        note_id: Id,
    },
    OpenTaskRun {
        task_run_id: Id,
    },
    UndoAiAction {
        action_id: Id,
    },
    UndoTurn {
        turn_id: Id,
    },
    OpenSettings {
        page: String,
    },
    /// Write the meeting summary again (calls `regenerate_summary`).
    RegenerateSummary {
        note_id: Id,
    },
}

/// A toast raised from Rust (scheduler, detector, STT errors). Rendered by the same toast component as UI toasts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ToastRequest {
    pub kind: ToastKind,
    pub title: String,
    pub description: Option<String>,
    /// Button label and what it does.
    pub action_label: Option<String>,
    pub action: Option<ToastAction>,
}

/// Which lists the UI should refetch. Coarse on purpose: the UI invalidates query keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DataScope {
    Notes,
    Categories,
    Trash,
    Versions,
    Transcript,
    People,
    Tasks,
    Proposals,
    AiActions,
    Threads,
    /// The swear jar's counts (coins typed or emptied by hand).
    SwearJar,
    /// Settings › Import: runs and detections.
    Imports,
}

/// Everything the backend tells the UI. Crates emit these through an [`EventSink`];
/// the Tauri shell turns each variant into a typed event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    /// Data changed outside the UI's own mutation (AI write, scheduler, recording pipeline).
    /// `note_id` narrows the invalidation when only one note is affected.
    DataChanged {
        scopes: Vec<DataScope>,
        note_id: Option<Id>,
    },
    RecordingStateChanged {
        state: RecordingState,
    },
    AudioLevels {
        levels: AudioLevels,
    },
    /// Upsert one live transcript segment by id.
    TranscriptSegment {
        segment: Segment,
    },
    /// The note body is being streamed (end-of-meeting summary). `body_md` is the whole body so far.
    NoteBodyStream {
        note_id: Id,
        body_md: String,
        done: bool,
    },
    MeetingDetected {
        meeting: MeetingDetected,
    },
    /// The meeting app released the mic.
    MeetingEnded {
        app_name: String,
    },
    ModelDownload {
        progress: ModelDownloadProgress,
    },
    Assistant {
        event: AssistantEvent,
    },
    TaskRunFinished {
        run: TaskRun,
    },
    SettingsChanged {
        settings: Settings,
    },
    /// The app is about to quit: the UI must save any unsaved edits now. The shell waits briefly.
    WillQuit,
    Toast {
        toast: ToastRequest,
    },
    /// An import from another app moved on (docs/import.md).
    ImportProgress {
        progress: ImportProgress,
    },
    /// Open this note in the main window (the welcome note when setup finishes).
    OpenNote {
        note_id: Id,
    },
    /// What the transcription test has heard so far (the whole text, replacing the last).
    TranscriptionTestProgress {
        text: String,
    },
}

/// How crates talk to the UI without depending on Tauri. Cheap to clone behind an `Arc`.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: AppEvent);
}

/// Shared handle to the event sink.
pub type Events = std::sync::Arc<dyn EventSink>;

/// A sink that drops everything. For tests.
pub struct NullSink;
impl EventSink for NullSink {
    fn emit(&self, _event: AppEvent) {}
}

/// A sink that records events. For tests.
#[derive(Default)]
pub struct RecordingSink(pub std::sync::Mutex<Vec<AppEvent>>);
impl EventSink for RecordingSink {
    fn emit(&self, event: AppEvent) {
        self.0.lock().expect("sink poisoned").push(event);
    }
}
