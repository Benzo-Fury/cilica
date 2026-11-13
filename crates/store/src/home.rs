//! Home's facts about the last week: lines worth hearing again, who talked, filler phrases,
//! overtime, slow decisions and monologues. Reads notes, segments, speakers and ai_actions;
//! owns no tables. Everything is counted here, no model.

use std::collections::HashMap;

use cilica_domain::{
    AiActionStatus, AudioChannel, HomeGlance, Id, MeetingDecisions, Monologue, Overheard, Overtime, Pace, Quote, Segment, Silence, SlowDecision, SwearJar,
    TalkShare, TalkSlice, TicCount, Timestamp, UndoneEdit, Undos, VerbalTics, now,
};
use sqlx::Row;

use crate::assistant::enum_str;
use crate::codec::{parse_id, parse_opt_ts, parse_ts};
use crate::{Db, OTHERS_LABEL, SELF_LABEL, StoreResult};

/// Meetings looked at, newest first.
const MEETING_LIMIT: i64 = 40;
/// Quotes handed to Home; it shows one at a time.
const QUOTE_LIMIT: usize = 12;

// ---------- Verbal tics ---------- //

/// Stock filler phrases. Only these count, so nothing private is ever singled out.
const FILLERS: &[&str] = &[
    "basically",
    "to be fair",
    "to be honest",
    "sounds good",
    "you know",
    "i mean",
    "kind of",
    "sort of",
    "literally",
    "actually",
    "honestly",
    "obviously",
    "essentially",
    "at the end of the day",
    "circle back",
    "no worries",
    "makes sense",
    "for sure",
    "like i said",
    "per se",
];
/// A phrase said fewer times than this is not a tic.
const MIN_TICS: u32 = 3;
/// Other people shown beside you.
const TIC_OTHERS: usize = 2;

/// Lowercased words, apostrophes kept, so phrases match on whole words only.
fn line_words(text: &str) -> Vec<String> {
    let flat: String = text.to_lowercase().replace('’', "'").chars().map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' }).collect();
    flat.split_whitespace().map(str::to_string).collect()
}

/// How many times a phrase appears as consecutive words, back-to-back repeats included.
fn phrase_count(words: &[String], phrase: &str) -> u32 {
    let want: Vec<&str> = phrase.split(' ').collect();
    words.windows(want.len()).filter(|w| w.iter().zip(&want).all(|(a, b)| a == b)).count() as u32
}

/// Filler phrase counts per person across the week. You are one entry; others by display name.
#[derive(Default)]
struct Tics {
    /// (name, is self, colour, phrase counts), in first-heard order.
    by_person: Vec<(String, bool, u8, HashMap<&'static str, u32>)>,
}

impl Tics {
    /// Count the fillers in one line of speech.
    fn add(&mut self, name: &str, mine: bool, color: u8, text: &str) {
        let line = line_words(text);
        let key = if mine { SELF_LABEL } else { name };
        let idx = match self.by_person.iter().position(|(n, me, _, _)| *me == mine && n == key) {
            Some(i) => i,
            None => {
                self.by_person.push((key.to_string(), mine, color, HashMap::new()));
                self.by_person.len() - 1
            }
        };
        let counts = &mut self.by_person[idx].3;
        for phrase in FILLERS {
            let n = phrase_count(&line, phrase);
            if n > 0 {
                *counts.entry(phrase).or_default() += n;
            }
        }
    }

    /// Your top phrase, and a couple of other people's, once you have said one enough.
    fn result(self) -> Option<VerbalTics> {
        // Most said first; ties go to the phrase earlier in the list so the answer is stable.
        let top = |counts: &HashMap<&'static str, u32>| {
            FILLERS.iter().filter_map(|p| counts.get(p).map(|n| (*p, *n))).max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0)))
        };
        let mut you = None;
        let mut others = Vec::new();
        for (name, mine, color, counts) in &self.by_person {
            let Some((phrase, count)) = top(counts).filter(|(_, n)| *n >= MIN_TICS) else { continue };
            if *mine {
                you = Some(TicCount { phrase: phrase.to_string(), count, name: None, color_index: None });
            } else {
                others.push(TicCount { phrase: phrase.to_string(), count, name: Some(name.clone()), color_index: Some(*color) });
            }
        }
        others.sort_by(|a, b| b.count.cmp(&a.count));
        others.truncate(TIC_OTHERS);
        you.map(|you| VerbalTics { you, others })
    }
}

// ---------- Decisions and turns ---------- //

/// A meeting shorter than this says nothing about how slowly it decided.
const MIN_DECISION_MEETING_MS: u32 = 10 * 60_000;
/// Fewer minutes per decision than this is not slow enough to joke about.
const SLOW_MS_PER_DECISION: u32 = 15 * 60_000;
/// Two lines by the same person closer than this are one turn.
const TURN_GAP_MS: u32 = 2_000;
/// A turn shorter than this is not a monologue.
const MIN_MONOLOGUE_MS: u32 = 60_000;

/// A `[mm:ss]` or `[h:mm:ss]` timestamp at the end of a summary line, in ms.
fn trailing_timestamp(line: &str) -> Option<u32> {
    let open = line.rfind('[')?;
    let inner = line[open + 1..].strip_suffix(']').or_else(|| line[open + 1..].split(']').next())?;
    let parts: Vec<u32> = inner.split(':').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let secs = match parts.as_slice() {
        [m, s] => m * 60 + s,
        [h, m, s] => h * 3600 + m * 60 + s,
        _ => return None,
    };
    Some(secs * 1000)
}

/// The bullets under a summary's "Decisions" heading: how many, and when each timed one was made.
pub fn decisions(body_md: &str) -> (u32, Vec<u32>) {
    let mut inside = false;
    let mut count = 0;
    let mut at = Vec::new();
    for line in body_md.lines() {
        let t = line.trim();
        // The section runs from its heading to the next heading
        if t.starts_with('#') {
            inside = t.trim_start_matches('#').trim().eq_ignore_ascii_case("decisions");
            continue;
        }
        let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) else { continue };
        if !inside || item.trim().to_lowercase().starts_with("none") {
            continue;
        }
        count += 1;
        if let Some(ms) = trailing_timestamp(item) {
            at.push(ms);
        }
    }
    at.sort_unstable();
    (count, at)
}

/// Consecutive lines by the same person, merged: (name, is self, start, end).
fn turns(spoken: &[(&Segment, String, bool)]) -> Vec<(String, bool, u32, u32)> {
    let mut out: Vec<(String, bool, u32, u32)> = Vec::new();
    for (s, name, mine) in spoken {
        match out.last_mut() {
            Some(last) if last.0 == *name && last.1 == *mine && s.start_ms <= last.3 + TURN_GAP_MS => last.3 = last.3.max(s.end_ms),
            _ => out.push((name.clone(), *mine, s.start_ms, s.end_ms)),
        }
    }
    out
}

/// A meeting's length: as recorded, else its start and end times, else where its speech ends.
fn meeting_length(duration_ms: Option<i64>, started: Option<Timestamp>, ended: Option<Timestamp>) -> Option<u32> {
    duration_ms
        .and_then(|d| u32::try_from(d).ok())
        .or_else(|| started.zip(ended).and_then(|(a, b)| u32::try_from((b - a).num_milliseconds()).ok()))
        .filter(|d| *d > 0)
}

// ---------- Overtime ---------- //

/// Earlier meetings needed before "usual" means anything.
const MIN_HISTORY: usize = 2;
/// Earlier meetings the usual length is taken from.
const HISTORY_LIMIT: usize = 6;
/// Running over by less than this is not overtime.
const MIN_OVER_MS: u32 = 5 * 60_000;

/// The middle of some lengths (the lower middle for an even count).
fn median(mut xs: Vec<u32>) -> u32 {
    xs.sort_unstable();
    xs[(xs.len() - 1) / 2]
}

/// The meeting held since `since` that ran furthest past the usual length of earlier meetings with
/// the same title. `history` is (note id, title, held at, length), oldest first.
pub fn overtime(history: &[(Id, String, Timestamp, u32)], since: Timestamp) -> Option<Overtime> {
    let key = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let mut best: Option<(u32, Overtime)> = None;
    for (i, (note_id, title, held_at, actual)) in history.iter().enumerate() {
        if *held_at < since || title.trim().is_empty() {
            continue;
        }
        // Earlier meetings with the same title, newest last
        let earlier: Vec<u32> = history[..i].iter().filter(|(_, t, _, _)| key(t) == key(title)).map(|(_, _, _, d)| *d).collect();
        if earlier.len() < MIN_HISTORY {
            continue;
        }
        let usual = median(earlier.iter().rev().take(HISTORY_LIMIT).copied().collect());
        let over = actual.saturating_sub(usual);
        if over < MIN_OVER_MS || (*actual as u64) * 100 < (usual as u64) * 115 || best.as_ref().is_some_and(|(b, _)| over <= *b) {
            continue;
        }
        let mut recent: Vec<u32> = earlier.iter().rev().take(3).rev().copied().collect();
        recent.push(*actual);
        let again = earlier.last().is_some_and(|prev| *prev >= usual + MIN_OVER_MS);
        best = Some((over, Overtime { note_id: *note_id, title: title.clone(), usual_ms: usual, actual_ms: *actual, recent_ms: recent, again }));
    }
    best.map(|(_, o)| o)
}

// ---------- Undos ---------- //

/// Undos needed before the count is a joke rather than a slip.
const MIN_UNDOS: u32 = 3;
/// Quoted lines are cut to about this many characters.
const LINE_MAX: usize = 140;

/// The first non-empty line of `a` that `b` does not have, without Markdown markers, cut short.
fn first_new_line(a: &str, b: &str) -> Option<String> {
    let have: Vec<&str> = b.lines().map(str::trim).collect();
    a.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("```") && !have.contains(l))
        .map(|l| l.trim_start_matches(['#', '>', '-', '*', ' ']).trim().to_string())
        .filter(|l| !l.is_empty())
        .map(|l| if l.chars().count() > LINE_MAX { format!("{}…", l.chars().take(LINE_MAX).collect::<String>().trim_end()) } else { l })
}

impl Db {
    /// Facts for Home from meetings held since `since`, and from written notes.
    pub async fn home_glance(&self, since: Timestamp) -> StoreResult<HomeGlance> {
        let rows = sqlx::query(
            "SELECT id, title, body_md, started_at, ended_at, duration_ms, created_at, quotes_json FROM notes \
             WHERE deleted_at IS NULL AND kind = 'meeting' ORDER BY COALESCE(started_at, created_at) DESC LIMIT ?",
        )
        .bind(MEETING_LIMIT)
        .fetch_all(self.pool())
        .await?;

        let mut quotes: Vec<Overheard> = Vec::new();
        let mut talk = Talk::default();
        let mut tics = Tics::default();
        let mut last_week = Speech::default();
        let mut silence: Option<Silence> = None;
        let mut decided: Vec<MeetingDecisions> = Vec::new();
        let mut monologue: Option<Monologue> = None;
        let mut record: Option<(String, u32)> = None;
        // Pace compares with the week before, so look back two windows.
        let previous = since - (now() - since);
        for row in &rows {
            let started = parse_opt_ts(row.try_get("started_at")?)?;
            let held_at = match started {
                Some(t) => t,
                None => parse_ts(&row.try_get::<String, _>("created_at")?)?,
            };
            if held_at < previous {
                continue;
            }
            let note_id = parse_id(&row.try_get::<String, _>("id")?)?;
            let note_title: String = row.try_get("title")?;
            let segments = self.list_segments(note_id).await?;
            let speakers = self.list_speakers(note_id).await?;
            let display = self.speaker_display_names(&speakers).await?;
            let spoken: Vec<&Segment> = segments.iter().filter(|s| s.is_final && !s.text.trim().is_empty()).collect();
            let is_self = |s: &Segment| match s.speaker_id.and_then(|id| speakers.iter().find(|sp| sp.id == id)) {
                Some(sp) => sp.is_self,
                None => s.channel == AudioChannel::Mic,
            };
            if held_at < since {
                for s in spoken.iter().filter(|s| is_self(s)) {
                    last_week.add(s);
                }
                continue;
            }

            // Who spoke, how fast, what they kept saying, and the longest pause between two lines.
            if !spoken.is_empty() {
                talk.meetings += 1;
            }
            let mut reached = 0u32;
            let mut named: Vec<(&Segment, String, bool)> = Vec::new();
            for (i, s) in spoken.iter().enumerate() {
                let speaker = s.speaker_id.and_then(|id| speakers.iter().find(|sp| sp.id == id));
                let mine = is_self(s);
                let name = speaker.and_then(|sp| display.get(&sp.id).cloned()).unwrap_or_else(|| default_name(s.channel).to_string());
                let color = speaker.map(|sp| sp.color_index).unwrap_or(1);
                talk.add(&name, color, s, mine);
                tics.add(&name, mine, color, &s.text);
                if i > 0 {
                    let gap = s.start_ms.saturating_sub(reached);
                    if (SILENCE_MIN_MS..=SILENCE_MAX_MS).contains(&gap) && silence.as_ref().is_none_or(|b| gap > b.gap_ms) {
                        // The line that ran longest into the pause is the one before it.
                        let before = spoken[..i].iter().max_by_key(|p| p.end_ms).copied().unwrap_or(spoken[i - 1]);
                        let who = before.speaker_id.and_then(|id| speakers.iter().find(|sp| sp.id == id));
                        silence = Some(Silence {
                            note_id,
                            note_title: note_title.clone(),
                            held_at,
                            gap_ms: gap,
                            at_ms: before.start_ms,
                            before: before.text.trim().to_string(),
                            speaker: who.and_then(|sp| display.get(&sp.id).cloned()),
                            color_index: who.map(|sp| sp.color_index),
                            is_self: is_self(before),
                        });
                    }
                }
                reached = reached.max(s.end_ms);
                named.push((s, name, mine));
            }

            // Turns: your longest against this meeting's average, and anyone else's longer one.
            let all = turns(&named);
            if !all.is_empty() {
                let average = (all.iter().map(|t| (t.3 - t.2) as u64).sum::<u64>() / all.len() as u64) as u32;
                for (name, mine, start, end) in &all {
                    let len = end - start;
                    if *mine {
                        if len >= MIN_MONOLOGUE_MS && monologue.as_ref().is_none_or(|m| len > m.your_ms) {
                            monologue = Some(Monologue {
                                note_id,
                                note_title: note_title.clone(),
                                at_ms: *start,
                                your_ms: len,
                                average_ms: average,
                                record_name: None,
                                record_ms: None,
                            });
                        }
                    } else if record.as_ref().is_none_or(|r| len > r.1) {
                        record = Some((name.clone(), len));
                    }
                }
            }

            // Decisions, placed by the timestamps the summary gave them.
            let body: String = row.try_get("body_md")?;
            let (count, decided_at_ms) = decisions(&body);
            let length = meeting_length(row.try_get("duration_ms")?, started, parse_opt_ts(row.try_get("ended_at")?)?)
                .or_else(|| segments.iter().map(|s| s.end_ms).max());
            if let Some(duration_ms) = length.filter(|d| count > 0 && *d >= MIN_DECISION_MEETING_MS) {
                decided.push(MeetingDecisions { note_id, title: note_title.clone(), held_at, duration_ms, decisions: count, decided_at_ms });
            }

            let picked: Vec<Quote> = row.try_get::<Option<String>, _>("quotes_json")?.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
            if segments.is_empty() {
                continue;
            }
            for q in picked {
                // The segment that holds the quote, else the last one that started before it.
                let seg = segments
                    .iter()
                    .find(|s| s.start_ms <= q.at_ms && q.at_ms <= s.end_ms)
                    .or_else(|| segments.iter().rev().find(|s| s.start_ms <= q.at_ms));
                let speaker = seg.and_then(|s| s.speaker_id).and_then(|id| speakers.iter().find(|sp| sp.id == id));
                quotes.push(Overheard {
                    note_id,
                    note_title: note_title.clone(),
                    held_at,
                    at_ms: q.at_ms,
                    text: q.text,
                    speaker: speaker.and_then(|sp| display.get(&sp.id).cloned()),
                    color_index: speaker.map(|sp| sp.color_index),
                });
            }
        }
        quotes.truncate(QUOTE_LIMIT);

        // The record only counts when it beats your own longest turn.
        if let (Some(m), Some((name, ms))) = (monologue.as_mut(), record)
            && ms > m.your_ms
        {
            m.record_name = Some(name);
            m.record_ms = Some(ms);
        }
        let pace = talk.you.wpm().map(|you_wpm| Pace { you_wpm, others_wpm: talk.others.wpm(), last_week_wpm: last_week.wpm() });
        let written = self.written_glance().await?;
        Ok(HomeGlance {
            tics: tics.result(),
            quotes,
            talk: talk.share(),
            pace,
            silence,
            own_lines: written.own_lines,
            writing: written.writing,
            questions: written.questions,
            overtime: overtime(&self.meeting_lengths().await?, since),
            slow_decision: slow_decision(decided),
            monologue,
            bare_note: written.bare_note,
            mid_sentence: written.mid_sentence,
            spelling: written.spelling,
            ghostwriter: written.ghostwriter,
            undos: self.undos(since).await?,
            swear_jar: self.glance_swear_jar().await?,
        })
    }

    /// This week's swear jar, in local time, when Settings has it on.
    async fn glance_swear_jar(&self) -> StoreResult<Option<SwearJar>> {
        let prefs = self.get_settings().await?.swear_jar;
        if !prefs.enabled {
            return Ok(None);
        }
        let start = crate::week_start(cilica_domain::chrono::Local::now());
        Ok(Some(self.swear_jar(start, &prefs).await?))
    }

        /// Every meeting with a known length, oldest first: (id, title, held at, length).
    async fn meeting_lengths(&self) -> StoreResult<Vec<(Id, String, Timestamp, u32)>> {
        let rows = sqlx::query(
            "SELECT id, title, started_at, ended_at, duration_ms, created_at FROM notes \
             WHERE deleted_at IS NULL AND kind = 'meeting' ORDER BY COALESCE(started_at, created_at) ASC",
        )
        .fetch_all(self.pool())
        .await?;
        let mut out = Vec::new();
        for row in &rows {
            let started = parse_opt_ts(row.try_get("started_at")?)?;
            let Some(length) = meeting_length(row.try_get("duration_ms")?, started, parse_opt_ts(row.try_get("ended_at")?)?) else { continue };
            let held_at = match started {
                Some(t) => t,
                None => parse_ts(&row.try_get::<String, _>("created_at")?)?,
            };
            out.push((parse_id(&row.try_get::<String, _>("id")?)?, row.try_get("title")?, held_at, length));
        }
        Ok(out)
    }

    /// Cilica's actions undone since `since`, with the latest note edit's before and after lines.
    async fn undos(&self, since: Timestamp) -> StoreResult<Option<Undos>> {
        let rows = sqlx::query("SELECT summary, undo_json FROM ai_actions WHERE status = ? AND created_at >= ? ORDER BY created_at DESC")
            .bind(enum_str(&AiActionStatus::Undone))
            .bind(since.to_rfc3339())
            .fetch_all(self.pool())
            .await?;
        let count = rows.len() as u32;
        if count < MIN_UNDOS {
            return Ok(None);
        }
        // The newest undone body edit shows best: Cilica's line struck out, yours back.
        let mut latest = None;
        for row in &rows {
            let summary: String = row.try_get("summary")?;
            let undo: Option<serde_json::Value> = row.try_get::<Option<String>, _>("undo_json")?.and_then(|j| serde_json::from_str(&j).ok());
            let edit = undo.filter(|u| u.get("op").and_then(|o| o.as_str()) == Some("restore_body"));
            let Some(u) = edit else {
                latest.get_or_insert(UndoneEdit { note_id: None, summary, cilica_text: None, your_text: None });
                continue;
            };
            let text = |k: &str| u.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let (before, after) = (text("before_md"), text("after_md"));
            latest = Some(UndoneEdit {
                note_id: u.get("note_id").and_then(|v| v.as_str()).and_then(|s| parse_id(s).ok()),
                summary,
                cilica_text: first_new_line(&after, &before),
                your_text: first_new_line(&before, &after),
            });
            break;
        }
        Ok(Some(Undos { count, latest }))
    }
}

/// The meeting with the most minutes per decision, slow enough to joke about, and the quickest
/// other one.
pub fn slow_decision(meetings: Vec<MeetingDecisions>) -> Option<SlowDecision> {
    let per = |m: &MeetingDecisions| m.duration_ms / m.decisions.max(1);
    let slowest = meetings.iter().max_by_key(|m| per(m)).filter(|m| per(m) >= SLOW_MS_PER_DECISION)?.clone();
    let fastest = meetings.iter().filter(|m| m.note_id != slowest.note_id && per(m) < per(&slowest)).min_by_key(|m| per(m)).cloned();
    Some(SlowDecision { slowest, fastest })
}

/// Pauses shorter than this are just breathing.
const SILENCE_MIN_MS: u32 = 3_000;
/// Longer than this is a break in the meeting, not a silence in the conversation.
const SILENCE_MAX_MS: u32 = 60_000;
/// Speaking time needed before a pace or a share means anything.
const MIN_SPEECH_MS: u32 = 60_000;
/// Named slices in the talk bar; the rest fold into one.
const SLICE_LIMIT: usize = 4;

fn default_name(channel: AudioChannel) -> &'static str {
    match channel {
        AudioChannel::Mic => SELF_LABEL,
        AudioChannel::System => OTHERS_LABEL,
    }
}

/// Words and speaking time, for a pace.
#[derive(Default)]
struct Speech {
    words: u32,
    ms: u32,
}

impl Speech {
    fn add(&mut self, s: &Segment) {
        self.words += s.text.split_whitespace().count() as u32;
        self.ms = self.ms.saturating_add(s.end_ms.saturating_sub(s.start_ms));
    }

    /// Words a minute, once there is a minute of speech to measure.
    fn wpm(&self) -> Option<u32> {
        (self.ms >= MIN_SPEECH_MS).then(|| (self.words as u64 * 60_000 / self.ms as u64) as u32)
    }
}

/// Speaking time per person across the week. People linked to a Saved person share a name, so
/// one person in several meetings is one slice; you are always one slice.
#[derive(Default)]
struct Talk {
    meetings: u32,
    you: Speech,
    others: Speech,
    /// Name → (colour, ms, is self), in first-heard order.
    by_name: Vec<(String, u8, u32, bool)>,
}

impl Talk {
    fn add(&mut self, name: &str, color: u8, s: &Segment, mine: bool) {
        let ms = s.end_ms.saturating_sub(s.start_ms);
        if mine {
            self.you.add(s)
        } else {
            self.others.add(s)
        }
        let key = if mine { SELF_LABEL } else { name };
        match self.by_name.iter_mut().find(|(n, _, _, me)| *me == mine && n == key) {
            Some(entry) => entry.2 = entry.2.saturating_add(ms),
            None => self.by_name.push((key.to_string(), color, ms, mine)),
        }
    }

    fn share(mut self) -> Option<TalkShare> {
        let total_ms = self.by_name.iter().fold(0u32, |a, e| a.saturating_add(e.2));
        if total_ms < MIN_SPEECH_MS || self.by_name.len() < 2 {
            return None;
        }
        self.by_name.sort_by(|a, b| b.2.cmp(&a.2));
        let mut slices: Vec<TalkSlice> = Vec::new();
        let mut rest = 0u32;
        for (i, (name, color, ms, is_self)) in self.by_name.into_iter().enumerate() {
            if i < SLICE_LIMIT || is_self {
                slices.push(TalkSlice { name: Some(name), color_index: Some(color), ms, is_self });
            } else {
                rest = rest.saturating_add(ms);
            }
        }
        if rest > 0 {
            slices.push(TalkSlice { name: None, color_index: None, ms: rest, is_self: false });
        }
        Some(TalkShare { total_ms, meetings: self.meetings, slices })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cilica_domain::{NoteKind, new_id};

    use crate::NewNote;

    fn seg(note_id: Id, speaker: Id, start: u32, text: &str) -> Segment {
        Segment {
            id: new_id(),
            note_id,
            speaker_id: Some(speaker),
            channel: AudioChannel::System,
            start_ms: start,
            end_ms: start + 2000,
            text: text.into(),
            is_final: true,
        }
    }

    #[test]
    fn counts_fillers_per_person_on_whole_words() {
        let mut t = Tics::default();
        t.add("You", true, 1, "Basically, it's basically done. Basically.");
        t.add("Marco", false, 2, "To be fair, to be fair. To be fair it works.");
        t.add("Priya", false, 3, "Actually fine. Factually.");
        let r = t.result().unwrap();
        assert_eq!((r.you.phrase.as_str(), r.you.count), ("basically", 3));
        // Priya said one phrase once, so only Marco is shown; "factually" is not "actually".
        assert_eq!(r.others.len(), 1);
        assert_eq!((r.others[0].phrase.as_str(), r.others[0].count, r.others[0].name.as_deref()), ("to be fair", 3, Some("Marco")));
        assert!(Tics::default().result().is_none());
    }

    #[test]
    fn reads_decisions_and_their_times() {
        let body = "## Summary\nWe met.\n\n## Decisions\n- Pilot starts in May [03:10]\n- Ship the short onboarding [1:02:05]\n- Keep the free tier\n\n## Action items\n- [ ] Sam: send pricing [04:00]";
        assert_eq!(decisions(body), (3, vec![190_000, 3_725_000]));
        assert_eq!(decisions("## Decisions\n- None"), (0, vec![]));
    }

    #[test]
    fn slowest_decision_needs_to_be_slow() {
        let m = |mins: u32, n: u32| MeetingDecisions {
            note_id: new_id(),
            title: "Sync".into(),
            held_at: now(),
            duration_ms: mins * 60_000,
            decisions: n,
            decided_at_ms: vec![],
        };
        let r = slow_decision(vec![m(54, 1), m(20, 4), m(30, 1)]).unwrap();
        assert_eq!((r.slowest.duration_ms, r.fastest.as_ref().unwrap().duration_ms), (54 * 60_000, 20 * 60_000));
        assert!(slow_decision(vec![m(20, 4)]).is_none());
    }

    #[test]
    fn overtime_against_the_usual_length() {
        let base = now() - cilica_domain::chrono::TimeDelta::days(30);
        let day = |d: i64| base + cilica_domain::chrono::TimeDelta::days(d);
        let mins = |m: u32| m * 60_000;
        let sam = |d: i64, m: u32| (new_id(), "1:1 with Sam".to_string(), day(d), mins(m));
        let history = vec![sam(0, 30), sam(7, 31), sam(14, 41), sam(21, 32), sam(29, 47), (new_id(), "Standup".into(), day(29), mins(15))];
        let o = overtime(&history, day(28)).unwrap();
        // Usual is the median of 30, 31, 41, 32: 31. The one before (32) was not 5 over, so no "again".
        assert_eq!((o.usual_ms, o.actual_ms, o.again), (mins(31), mins(47), false));
        assert_eq!(o.recent_ms, vec![mins(31), mins(41), mins(32), mins(47)]);
        // Too little history, or not enough over, says nothing.
        assert!(overtime(&history[3..], day(28)).is_none());
        assert!(overtime(&[sam(0, 30), sam(7, 30), sam(29, 33)], day(28)).is_none());
    }

    #[test]
    fn undone_lines_come_from_each_side() {
        assert_eq!(first_new_line("# Intro\nWe will prioritise synergies.", "# Intro\nFix onboarding first.").as_deref(), Some("We will prioritise synergies."));
        assert_eq!(first_new_line("same", "same"), None);
    }

    #[tokio::test]
    async fn glance_reads_recent_meetings() {
        let db = Db::open_in_memory().await.unwrap();
        let note = db
            .create_note(NewNote { title: "Q3 planning sync".into(), kind: NoteKind::Meeting, started_at: Some(now()), ..NewNote::default() })
            .await
            .unwrap();
        let (_, others) = db.ensure_channel_speakers(note.id).await.unwrap();
        db.rename_speaker(others.id, "Priya").await.unwrap();
        for (i, t) in ["Pricing first, then pricing again.", "Priya says pricing.", "The pricing page."].iter().enumerate() {
            db.upsert_segment(&seg(note.id, others.id, i as u32 * 3000, t)).await.unwrap();
        }
        db.set_note_quotes(note.id, &[Quote { at_ms: 3500, text: "Priya says pricing.".into() }]).await.unwrap();

        let g = db.home_glance(now() - cilica_domain::chrono::TimeDelta::days(7)).await.unwrap();
        assert_eq!(g.quotes.len(), 1);
        assert_eq!(g.quotes[0].speaker.as_deref(), Some("Priya"));
        assert_eq!(g.quotes[0].note_title, "Q3 planning sync");

        // Nothing from before the window.
        let later = db.home_glance(now() + cilica_domain::chrono::TimeDelta::days(1)).await.unwrap();
        assert!(later.quotes.is_empty() && later.talk.is_none());
    }

    #[tokio::test]
    async fn talk_pace_and_silence() {
        let db = Db::open_in_memory().await.unwrap();
        let note =
            db.create_note(NewNote { title: "Sync".into(), kind: NoteKind::Meeting, started_at: Some(now()), ..NewNote::default() }).await.unwrap();
        let (you, others) = db.ensure_channel_speakers(note.id).await.unwrap();
        // You: 72 s with 103 words; Others: 30 s with 60 words. A 7 s pause after your question.
        let line = |n: usize| vec!["word"; n].join(" ");
        let mut a = seg(note.id, you.id, 0, &line(100));
        a.channel = AudioChannel::Mic;
        a.end_ms = 70_000;
        let mut q = seg(note.id, you.id, 70_000, "Who signs off?");
        q.channel = AudioChannel::Mic;
        q.end_ms = 72_000;
        let mut b = seg(note.id, others.id, 79_000, &line(60));
        b.end_ms = 109_000;
        for s in [&a, &q, &b] {
            db.upsert_segment(s).await.unwrap();
        }

        let g = db.home_glance(now() - cilica_domain::chrono::TimeDelta::days(7)).await.unwrap();
        let talk = g.talk.unwrap();
        assert_eq!((talk.total_ms, talk.meetings), (102_000, 1));
        assert!(talk.slices[0].is_self && talk.slices[0].ms == 72_000);
        let silence = g.silence.unwrap();
        assert_eq!((silence.gap_ms, silence.at_ms, silence.is_self), (7_000, 70_000, true));
        assert_eq!(silence.before, "Who signs off?");
        // 103 words over 72 s of your speech; others spoke under a minute, so no figure for them.
        let pace = g.pace.unwrap();
        assert_eq!((pace.you_wpm, pace.others_wpm, pace.last_week_wpm), (85, None, None));
    }
}
