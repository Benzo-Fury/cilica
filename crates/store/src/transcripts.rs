//! segments, speakers, people. Owner: Audio agent.
//!
//! The first block of methods is the cross-area contract (the agent's tools call them); their
//! signatures do not change. The rest serve the recording pipeline and the recording commands.
//!
//! Storage conventions: ids are UUID strings, timestamps RFC 3339 strings, embeddings and
//! voiceprints little-endian `f32` blobs. A person's voiceprint is the L2-normalised average of
//! every speaker embedding folded into it; `sample_count` is how many were folded.

use std::collections::{HashMap, HashSet};

use margin_domain::{AudioChannel, Id, Person, Quote, Segment, Speaker, SpeakerSuggestion, Timestamp, Transcript};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::{Db, StoreError, StoreResult};

/// `SELECT` for [`Person`] rows with their meeting count, followed by `$tail`.
macro_rules! person_select {
    ($tail:literal) => {
        concat!(
            "SELECT p.id, p.name, p.color_index, p.sample_count, p.created_at, p.is_self, ",
            "(SELECT count(DISTINCT s.note_id) FROM speakers s JOIN notes n ON n.id = s.note_id ",
            "WHERE s.person_id = p.id AND n.deleted_at IS NULL) AS meeting_count FROM people p",
            $tail
        )
    };
}
/// Label of the mic-channel speaker before (and usually after) diarization.
pub const SELF_LABEL: &str = "You";
/// Label of the system-channel speaker before diarization.
pub const OTHERS_LABEL: &str = "Others";

/// Number of `speaker-N` colour tokens.
const COLOR_COUNT: u8 = 8;

/// One speaker found by diarization, ready to be stored.
#[derive(Debug, Clone)]
pub struct NewSpeaker {
    pub label: String,
    pub color_index: u8,
    pub is_self: bool,
    /// Cluster centroid (L2-normalised by the caller or here).
    pub embedding: Option<Vec<f32>>,
}

/// A Saved person's voice, for matching: the mean of their samples, and the samples.
#[derive(Debug, Clone)]
pub struct Voiceprint {
    pub person_id: Id,
    pub name: String,
    pub vector: Vec<f32>,
    pub samples: Vec<Vec<f32>>,
}

impl Voiceprint {
    /// How much `embedding` sounds like this person: the best of their mean and each sample, so
    /// a voice that sounds different on a call and in the room is still recognised.
    pub fn similarity(&self, embedding: &[f32]) -> f32 {
        self.samples.iter().map(|s| cosine(embedding, s)).fold(cosine(embedding, &self.vector), f32::max)
    }
}

/// Name of the Saved person who is the app's user.
pub const SELF_PERSON_NAME: &str = "You";

impl Db {
    // ── Contract (used by the assistant) ─────────────────────────────────

    /// Speakers and segments of a note, segments ordered by start time.
    pub async fn get_transcript(&self, note_id: Id) -> StoreResult<Transcript> {
        let speakers = self.list_speakers(note_id).await?;
        let segments = self.list_segments(note_id).await?;
        let row = sqlx::query("SELECT audio_path, duration_ms, quotes_json, transcript_timed FROM notes WHERE id = ?")
            .bind(note_id.to_string())
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("The note".into()))?;
        let audio_path: Option<String> = row.try_get("audio_path")?;
        let duration_ms: Option<i64> = row.try_get("duration_ms")?;
        let duration_ms = duration_ms.map(|d| d.clamp(0, u32::MAX as i64) as u32).or_else(|| segments.iter().map(|s| s.end_ms).max());
        let quotes_json: Option<String> = row.try_get("quotes_json")?;
        // Quotes only make sense beside the words they came from.
        let quotes = if segments.is_empty() { Vec::new() } else { quotes_json.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default() };
        let timed: bool = row.try_get("transcript_timed")?;
        Ok(Transcript { note_id, speakers, segments, duration_ms, has_audio: audio_path.is_some(), quotes, timed })
    }

    /// Replace the lines shown under a meeting's transcript.
    pub async fn set_note_quotes(&self, note_id: Id, quotes: &[Quote]) -> StoreResult<()> {
        let json = if quotes.is_empty() { None } else { serde_json::to_string(quotes).ok() };
        sqlx::query("UPDATE notes SET quotes_json = ? WHERE id = ?").bind(json).bind(note_id.to_string()).execute(self.pool()).await?;
        Ok(())
    }

    /// The final transcript as plain text for the LLM and for search, one line per turn:
    /// `[12:34] Sam: text`. Empty when the note has no transcript.
    pub async fn transcript_text(&self, note_id: Id) -> StoreResult<String> {
        let speakers = self.list_speakers(note_id).await?;
        let names = self.speaker_display_names(&speakers).await?;
        let segments = self.list_segments(note_id).await?;
        let mut out = String::new();
        // Consecutive segments from the same speaker are one turn.
        let mut current: Option<(String, u32, String)> = None;
        for seg in segments.iter().filter(|s| s.is_final && !s.text.trim().is_empty()) {
            let name = seg.speaker_id.and_then(|id| names.get(&id).cloned()).unwrap_or_else(|| default_label(seg.channel).to_string());
            match &mut current {
                Some((who, _, text)) if *who == name => {
                    text.push(' ');
                    text.push_str(seg.text.trim());
                }
                _ => {
                    if let Some(turn) = current.take() {
                        push_turn(&mut out, turn);
                    }
                    current = Some((name, seg.start_ms, seg.text.trim().to_string()));
                }
            }
        }
        if let Some(turn) = current.take() {
            push_turn(&mut out, turn);
        }
        Ok(out)
    }

    pub async fn list_speakers(&self, note_id: Id) -> StoreResult<Vec<Speaker>> {
        let rows = sqlx::query(
            "SELECT id, note_id, label, person_id, color_index, is_self FROM speakers WHERE note_id = ? \
             ORDER BY is_self DESC, rowid",
        )
        .bind(note_id.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(speaker_from_row).collect()
    }

    pub async fn rename_speaker(&self, speaker_id: Id, label: &str) -> StoreResult<Speaker> {
        let label = label.trim();
        if label.is_empty() {
            return Err(StoreError::Invalid("A speaker needs a name. Type one, or press Escape to keep the old one.".into()));
        }
        let done = sqlx::query("UPDATE speakers SET label = ? WHERE id = ?").bind(label).bind(speaker_id.to_string()).execute(self.pool()).await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound("The speaker".into()));
        }
        self.get_speaker(speaker_id).await
    }

    /// Link a speaker to a Saved person or unlink it with `None`. This is the person confirming
    /// who someone is, so it teaches Margin their voice: the speaker's embedding becomes one of
    /// the person's samples. A sample this speaker gave someone else before is taken back out,
    /// so a correction ("not Sam, Kim") untrains the wrong person. Confirming the person Margin
    /// already picked adds the sample too.
    pub async fn link_speaker(&self, speaker_id: Id, person_id: Option<Id>) -> StoreResult<Speaker> {
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query("SELECT embedding FROM speakers WHERE id = ?")
            .bind(speaker_id.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| StoreError::NotFound("The speaker".into()))?;
        let embedding: Option<Vec<u8>> = row.try_get("embedding")?;
        if let Some(pid) = person_id {
            sqlx::query("SELECT 1 FROM people WHERE id = ?")
                .bind(pid.to_string())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| StoreError::NotFound("That person".into()))?;
        }
        // Take back what this speaker taught anyone before.
        let before = sqlx::query("SELECT DISTINCT person_id FROM voice_samples WHERE speaker_id = ?")
            .bind(speaker_id.to_string())
            .fetch_all(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM voice_samples WHERE speaker_id = ?").bind(speaker_id.to_string()).execute(&mut *tx).await?;
        let mut touched: Vec<Id> = before.iter().map(|r| r.try_get::<String, _>("person_id").map_err(StoreError::from).and_then(|p| parse_id(&p))).collect::<StoreResult<_>>()?;
        if let (Some(pid), Some(emb)) = (person_id, embedding) {
            insert_sample(&mut tx, pid, Some(speaker_id), &blob_to_vec(&emb)).await?;
            touched.push(pid);
        }
        sqlx::query("UPDATE speakers SET person_id = ? WHERE id = ?")
            .bind(person_id.map(|p| p.to_string()))
            .bind(speaker_id.to_string())
            .execute(&mut *tx)
            .await?;
        for pid in touched {
            refresh_voiceprint(&mut tx, pid).await?;
        }
        tx.commit().await?;
        self.get_speaker(speaker_id).await
    }

    /// Margin's own guess of who a speaker is (a voice it recognised): shown like a link, but it
    /// teaches nothing until the person confirms it with [`Db::link_speaker`].
    pub async fn assign_speaker(&self, speaker_id: Id, person_id: Id) -> StoreResult<()> {
        sqlx::query("UPDATE speakers SET person_id = ? WHERE id = ?")
            .bind(person_id.to_string())
            .bind(speaker_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// "You", created the first time it is needed.
    pub async fn ensure_self_person(&self) -> StoreResult<Person> {
        let found: Option<(String,)> = sqlx::query_as("SELECT id FROM people WHERE is_self = 1").fetch_optional(self.pool()).await?;
        if let Some((id,)) = found {
            return self.get_person(parse_id(&id)?).await;
        }
        let id = margin_domain::new_id();
        // The unique index keeps it to one even if two callers race here.
        sqlx::query("INSERT OR IGNORE INTO people (id, name, color_index, voiceprint, sample_count, created_at, is_self) VALUES (?, ?, 1, NULL, 0, ?, 1)")
            .bind(id.to_string())
            .bind(SELF_PERSON_NAME)
            .bind(crate::codec::ts(margin_domain::now()))
            .execute(self.pool())
            .await?;
        let (id,): (String,) = sqlx::query_as("SELECT id FROM people WHERE is_self = 1").fetch_one(self.pool()).await?;
        self.get_person(parse_id(&id)?).await
    }

    /// Link the note's "You" speakers (the app's user) to the "You" person, without teaching.
    pub async fn link_self_speakers(&self, note_id: Id) -> StoreResult<()> {
        let me = self.ensure_self_person().await?;
        sqlx::query("UPDATE speakers SET person_id = ? WHERE note_id = ? AND is_self = 1 AND person_id IS NULL")
            .bind(me.id.to_string())
            .bind(note_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Forget how a person sounds: their samples go, the person and their links stay.
    pub async fn forget_voice(&self, person_id: Id) -> StoreResult<Person> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM voice_samples WHERE person_id = ?").bind(person_id.to_string()).execute(&mut *tx).await?;
        refresh_voiceprint(&mut tx, person_id).await?;
        tx.commit().await?;
        self.get_person(person_id).await
    }

    /// The Saved person `embedding` sounds most like, at or above `threshold`, leaving out
    /// `exclude` (people already in this meeting). Returns the person and the similarity.
    pub async fn recognise_voice(&self, embedding: &[f32], threshold: f32, exclude: &HashSet<Id>) -> StoreResult<Option<(Id, f32)>> {
        let best = self
            .list_voiceprints()
            .await?
            .into_iter()
            .filter(|v| !exclude.contains(&v.person_id))
            .map(|v| (v.person_id, v.similarity(embedding)))
            .filter(|(_, sim)| *sim >= threshold)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        Ok(best)
    }

    pub async fn list_people(&self) -> StoreResult<Vec<Person>> {
        let rows = sqlx::query(person_select!(" GROUP BY p.id ORDER BY p.is_self DESC, p.name COLLATE NOCASE")).fetch_all(self.pool()).await?;
        rows.iter().map(person_from_row).collect()
    }

    pub async fn get_person(&self, person_id: Id) -> StoreResult<Person> {
        let row = sqlx::query(person_select!(" WHERE p.id = ? GROUP BY p.id"))
            .bind(person_id.to_string())
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("That person".into()))?;
        person_from_row(&row)
    }

    /// Delete a Saved person; their speakers keep their labels but lose the link.
    pub async fn delete_person(&self, person_id: Id) -> StoreResult<()> {
        if self.get_person(person_id).await?.is_self {
            return Err(StoreError::Invalid("You are always kept. To start your voice afresh, choose Forget voice.".into()));
        }
        let mut tx = self.pool().begin().await?;
        // Explicit, so it holds even when foreign keys are off.
        sqlx::query("UPDATE speakers SET person_id = NULL WHERE person_id = ?").bind(person_id.to_string()).execute(&mut *tx).await?;
        let done = sqlx::query("DELETE FROM people WHERE id = ?").bind(person_id.to_string()).execute(&mut *tx).await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound("That person".into()));
        }
        tx.commit().await?;
        Ok(())
    }

    // ── Segments ─────────────────────────────────────────────────────────

    /// All segments of a note (partial and final), ordered by start time.
    pub async fn list_segments(&self, note_id: Id) -> StoreResult<Vec<Segment>> {
        let rows = sqlx::query(
            "SELECT id, note_id, speaker_id, channel, start_ms, end_ms, text, is_final FROM segments \
             WHERE note_id = ? ORDER BY start_ms, end_ms, rowid",
        )
        .bind(note_id.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(segment_from_row).collect()
    }

    /// Insert or replace a segment by id (partials are refined in place).
    pub async fn upsert_segment(&self, segment: &Segment) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO segments (id, note_id, speaker_id, channel, start_ms, end_ms, text, is_final) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET speaker_id = excluded.speaker_id, channel = excluded.channel, \
             start_ms = excluded.start_ms, end_ms = excluded.end_ms, text = excluded.text, is_final = excluded.is_final",
        )
        .bind(segment.id.to_string())
        .bind(segment.note_id.to_string())
        .bind(segment.speaker_id.map(|s| s.to_string()))
        .bind(channel_str(segment.channel))
        .bind(segment.start_ms as i64)
        .bind(segment.end_ms as i64)
        .bind(&segment.text)
        .bind(segment.is_final)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_segment(&self, segment_id: Id) -> StoreResult<()> {
        sqlx::query("DELETE FROM segments WHERE id = ?").bind(segment_id.to_string()).execute(self.pool()).await?;
        Ok(())
    }

    /// Swap one channel's segments from `from_ms` on for a fresh transcription, in one
    /// transaction (the after-meeting pass). Earlier recordings in the same note keep theirs.
    pub async fn replace_channel_segments(&self, note_id: Id, channel: AudioChannel, from_ms: u32, segments: &[Segment]) -> StoreResult<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query("DELETE FROM segments WHERE note_id = ? AND channel = ? AND start_ms >= ?")
            .bind(note_id.to_string())
            .bind(channel_str(channel))
            .bind(from_ms as i64)
            .execute(&mut *tx)
            .await?;
        for segment in segments {
            sqlx::query(
                "INSERT INTO segments (id, note_id, speaker_id, channel, start_ms, end_ms, text, is_final) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(segment.id.to_string())
            .bind(segment.note_id.to_string())
            .bind(segment.speaker_id.map(|s| s.to_string()))
            .bind(channel_str(segment.channel))
            .bind(segment.start_ms as i64)
            .bind(segment.end_ms as i64)
            .bind(&segment.text)
            .bind(segment.is_final)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Remove partial segments left behind (for example after a crash). Returns how many.
    pub async fn delete_partial_segments(&self, note_id: Id) -> StoreResult<u32> {
        let done = sqlx::query("DELETE FROM segments WHERE note_id = ? AND is_final = 0").bind(note_id.to_string()).execute(self.pool()).await?;
        Ok(done.rows_affected() as u32)
    }

    // ── Speakers ─────────────────────────────────────────────────────────

    pub async fn get_speaker(&self, speaker_id: Id) -> StoreResult<Speaker> {
        let row = sqlx::query("SELECT id, note_id, label, person_id, color_index, is_self FROM speakers WHERE id = ?")
            .bind(speaker_id.to_string())
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("The speaker".into()))?;
        speaker_from_row(&row)
    }

    pub async fn create_speaker(&self, note_id: Id, new: NewSpeaker) -> StoreResult<Speaker> {
        let id = margin_domain::new_id();
        sqlx::query("INSERT INTO speakers (id, note_id, label, color_index, is_self, embedding) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(id.to_string())
            .bind(note_id.to_string())
            .bind(&new.label)
            .bind(clamp_color(new.color_index) as i64)
            .bind(new.is_self)
            .bind(new.embedding.as_deref().map(|e| vec_to_blob(&normalized(e))))
            .execute(self.pool())
            .await?;
        self.get_speaker(id).await
    }

    /// The "You" (mic) and "Others" (system) speakers of a note, created on first use.
    pub async fn ensure_channel_speakers(&self, note_id: Id) -> StoreResult<(Speaker, Speaker)> {
        let speakers = self.list_speakers(note_id).await?;
        if !speakers.iter().any(|s| s.is_self) {
            self.create_speaker(note_id, NewSpeaker { label: SELF_LABEL.into(), color_index: 1, is_self: true, embedding: None }).await?;
        }
        // The mic speaker is the "You" person (a link, not a sample: nothing is learnt from it).
        self.link_self_speakers(note_id).await?;
        let you = self.list_speakers(note_id).await?.into_iter().find(|s| s.is_self).ok_or_else(|| StoreError::NotFound("The You speaker".into()))?;
        let others = match speakers.iter().find(|s| !s.is_self && s.label == OTHERS_LABEL) {
            Some(s) => s.clone(),
            None => self.create_speaker(note_id, NewSpeaker { label: OTHERS_LABEL.into(), color_index: 2, is_self: false, embedding: None }).await?,
        };
        Ok((you, others))
    }

    /// A speaker's centroid embedding, if diarization produced one.
    pub async fn speaker_embedding(&self, speaker_id: Id) -> StoreResult<Option<Vec<f32>>> {
        let row = sqlx::query("SELECT embedding FROM speakers WHERE id = ?")
            .bind(speaker_id.to_string())
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("The speaker".into()))?;
        let blob: Option<Vec<u8>> = row.try_get("embedding")?;
        Ok(blob.as_deref().map(blob_to_vec))
    }

    pub async fn set_speaker_embedding(&self, speaker_id: Id, embedding: Option<&[f32]>) -> StoreResult<()> {
        sqlx::query("UPDATE speakers SET embedding = ? WHERE id = ?")
            .bind(embedding.map(|e| vec_to_blob(&normalized(e))))
            .bind(speaker_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Remove the note's speakers that no segment uses and no person is linked to (an "Others"
    /// left empty once the service labelled everyone on the call).
    pub async fn drop_unused_speakers(&self, note_id: Id) -> StoreResult<()> {
        sqlx::query(
            "DELETE FROM speakers WHERE note_id = ? AND person_id IS NULL \
             AND NOT EXISTS (SELECT 1 FROM segments g WHERE g.speaker_id = speakers.id)",
        )
        .bind(note_id.to_string())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Replace the provisional speaker of one channel with the speakers diarization found.
    ///
    /// `assignments` maps segment ids to an index into `speakers`. Segments of that channel that
    /// are not listed keep `fallback` (the index of the speaker to give them), or their current
    /// speaker when `fallback` is `None`. Speakers of the channel left with no segments and no
    /// person link are removed. Returns the stored speakers in the order given.
    pub async fn apply_diarization(
        &self,
        note_id: Id,
        channel: AudioChannel,
        speakers: Vec<NewSpeaker>,
        assignments: &[(Id, usize)],
        fallback: Option<usize>,
    ) -> StoreResult<Vec<Speaker>> {
        let mut tx = self.pool().begin().await?;
        let mut ids = Vec::with_capacity(speakers.len());
        for new in &speakers {
            let id = margin_domain::new_id();
            sqlx::query("INSERT INTO speakers (id, note_id, label, color_index, is_self, embedding) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(id.to_string())
                .bind(note_id.to_string())
                .bind(&new.label)
                .bind(clamp_color(new.color_index) as i64)
                .bind(new.is_self)
                .bind(new.embedding.as_deref().map(|e| vec_to_blob(&normalized(e))))
                .execute(&mut *tx)
                .await?;
            ids.push(id);
        }
        if let Some(f) = fallback.and_then(|f| ids.get(f)) {
            sqlx::query("UPDATE segments SET speaker_id = ? WHERE note_id = ? AND channel = ?")
                .bind(f.to_string())
                .bind(note_id.to_string())
                .bind(channel_str(channel))
                .execute(&mut *tx)
                .await?;
        }
        for (segment_id, idx) in assignments {
            let Some(speaker) = ids.get(*idx) else {
                continue;
            };
            sqlx::query("UPDATE segments SET speaker_id = ? WHERE id = ? AND note_id = ?")
                .bind(speaker.to_string())
                .bind(segment_id.to_string())
                .bind(note_id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        // Drop the channel's old, now unused speakers (never ones a person is linked to).
        let new_ids: HashSet<String> = ids.iter().map(|i| i.to_string()).collect();
        let rows = sqlx::query(
            "SELECT s.id FROM speakers s WHERE s.note_id = ? AND s.person_id IS NULL \
             AND NOT EXISTS (SELECT 1 FROM segments g WHERE g.speaker_id = s.id)",
        )
        .bind(note_id.to_string())
        .fetch_all(&mut *tx)
        .await?;
        for row in rows {
            let id: String = row.try_get("id")?;
            if !new_ids.contains(&id) {
                sqlx::query("DELETE FROM speakers WHERE id = ?").bind(id).execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Ok(s) = self.get_speaker(id).await {
                out.push(s);
            }
        }
        Ok(out)
    }

    // ── People ───────────────────────────────────────────────────────────

    /// Create a Saved person, optionally seeded with one voice sample.
    pub async fn create_person(&self, name: &str, voiceprint: Option<&[f32]>) -> StoreResult<Person> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Invalid("A person needs a name. Type one and try again.".into()));
        }
        let id = margin_domain::new_id();
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM people").fetch_one(self.pool()).await?;
        let color = (count.rem_euclid(COLOR_COUNT as i64) as u8) + 1;
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO people (id, name, color_index, voiceprint, sample_count, created_at) VALUES (?, ?, ?, NULL, 0, ?)")
            .bind(id.to_string())
            .bind(name)
            .bind(color as i64)
            .bind(crate::codec::ts(margin_domain::now()))
            .execute(&mut *tx)
            .await?;
        if let Some(v) = voiceprint {
            insert_sample(&mut tx, id, None, v).await?;
            refresh_voiceprint(&mut tx, id).await?;
        }
        tx.commit().await?;
        self.get_person(id).await
    }

    /// "Save as person": create a person named `name` from this speaker's voice and link them.
    pub async fn save_speaker_as_person(&self, speaker_id: Id, name: &str) -> StoreResult<Person> {
        let speaker = self.get_speaker(speaker_id).await?;
        let person = self.create_person(name, None).await?;
        // Linking folds the embedding in (sample_count 0 → 1).
        self.link_speaker(speaker.id, Some(person.id)).await?;
        self.get_person(person.id).await
    }

    pub async fn rename_person(&self, person_id: Id, name: &str) -> StoreResult<Person> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Invalid("A person needs a name. Type one and try again.".into()));
        }
        let done = sqlx::query("UPDATE people SET name = ? WHERE id = ?").bind(name).bind(person_id.to_string()).execute(self.pool()).await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound("That person".into()));
        }
        self.get_person(person_id).await
    }

    pub async fn set_person_color(&self, person_id: Id, color_index: u8) -> StoreResult<Person> {
        if !(1..=COLOR_COUNT).contains(&color_index) {
            return Err(StoreError::Invalid(format!("Pick a colour from 1 to {COLOR_COUNT}.")));
        }
        let done = sqlx::query("UPDATE people SET color_index = ? WHERE id = ?")
            .bind(color_index as i64)
            .bind(person_id.to_string())
            .execute(self.pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound("That person".into()));
        }
        self.get_person(person_id).await
    }

    /// Fold `source` into `target`: voiceprints averaged by sample count, speakers relinked,
    /// `source` deleted.
    pub async fn merge_people(&self, source_id: Id, target_id: Id) -> StoreResult<Person> {
        if source_id == target_id {
            return Err(StoreError::Invalid("Pick two different people to merge.".into()));
        }
        if self.get_person(source_id).await?.is_self {
            return Err(StoreError::Invalid("You are always kept. Merge the other person into You instead.".into()));
        }
        self.get_person(target_id).await?;
        let mut tx = self.pool().begin().await?;
        sqlx::query("UPDATE voice_samples SET person_id = ? WHERE person_id = ?")
            .bind(target_id.to_string())
            .bind(source_id.to_string())
            .execute(&mut *tx)
            .await?;
        refresh_voiceprint(&mut tx, target_id).await?;
        sqlx::query("UPDATE speakers SET person_id = ? WHERE person_id = ?")
            .bind(target_id.to_string())
            .bind(source_id.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM people WHERE id = ?").bind(source_id.to_string()).execute(&mut *tx).await?;
        tx.commit().await?;
        self.get_person(target_id).await
    }

    /// Every Saved person Margin knows the voice of, with their samples.
    pub async fn list_voiceprints(&self) -> StoreResult<Vec<Voiceprint>> {
        let rows = sqlx::query("SELECT id, name, voiceprint FROM people WHERE voiceprint IS NOT NULL").fetch_all(self.pool()).await?;
        let mut samples: HashMap<String, Vec<Vec<f32>>> = HashMap::new();
        for r in sqlx::query("SELECT person_id, embedding FROM voice_samples").fetch_all(self.pool()).await? {
            let person: String = r.try_get("person_id")?;
            let blob: Vec<u8> = r.try_get("embedding")?;
            samples.entry(person).or_default().push(blob_to_vec(&blob));
        }
        rows.iter()
            .map(|r| {
                let id: String = r.try_get("id")?;
                let blob: Vec<u8> = r.try_get("voiceprint")?;
                Ok(Voiceprint { person_id: parse_id(&id)?, name: r.try_get("name")?, vector: blob_to_vec(&blob), samples: samples.remove(&id).unwrap_or_default() })
            })
            .collect()
    }

    /// "Is Person 2 Sam?": for each unlinked, non-self speaker of the note that has an embedding,
    /// the best-matching Saved person with cosine similarity at or above `threshold`. One person is
    /// suggested for at most one speaker, and people already linked in this note are skipped.
    pub async fn speaker_suggestions(&self, note_id: Id, threshold: f32) -> StoreResult<Vec<SpeakerSuggestion>> {
        let rows = sqlx::query("SELECT id, person_id, is_self, embedding FROM speakers WHERE note_id = ?")
            .bind(note_id.to_string())
            .fetch_all(self.pool())
            .await?;
        let mut linked: HashSet<Id> = HashSet::new();
        let mut candidates: Vec<(Id, Vec<f32>)> = Vec::new();
        for r in &rows {
            let person: Option<String> = r.try_get("person_id")?;
            if let Some(p) = person {
                linked.insert(parse_id(&p)?);
                continue;
            }
            let is_self: bool = r.try_get("is_self")?;
            let emb: Option<Vec<u8>> = r.try_get("embedding")?;
            if let (false, Some(emb)) = (is_self, emb) {
                let id: String = r.try_get("id")?;
                candidates.push((parse_id(&id)?, blob_to_vec(&emb)));
            }
        }
        let prints: Vec<Voiceprint> = self.list_voiceprints().await?.into_iter().filter(|v| !linked.contains(&v.person_id)).collect();
        // Every pair above the threshold, best first, then assign greedily one-to-one.
        let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
        for (si, (_, emb)) in candidates.iter().enumerate() {
            for (pi, vp) in prints.iter().enumerate() {
                let sim = vp.similarity(emb);
                if sim >= threshold {
                    pairs.push((sim, si, pi));
                }
            }
        }
        pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (mut used_s, mut used_p) = (HashSet::new(), HashSet::new());
        let mut out = Vec::new();
        for (sim, si, pi) in pairs {
            if used_s.contains(&si) || used_p.contains(&pi) {
                continue;
            }
            used_s.insert(si);
            used_p.insert(pi);
            out.push(SpeakerSuggestion {
                speaker_id: candidates[si].0,
                person_id: prints[pi].person_id,
                person_name: prints[pi].name.clone(),
                similarity: sim.clamp(0.0, 1.0),
            });
        }
        Ok(out)
    }

    /// Display names for speakers: the linked person's name wins over the label.
    pub(crate) async fn speaker_display_names(&self, speakers: &[Speaker]) -> StoreResult<HashMap<Id, String>> {
        let mut out = HashMap::new();
        for s in speakers {
            let name = match s.person_id {
                Some(pid) => match self.get_person(pid).await {
                    Ok(p) => p.name,
                    Err(StoreError::NotFound(_)) => s.label.clone(),
                    Err(e) => return Err(e),
                },
                None => s.label.clone(),
            };
            out.insert(s.id, name);
        }
        Ok(out)
    }
}

// ── Row mapping and helpers ──────────────────────────────────────────────

fn push_turn(out: &mut String, (who, start_ms, text): (String, u32, String)) {
    let secs = start_ms / 1000;
    let stamp =
        if secs >= 3600 { format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60) } else { format!("{:02}:{:02}", secs / 60, secs % 60) };
    out.push_str(&format!("[{stamp}] {who}: {text}\n"));
}

fn default_label(channel: AudioChannel) -> &'static str {
    match channel {
        AudioChannel::Mic => SELF_LABEL,
        AudioChannel::System => OTHERS_LABEL,
    }
}

fn channel_str(channel: AudioChannel) -> &'static str {
    match channel {
        AudioChannel::Mic => "mic",
        AudioChannel::System => "system",
    }
}

fn clamp_color(c: u8) -> u8 {
    c.clamp(1, COLOR_COUNT)
}

fn parse_id(s: &str) -> StoreResult<Id> {
    Id::parse_str(s).map_err(|e| StoreError::Invalid(format!("Stored id {s:?} is not valid: {e}")))
}

fn parse_ts(s: &str) -> StoreResult<Timestamp> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&chrono::Utc))
        .map_err(|e| StoreError::Invalid(format!("Stored time {s:?} is not valid: {e}")))
}

fn speaker_from_row(r: &SqliteRow) -> StoreResult<Speaker> {
    let id: String = r.try_get("id")?;
    let note_id: String = r.try_get("note_id")?;
    let person: Option<String> = r.try_get("person_id")?;
    let color: i64 = r.try_get("color_index")?;
    Ok(Speaker {
        id: parse_id(&id)?,
        note_id: parse_id(&note_id)?,
        label: r.try_get("label")?,
        person_id: person.as_deref().map(parse_id).transpose()?,
        color_index: clamp_color(color.clamp(1, COLOR_COUNT as i64) as u8),
        is_self: r.try_get("is_self")?,
    })
}

fn segment_from_row(r: &SqliteRow) -> StoreResult<Segment> {
    let id: String = r.try_get("id")?;
    let note_id: String = r.try_get("note_id")?;
    let speaker: Option<String> = r.try_get("speaker_id")?;
    let channel: String = r.try_get("channel")?;
    let start: i64 = r.try_get("start_ms")?;
    let end: i64 = r.try_get("end_ms")?;
    Ok(Segment {
        id: parse_id(&id)?,
        note_id: parse_id(&note_id)?,
        speaker_id: speaker.as_deref().map(parse_id).transpose()?,
        channel: if channel == "mic" { AudioChannel::Mic } else { AudioChannel::System },
        start_ms: start.clamp(0, u32::MAX as i64) as u32,
        end_ms: end.clamp(0, u32::MAX as i64) as u32,
        text: r.try_get("text")?,
        is_final: r.try_get("is_final")?,
    })
}

fn person_from_row(r: &SqliteRow) -> StoreResult<Person> {
    let id: String = r.try_get("id")?;
    let color: i64 = r.try_get("color_index")?;
    let samples: i64 = r.try_get("sample_count")?;
    let meetings: i64 = r.try_get("meeting_count")?;
    let created: String = r.try_get("created_at")?;
    Ok(Person {
        id: parse_id(&id)?,
        name: r.try_get("name")?,
        color_index: color.clamp(1, COLOR_COUNT as i64) as u8,
        sample_count: samples.clamp(0, u32::MAX as i64) as u32,
        meeting_count: meetings.clamp(0, u32::MAX as i64) as u32,
        created_at: parse_ts(&created)?,
        is_self: r.try_get("is_self")?,
    })
}

/// Little-endian `f32` bytes.
pub fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// Inverse of [`vec_to_blob`]; trailing bytes that do not make a whole `f32` are ignored.
pub fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// L2-normalised copy (a zero vector stays zero).
pub fn normalized(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= f32::EPSILON { v.to_vec() } else { v.iter().map(|x| x / norm).collect() }
}

/// Cosine similarity; 0 when the lengths differ or either vector is zero.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= f32::EPSILON || nb <= f32::EPSILON { 0.0 } else { dot / (na.sqrt() * nb.sqrt()) }
}

/// Add one voice sample (normalised) for a person.
async fn insert_sample(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, person_id: Id, speaker_id: Option<Id>, embedding: &[f32]) -> StoreResult<()> {
    sqlx::query("INSERT INTO voice_samples (id, person_id, speaker_id, embedding, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(margin_domain::new_id().to_string())
        .bind(person_id.to_string())
        .bind(speaker_id.map(|s| s.to_string()))
        .bind(vec_to_blob(&normalized(embedding)))
        .bind(crate::codec::ts(margin_domain::now()))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Recompute a person's voiceprint (the normalised mean of their samples) and sample count.
async fn refresh_voiceprint(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, person_id: Id) -> StoreResult<()> {
    let rows = sqlx::query("SELECT embedding FROM voice_samples WHERE person_id = ?").bind(person_id.to_string()).fetch_all(&mut **tx).await?;
    let samples: Vec<Vec<f32>> = rows.iter().map(|r| r.try_get::<Vec<u8>, _>("embedding").map(|b| blob_to_vec(&b))).collect::<Result<_, _>>()?;
    let dim = samples.first().map(Vec::len).unwrap_or(0);
    let mean: Option<Vec<f32>> = (dim > 0).then(|| {
        let mut sum = vec![0.0f32; dim];
        for v in samples.iter().filter(|v| v.len() == dim) {
            for (a, b) in sum.iter_mut().zip(v) {
                *a += b;
            }
        }
        normalized(&sum)
    });
    sqlx::query("UPDATE people SET voiceprint = ?, sample_count = ? WHERE id = ?")
        .bind(mean.map(|m| vec_to_blob(&m)))
        .bind(samples.len() as i64)
        .bind(person_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use margin_domain::new_id;

    async fn note(db: &Db) -> Id {
        let id = new_id();
        let now = margin_domain::now().to_rfc3339();
        sqlx::query("INSERT INTO notes (id, title, kind, created_at, updated_at) VALUES (?, 'Test', 'meeting', ?, ?)")
            .bind(id.to_string())
            .bind(&now)
            .bind(&now)
            .execute(db.pool())
            .await
            .unwrap();
        id
    }

    fn seg(note_id: Id, speaker: Option<Id>, channel: AudioChannel, start: u32, text: &str) -> Segment {
        Segment { id: new_id(), note_id, speaker_id: speaker, channel, start_ms: start, end_ms: start + 1500, text: text.into(), is_final: true }
    }

    fn voice(label: &str, embedding: &[f32]) -> NewSpeaker {
        NewSpeaker { label: label.into(), color_index: 3, is_self: false, embedding: Some(embedding.to_vec()) }
    }

    /// Confirming teaches, correcting untrains: each confirmed speaker is one sample.
    #[tokio::test]
    async fn corrections_move_voice_samples() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let sam = db.create_person("Sam", None).await.unwrap();
        let kim = db.create_person("Kim", None).await.unwrap();
        let s = db.create_speaker(n, voice("Person 1", &[1.0, 0.0])).await.unwrap();

        db.link_speaker(s.id, Some(sam.id)).await.unwrap();
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 1);
        // Not Sam after all: the sample moves to Kim.
        db.link_speaker(s.id, Some(kim.id)).await.unwrap();
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 0);
        assert_eq!(db.get_person(kim.id).await.unwrap().sample_count, 1);
        assert!(db.list_voiceprints().await.unwrap().iter().all(|v| v.person_id == kim.id));
        // Confirming again does not count twice; unlinking takes it back.
        db.link_speaker(s.id, Some(kim.id)).await.unwrap();
        assert_eq!(db.get_person(kim.id).await.unwrap().sample_count, 1);
        db.link_speaker(s.id, None).await.unwrap();
        assert_eq!(db.get_person(kim.id).await.unwrap().sample_count, 0);
    }

    /// Margin's own guesses link without teaching; recognition picks the closest sample.
    #[tokio::test]
    async fn recognition_uses_every_sample_and_guesses_teach_nothing() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let sam = db.create_person("Sam", None).await.unwrap();
        // Sam on a call and in the room: two different-sounding samples.
        for e in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            let s = db.create_speaker(n, voice("Person", &e)).await.unwrap();
            db.link_speaker(s.id, Some(sam.id)).await.unwrap();
        }
        let none = HashSet::new();
        // Close to the second sample only, far from the mean.
        let hit = db.recognise_voice(&[0.05, 1.0, 0.0], 0.9, &none).await.unwrap();
        assert_eq!(hit.map(|(p, _)| p), Some(sam.id));
        assert!(db.recognise_voice(&[0.0, 0.0, 1.0], 0.5, &none).await.unwrap().is_none());
        assert!(db.recognise_voice(&[0.05, 1.0, 0.0], 0.9, &HashSet::from([sam.id])).await.unwrap().is_none(), "already in the meeting");

        let guess = db.create_speaker(n, voice("Person 3", &[0.0, 1.0, 0.0])).await.unwrap();
        db.assign_speaker(guess.id, sam.id).await.unwrap();
        assert_eq!(db.get_speaker(guess.id).await.unwrap().person_id, Some(sam.id));
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 2, "a guess is not a sample");
    }

    /// "You" is a Saved person that is always there, and the mic speaker is linked to it.
    #[tokio::test]
    async fn you_are_always_a_saved_person() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let (you_speaker, _) = db.ensure_channel_speakers(n).await.unwrap();
        let me = db.ensure_self_person().await.unwrap();
        assert!(me.is_self);
        assert_eq!(me.name, SELF_PERSON_NAME);
        assert_eq!(db.ensure_self_person().await.unwrap().id, me.id, "only one");
        assert_eq!(you_speaker.person_id, Some(me.id));
        assert_eq!(db.get_person(me.id).await.unwrap().sample_count, 0, "linking the mic teaches nothing");

        assert!(matches!(db.delete_person(me.id).await, Err(StoreError::Invalid(_))));
        let other = db.create_person("Sam", Some(&[1.0, 0.0])).await.unwrap();
        assert!(matches!(db.merge_people(me.id, other.id).await, Err(StoreError::Invalid(_))));
        // Merging someone into You keeps You, with their samples.
        let me = db.merge_people(other.id, me.id).await.unwrap();
        assert_eq!(me.sample_count, 1);
        let me = db.forget_voice(me.id).await.unwrap();
        assert_eq!(me.sample_count, 0);
        assert!(db.list_voiceprints().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn unused_speakers_are_dropped_unless_linked() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let (you, others) = db.ensure_channel_speakers(n).await.unwrap();
        let person1 = db.create_speaker(n, NewSpeaker { label: "Person 1".into(), color_index: 3, is_self: false, embedding: None }).await.unwrap();
        let linked = db.create_speaker(n, NewSpeaker { label: "Person 2".into(), color_index: 4, is_self: false, embedding: None }).await.unwrap();
        let karen = db.create_person("Karen", None).await.unwrap();
        db.link_speaker(linked.id, Some(karen.id)).await.unwrap();
        db.upsert_segment(&seg(n, Some(you.id), AudioChannel::Mic, 0, "Hello.")).await.unwrap();
        db.upsert_segment(&seg(n, Some(person1.id), AudioChannel::System, 2000, "Hi.")).await.unwrap();

        db.drop_unused_speakers(n).await.unwrap();
        let left: Vec<Id> = db.list_speakers(n).await.unwrap().into_iter().map(|s| s.id).collect();
        assert!(left.contains(&you.id) && left.contains(&person1.id) && left.contains(&linked.id));
        assert!(!left.contains(&others.id), "Others had nothing said");
    }

    #[tokio::test]
    async fn segments_upsert_and_transcript_text() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let (you, others) = db.ensure_channel_speakers(n).await.unwrap();
        // Idempotent.
        let (you2, others2) = db.ensure_channel_speakers(n).await.unwrap();
        assert_eq!((you.id, others.id), (you2.id, others2.id));

        let mut a = seg(n, Some(you.id), AudioChannel::Mic, 0, "Hello");
        a.is_final = false;
        db.upsert_segment(&a).await.unwrap();
        a.text = "Hello there.".into();
        a.is_final = true;
        db.upsert_segment(&a).await.unwrap();
        db.upsert_segment(&seg(n, Some(you.id), AudioChannel::Mic, 2000, "How are you?")).await.unwrap();
        db.upsert_segment(&seg(n, Some(others.id), AudioChannel::System, 65_000, "Fine, thanks.")).await.unwrap();
        let mut partial = seg(n, None, AudioChannel::System, 70_000, "and you");
        partial.is_final = false;
        db.upsert_segment(&partial).await.unwrap();

        let t = db.get_transcript(n).await.unwrap();
        assert_eq!(t.segments.len(), 4);
        assert_eq!(t.speakers.len(), 2);
        assert!(t.speakers[0].is_self);
        assert!(!t.has_audio);
        assert_eq!(t.duration_ms, Some(71_500));

        let text = db.transcript_text(n).await.unwrap();
        assert_eq!(text, "[00:00] You: Hello there. How are you?\n[01:05] Others: Fine, thanks.\n");

        assert_eq!(db.delete_partial_segments(n).await.unwrap(), 1);
        assert_eq!(db.list_segments(n).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn empty_transcript() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        assert_eq!(db.transcript_text(n).await.unwrap(), "");
        let t = db.get_transcript(n).await.unwrap();
        assert!(t.segments.is_empty() && t.duration_ms.is_none());
        assert!(matches!(db.get_transcript(new_id()).await, Err(StoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn rename_and_link_speaker_folds_voiceprint() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let s = db
            .create_speaker(n, NewSpeaker { label: "Person 1".into(), color_index: 3, is_self: false, embedding: Some(vec![3.0, 4.0]) })
            .await
            .unwrap();
        assert_eq!(db.speaker_embedding(s.id).await.unwrap().unwrap(), vec![0.6, 0.8]);
        let s = db.rename_speaker(s.id, "  Sam ").await.unwrap();
        assert_eq!(s.label, "Sam");
        assert!(db.rename_speaker(s.id, " ").await.is_err());

        let p = db.save_speaker_as_person(s.id, "Sam").await.unwrap();
        assert_eq!(p.sample_count, 1);
        assert_eq!(p.meeting_count, 1);
        assert_eq!(db.get_speaker(s.id).await.unwrap().person_id, Some(p.id));

        // A second meeting's speaker with a different embedding: voiceprint is the normalised mean.
        let n2 = note(&db).await;
        let s2 = db
            .create_speaker(n2, NewSpeaker { label: "Person 1".into(), color_index: 3, is_self: false, embedding: Some(vec![1.0, 0.0]) })
            .await
            .unwrap();
        db.link_speaker(s2.id, Some(p.id)).await.unwrap();
        let p = db.get_person(p.id).await.unwrap();
        assert_eq!((p.sample_count, p.meeting_count), (2, 2));
        let vp = &db.list_voiceprints().await.unwrap()[0].vector;
        let expect = normalized(&[1.6, 0.8]);
        assert!((vp[0] - expect[0]).abs() < 1e-5 && (vp[1] - expect[1]).abs() < 1e-5);
        let norm: f32 = vp.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);

        // Re-linking the same person is a no-op; unlink clears the link only.
        db.link_speaker(s2.id, Some(p.id)).await.unwrap();
        assert_eq!(db.get_person(p.id).await.unwrap().sample_count, 2);
        let s2 = db.link_speaker(s2.id, None).await.unwrap();
        assert_eq!(s2.person_id, None);
        assert_eq!(db.get_person(p.id).await.unwrap().meeting_count, 1);

        // The linked person's name wins in the plain-text transcript.
        db.upsert_segment(&seg(n, Some(s.id), AudioChannel::System, 0, "Hi")).await.unwrap();
        db.rename_person(p.id, "Samantha").await.unwrap();
        assert_eq!(db.transcript_text(n).await.unwrap(), "[00:00] Samantha: Hi\n");
    }

    #[tokio::test]
    async fn people_crud_merge_and_delete() {
        let db = Db::open_in_memory().await.unwrap();
        let a = db.create_person("Ana", Some(&[1.0, 0.0, 0.0])).await.unwrap();
        let b = db.create_person("Ben", Some(&[0.0, 1.0, 0.0])).await.unwrap();
        assert_ne!(a.color_index, b.color_index);
        assert!(db.create_person(" ", None).await.is_err());
        assert_eq!(db.list_people().await.unwrap().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Ana", "Ben"]);
        assert_eq!(db.set_person_color(a.id, 5).await.unwrap().color_index, 5);
        assert!(db.set_person_color(a.id, 9).await.is_err());

        let n = note(&db).await;
        let s = db.create_speaker(n, NewSpeaker { label: "Person 1".into(), color_index: 3, is_self: false, embedding: None }).await.unwrap();
        db.link_speaker(s.id, Some(b.id)).await.unwrap();

        let merged = db.merge_people(b.id, a.id).await.unwrap();
        assert_eq!(merged.sample_count, 2);
        assert_eq!(merged.meeting_count, 1);
        assert_eq!(db.get_speaker(s.id).await.unwrap().person_id, Some(a.id));
        assert!(matches!(db.get_person(b.id).await, Err(StoreError::NotFound(_))));
        let vp = &db.list_voiceprints().await.unwrap()[0].vector;
        assert!((vp[0] - vp[1]).abs() < 1e-5);
        assert!(db.merge_people(a.id, a.id).await.is_err());

        db.delete_person(a.id).await.unwrap();
        assert!(db.list_people().await.unwrap().is_empty());
        let s = db.get_speaker(s.id).await.unwrap();
        assert_eq!((s.person_id, s.label.as_str()), (None, "Person 1"));
        assert!(matches!(db.delete_person(a.id).await, Err(StoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn diarization_and_suggestions() {
        let db = Db::open_in_memory().await.unwrap();
        let sam = db.create_person("Sam", Some(&[1.0, 0.0, 0.0])).await.unwrap();
        let _kim = db.create_person("Kim", Some(&[0.0, 0.0, 1.0])).await.unwrap();
        let n = note(&db).await;
        let (you, others) = db.ensure_channel_speakers(n).await.unwrap();
        let s1 = seg(n, Some(others.id), AudioChannel::System, 0, "One");
        let s2 = seg(n, Some(others.id), AudioChannel::System, 2000, "Two");
        let m1 = seg(n, Some(you.id), AudioChannel::Mic, 1000, "Mine");
        for s in [&s1, &s2, &m1] {
            db.upsert_segment(s).await.unwrap();
        }
        let speakers = db
            .apply_diarization(
                n,
                AudioChannel::System,
                vec![
                    NewSpeaker { label: "Person 1".into(), color_index: 3, is_self: false, embedding: Some(vec![0.9, 0.1, 0.0]) },
                    NewSpeaker { label: "Person 2".into(), color_index: 4, is_self: false, embedding: Some(vec![0.0, 1.0, 0.0]) },
                ],
                &[(s1.id, 0), (s2.id, 1)],
                Some(0),
            )
            .await
            .unwrap();
        let t = db.get_transcript(n).await.unwrap();
        // "Others" is gone, "You" stays.
        assert_eq!(t.speakers.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(), ["You", "Person 1", "Person 2"]);
        let by_id: HashMap<Id, Option<Id>> = t.segments.iter().map(|s| (s.id, s.speaker_id)).collect();
        assert_eq!(by_id[&s1.id], Some(speakers[0].id));
        assert_eq!(by_id[&s2.id], Some(speakers[1].id));
        assert_eq!(by_id[&m1.id], Some(you.id));

        let sugg = db.speaker_suggestions(n, 0.7).await.unwrap();
        assert_eq!(sugg.len(), 1);
        assert_eq!((sugg[0].speaker_id, sugg[0].person_id, sugg[0].person_name.as_str()), (speakers[0].id, sam.id, "Sam"));
        assert!(sugg[0].similarity > 0.9);

        // Once linked, no more suggestions for that person or speaker.
        db.link_speaker(speakers[0].id, Some(sam.id)).await.unwrap();
        assert!(db.speaker_suggestions(n, 0.7).await.unwrap().is_empty());
    }

    #[test]
    fn blob_roundtrip() {
        let v = vec![0.25f32, -1.5, 3.0e-7];
        assert_eq!(blob_to_vec(&vec_to_blob(&v)), v);
        assert_eq!(vec_to_blob(&[1.0]), 1.0f32.to_le_bytes().to_vec());
    }
}
