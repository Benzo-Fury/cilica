//! Importing notes from other apps (docs/import.md). These types cross IPC; the importer itself
//! lives in `crates/import`.
//!
//! Providers are named by a stable string id (`"obsidian"`, `"granola"`…). The catalogue the
//! person sees (names, icons, steps) lives in the UI; the backend knows how to detect, read and
//! write each id it supports and answers `NotImplemented` for any other.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{Id, Timestamp};

/// What the person gives Margin to import from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImportInput {
    /// Data found on this computer. `path` picks one of the detected locations (an Obsidian vault);
    /// `None` takes the first.
    Local { path: Option<String> },
    /// Files or folders the person chose or dropped.
    Paths { paths: Vec<String> },
    /// The API key saved for this provider with `import_save_key`.
    Account,
    /// Text pasted into the import sheet.
    Paste { text: String, title: Option<String> },
}

/// One place on this computer where a provider's data was found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct DetectedLocation {
    /// "Neos Vault", "Anarlog on this Mac".
    pub label: String,
    pub path: String,
    /// Notes (documents without a transcript).
    pub notes: u32,
    /// Meetings (anything with a transcript or meeting times).
    pub meetings: u32,
    /// Transcripts among them.
    pub transcripts: u32,
    /// Attachments (images, PDFs) that come along.
    pub attachments: u32,
    /// Things found that Margin will not import, such as Obsidian canvases.
    pub unsupported: Vec<UnsupportedItem>,
}

/// What Margin found for a provider without being asked to read anything heavy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ImportDetection {
    pub provider: String,
    /// The provider's app is installed on this computer.
    pub app_installed: bool,
    /// Data Margin can read directly, with counts.
    pub locations: Vec<DetectedLocation>,
    /// An API key for this provider is saved in the keychain.
    pub has_key: bool,
    /// A sentence for people when something is in the way (permissions, an encrypted cache).
    pub note: Option<String>,
}

/// Something Margin saw but will not bring across, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct UnsupportedItem {
    /// "Canvases", "Encrypted notes".
    pub kind: String,
    pub count: u32,
    pub reason: String,
}

/// Which parts of a note the import carries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ImportFidelity {
    pub summary: bool,
    pub my_notes: bool,
    pub transcript: bool,
    pub speakers: bool,
    pub timestamps: bool,
    pub attachments: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SpeakerCount {
    pub name: String,
    /// Meetings this name speaks in.
    pub meetings: u32,
}

/// A note as the preview shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ImportSample {
    pub title: String,
    pub date: Option<Timestamp>,
    pub is_meeting: bool,
    pub excerpt: String,
}

/// Everything the preview needs, before anything is written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ImportPreview {
    pub scan_id: Id,
    pub provider: String,
    /// Where it came from, in words: "Neos Vault", "3 files", "your Fireflies account".
    pub origin: String,
    pub total: u32,
    pub notes: u32,
    pub meetings: u32,
    pub transcripts: u32,
    pub attachments: u32,
    pub earliest: Option<Timestamp>,
    pub latest: Option<Timestamp>,
    pub includes: ImportFidelity,
    /// Folders that become categories, in source order.
    pub folders: Vec<String>,
    /// Named speakers, most frequent first.
    pub speakers: Vec<SpeakerCount>,
    /// Already imported from this provider and unchanged since: skipped.
    pub unchanged: u32,
    /// Already imported and changed at the source: updated unless edited in Margin.
    pub changed: u32,
    pub unsupported: Vec<UnsupportedItem>,
    /// Problems with single items ("2 files could not be read"), written for people.
    pub warnings: Vec<String>,
    pub samples: Vec<ImportSample>,
    /// The details are fetched while importing (API providers), so counts of transcripts and
    /// speakers are not known yet.
    pub details_pending: bool,
}

/// Where imported notes go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ImportCategoryMode {
    /// Source folders become categories; notes outside any folder go to "From {Provider}".
    KeepFolders,
    /// Everything goes into one "From {Provider}" category.
    OneCategory,
    /// Notes are left uncategorised.
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ImportOptions {
    pub category_mode: ImportCategoryMode,
    /// Speaker names that are the person using Margin; their lines show as "You".
    pub self_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ImportRunStatus {
    Running,
    Succeeded,
    /// Finished, but some items were skipped or failed.
    Partial,
    Failed,
    Cancelled,
    /// Its notes were moved to Trash.
    Undone,
}

/// One import, as Settings › Import lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ImportRun {
    pub id: Id,
    pub provider: String,
    pub origin: String,
    pub status: ImportRunStatus,
    pub imported: u32,
    pub updated: u32,
    pub skipped: u32,
    pub failed: u32,
    /// Per-item problems, written for people.
    pub report: Vec<String>,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ImportPhase {
    Reading,
    Writing,
    /// Waiting for a provider's rate limit.
    Waiting,
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ImportProgress {
    pub run_id: Id,
    pub provider: String,
    pub phase: ImportPhase,
    pub done: u32,
    pub total: u32,
    /// The note being written, or why it is waiting.
    pub current: Option<String>,
    /// Set with `phase: done`.
    pub run: Option<ImportRun>,
}

/// Whether a key works, after `import_save_key`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ImportKeyStatus {
    pub ok: bool,
    /// What the provider said, written for people. `None` when it worked.
    pub problem: Option<String>,
    /// "Signed in as …" or "214 meetings", when the provider says.
    pub account: Option<String>,
}

/// Where an imported note came from, for its header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ImportedFrom {
    pub provider: String,
    pub imported_at: Timestamp,
}
