use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{Id, Timestamp};

/// Which capture stream a segment came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AudioChannel {
    /// The microphone: you (and the room, for in-person meetings).
    Mic,
    /// System output: everyone else on the call.
    System,
}

/// One utterance. Partial segments (`is_final == false`) are replaced in place by id as the engine refines them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Segment {
    pub id: Id,
    pub note_id: Id,
    pub speaker_id: Option<Id>,
    pub channel: AudioChannel,
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
    pub is_final: bool,
}

/// A speaker within one meeting. "You", "Others" (before diarization) or "Person 2".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Speaker {
    pub id: Id,
    pub note_id: Id,
    pub label: String,
    /// The Saved person this speaker is linked to, if any.
    pub person_id: Option<Id>,
    /// 1..=8, maps to the `speaker-N` colour tokens. A linked person's colour wins.
    pub color_index: u8,
    /// True for the mic-channel speaker that is the app's user.
    pub is_self: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Transcript {
    pub note_id: Id,
    pub speakers: Vec<Speaker>,
    pub segments: Vec<Segment>,
    /// Length of the recording, if one exists.
    pub duration_ms: Option<u32>,
    /// True when the audio file is still on disk (retention has not removed it).
    pub has_audio: bool,
    /// The few lines Margin picked out when it wrote the notes. Empty until then.
    #[serde(default)]
    pub quotes: Vec<Quote>,
    /// False for transcripts imported without times (pasted text, some apps' exports): segment
    /// times then only keep the order, and the transcript hides them.
    #[serde(default = "timed_default")]
    pub timed: bool,
}

fn timed_default() -> bool {
    true
}

/// One line worth keeping from a meeting. The speaker comes from the segment at `at_ms`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Quote {
    pub at_ms: u32,
    pub text: String,
}

/// "Is Person 2 Sam?" Suggested, never applied silently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SpeakerSuggestion {
    pub speaker_id: Id,
    pub person_id: Id,
    pub person_name: String,
    /// Cosine similarity, 0..=1.
    pub similarity: f32,
}

/// A Saved person: a name and an averaged voiceprint (the voiceprint never crosses IPC).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Person {
    pub id: Id,
    pub name: String,
    /// 1..=8, maps to the `speaker-N` colour tokens.
    pub color_index: u8,
    pub sample_count: u32,
    pub meeting_count: u32,
    pub created_at: Timestamp,
}
