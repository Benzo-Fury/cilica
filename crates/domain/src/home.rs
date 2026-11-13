//! What Home shows when no note is open: small facts from the last week of meetings and notes.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{Id, Timestamp};

/// Everything Home can show. Each part is empty when there is not enough to say.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct HomeGlance {
    /// Your most repeated filler phrase this week, with the favourites of a couple of others.
    pub tics: Option<VerbalTics>,
    /// Lines Cilica lifted from recent meetings, newest meeting first.
    pub quotes: Vec<Overheard>,
    /// Who did the talking, across the week's meetings.
    pub talk: Option<TalkShare>,
    /// How fast you speak, against everyone else and last week.
    pub pace: Option<Pace>,
    /// The longest pause between two lines of speech.
    pub silence: Option<Silence>,
    /// Sentences from your written notes that read like a thought or a decision, best first.
    pub own_lines: Vec<NoteLine>,
    /// When in the day your notes get written.
    pub writing: Option<WritingHours>,
    /// Lines in your written notes that end with a question mark, newest note first.
    pub questions: Vec<NoteLine>,
    /// This week's recurring meeting that ran furthest past its usual length.
    pub overtime: Option<Overtime>,
    /// The week's meeting with the most minutes per decision, and the one with the fewest.
    pub slow_decision: Option<SlowDecision>,
    /// Your longest stretch of talking without a break this week.
    pub monologue: Option<Monologue>,
    /// The oldest written note with a title and nothing under it.
    pub bare_note: Option<BareNote>,
    /// How many written notes end in the middle of a sentence. None below two.
    pub mid_sentence: Option<u32>,
    /// The word spellcheck fixed for you most often across saved versions.
    pub spelling: Option<SpellFix>,
    /// How much of the text in your written notes Cilica wrote.
    pub ghostwriter: Option<Ghostwriter>,
    /// Cilica edits you undid this week.
    pub undos: Option<Undos>,
    /// The swear jar. None when it is switched off.
    pub swear_jar: Option<crate::SwearJar>,
}

/// One filler phrase and how often someone said it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct TicCount {
    pub phrase: String,
    pub count: u32,
    /// None for you.
    pub name: Option<String>,
    /// 1..=8, the `speaker-N` colour. None for you.
    pub color_index: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct VerbalTics {
    pub you: TicCount,
    /// Up to two other people's favourite phrases, most said first.
    pub others: Vec<TicCount>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Overtime {
    pub note_id: Id,
    pub title: String,
    /// The median length of the earlier meetings with this title.
    pub usual_ms: u32,
    pub actual_ms: u32,
    /// The latest few lengths, oldest first, ending with this one.
    pub recent_ms: Vec<u32>,
    /// The meeting before this one ran over too.
    pub again: bool,
}

/// A meeting and when in it each decision was made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct MeetingDecisions {
    pub note_id: Id,
    pub title: String,
    pub held_at: Timestamp,
    pub duration_ms: u32,
    pub decisions: u32,
    /// Where each decision with a timestamp was made, in order.
    pub decided_at_ms: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SlowDecision {
    pub slowest: MeetingDecisions,
    /// Another meeting with the fewest minutes per decision, when there is one.
    pub fastest: Option<MeetingDecisions>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Monologue {
    pub note_id: Id,
    pub note_title: String,
    /// Where your longest turn starts, to play it.
    pub at_ms: u32,
    pub your_ms: u32,
    /// The average turn in that meeting, everyone included.
    pub average_ms: u32,
    /// Someone else's longer turn this week, when there was one.
    pub record_name: Option<String>,
    pub record_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct BareNote {
    pub note_id: Id,
    pub title: String,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SpellFix {
    /// The word as it should be spelled.
    pub word: String,
    /// How it was typed.
    pub wrong: String,
    /// Saves where it went from wrong to right.
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Ghostwriter {
    /// 0..=100.
    pub cilica_pct: u8,
    /// Words added across the notes counted, by you and Cilica together.
    pub words: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Undos {
    pub count: u32,
    /// The most recent one, to show.
    pub latest: Option<UndoneEdit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct UndoneEdit {
    pub note_id: Option<Id>,
    /// What Cilica said it did: "Rewrote the intro".
    pub summary: String,
    /// A line Cilica wrote that the undo removed.
    pub cilica_text: Option<String>,
    /// A line of yours the undo brought back.
    pub your_text: Option<String>,
}

/// A line from one of your written notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NoteLine {
    pub note_id: Id,
    pub note_title: String,
    /// When the note was last changed.
    pub written_at: Timestamp,
    pub text: String,
    /// Who the line is quoted from when the note attributes it to someone else; None when it is
    /// your own.
    pub said_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct WritingHours {
    /// Saves per hour of the day, local time, midnight first. Always 24 long.
    pub hours: Vec<u32>,
    /// All saves counted.
    pub saves: u32,
    /// The busiest hour, 0..=23.
    pub peak_hour: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct TalkShare {
    /// All speech in the week's meetings.
    pub total_ms: u32,
    pub meetings: u32,
    /// Largest first. The tail beyond a few people is folded into one slice with no name.
    pub slices: Vec<TalkSlice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct TalkSlice {
    /// None for the folded "everyone else" slice.
    pub name: Option<String>,
    /// 1..=8, the `speaker-N` colour. None for the folded slice.
    pub color_index: Option<u8>,
    pub ms: u32,
    pub is_self: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Pace {
    /// Your words per minute of speaking time this week.
    pub you_wpm: u32,
    /// Everyone else's, when they spoke enough to measure.
    pub others_wpm: Option<u32>,
    /// Yours the week before, when there is one.
    pub last_week_wpm: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Silence {
    pub note_id: Id,
    pub note_title: String,
    pub held_at: Timestamp,
    pub gap_ms: u32,
    /// Where the line before the silence starts, to play it.
    pub at_ms: u32,
    /// The line that came before it.
    pub before: String,
    pub speaker: Option<String>,
    pub color_index: Option<u8>,
    pub is_self: bool,
}

/// One quote from a meeting, with who said it and where to find it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Overheard {
    pub note_id: Id,
    pub note_title: String,
    /// When the meeting happened.
    pub held_at: Timestamp,
    pub at_ms: u32,
    pub text: String,
    /// The speaker's name, or the person it is linked to. None when no segment covers the quote.
    pub speaker: Option<String>,
    /// 1..=8, the `speaker-N` colour.
    pub color_index: Option<u8>,
}
