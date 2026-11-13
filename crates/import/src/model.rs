//! The one shape every provider produces, and how it becomes a Margin note.

use margin_domain::Timestamp;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// A note read from another app, before it is written. Never crosses IPC.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ImportedNote {
    /// Stable id in the source: an API id, or a path relative to the chosen folder.
    pub external_id: String,
    pub title: String,
    pub created_at: Option<Timestamp>,
    pub updated_at: Option<Timestamp>,
    /// Meeting times, when it was a meeting.
    pub started_at: Option<Timestamp>,
    pub ended_at: Option<Timestamp>,
    pub duration_ms: Option<u32>,
    /// "Zoom", "Google Meet"… when the source says.
    pub meeting_app: Option<String>,
    pub attendees: Vec<String>,
    /// The other app's AI notes.
    pub summary_md: Option<String>,
    /// What the person wrote: their own meeting notes, or the whole note for notes apps.
    pub notes_md: Option<String>,
    /// Only when the source keeps them apart from the summary.
    pub action_items: Vec<String>,
    pub transcript: Option<ImportedTranscript>,
    /// Folder path in the source, outermost first.
    pub folder: Vec<String>,
    pub tags: Vec<String>,
    /// `key: value` properties the source kept that Margin has no field for.
    pub properties: Vec<(String, String)>,
    /// Images carried inline.
    pub attachments: u32,
    /// The source calls it a meeting even without a transcript.
    pub meeting: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportedTranscript {
    /// False when no turn has a time.
    pub timed: bool,
    pub turns: Vec<Turn>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Turn {
    pub speaker: SpeakerRef,
    pub start_ms: Option<u32>,
    pub end_ms: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum SpeakerRef {
    /// The person using Margin (a mic channel, "me", or a name they picked).
    Me,
    /// Everyone else, unnamed (a system channel).
    Them,
    Named(String),
    Unknown,
}

impl ImportedTranscript {
    pub fn new(turns: Vec<Turn>) -> Self {
        let timed = turns.iter().any(|t| t.start_ms.is_some());
        Self { timed, turns }
    }

    /// Distinct speaker names, in order of first appearance.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for turn in &self.turns {
            if let SpeakerRef::Named(n) = &turn.speaker
                && !names.iter().any(|m| m.eq_ignore_ascii_case(n))
            {
                names.push(n.clone());
            }
        }
        names
    }
}

impl ImportedNote {
    pub fn is_meeting(&self) -> bool {
        self.meeting || self.transcript.as_ref().is_some_and(|t| !t.turns.is_empty()) || self.started_at.is_some()
    }

    /// The most useful single date: when it happened, else when it was written.
    pub fn date(&self) -> Option<Timestamp> {
        self.started_at.or(self.created_at).or(self.updated_at)
    }

    /// The note body Margin stores: the summary first, then the person's own notes, then action
    /// items and anything else the source kept, as Margin lays out its own meeting notes.
    pub fn body_md(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let summary = self.summary_md.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let notes = self.notes_md.as_deref().map(str::trim).filter(|s| !s.is_empty());
        if !self.properties.is_empty() {
            parts.push(self.properties.iter().map(|(k, v)| format!("**{k}:** {v}")).collect::<Vec<_>>().join("  \n"));
        }
        match (summary, notes) {
            (Some(s), Some(n)) => {
                parts.push(s.to_string());
                parts.push(format!("## My notes\n\n{n}"));
            }
            (Some(s), None) => parts.push(s.to_string()),
            (None, Some(n)) => parts.push(n.to_string()),
            (None, None) => {}
        }
        let items: Vec<&str> = self.action_items.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        let already = |item: &str| summary.is_some_and(|s| s.contains(item));
        if !items.is_empty() && !items.iter().all(|i| already(i)) {
            parts.push(format!("## Action items\n\n{}", items.iter().map(|i| format!("- [ ] {i}")).collect::<Vec<_>>().join("\n")));
        }
        if !self.attendees.is_empty() {
            parts.push(format!("**Attendees:** {}", self.attendees.join(", ")));
        }
        if !self.tags.is_empty() {
            let tags: Vec<String> = self.tags.iter().map(|t| format!("#{}", t.trim_start_matches('#').replace(' ', "-"))).collect();
            parts.push(format!("Tags: {}", tags.join(" ")));
        }
        parts.join("\n\n")
    }
}

/// Stable hash of a note's title and body, ignoring surrounding whitespace. Pasted text has no id
/// of its own, so this is its id.
pub fn note_hash(title: &str, body_md: &str) -> String {
    let mut h = Sha256::new();
    h.update(title.trim().as_bytes());
    h.update([0]);
    h.update(body_md.trim().as_bytes());
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_puts_summary_first_then_my_notes_and_actions() {
        let note = ImportedNote {
            summary_md: Some("## Summary\n\nWe agreed.".into()),
            notes_md: Some("- ask Sam".into()),
            action_items: vec!["Send the deck".into()],
            attendees: vec!["Sam".into(), "Ana".into()],
            tags: vec!["client work".into()],
            ..Default::default()
        };
        let body = note.body_md();
        let summary = body.find("We agreed").unwrap();
        let mine = body.find("## My notes").unwrap();
        let actions = body.find("- [ ] Send the deck").unwrap();
        assert!(summary < mine && mine < actions);
        assert!(body.contains("**Attendees:** Sam, Ana"));
        assert!(body.contains("#client-work"));
    }

    #[test]
    fn action_items_already_in_the_summary_are_not_repeated() {
        let note = ImportedNote { summary_md: Some("- [ ] Send the deck".into()), action_items: vec!["Send the deck".into()], ..Default::default() };
        assert_eq!(note.body_md().matches("Send the deck").count(), 1);
    }

    #[test]
    fn hashes_are_stable_and_ignore_surrounding_space() {
        assert_ne!(note_hash("T", "body"), note_hash("T", "other"));
        assert_eq!(note_hash("T", "body\n"), note_hash("T ", "body"));
    }
}
