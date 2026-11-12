//! The swear jar's counts. Swears said in meetings are read from transcript segments each time the
//! jar is counted, so a coin follows its line (a dropped echo or a deleted meeting takes it away).
//! Swears typed into notes are kept in `swear_coins`, because a note's text keeps changing and a
//! typed coin should stay put once it has gone in.

use std::collections::HashMap;

use margin_domain::chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone};
use margin_domain::{
    AudioChannel, Id, SwearCount, SwearJar, SwearJarSettings, SwearPerson, SwearWeek, Swears, Timestamp, new_id,
};
use sqlx::Row;

use crate::codec::{self, parse_opt_ts, parse_ts};
use crate::{Db, OTHERS_LABEL, StoreError, StoreResult};

// ---------- Weeks ---------- //

/// How many weeks the jar remembers, this one included.
const WEEKS: usize = 9;
/// How many of this week's words the jar lists.
const WORD_LIMIT: usize = 6;

/// Monday at midnight of the week holding `now`, in `now`'s own time zone.
pub fn week_start<Tz: TimeZone>(now: DateTime<Tz>) -> Timestamp {
    let tz = now.timezone();
    let back = Duration::days(now.weekday().num_days_from_monday() as i64);
    let monday = now.date_naive() - back;
    match tz.from_local_datetime(&monday.and_time(NaiveTime::MIN)).earliest() {
        Some(t) => t.to_utc(),
        // Midnight skipped by a clock change: an hour either way does not matter here.
        None => (now - back).to_utc(),
    }
}

// ---------- Tally ---------- //

/// Who put a coin in.
enum Who {
    You,
    /// Someone in a meeting, keyed by their Saved person when linked, else by the meeting speaker.
    Speaker { key: String, name: String, color: u8 },
}

/// Adds coins into weeks, people and words, then becomes a [`SwearJar`].
struct Tally {
    week_start: Timestamp,
    /// This week's coins only count from here: the later of the week's start and the last time the
    /// jar was emptied by hand.
    counted_from: Timestamp,
    /// Coins per week, oldest first; the last one is this week.
    weeks: [u32; WEEKS],
    last_week: u32,
    count: u32,
    /// In order of first appearance; `None` key is you.
    people: Vec<(Option<String>, SwearPerson)>,
    words: HashMap<&'static str, u32>,
    has_meetings: bool,
}

impl Tally {
    fn new(week_start: Timestamp, emptied: Option<Timestamp>) -> Self {
        Self {
            week_start,
            counted_from: emptied.filter(|e| *e > week_start).unwrap_or(week_start),
            weeks: [0; WEEKS],
            last_week: 0,
            count: 0,
            people: Vec::new(),
            words: HashMap::new(),
            has_meetings: false,
        }
    }

    /// The first moment the jar looks at.
    fn earliest(&self) -> Timestamp {
        self.week_start - Duration::weeks(WEEKS as i64 - 1)
    }

    /// The person's row this week, made with no coins if new.
    fn person(&mut self, who: &Who) -> &mut SwearPerson {
        let key = match who {
            Who::You => None,
            Who::Speaker { key, .. } => Some(key.clone()),
        };
        let index = match self.people.iter().position(|(k, _)| *k == key) {
            Some(i) => i,
            None => {
                // New this week: you have no name or colour, others carry theirs
                let person = match who {
                    Who::You => SwearPerson { name: None, color_index: None, count: 0 },
                    Who::Speaker { name, color, .. } => SwearPerson { name: Some(name.clone()), color_index: Some(*color), count: 0 },
                };
                self.people.push((key, person));
                self.people.len() - 1
            }
        };
        &mut self.people[index].1
    }

    /// Someone spoke in one of this week's meetings, coins or not.
    fn meet(&mut self, who: &Who) {
        self.has_meetings = true;
        self.person(who);
    }

    /// One coin, said or typed at `at`.
    fn add(&mut self, at: Timestamp, root: &'static str, who: &Who) {
        // Past weeks just add up
        if at < self.week_start {
            let back = ((self.week_start - at).num_seconds() / Duration::weeks(1).num_seconds()) as usize;
            if let Some(slot) = (WEEKS - 1).checked_sub(back + 1) {
                self.weeks[slot] += 1;
            }
            if back == 0 {
                self.last_week += 1;
            }
            return;
        }

        // This week counts from the last emptying, by person and by word
        if at < self.counted_from {
            return;
        }
        self.weeks[WEEKS - 1] += 1;
        self.count += 1;
        self.person(who).count += 1;
        *self.words.entry(root).or_default() += 1;
    }

    fn finish(self) -> SwearJar {
        // People: most coins first, you ahead on a tie
        let mut people: Vec<SwearPerson> = self.people.into_iter().map(|(_, p)| p).collect();
        people.sort_by_key(|p| (std::cmp::Reverse(p.count), p.name.is_some()));

        // Words: most used first, then alphabetical so the order is stable
        let mut words: Vec<SwearCount> = self.words.into_iter().map(|(w, count)| SwearCount { word: w.to_string(), count }).collect();
        words.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.word.cmp(&b.word)));
        words.truncate(WORD_LIMIT);

        // Weeks, oldest first, and the fullest of them
        let weeks: Vec<SwearWeek> = self
            .weeks
            .iter()
            .enumerate()
            .map(|(i, &count)| SwearWeek { start: self.week_start - Duration::weeks((WEEKS - 1 - i) as i64), count })
            .collect();
        let record = weeks.iter().filter(|w| w.count > 0).max_by_key(|w| (w.count, w.start)).cloned();

        SwearJar {
            count: self.count,
            last_week: self.last_week,
            people,
            words,
            weeks,
            record,
            week_start: self.week_start,
            has_meetings: self.has_meetings,
        }
    }
}

// ---------- Store ---------- //

impl Db {
    /// Put a coin in for a word typed into a note. Fails when the word is not on the jar's list.
    /// Returns the root it was counted under.
    pub async fn add_swear_coin(&self, note_id: Option<Id>, word: &str, at: Timestamp) -> StoreResult<String> {
        let Some((root, _)) = Swears::canonical(word) else {
            return Err(StoreError::Invalid(format!("“{word}” is not a word the swear jar counts.")));
        };
        sqlx::query("INSERT INTO swear_coins (id, note_id, word, at) VALUES (?, ?, ?, ?)")
            .bind(codec::id(new_id()))
            .bind(codec::opt_id(note_id))
            .bind(root)
            .bind(codec::ts(at))
            .execute(self.pool())
            .await?;
        Ok(root.to_string())
    }

    /// Empty this week's jar. Past weeks keep their coins.
    pub async fn empty_swear_jar(&self, at: Timestamp) -> StoreResult<()> {
        sqlx::query("INSERT INTO swear_jar_emptied (at) VALUES (?)").bind(codec::ts(at)).execute(self.pool()).await?;
        Ok(())
    }

    /// The jar for the week starting `week_start`, counted the way `prefs` says.
    pub async fn swear_jar(&self, week_start: Timestamp, prefs: &SwearJarSettings) -> StoreResult<SwearJar> {
        let emptied: Option<String> = sqlx::query_scalar("SELECT max(at) FROM swear_jar_emptied").fetch_one(self.pool()).await?;
        let mut tally = Tally::new(week_start, parse_opt_ts(emptied)?);

        // Meetings: every final line, everyone in them
        if prefs.transcripts {
            self.tally_meetings(&mut tally, true).await?;
        }

        // Notes: the coins you typed
        if prefs.notes {
            let rows = sqlx::query("SELECT word, at FROM swear_coins WHERE at >= ?").bind(codec::ts(tally.earliest())).fetch_all(self.pool()).await?;
            for row in rows {
                let word: String = row.try_get("word")?;
                let Some((root, _)) = Swears::canonical(&word) else { continue };
                tally.add(parse_ts(&row.try_get::<String, _>("at")?)?, root, &Who::You);
            }
        }
        Ok(tally.finish())
    }

    /// Adds every swear said in the jar's weeks of meetings.
    async fn tally_meetings(&self, tally: &mut Tally, mild: bool) -> StoreResult<()> {
        let rows = sqlx::query(
            "SELECT id, started_at, created_at FROM notes WHERE deleted_at IS NULL AND kind = 'meeting' \
             AND COALESCE(started_at, created_at) >= ?",
        )
        .bind(codec::ts(tally.earliest()))
        .fetch_all(self.pool())
        .await?;

        for row in rows {
            // When the meeting was held, and who was in it
            let held = match parse_opt_ts(row.try_get("started_at")?)? {
                Some(t) => t,
                None => parse_ts(&row.try_get::<String, _>("created_at")?)?,
            };
            let note_id = codec::parse_id(&row.try_get::<String, _>("id")?)?;
            let segments = self.list_segments(note_id).await?;
            let speakers = self.list_speakers(note_id).await?;
            let display = self.speaker_display_names(&speakers).await?;
            let who_said = |speaker_id: Option<Id>, channel: AudioChannel| {
                match speaker_id.and_then(|id| speakers.iter().find(|sp| sp.id == id)) {
                    Some(sp) if sp.is_self => Who::You,
                    Some(sp) => Who::Speaker {
                        key: codec::id(sp.person_id.unwrap_or(sp.id)),
                        name: display.get(&sp.id).cloned().unwrap_or_else(|| sp.label.clone()),
                        color: sp.color_index,
                    },
                    None if channel == AudioChannel::Mic => Who::You,
                    None => Who::Speaker { key: codec::id(note_id), name: OTHERS_LABEL.to_string(), color: 2 },
                }
            };

            // Each final line: its speaker joins this week's people, its swears go in
            for s in segments.iter().filter(|s| s.is_final && !s.text.trim().is_empty()) {
                let who = who_said(s.speaker_id, s.channel);
                if held >= tally.week_start {
                    tally.meet(&who);
                }
                let at = held + Duration::milliseconds(s.start_ms as i64);
                for hit in Swears::find(&s.text, mild) {
                    tally.add(at, hit.root, &who);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use margin_domain::chrono::{FixedOffset, Utc};
    use margin_domain::{NoteKind, Segment, now};

    use crate::NewNote;

    fn line(note_id: Id, speaker: Id, start: u32, text: &str) -> Segment {
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
    fn weeks_start_on_monday_at_local_midnight() {
        let tz = FixedOffset::east_opt(10 * 3600).unwrap();
        // Thursday 12 November 2026, 08:00 in UTC+10.
        let now = tz.with_ymd_and_hms(2026, 11, 12, 8, 0, 0).unwrap();
        let start = week_start(now);
        assert_eq!(start, tz.with_ymd_and_hms(2026, 11, 9, 0, 0, 0).unwrap().to_utc());
        // A Monday is its own week's start.
        assert_eq!(week_start(tz.with_ymd_and_hms(2026, 11, 9, 0, 0, 0).unwrap()), start);
    }

    #[test]
    fn tally_splits_weeks_and_respects_emptying() {
        let start = Utc::now() - Duration::days(2);
        let mut t = Tally::new(start, Some(start + Duration::hours(1)));
        t.add(start - Duration::days(3), "fuck", &Who::You);
        t.add(start - Duration::days(10), "fuck", &Who::You);
        t.add(start + Duration::minutes(30), "shit", &Who::You);
        t.add(start + Duration::hours(2), "shit", &Who::You);
        let jar = t.finish();
        assert_eq!(jar.count, 1, "the coin before emptying is gone");
        assert_eq!(jar.last_week, 1);
        assert_eq!(jar.weeks.iter().map(|w| w.count).collect::<Vec<_>>(), [0, 0, 0, 0, 0, 0, 1, 1, 1]);
        assert_eq!(jar.record.unwrap().start, start);
    }

    #[tokio::test]
    async fn counts_meetings_and_typed_coins() {
        let db = Db::open_in_memory().await.unwrap();
        let start = week_start(Utc::now());
        let meeting = db
            .create_note(NewNote { title: "Q3".into(), kind: NoteKind::Meeting, started_at: Some(now()), ..NewNote::default() })
            .await
            .unwrap();
        let (you, others) = db.ensure_channel_speakers(meeting.id).await.unwrap();
        db.rename_speaker(others.id, "Marco").await.unwrap();
        let quiet = db.create_speaker(meeting.id, crate::NewSpeaker { label: "Priya".into(), color_index: 3, is_self: false, embedding: None }).await.unwrap();
        db.upsert_segment(&line(meeting.id, you.id, 0, "Oh, for fuck’s sake.")).await.unwrap();
        db.upsert_segment(&line(meeting.id, others.id, 3000, "It was a damn good idea.")).await.unwrap();
        db.upsert_segment(&line(meeting.id, quiet.id, 6000, "Can we not?")).await.unwrap();
        let note = db.create_note(NewNote { title: "Retro".into(), ..NewNote::default() }).await.unwrap();
        assert_eq!(db.add_swear_coin(Some(note.id), "Bullshit", now()).await.unwrap(), "shit");
        db.add_swear_coin(None, "fucking", now()).await.unwrap();
        assert!(db.add_swear_coin(None, "pricing", now()).await.is_err());

        let jar = db.swear_jar(start, &SwearJarSettings::default()).await.unwrap();
        assert_eq!(jar.count, 4);
        assert!(jar.has_meetings);
        let people: Vec<_> = jar.people.iter().map(|p| (p.name.clone(), p.count)).collect();
        assert_eq!(people, [(None, 3), (Some("Marco".into()), 1), (Some("Priya".into()), 0)]);
        assert_eq!(jar.words[0], SwearCount { word: "fuck".into(), count: 2 });

        // Whole sources can be left out.
        let strict = SwearJarSettings { notes: false, ..SwearJarSettings::default() };
        assert_eq!(db.swear_jar(start, &strict).await.unwrap().count, 2);

        // A coin typed into a note stays when the note goes.
        db.delete_note_forever(note.id).await.unwrap();
        assert_eq!(db.swear_jar(start, &SwearJarSettings::default()).await.unwrap().count, 4);

        // Emptying clears this week only.
        db.empty_swear_jar(now() + Duration::minutes(1)).await.unwrap();
        let emptied = db.swear_jar(start, &SwearJarSettings::default()).await.unwrap();
        assert_eq!(emptied.count, 0);
        assert_eq!(emptied.people.iter().map(|p| p.count).sum::<u32>(), 0);
    }
}
