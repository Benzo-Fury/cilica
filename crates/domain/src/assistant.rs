use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{AppResult, Id, NoteRef, Timestamp};

/// Which half of the note view is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NotePanel {
    Notes,
    Transcript,
}

/// What the user is looking at. Built by the UI and sent with every assistant turn,
/// so the assistant knows the context without being told.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
pub struct ViewContext {
    pub note_id: Option<Id>,
    pub note_title: Option<String>,
    pub panel: Option<NotePanel>,
    pub selected_text: Option<String>,
    pub category_id: Option<Id>,
    pub search_query: Option<String>,
    pub is_recording: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Thread {
    pub id: Id,
    pub title: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// Permission tier of a tool. Decides how a call is applied (see the Guardrails section of the spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ToolTier {
    /// Runs freely.
    Read,
    /// Applies at once, with Undo.
    Additive,
    /// Becomes a diff proposal (unless the user turned on auto-apply; still versioned).
    Edit,
    /// Always becomes a confirm proposal. Cannot be switched off.
    Destructive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AiActionStatus {
    Running,
    /// Done and in effect.
    Applied,
    /// Waiting on a proposal the user has not answered.
    Proposed,
    Rejected,
    Undone,
    Failed,
}

/// One tool call by the assistant: the audit-log row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct AiAction {
    pub id: Id,
    pub thread_id: Option<Id>,
    pub turn_id: Id,
    pub tool: String,
    pub tier: ToolTier,
    pub args_json: String,
    /// Past-tense, human-readable: "Searched 14 notes", "Moved Acme kickoff to Client work".
    pub summary: String,
    pub status: AiActionStatus,
    pub error: Option<String>,
    pub can_undo: bool,
    pub proposal_id: Option<Id>,
    pub created_at: Timestamp,
}

/// One piece of a chat message, in display order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessagePart {
    Text {
        text: String,
    },
    /// A compact tool row: "Searched 14 notes".
    Tool {
        action: AiAction,
    },
    /// A diff or confirm card. Look the proposal up by id for its current status.
    Proposal {
        proposal_id: Id,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Message {
    pub id: Id,
    pub thread_id: Id,
    pub turn_id: Id,
    pub role: Role,
    pub parts: Vec<MessagePart>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProposalSource {
    Chat,
    Task,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Pending,
    Applied,
    Dismissed,
}

/// What a proposal would do. Each variant carries everything the card needs to render.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProposalKind {
    /// A change to one note's Markdown. The UI diffs `before_md` against `after_md` into hunks.
    EditNote {
        note: NoteRef,
        before_md: String,
        after_md: String,
    },
    RenameNote {
        note: NoteRef,
        new_title: String,
    },
    TrashNotes {
        notes: Vec<NoteRef>,
    },
    /// Combine several notes into one; the sources go to Trash.
    MergeNotes {
        sources: Vec<NoteRef>,
        merged_title: String,
        merged_md: String,
    },
    DeleteCategory {
        category_id: Id,
        name: String,
        note_count: u32,
    },
    DeletePerson {
        person_id: Id,
        name: String,
    },
    /// A write that touches more than five notes at once.
    BulkMove {
        notes: Vec<NoteRef>,
        category_id: Option<Id>,
        category_name: String,
    },
    /// A new note suggested by a scheduled task (tasks only propose). Render like a merge preview.
    CreateNote {
        title: String,
        body_md: String,
        category_id: Option<Id>,
        category_name: Option<String>,
    },
}

/// A change the assistant wants to make that waits for the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Proposal {
    pub id: Id,
    pub source: ProposalSource,
    pub thread_id: Option<Id>,
    pub task_run_id: Option<Id>,
    pub tier: ToolTier,
    pub kind: ProposalKind,
    /// Why the assistant suggests this, one or two plain sentences.
    pub reason: String,
    pub status: ProposalStatus,
    pub created_at: Timestamp,
    pub resolved_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnEnd {
    Completed,
    Cancelled,
    Error { message: String },
}

/// Streamed while the assistant works on a turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantEvent {
    TurnStarted {
        thread_id: Id,
        turn_id: Id,
        user_message: Message,
    },
    /// Append `delta` to the assistant message `message_id` (create it on first sight).
    TextDelta {
        thread_id: Id,
        turn_id: Id,
        message_id: Id,
        delta: String,
    },
    /// A tool row appeared or changed status. Upsert by `action.id`.
    Tool {
        thread_id: Id,
        turn_id: Id,
        message_id: Id,
        action: AiAction,
    },
    Proposal {
        thread_id: Id,
        turn_id: Id,
        message_id: Id,
        proposal: Proposal,
    },
    TurnFinished {
        thread_id: Id,
        turn_id: Id,
        end: TurnEnd,
    },
}

/// A suggested prompt shown in the assistant's empty state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct PromptChip {
    pub label: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Anthropic,
    OpenaiCompat,
    Ollama,
    /// Experimental: drives the user's installed `claude` CLI.
    ClaudeCode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ProviderStatus {
    pub kind: ProviderKind,
    /// A key is stored in the OS keychain (the key itself never crosses IPC).
    pub has_key: bool,
    /// Ready to answer: key present, server reachable or CLI signed in.
    pub ready: bool,
    /// When not ready, what to do about it.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ClaudeCodeStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub signed_in: bool,
}

/// Writes the end-of-meeting summary. Implemented by the `agent` crate, called by the recording pipeline.
#[async_trait::async_trait]
pub trait MeetingSummarizer: Send + Sync {
    /// Read the note's transcript and jottings, then write Summary / Decisions / Action items /
    /// Open questions above the user's jottings, streaming the body through `AppEvent::NoteBodyStream`.
    /// Also sets the note's title (when still the default) and snippet.
    async fn summarize_meeting(&self, note_id: Id) -> AppResult<()>;
}

/// A local model Cilica can pull into Ollama (Settings › Cilica › Local model).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct LocalLlmModel {
    /// Ollama tag, e.g. `qwen3:8b`.
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_mb: u32,
    /// Rough memory the Mac needs to run it comfortably.
    pub min_ram_gb: u32,
    pub recommended: bool,
    pub downloaded: bool,
}

/// Whether Ollama is on this Mac and answering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct OllamaStatus {
    pub installed: bool,
    pub running: bool,
    pub version: Option<String>,
}
