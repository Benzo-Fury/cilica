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
    /// Found by its details (a date, a person, a tag…) with no words to match.
    Details,
}

/// One full-text search result. `excerpt` marks matches with `<mark>…</mark>` and is otherwise plain text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SearchHit {
    pub note: NoteSummary,
    pub source: SearchSource,
    pub excerpt: String,
    /// For transcript hits: where in the recording the match is.
    pub at_ms: Option<u32>,
    /// When the search asked for a date or time: the date of the note that matched it (when the
    /// meeting started, the note was made, last edited or moved to Trash).
    #[serde(default)]
    pub matched_at: Option<Timestamp>,
}

/// Which date of a note a search's date range and time of day apply to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DateField {
    /// When the meeting started (or the note was made), or when it was last edited.
    #[default]
    Any,
    Created,
    /// Last edited, or any saved version from then.
    Edited,
    /// When the meeting started (meetings only).
    Met,
    /// When it was moved to Trash.
    Deleted,
}

/// Whether a search looks at notes in Trash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TrashScope {
    #[default]
    Exclude,
    Include,
    Only,
}

/// A window of the day in local time, in minutes after midnight. `end` is exclusive and may be
/// less than `start` for a window across midnight ("tonight").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TimeOfDay {
    pub start: u32,
    pub end: u32,
}

/// A structured search: words plus filters on a note's details. Every part is optional and they
/// all have to match. Built by the command palette's parser and by the assistant's search tool.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
pub struct NoteQuery {
    /// Words to find in the title, body or transcript; all of them, the last as a prefix.
    #[serde(default)]
    pub text: String,
    /// Exact phrases that must appear.
    #[serde(default)]
    pub phrases: Vec<String>,
    /// Words or phrases that must not appear.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Inclusive start of the date range.
    #[serde(default)]
    pub after: Option<Timestamp>,
    /// Exclusive end of the date range.
    #[serde(default)]
    pub before: Option<Timestamp>,
    #[serde(default)]
    pub date_field: DateField,
    /// Only notes whose date falls in this window of the day, on any day.
    #[serde(default)]
    pub time_of_day: Option<TimeOfDay>,
    /// Minutes east of UTC on this computer, to read `time_of_day` in local time.
    #[serde(default)]
    pub utc_offset_minutes: i32,
    #[serde(default)]
    pub kind: Option<NoteKind>,
    /// In any of these categories.
    #[serde(default)]
    pub category_ids: Vec<Id>,
    /// Only notes outside every category.
    #[serde(default)]
    pub uncategorized: bool,
    /// Meetings every one of these people spoke in.
    #[serde(default)]
    pub person_ids: Vec<Id>,
    /// Notes with every one of these tags (without the `#`).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Notes that link to or embed every one of these notes, by title.
    #[serde(default)]
    pub links_to: Vec<String>,
    /// Meetings recorded in this app ("Zoom"), compared ignoring case.
    #[serde(default)]
    pub meeting_app: Option<String>,
    #[serde(default)]
    pub min_duration_ms: Option<u32>,
    #[serde(default)]
    pub max_duration_ms: Option<u32>,
    /// Whether the recording's audio is still kept.
    #[serde(default)]
    pub has_recording: Option<bool>,
    /// Whether the meeting has saved quotes.
    #[serde(default)]
    pub has_quotes: Option<bool>,
    /// Imported from this app (an import provider id, "notion").
    #[serde(default)]
    pub import_provider: Option<String>,
    #[serde(default)]
    pub trash: TrashScope,
}

/// What a search can recognise beyond people and categories: the tags in use, the meeting apps
/// that recorded meetings and the apps notes were imported from.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
pub struct SearchFacets {
    /// As first written, without the `#`.
    pub tags: Vec<String>,
    pub meeting_apps: Vec<String>,
    /// Import provider ids.
    pub import_providers: Vec<String>,
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
