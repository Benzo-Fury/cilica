//! Moving words someone selected in a transcript to who actually said them. Owner: Audio agent.
//!
//! A selection runs from one word of a line to a word of the same or a later line. Each line it
//! touches is split at the selection's edges (times spread across the line by characters, since
//! lines carry no per-word times) and the selected piece goes to the chosen speaker. The piece
//! before keeps the line's id; the others get new ids. Undoing gives each moved piece back to who
//! said it before: the split pieces stay, but they read as one turn again.
//!
//! Words moved to someone confirmed as a Saved person can teach Cilica their voice: the caller
//! voiceprints the audio of the moved lines and stores it with [`Db::learn_moved_words`]. Such a
//! sample points at the moved line, so undoing the move takes it back.

use std::collections::HashSet;

use cilica_domain::{Id, MovedLine, Segment, Speaker, WordPoint};
use sqlx::Row;

use crate::transcripts::{NewSpeaker, channel_str, normalized, parse_id, refresh_voiceprints, vec_to_blob};
use crate::{Db, StoreError, StoreResult};

/// Number of `speaker-N` colour tokens.
const COLOR_COUNT: usize = 8;

/// One piece of a split line.
#[derive(Debug, Clone, PartialEq)]
struct Piece {
    text: String,
    start_ms: u32,
    end_ms: u32,
    /// The selected words.
    moved: bool,
}

/// Split `segment` around its words `from..=to` (clamped to the line), with times spread across
/// the line by characters. Empty pieces are left out.
fn split(segment: &Segment, from: usize, to: usize) -> Vec<Piece> {
    let words: Vec<&str> = segment.text.split_whitespace().collect();
    let to = to.min(words.len().saturating_sub(1));
    let parts = [(words[..from].join(" "), false), (words[from..=to].join(" "), true), (words[to + 1..].join(" "), false)];
    // Characters up to each piece's start, counting the space before it.
    let total: usize = parts.iter().filter(|(t, _)| !t.is_empty()).map(|(t, _)| t.chars().count() + 1).sum::<usize>().max(1);
    let span = segment.end_ms.saturating_sub(segment.start_ms) as u64;
    let at = |chars: usize| segment.start_ms + (span * chars as u64 / total as u64) as u32;
    let mut out = Vec::new();
    let mut chars = 0;
    for (text, moved) in parts {
        if text.is_empty() {
            continue;
        }
        let start_ms = at(chars);
        chars += text.chars().count() + 1;
        out.push(Piece { text, start_ms, end_ms: at(chars), moved });
    }
    if let Some(last) = out.last_mut() {
        last.end_ms = segment.end_ms;
    }
    out
}

impl Db {
    /// Move the words from `from` to `to` (both included) to `speaker_id`. Lines that were theirs
    /// already are left alone. Returns the lines the words became, with who said them before.
    pub async fn move_words(&self, note_id: Id, from: WordPoint, to: WordPoint, speaker_id: Id) -> StoreResult<Vec<MovedLine>> {
        let speaker_id = self.resolve_speaker(speaker_id);
        if self.get_speaker(speaker_id).await?.note_id != note_id {
            return Err(StoreError::Invalid("That speaker is in a different meeting. Pick someone from this one.".into()));
        }
        let segments = self.list_segments(note_id).await?;
        let index = |id: Id| segments.iter().position(|s| s.id == id).ok_or_else(|| StoreError::NotFound("Those words".into()));
        let (mut first, mut last) = ((index(from.segment_id)?, from.word as usize), (index(to.segment_id)?, to.word as usize));
        if first > last {
            std::mem::swap(&mut first, &mut last);
        }
        if segments[first.0..=last.0].iter().any(|s| !s.is_final) {
            return Err(StoreError::Invalid("Cilica is still writing some of these words. Wait for the line to finish, then try again.".into()));
        }

        let mut tx = self.begin_write().await?;
        let mut moved = Vec::new();
        for (i, segment) in segments.iter().enumerate().take(last.0 + 1).skip(first.0) {
            let count = segment.text.split_whitespace().count();
            let a = if i == first.0 { first.1 } else { 0 };
            let b = if i == last.0 { last.1.min(count.saturating_sub(1)) } else { count.saturating_sub(1) };
            if count == 0 || a > b || segment.speaker_id.map(|s| self.resolve_speaker(s)) == Some(speaker_id) {
                continue;
            }
            for (k, piece) in split(segment, a, b).into_iter().enumerate() {
                // The first piece keeps the line's id; the others are new lines.
                let id = if k == 0 { segment.id } else { cilica_domain::new_id() };
                let who = if piece.moved { Some(speaker_id) } else { segment.speaker_id };
                sqlx::query(
                    "INSERT INTO segments (id, note_id, speaker_id, channel, start_ms, end_ms, text, is_final) VALUES (?, ?, ?, ?, ?, ?, ?, 1) \
                     ON CONFLICT(id) DO UPDATE SET speaker_id = excluded.speaker_id, start_ms = excluded.start_ms, \
                     end_ms = excluded.end_ms, text = excluded.text",
                )
                .bind(id.to_string())
                .bind(note_id.to_string())
                .bind(who.map(|s| s.to_string()))
                .bind(channel_str(segment.channel))
                .bind(piece.start_ms as i64)
                .bind(piece.end_ms as i64)
                .bind(&piece.text)
                .execute(&mut *tx)
                .await?;
                if piece.moved {
                    moved.push(MovedLine { segment_id: id, previous_speaker_id: segment.speaker_id });
                }
            }
        }
        tx.commit().await?;
        Ok(moved)
    }

    /// Undo [`Db::move_words`]: each moved line goes back to who said it before, and any voice
    /// sample learnt from it is taken back.
    pub async fn restore_moved_words(&self, note_id: Id, lines: &[MovedLine]) -> StoreResult<()> {
        let mut tx = self.begin_write().await?;
        let mut touched = Vec::new();
        for line in lines {
            sqlx::query("UPDATE segments SET speaker_id = ? WHERE id = ? AND note_id = ?")
                .bind(line.previous_speaker_id.map(|s| self.resolve_speaker(s).to_string()))
                .bind(line.segment_id.to_string())
                .bind(note_id.to_string())
                .execute(&mut *tx)
                .await?;
            for row in sqlx::query("SELECT DISTINCT person_id FROM voice_samples WHERE segment_id = ?").bind(line.segment_id.to_string()).fetch_all(&mut *tx).await? {
                touched.push(parse_id(&row.try_get::<String, _>("person_id")?)?);
            }
            sqlx::query("DELETE FROM voice_samples WHERE segment_id = ?").bind(line.segment_id.to_string()).execute(&mut *tx).await?;
        }
        refresh_voiceprints(&mut tx, touched).await?;
        tx.commit().await?;
        Ok(())
    }

    /// The speaker in this meeting who is `person_id`, linked (and confirmed) as a new speaker
    /// when they have not been heard in it yet.
    pub async fn speaker_for_person(&self, note_id: Id, person_id: Id) -> StoreResult<Speaker> {
        let person = self.get_person(person_id).await?;
        let speakers = self.list_speakers(note_id).await?;
        if let Some(s) = speakers.iter().find(|s| s.person_id == Some(person_id) || (person.is_self && s.is_self && s.person_id.is_none())) {
            if s.person_id.is_none() {
                return self.link_speaker(s.id, Some(person_id)).await;
            }
            return Ok(s.clone());
        }
        let speaker = self
            .create_speaker(note_id, NewSpeaker { label: person.name.clone(), color_index: person.color_index, is_self: person.is_self, embedding: None })
            .await?;
        self.link_speaker(speaker.id, Some(person_id)).await
    }

    /// A new speaker in this meeting called `name` here only, in the next free colour.
    pub async fn speaker_named(&self, note_id: Id, name: &str) -> StoreResult<Speaker> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Invalid("They need a name. Type one and try again.".into()));
        }
        let speakers = self.list_speakers(note_id).await?;
        let used: HashSet<u8> = speakers.iter().map(|s| s.color_index).collect();
        let color = (1..=COLOR_COUNT as u8).find(|c| !used.contains(c)).unwrap_or((speakers.len() % COLOR_COUNT) as u8 + 1);
        let speaker = self.create_speaker(note_id, NewSpeaker { label: name.into(), color_index: color, is_self: false, embedding: None }).await?;
        sqlx::query("UPDATE speakers SET person_picked = 1 WHERE id = ?").bind(speaker.id.to_string()).execute(self.pool()).await?;
        Ok(speaker)
    }

    /// Whether words moved to this speaker can teach Cilica a voice: someone confirmed who they are.
    pub async fn learns_from_words(&self, speaker_id: Id) -> StoreResult<bool> {
        let row = sqlx::query("SELECT person_id IS NOT NULL AND voice_confirmed = 1 AS learns FROM speakers WHERE id = ?")
            .bind(self.resolve_speaker(speaker_id).to_string())
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("The speaker".into()))?;
        Ok(row.try_get("learns")?)
    }

    /// Add a voiceprint of words moved to `speaker_id` to their person, tied to the first moved
    /// line (`segment_id`) so undoing the move takes it back. Returns whether it was added.
    pub async fn learn_moved_words(&self, speaker_id: Id, segment_id: Id, embedding: &[f32], speech_ms: u32) -> StoreResult<bool> {
        let speaker_id = self.resolve_speaker(speaker_id);
        let mut tx = self.begin_write().await?;
        let row = sqlx::query("SELECT person_id FROM speakers WHERE id = ? AND voice_confirmed = 1").bind(speaker_id.to_string()).fetch_optional(&mut *tx).await?;
        let Some(person) = row.map(|r| r.try_get::<Option<String>, _>("person_id")).transpose()?.flatten() else {
            return Ok(false);
        };
        let person = parse_id(&person)?;
        sqlx::query("INSERT INTO voice_samples (id, person_id, speaker_id, embedding, created_at, speech_ms, segment_id) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(cilica_domain::new_id().to_string())
            .bind(person.to_string())
            .bind(speaker_id.to_string())
            .bind(vec_to_blob(&normalized(embedding)))
            .bind(crate::codec::ts(cilica_domain::now()))
            .bind(speech_ms as i64)
            .bind(segment_id.to_string())
            .execute(&mut *tx)
            .await?;
        refresh_voiceprints(&mut tx, vec![person]).await?;
        tx.commit().await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cilica_domain::{AudioChannel, new_id};

    async fn note(db: &Db) -> Id {
        let id = new_id();
        let now = cilica_domain::now().to_rfc3339();
        sqlx::query("INSERT INTO notes (id, title, kind, created_at, updated_at) VALUES (?, 'Test', 'meeting', ?, ?)")
            .bind(id.to_string())
            .bind(&now)
            .bind(&now)
            .execute(db.pool())
            .await
            .unwrap();
        id
    }

    fn line(note_id: Id, speaker: Id, start_ms: u32, end_ms: u32, text: &str) -> Segment {
        Segment { id: new_id(), note_id, speaker_id: Some(speaker), channel: AudioChannel::System, start_ms, end_ms, text: text.into(), is_final: true }
    }

    fn speaker(label: &str) -> NewSpeaker {
        NewSpeaker { label: label.into(), color_index: 2, is_self: false, embedding: None }
    }

    fn texts(segments: &[Segment]) -> Vec<(String, Option<Id>)> {
        segments.iter().map(|s| (s.text.clone(), s.speaker_id)).collect()
    }

    #[test]
    fn split_spreads_time_by_characters() {
        let s = Segment { id: new_id(), note_id: new_id(), speaker_id: None, channel: AudioChannel::Mic, start_ms: 1000, end_ms: 2000, text: "aaa bbb ccc dd".into(), is_final: true };
        let pieces = split(&s, 1, 2);
        assert_eq!(pieces.iter().map(|p| (p.text.as_str(), p.moved)).collect::<Vec<_>>(), [("aaa", false), ("bbb ccc", true), ("dd", false)]);
        assert_eq!((pieces[0].start_ms, pieces[2].end_ms), (1000, 2000));
        assert!(pieces.windows(2).all(|w| w[0].end_ms == w[1].start_ms && w[0].start_ms < w[1].start_ms));
        // The whole line is one moved piece.
        assert_eq!(split(&s, 0, 9), [Piece { text: "aaa bbb ccc dd".into(), start_ms: 1000, end_ms: 2000, moved: true }]);
    }

    /// Words in the middle of a line become their own line; undo gives them back.
    #[tokio::test]
    async fn moves_words_in_the_middle_of_a_line() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let maya = db.create_speaker(n, speaker("Maya")).await.unwrap();
        let tomas = db.create_speaker(n, speaker("Tomás")).await.unwrap();
        let s = line(n, maya.id, 0, 6000, "So the plan is fine. Does that include Notion? Yes it does.");
        db.upsert_segment(&s).await.unwrap();

        let moved = db.move_words(n, WordPoint { segment_id: s.id, word: 5 }, WordPoint { segment_id: s.id, word: 8 }, tomas.id).await.unwrap();
        let after = db.list_segments(n).await.unwrap();
        assert_eq!(
            texts(&after),
            [
                ("So the plan is fine.".into(), Some(maya.id)),
                ("Does that include Notion?".into(), Some(tomas.id)),
                ("Yes it does.".into(), Some(maya.id)),
            ]
        );
        assert_eq!(after[0].id, s.id);
        assert_eq!(moved, [MovedLine { segment_id: after[1].id, previous_speaker_id: Some(maya.id) }]);

        db.restore_moved_words(n, &moved).await.unwrap();
        assert!(db.list_segments(n).await.unwrap().iter().all(|s| s.speaker_id == Some(maya.id)));
    }

    /// A selection across lines moves the tail of one, all of the next, and the head of the last;
    /// lines already theirs are left alone.
    #[tokio::test]
    async fn moves_words_across_lines() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let a = db.create_speaker(n, speaker("A")).await.unwrap();
        let b = db.create_speaker(n, speaker("B")).await.unwrap();
        let one = line(n, a.id, 0, 3000, "one two three");
        let two = line(n, b.id, 3000, 5000, "four five");
        let three = line(n, a.id, 5000, 8000, "six seven eight");
        for s in [&one, &two, &three] {
            db.upsert_segment(s).await.unwrap();
        }
        let moved = db.move_words(n, WordPoint { segment_id: three.id, word: 0 }, WordPoint { segment_id: one.id, word: 2 }, b.id).await.unwrap();
        assert_eq!(
            texts(&db.list_segments(n).await.unwrap()),
            [
                ("one two".into(), Some(a.id)),
                ("three".into(), Some(b.id)),
                ("four five".into(), Some(b.id)),
                ("six".into(), Some(b.id)),
                ("seven eight".into(), Some(a.id)),
            ]
        );
        assert_eq!(moved.len(), 2);
    }

    #[tokio::test]
    async fn refuses_lines_still_being_written() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let a = db.create_speaker(n, speaker("A")).await.unwrap();
        let mut s = line(n, a.id, 0, 1000, "not done yet");
        s.is_final = false;
        db.upsert_segment(&s).await.unwrap();
        let p = WordPoint { segment_id: s.id, word: 0 };
        assert!(matches!(db.move_words(n, p, p, a.id).await, Err(StoreError::Invalid(_))));
    }

    /// A Saved person not heard yet gets a confirmed speaker; words moved to them teach their
    /// voice, and undoing the move takes that back.
    #[tokio::test]
    async fn words_teach_a_voice_until_undone() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let a = db.create_speaker(n, speaker("A")).await.unwrap();
        let sam = db.create_person("Sam", None).await.unwrap();
        let s = line(n, a.id, 0, 4000, "hello there friend");
        db.upsert_segment(&s).await.unwrap();

        let them = db.speaker_for_person(n, sam.id).await.unwrap();
        assert_eq!(them.person_id, Some(sam.id));
        assert_eq!(db.speaker_for_person(n, sam.id).await.unwrap().id, them.id);
        assert!(db.learns_from_words(them.id).await.unwrap());
        assert!(!db.learns_from_words(a.id).await.unwrap());

        let p = WordPoint { segment_id: s.id, word: 0 };
        let moved = db.move_words(n, p, p, them.id).await.unwrap();
        assert!(db.learn_moved_words(them.id, moved[0].segment_id, &[1.0, 0.0], 2000).await.unwrap());
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 1);

        db.restore_moved_words(n, &moved).await.unwrap();
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 0);
    }

    #[tokio::test]
    async fn names_a_speaker_for_this_meeting_in_a_free_colour() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        db.create_speaker(n, NewSpeaker { label: "A".into(), color_index: 1, is_self: false, embedding: None }).await.unwrap();
        let s = db.speaker_named(n, " Ines ").await.unwrap();
        assert_eq!((s.label.as_str(), s.color_index, s.person_id), ("Ines", 2, None));
        assert!(matches!(db.speaker_named(n, "  ").await, Err(StoreError::Invalid(_))));
    }
}
