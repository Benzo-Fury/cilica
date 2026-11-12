use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{Id, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    Manual,
    Meeting,
}

/// Who wrote a version of a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Author {
    User,
    Ai,
}

/// A full note. `body_md` is Markdown and is the stored format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Note {
    pub id: Id,
    pub title: String,
    pub body_md: String,
    /// One-line preview shown in the sidebar (AI-written for meetings, first line otherwise).
    pub snippet: String,
    pub category_id: Option<Id>,
    pub kind: NoteKind,
    pub meeting_app: Option<String>,
    pub started_at: Option<Timestamp>,
    pub ended_at: Option<Timestamp>,
    /// Sort key inside its category (fractional, lower first).
    pub position: f64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// Set while the note is in Trash.
    pub deleted_at: Option<Timestamp>,
}

/// A note without its body, for lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NoteSummary {
    pub id: Id,
    pub title: String,
    pub snippet: String,
    pub category_id: Option<Id>,
    pub kind: NoteKind,
    pub meeting_app: Option<String>,
    pub position: f64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}

/// A lightweight reference used in proposals, reports and citations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NoteRef {
    pub id: Id,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NoteVersion {
    pub id: Id,
    pub note_id: Id,
    pub title: String,
    pub body_md: String,
    pub author: Author,
    /// The AI action that caused this snapshot, if any.
    pub ai_action_id: Option<Id>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Category {
    pub id: Id,
    pub name: String,
    pub position: f64,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SearchSource {
    Title,
    Body,
    Transcript,
}

/// One full-text search result. `excerpt` marks matches with `<mark>…</mark>` and is otherwise plain text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SearchHit {
    pub note: NoteSummary,
    pub source: SearchSource,
    pub excerpt: String,
    /// For transcript hits: where in the recording the match is.
    pub at_ms: Option<u32>,
}

/// Something said in a meeting that is close to a search nobody matched: a near spelling, the same
/// words run together or split apart ("road map" for "roadmap").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NearWord {
    /// As it was said, lowercased.
    pub phrase: String,
    /// How many times it was said.
    pub count: u32,
    /// In how many meetings.
    pub meetings: u32,
    /// Who said it most. None when nobody is named.
    pub speaker: Option<String>,
    /// The one who said it most is you.
    pub is_self: bool,
    /// The latest meeting it was said in, and where, to open it there.
    pub note_id: Id,
    pub note_title: String,
    pub at_ms: u32,
}
