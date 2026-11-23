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

/// A speaker within one meeting: "Person 2" until someone (or Cilica, from their voice) knows
/// who they are. Meetings recorded before that also have "You" and "Others".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Speaker {
    pub id: Id,
    pub note_id: Id,
    pub label: String,
    /// The Saved person this speaker is linked to, if any.
    pub person_id: Option<Id>,
    /// 1..=8, maps to the `speaker-N` colour tokens. A linked person's colour wins.
    pub color_index: u8,
    /// True for the speaker that is the app's user (linked to "You").
    pub is_self: bool,
}

/// Why a meeting's speakers could not be told apart, so its lines have no speaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerDetection {
    /// Speaker labels were off in Settings › Transcription when it was recorded.
    Off,
    /// The voice model could not be loaded.
    Unavailable,
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
    /// The few lines Cilica picked out when it wrote the notes. Empty until then.
    #[serde(default)]
    pub quotes: Vec<Quote>,
    /// False for transcripts imported without times (pasted text, some apps' exports): segment
    /// times then only keep the order, and the transcript hides them.
    #[serde(default = "timed_default")]
    pub timed: bool,
    /// Set when this meeting's speakers could not be told apart.
    #[serde(default)]
    pub speaker_detection: Option<SpeakerDetection>,
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

/// One word of a line: the `word`-th word (0-based, counted by whitespace) of a segment's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct WordPoint {
    pub segment_id: Id,
    pub word: u32,
}

/// Who words someone selected were said by.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WordsTarget {
    /// Someone already heard in this meeting.
    Speaker { speaker_id: Id },
    /// A Saved person, whether or not they were heard in this meeting yet.
    Person { person_id: Id },
    /// A new Saved person with this name.
    NewPerson { name: String },
    /// Someone named for this meeting only.
    Label { name: String },
}

/// A line that words were moved into, and who said it before, so the move can be undone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct MovedLine {
    pub segment_id: Id,
    pub previous_speaker_id: Option<Id>,
}

/// What moving words to someone did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct WordsMoved {
    /// The speaker the words are theirs now.
    pub speaker: Speaker,
    /// The lines they became, empty when they were theirs already.
    pub moved: Vec<MovedLine>,
    /// Cilica learnt the person's voice from these words.
    pub learnt: bool,
}

/// A Saved person: a name and an averaged voiceprint (the voiceprint itself never crosses IPC;
/// [`PersonVoice`] carries a coarse picture of it for drawing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Person {
    pub id: Id,
    pub name: String,
    /// 1..=8, maps to the `speaker-N` colour tokens.
    pub color_index: u8,
    /// Voice samples Cilica recognises them by (one per speaker you confirmed as them).
    pub sample_count: u32,
    pub meeting_count: u32,
    pub created_at: Timestamp,
    /// "You": the app's user. Always there; cannot be deleted or merged away.
    pub is_self: bool,
    /// When they were last in a meeting.
    pub last_met: Option<Timestamp>,
}

/// Who put a Saved person's name on a meeting's speaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NamedBy {
    /// Someone picked them for the speaker.
    You,
    /// Cilica recognised their voice.
    Cilica,
}

/// A meeting a Saved person was in, for their page in Saved people. Newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct PersonMeeting {
    pub note_id: Id,
    pub title: String,
    /// When it started, or when the note was made.
    pub at: Timestamp,
    /// Length of the recording, if known.
    pub duration_ms: Option<u32>,
    /// How long they spoke in it.
    pub speech_ms: u32,
    /// How many lines are theirs.
    pub lines: u32,
    pub named_by: NamedBy,
}

/// What Cilica knows about a Saved person's voice, for the Saved people page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct PersonVoice {
    pub person_id: Id,
    /// Speech their samples were made from, in all.
    pub speech_ms: u32,
    /// How alike their samples are (mean cosine similarity to their voiceprint), 0..=1. `None`
    /// with fewer than two samples.
    pub consistency: Option<f32>,
    /// Speakers Cilica named as them.
    pub named: u32,
    /// Of those, the ones someone then said were someone else (or no one).
    pub corrected: u32,
    pub meeting_count: u32,
    /// When they were last in a meeting.
    pub last_heard: Option<Timestamp>,
    /// `None` until they have a sample.
    pub shape: Option<VoiceShape>,
}

/// A coarse picture of a voiceprint for drawing: values folded into a few bins and scaled to
/// -1..=1. Too coarse to recognise anyone by.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct VoiceShape {
    /// The voiceprint, binned.
    pub mean: Vec<f32>,
    /// How much the samples differ from it in each bin, 0..=1.
    pub spread: Vec<f32>,
    /// Newest first.
    pub samples: Vec<VoiceSampleShape>,
}

/// One sample: binned like the voiceprint, and placed on a 2D map around it (0, 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct VoiceSampleShape {
    pub id: Id,
    pub bins: Vec<f32>,
    /// How much it sounds like the voiceprint (cosine similarity), 0..=1.
    pub similarity: f32,
    pub x: f32,
    pub y: f32,
    pub speech_ms: u32,
    /// The meeting it came from, while that is known.
    pub note_id: Option<Id>,
    pub note_title: Option<String>,
    pub created_at: Timestamp,
}
