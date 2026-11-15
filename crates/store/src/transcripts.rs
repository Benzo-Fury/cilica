//! segments, speakers, people. Owner: Audio agent.
//!
//! The first block of methods is the cross-area contract (the agent's tools call them); their
//! signatures do not change. The rest serve the recording pipeline and the recording commands.
//!
//! Storage conventions: ids are UUID strings, timestamps RFC 3339 strings, embeddings and
//! voiceprints little-endian `f32` blobs. A person's voiceprint is the L2-normalised average of
//! every speaker embedding folded into it; `sample_count` is how many were folded.

use std::collections::{HashMap, HashSet};

use cilica_domain::{AudioChannel, Id, NamedBy, Person, PersonMeeting, PersonVoice, Quote, Segment, Speaker, SpeakerDetection, SpeakerSuggestion, Timestamp, Transcript, VoiceSampleShape, VoiceShape};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::{Db, StoreError, StoreResult};

/// `SELECT` for [`Person`] rows with their meeting count, followed by `$tail`.
macro_rules! person_select {
    ($tail:literal) => {
        concat!(
            "SELECT p.id, p.name, p.color_index, p.sample_count, p.created_at, p.is_self, ",
            "(SELECT count(DISTINCT s.note_id) FROM speakers s JOIN notes n ON n.id = s.note_id ",
            "WHERE s.person_id = p.id AND n.deleted_at IS NULL) AS meeting_count, ",
            "(SELECT max(coalesce(n.started_at, n.created_at)) FROM speakers s JOIN notes n ON n.id = s.note_id ",
            "WHERE s.person_id = p.id AND n.deleted_at IS NULL) AS last_met FROM people p",
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
/// Voice samples kept per Saved person, the most recent: enough to cover how someone sounds on a
/// call and in the room, few enough that comparing a voice with everyone stays quick.
pub const MAX_SAMPLES: usize = 30;

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
        let row = sqlx::query("SELECT audio_path, duration_ms, quotes_json, transcript_timed, speaker_detection FROM notes WHERE id = ?")
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
        let detection: Option<String> = row.try_get("speaker_detection")?;
        let speaker_detection = match detection.as_deref() {
            Some("off") => Some(SpeakerDetection::Off),
            Some("unavailable") => Some(SpeakerDetection::Unavailable),
            _ => None,
        };
        Ok(Transcript { note_id, speakers, segments, duration_ms, has_audio: audio_path.is_some(), quotes, timed, speaker_detection })
    }

    /// Replace the lines shown under a meeting's transcript.
    pub async fn set_note_quotes(&self, note_id: Id, quotes: &[Quote]) -> StoreResult<()> {
        let json = if quotes.is_empty() { None } else { serde_json::to_string(quotes).ok() };
        sqlx::query("UPDATE notes SET quotes_json = ? WHERE id = ?").bind(json).bind(note_id.to_string()).execute(self.pool()).await?;
        Ok(())
    }

    /// Record why this meeting's speakers could not be told apart (`None`: they could).
    pub async fn set_speaker_detection(&self, note_id: Id, detection: Option<SpeakerDetection>) -> StoreResult<()> {
        let value = detection.map(|d| match d {
            SpeakerDetection::Off => "off",
            SpeakerDetection::Unavailable => "unavailable",
        });
        sqlx::query("UPDATE notes SET speaker_detection = ? WHERE id = ?").bind(value).bind(note_id.to_string()).execute(self.pool()).await?;
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
        let done = sqlx::query("UPDATE speakers SET label = ?, person_picked = 1 WHERE id = ?").bind(label).bind(speaker_id.to_string()).execute(self.pool()).await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound("The speaker".into()));
        }
        self.get_speaker(speaker_id).await
    }

    /// Link a speaker to a Saved person or unlink it with `None`. This is the person confirming
    /// who someone is, so it teaches Cilica their voice: the speaker's voiceprint becomes one of
    /// the person's samples, now or as soon as the meeting has one (and a sharper one later in the
    /// meeting replaces it). A sample this speaker gave someone else before is taken back out, so
    /// a correction ("not Sam, Kim") or an unlink untrains the wrong person. Confirming the person
    /// Cilica already picked adds the sample too.
    ///
    /// The speaker is "You" exactly when linked to "You", so unlinking "You" makes them someone
    /// else. If another speaker in the meeting is already that person, the two become one: this
    /// speaker's lines move to the other, which is returned.
    pub async fn link_speaker(&self, speaker_id: Id, person_id: Option<Id>) -> StoreResult<Speaker> {
        let speaker_id = self.resolve_speaker(speaker_id);
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT 1 FROM speakers WHERE id = ?")
            .bind(speaker_id.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| StoreError::NotFound("The speaker".into()))?;
        if let Some(pid) = person_id {
            sqlx::query("SELECT 1 FROM people WHERE id = ?")
                .bind(pid.to_string())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| StoreError::NotFound("That person".into()))?;
        }
        sqlx::query(
            "UPDATE speakers SET person_id = ?1, voice_confirmed = (?1 IS NOT NULL), person_picked = 1, \
             is_self = COALESCE((SELECT is_self FROM people WHERE id = ?1), 0) WHERE id = ?2",
        )
        .bind(person_id.map(|p| p.to_string()))
        .bind(speaker_id.to_string())
        .execute(&mut *tx)
        .await?;
        sync_sample(&mut tx, speaker_id).await?;
        tx.commit().await?;
        let speaker = self.get_speaker(speaker_id).await?;
        let Some(pid) = person_id else { return Ok(speaker) };
        let same = self.list_speakers(speaker.note_id).await?.into_iter().find(|s| s.id != speaker.id && (s.person_id == Some(pid) || (speaker.is_self && s.is_self)));
        match same {
            Some(other) => self.merge_speaker_into(speaker.id, other.id).await,
            None => Ok(speaker),
        }
    }

    /// Cilica's own guess of who a speaker is (a voice it recognised): shown like a link, but it
    /// teaches nothing until the person confirms it with [`Db::link_speaker`]. A speaker someone
    /// chose for (a pick, an unlink, a one-meeting name) never takes a guess, so a guess never
    /// goes over someone's own choice; an earlier guess can be replaced. The speaker is "You"
    /// when the guess is You. Returns whether the guess was applied.
    pub async fn assign_speaker(&self, speaker_id: Id, person_id: Id) -> StoreResult<bool> {
        let done = sqlx::query(
            "UPDATE speakers SET person_id = ?1, guessed_person_id = ?1, \
             is_self = COALESCE((SELECT is_self FROM people WHERE id = ?1), 0) \
             WHERE id = ?2 AND person_picked = 0 AND voice_confirmed = 0",
        )
        .bind(person_id.to_string())
        .bind(self.resolve_speaker(speaker_id).to_string())
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Take back Cilica's guess (its voice no longer sounds like them). Someone's own pick stays.
    /// Returns whether there was a guess to take back.
    pub async fn clear_guess(&self, speaker_id: Id) -> StoreResult<bool> {
        let done = sqlx::query(
            "UPDATE speakers SET person_id = NULL, guessed_person_id = NULL, is_self = 0 \
             WHERE id = ? AND person_id IS NOT NULL AND person_picked = 0 AND voice_confirmed = 0",
        )
        .bind(self.resolve_speaker(speaker_id).to_string())
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The person Cilica guessed this speaker is, while that is still only a guess.
    pub async fn speaker_guess(&self, speaker_id: Id) -> StoreResult<Option<Id>> {
        let row: Option<(Option<String>,)> = sqlx::query_as("SELECT person_id FROM speakers WHERE id = ? AND person_picked = 0 AND voice_confirmed = 0")
            .bind(self.resolve_speaker(speaker_id).to_string())
            .fetch_optional(self.pool())
            .await?;
        row.and_then(|(p,)| p).map(|p| parse_id(&p)).transpose()
    }

    /// "You", created the first time it is needed.
    pub async fn ensure_self_person(&self) -> StoreResult<Person> {
        let found: Option<(String,)> = sqlx::query_as("SELECT id FROM people WHERE is_self = 1").fetch_optional(self.pool()).await?;
        if let Some((id,)) = found {
            return self.get_person(parse_id(&id)?).await;
        }
        let id = cilica_domain::new_id();
        // The unique index keeps it to one even if two callers race here.
        sqlx::query("INSERT OR IGNORE INTO people (id, name, color_index, voiceprint, sample_count, created_at, is_self) VALUES (?, ?, 1, NULL, 0, ?, 1)")
            .bind(id.to_string())
            .bind(SELF_PERSON_NAME)
            .bind(crate::codec::ts(cilica_domain::now()))
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

    /// Forget how a person sounds: their samples go, the person and their links stay. The links
    /// stop counting as confirmations, so the old meetings do not teach the voice back.
    pub async fn forget_voice(&self, person_id: Id) -> StoreResult<Person> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM voice_samples WHERE person_id = ?").bind(person_id.to_string()).execute(&mut *tx).await?;
        sqlx::query("UPDATE speakers SET voice_confirmed = 0 WHERE person_id = ?").bind(person_id.to_string()).execute(&mut *tx).await?;
        refresh_voiceprint(&mut tx, person_id).await?;
        tx.commit().await?;
        self.get_person(person_id).await
    }

    /// Take one meeting out of a person's voice: its sample goes, and the speaker it came from
    /// stops counting as a confirmation so it is not learnt back. The speaker keeps its person.
    pub async fn remove_voice_sample(&self, sample_id: Id) -> StoreResult<Person> {
        let mut tx = self.pool().begin().await?;
        let (person, speaker): (String, Option<String>) = sqlx::query_as("SELECT person_id, speaker_id FROM voice_samples WHERE id = ?")
            .bind(sample_id.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| StoreError::NotFound("That voice sample".into()))?;
        sqlx::query("DELETE FROM voice_samples WHERE id = ?").bind(sample_id.to_string()).execute(&mut *tx).await?;
        if let Some(speaker) = speaker {
            sqlx::query("UPDATE speakers SET voice_confirmed = 0 WHERE id = ?").bind(speaker).execute(&mut *tx).await?;
        }
        let person = parse_id(&person)?;
        refresh_voiceprint(&mut tx, person).await?;
        tx.commit().await?;
        self.get_person(person).await
    }

    /// Forget every voice, yours included: all samples go and no link counts as a confirmation.
    /// People, their links and meetings stay.
    pub async fn forget_all_voices(&self) -> StoreResult<Vec<Person>> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM voice_samples").execute(&mut *tx).await?;
        sqlx::query("UPDATE speakers SET voice_confirmed = 0").execute(&mut *tx).await?;
        sqlx::query("UPDATE people SET voiceprint = NULL, sample_count = 0").execute(&mut *tx).await?;
        tx.commit().await?;
        self.list_people().await
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

    /// How much `embedding` sounds like this person, if Cilica knows their voice.
    pub async fn voice_similarity(&self, person_id: Id, embedding: &[f32]) -> StoreResult<Option<f32>> {
        Ok(self.list_voiceprints().await?.into_iter().find(|v| v.person_id == person_id).map(|v| v.similarity(embedding)))
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
        sqlx::query("UPDATE speakers SET person_id = NULL, voice_confirmed = 0 WHERE person_id = ?").bind(person_id.to_string()).execute(&mut *tx).await?;
        sqlx::query("UPDATE speakers SET guessed_person_id = NULL WHERE guessed_person_id = ?").bind(person_id.to_string()).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM voice_samples WHERE person_id = ?").bind(person_id.to_string()).execute(&mut *tx).await?;
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
        let speaker_id = segment.speaker_id.map(|id| self.resolve_speaker(id));
        sqlx::query(
            "INSERT INTO segments (id, note_id, speaker_id, channel, start_ms, end_ms, text, is_final) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET speaker_id = excluded.speaker_id, channel = excluded.channel, \
             start_ms = excluded.start_ms, end_ms = excluded.end_ms, text = excluded.text, is_final = excluded.is_final",
        )
        .bind(segment.id.to_string())
        .bind(segment.note_id.to_string())
        .bind(speaker_id.map(|s| s.to_string()))
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
            .bind(segment.speaker_id.map(|s| self.resolve_speaker(s).to_string()))
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
        let id = cilica_domain::new_id();
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

    /// Replace a speaker's voiceprint, made from `speech_ms` of their speech. If someone confirmed
    /// who the speaker is, the sample they gave that person is replaced too, so a sharper
    /// voiceprint later in the meeting teaches.
    pub async fn set_speaker_embedding(&self, speaker_id: Id, embedding: Option<&[f32]>, speech_ms: u32) -> StoreResult<()> {
        let speaker_id = self.resolve_speaker(speaker_id);
        let mut tx = self.pool().begin().await?;
        sqlx::query("UPDATE speakers SET embedding = ?, voice_ms = ? WHERE id = ?")
            .bind(embedding.map(|e| vec_to_blob(&normalized(e))))
            .bind(if embedding.is_some() { speech_ms as i64 } else { 0 })
            .bind(speaker_id.to_string())
            .execute(&mut *tx)
            .await?;
        sync_sample(&mut tx, speaker_id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Whether the speaker teaches their person's voice: someone confirmed who they are.
    pub async fn speaker_teaches(&self, speaker_id: Id) -> StoreResult<bool> {
        let row: Option<(bool,)> = sqlx::query_as("SELECT voice_confirmed = 1 AND person_id IS NOT NULL FROM speakers WHERE id = ?")
            .bind(self.resolve_speaker(speaker_id).to_string())
            .fetch_optional(self.pool())
            .await?;
        Ok(row.is_some_and(|(t,)| t))
    }

    /// Whether someone chose who the speaker is (a pick, an unlink or a one-meeting name), so
    /// Cilica's guesses stay off them.
    pub async fn speaker_picked(&self, speaker_id: Id) -> StoreResult<bool> {
        let row: Option<(bool,)> = sqlx::query_as("SELECT person_picked = 1 FROM speakers WHERE id = ?")
            .bind(self.resolve_speaker(speaker_id).to_string())
            .fetch_optional(self.pool())
            .await?;
        Ok(row.is_some_and(|(t,)| t))
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

    /// Two of a note's speakers turned out to be one person: every line of one moves to the
    /// other, which keeps a person link or voiceprint either had. "You" is kept over anyone, a
    /// speaker linked to a person over one that is not, otherwise `into`. Returns the speaker
    /// that is left.
    pub async fn merge_speakers(&self, from: Id, into: Id) -> StoreResult<Id> {
        let (from, into) = (self.resolve_speaker(from), self.resolve_speaker(into));
        if from == into {
            return Ok(into);
        }
        let (a, b) = (self.get_speaker(from).await?, self.get_speaker(into).await?);
        let keep_from = (a.is_self && !b.is_self) || (!b.is_self && a.person_id.is_some() && b.person_id.is_none());
        let (keep, gone) = if keep_from { (a, b) } else { (b, a) };
        self.fold_speaker(gone.id, keep.id, true).await?;
        Ok(keep.id)
    }

    /// The person said "`from` is `into`": every line of `from` moves to `into`, which keeps its
    /// own name, person and "You"-ness whatever `from` had. Returns the speaker that is left.
    pub async fn merge_speaker_into(&self, from: Id, into: Id) -> StoreResult<Speaker> {
        let (from, into) = (self.resolve_speaker(from), self.resolve_speaker(into));
        if from != into {
            let (a, b) = (self.get_speaker(from).await?, self.get_speaker(into).await?);
            if a.note_id != b.note_id {
                return Err(StoreError::Invalid("Those speakers are in different meetings. Pick someone from this one.".into()));
            }
            self.fold_speaker(from, into, false).await?;
        }
        self.get_speaker(into).await
    }

    /// Move `gone`'s lines to `keep` and delete `gone`. With `inherit`, `keep` also takes `gone`'s
    /// person link if it has none. `keep` takes `gone`'s voiceprint if it has none either way, and
    /// counts as confirmed if `gone` was confirmed as the same person. Samples `gone` gave that
    /// person stay (now `keep`'s); a sample it gave anyone else is taken back.
    async fn fold_speaker(&self, gone: Id, keep: Id, inherit: bool) -> StoreResult<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("UPDATE segments SET speaker_id = ? WHERE speaker_id = ?")
            .bind(keep.to_string())
            .bind(gone.to_string())
            .execute(&mut *tx)
            .await?;
        let sql = if inherit {
            "UPDATE speakers SET person_id = COALESCE(person_id, (SELECT person_id FROM speakers WHERE id = ?1)), \
             embedding = COALESCE(embedding, (SELECT embedding FROM speakers WHERE id = ?1)), \
             voice_ms = CASE WHEN embedding IS NULL THEN (SELECT voice_ms FROM speakers WHERE id = ?1) ELSE voice_ms END, \
             guessed_person_id = COALESCE(guessed_person_id, (SELECT guessed_person_id FROM speakers WHERE id = ?1)) WHERE id = ?2"
        } else {
            "UPDATE speakers SET embedding = COALESCE(embedding, (SELECT embedding FROM speakers WHERE id = ?1)), \
             voice_ms = CASE WHEN embedding IS NULL THEN (SELECT voice_ms FROM speakers WHERE id = ?1) ELSE voice_ms END WHERE id = ?2"
        };
        sqlx::query(sql).bind(gone.to_string()).bind(keep.to_string()).execute(&mut *tx).await?;
        sqlx::query("UPDATE speakers SET person_picked = MAX(person_picked, (SELECT person_picked FROM speakers WHERE id = ?1)) WHERE id = ?2")
            .bind(gone.to_string())
            .bind(keep.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE speakers SET voice_confirmed = 1 WHERE id = ?2 AND person_id IS NOT NULL \
             AND EXISTS (SELECT 1 FROM speakers g WHERE g.id = ?1 AND g.voice_confirmed = 1 AND g.person_id = speakers.person_id)",
        )
        .bind(gone.to_string())
        .bind(keep.to_string())
        .execute(&mut *tx)
        .await?;
        // `gone`'s samples of the person `keep` is confirmed as stay (two voices of one person);
        // any other sample it gave is taken back.
        sqlx::query(
            "UPDATE voice_samples SET speaker_id = ?2 WHERE speaker_id = ?1 \
             AND person_id = (SELECT person_id FROM speakers WHERE id = ?2 AND voice_confirmed = 1)",
        )
        .bind(gone.to_string())
        .bind(keep.to_string())
        .execute(&mut *tx)
        .await?;
        let mut touched = take_samples(&mut tx, gone).await?;
        sqlx::query("DELETE FROM speakers WHERE id = ?").bind(gone.to_string()).execute(&mut *tx).await?;
        // Confirmed through `gone`, or confirmed before its voiceprint came from `gone`.
        let (has,): (bool,) = sqlx::query_as("SELECT EXISTS (SELECT 1 FROM voice_samples WHERE speaker_id = ?)").bind(keep.to_string()).fetch_one(&mut *tx).await?;
        if !has {
            touched.extend(give_sample(&mut tx, keep).await?);
        }
        refresh_voiceprints(&mut tx, touched).await?;
        tx.commit().await?;
        // Remember where `gone` went, and move anything that pointed at it along too.
        let mut merged = self.merged_speakers.lock().unwrap_or_else(|e| e.into_inner());
        for to in merged.values_mut() {
            if *to == gone {
                *to = keep;
            }
        }
        merged.insert(gone, keep);
        Ok(())
    }

    /// The speaker a (possibly merged-away) speaker id stands for now.
    pub fn resolve_speaker(&self, speaker_id: Id) -> Id {
        let merged = self.merged_speakers.lock().unwrap_or_else(|e| e.into_inner());
        merged.get(&speaker_id).copied().unwrap_or(speaker_id)
    }

    // ── People ───────────────────────────────────────────────────────────

    /// Create a Saved person, optionally seeded with one voice sample.
    pub async fn create_person(&self, name: &str, voiceprint: Option<&[f32]>) -> StoreResult<Person> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Invalid("A person needs a name. Type one and try again.".into()));
        }
        let id = cilica_domain::new_id();
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM people").fetch_one(self.pool()).await?;
        let color = (count.rem_euclid(COLOR_COUNT as i64) as u8) + 1;
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO people (id, name, color_index, voiceprint, sample_count, created_at) VALUES (?, ?, ?, NULL, 0, ?)")
            .bind(id.to_string())
            .bind(name)
            .bind(color as i64)
            .bind(crate::codec::ts(cilica_domain::now()))
            .execute(&mut *tx)
            .await?;
        if let Some(v) = voiceprint {
            insert_sample(&mut tx, id, None, v, 0).await?;
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
        sqlx::query("UPDATE speakers SET guessed_person_id = ? WHERE guessed_person_id = ?")
            .bind(target_id.to_string())
            .bind(source_id.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM people WHERE id = ?").bind(source_id.to_string()).execute(&mut *tx).await?;
        tx.commit().await?;
        self.get_person(target_id).await
    }

    /// What Cilica knows about a person's voice: how much it learnt, how steady it is, how its
    /// guesses went, and a coarse picture of it to draw.
    pub async fn person_voice(&self, person_id: Id) -> StoreResult<PersonVoice> {
        let person = self.get_person(person_id).await?;
        let pid = person_id.to_string();
        let rows = sqlx::query(
            "SELECT v.id, v.embedding, v.speech_ms, v.created_at, s.note_id, n.title FROM voice_samples v \
             LEFT JOIN speakers s ON s.id = v.speaker_id LEFT JOIN notes n ON n.id = s.note_id AND n.deleted_at IS NULL \
             WHERE v.person_id = ? ORDER BY v.rowid DESC",
        )
        .bind(&pid)
        .fetch_all(self.pool())
        .await?;
        let mut samples = Vec::with_capacity(rows.len());
        for r in &rows {
            let id: String = r.try_get("id")?;
            let blob: Vec<u8> = r.try_get("embedding")?;
            let ms: i64 = r.try_get("speech_ms")?;
            let created: String = r.try_get("created_at")?;
            let note: Option<String> = r.try_get("note_id")?;
            let title: Option<String> = r.try_get("title")?;
            let note_id = match (&note, &title) {
                (Some(n), Some(_)) => Some(parse_id(n)?),
                _ => None,
            };
            samples.push((parse_id(&id)?, blob_to_vec(&blob), ms.clamp(0, u32::MAX as i64) as u32, crate::codec::parse_ts(&created)?, note_id, title));
        }
        let vectors: Vec<Vec<f32>> = samples.iter().map(|s| s.1.clone()).collect();
        let mean = sqlx::query_as::<_, (Option<Vec<u8>>,)>("SELECT voiceprint FROM people WHERE id = ?").bind(&pid).fetch_one(self.pool()).await?.0.map(|b| blob_to_vec(&b));
        let consistency = match (&mean, vectors.len()) {
            (Some(m), n) if n >= 2 => Some((vectors.iter().map(|v| cosine(v, m)).sum::<f32>() / n as f32).clamp(0.0, 1.0)),
            _ => None,
        };
        let shape = mean.as_ref().filter(|_| !samples.is_empty()).map(|m| {
            let (mean_bins, scale) = voice_bins(m, None);
            let sample_bins: Vec<Vec<f32>> = vectors.iter().map(|v| voice_bins(v, Some(scale)).0).collect();
            let spread = (0..mean_bins.len())
                .map(|i| {
                    let var = sample_bins.iter().map(|b| (b[i] - mean_bins[i]).powi(2)).sum::<f32>() / sample_bins.len().max(1) as f32;
                    var.sqrt().clamp(0.0, 1.0)
                })
                .collect();
            let shapes = samples
                .iter()
                .zip(sample_bins)
                .map(|((id, v, ms, at, note_id, title), bins)| {
                    let (x, y) = voice_point(v, m);
                    let similarity = cosine(v, m).clamp(0.0, 1.0);
                    VoiceSampleShape { id: *id, bins, similarity, x, y, speech_ms: *ms, note_id: *note_id, note_title: title.clone(), created_at: *at }
                })
                .collect();
            VoiceShape { mean: mean_bins, spread, samples: shapes }
        });
        let (named, corrected): (i64, i64) = sqlx::query_as(
            "SELECT count(*), coalesce(sum(CASE WHEN person_picked = 1 AND (person_id IS NULL OR person_id != guessed_person_id) THEN 1 ELSE 0 END), 0) \
             FROM speakers s JOIN notes n ON n.id = s.note_id AND n.deleted_at IS NULL WHERE s.guessed_person_id = ?",
        )
        .bind(&pid)
        .fetch_one(self.pool())
        .await?;
        let (last,): (Option<String>,) = sqlx::query_as(
            "SELECT max(coalesce(n.started_at, n.created_at)) FROM speakers s JOIN notes n ON n.id = s.note_id \
             WHERE s.person_id = ? AND n.deleted_at IS NULL",
        )
        .bind(&pid)
        .fetch_one(self.pool())
        .await?;
        Ok(PersonVoice {
            person_id,
            speech_ms: samples.iter().map(|s| s.2).fold(0u32, u32::saturating_add),
            consistency,
            named: named.clamp(0, u32::MAX as i64) as u32,
            corrected: corrected.clamp(0, u32::MAX as i64) as u32,
            meeting_count: person.meeting_count,
            last_heard: last.as_deref().map(crate::codec::parse_ts).transpose()?,
            shape,
        })
    }

    /// The meetings a person was in (not in Trash), newest first: how long they spoke and who
    /// named them there.
    pub async fn person_meetings(&self, person_id: Id) -> StoreResult<Vec<PersonMeeting>> {
        self.get_person(person_id).await?;
        let rows = sqlx::query(
            "SELECT n.id, n.title, coalesce(n.started_at, n.created_at) AS at, n.duration_ms, \
             max(s.person_picked) AS picked, max(s.guessed_person_id = s.person_id) AS guessed, \
             (SELECT coalesce(sum(max(g.end_ms - g.start_ms, 0)), 0) FROM segments g JOIN speakers t ON t.id = g.speaker_id \
              WHERE g.note_id = n.id AND t.person_id = ?1) AS speech_ms, \
             (SELECT count(*) FROM segments g JOIN speakers t ON t.id = g.speaker_id \
              WHERE g.note_id = n.id AND t.person_id = ?1) AS lines \
             FROM speakers s JOIN notes n ON n.id = s.note_id \
             WHERE s.person_id = ?1 AND n.deleted_at IS NULL GROUP BY n.id ORDER BY at DESC",
        )
        .bind(person_id.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|r| {
                let id: String = r.try_get("id")?;
                let at: String = r.try_get("at")?;
                let duration: Option<i64> = r.try_get("duration_ms")?;
                let picked: Option<bool> = r.try_get("picked")?;
                let guessed: Option<bool> = r.try_get("guessed")?;
                let speech: i64 = r.try_get("speech_ms")?;
                let lines: i64 = r.try_get("lines")?;
                Ok(PersonMeeting {
                    note_id: parse_id(&id)?,
                    title: r.try_get("title")?,
                    at: parse_ts(&at)?,
                    duration_ms: duration.map(|d| d.clamp(0, u32::MAX as i64) as u32),
                    speech_ms: speech.clamp(0, u32::MAX as i64) as u32,
                    lines: lines.clamp(0, u32::MAX as i64) as u32,
                    // Cilica named them only where nobody picked them and its guess stands.
                    named_by: if !picked.unwrap_or(false) && guessed.unwrap_or(false) { NamedBy::Cilica } else { NamedBy::You },
                })
            })
            .collect()
    }

    /// Every Saved person Cilica knows the voice of, with their samples.
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

    /// "Is Person 2 Sam?": for each unlinked, non-self speaker of the note that has an embedding
    /// and that no one chose for (an unlink or a one-meeting name is an answer already),
    /// the best-matching Saved person with cosine similarity at or above `threshold`. One person is
    /// suggested for at most one speaker, and people already linked in this note are skipped.
    pub async fn speaker_suggestions(&self, note_id: Id, threshold: f32) -> StoreResult<Vec<SpeakerSuggestion>> {
        let rows = sqlx::query("SELECT id, person_id, is_self, person_picked, embedding FROM speakers WHERE note_id = ?")
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
            let picked: bool = r.try_get("person_picked")?;
            let emb: Option<Vec<u8>> = r.try_get("embedding")?;
            if let (false, false, Some(emb)) = (is_self, picked, emb) {
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
    let last: Option<String> = r.try_get("last_met")?;
    Ok(Person {
        id: parse_id(&id)?,
        name: r.try_get("name")?,
        color_index: color.clamp(1, COLOR_COUNT as i64) as u8,
        sample_count: samples.clamp(0, u32::MAX as i64) as u32,
        meeting_count: meetings.clamp(0, u32::MAX as i64) as u32,
        created_at: parse_ts(&created)?,
        is_self: r.try_get("is_self")?,
        last_met: last.as_deref().map(parse_ts).transpose()?,
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

/// Bins in a [`VoiceShape`].
const SHAPE_BINS: usize = 64;

/// Fold a voiceprint into [`SHAPE_BINS`] bins (the mean of each run of values), scaled to
/// -1..=1 by `scale` or by its own largest bin. Returns the bins and the scale used.
fn voice_bins(v: &[f32], scale: Option<f32>) -> (Vec<f32>, f32) {
    let n = SHAPE_BINS.min(v.len().max(1));
    let raw: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = (i * v.len() / n, ((i + 1) * v.len() / n).max(i * v.len() / n + 1).min(v.len()));
            if a >= b { 0.0 } else { v[a..b].iter().sum::<f32>() / (b - a) as f32 }
        })
        .collect();
    let scale = scale.unwrap_or_else(|| raw.iter().fold(0.0f32, |m, x| m.max(x.abs()))).max(f32::EPSILON);
    (raw.iter().map(|x| (x / scale).clamp(-1.0, 1.0)).collect(), scale)
}

/// Where a sample sits on a 2D map around its person's voiceprint at (0, 0): the distance is how
/// unlike the voiceprint it is (1 - cosine, 0.5 or more at the edge), the direction a fixed
/// projection of how it differs, so the same sample always lands in the same place.
fn voice_point(sample: &[f32], mean: &[f32]) -> (f32, f32) {
    let diff: Vec<f32> = sample.iter().zip(mean).map(|(s, m)| s - m).collect();
    // Two fixed pseudo-random directions.
    let axis = |seed: u32, i: usize| {
        let mut h = (i as u32).wrapping_mul(2_654_435_761).wrapping_add(seed);
        h ^= h >> 15;
        h = h.wrapping_mul(2_246_822_519);
        h ^= h >> 13;
        (h as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    let (px, py) = diff.iter().enumerate().fold((0.0f32, 0.0f32), |(x, y), (i, d)| (x + d * axis(17, i), y + d * axis(91, i)));
    let len = (px * px + py * py).sqrt();
    let r = ((1.0 - cosine(sample, mean)) / 0.5).clamp(0.0, 1.0);
    if len <= f32::EPSILON { (0.0, 0.0) } else { (px / len * r, py / len * r) }
}

/// Add one voice sample (normalised) for a person.
async fn insert_sample(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, person_id: Id, speaker_id: Option<Id>, embedding: &[f32], speech_ms: u32) -> StoreResult<()> {
    sqlx::query("INSERT INTO voice_samples (id, person_id, speaker_id, embedding, created_at, speech_ms) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(cilica_domain::new_id().to_string())
        .bind(person_id.to_string())
        .bind(speaker_id.map(|s| s.to_string()))
        .bind(vec_to_blob(&normalized(embedding)))
        .bind(crate::codec::ts(cilica_domain::now()))
        .bind(speech_ms as i64)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Take back every sample a speaker gave. Returns the people who had one.
async fn take_samples(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, speaker_id: Id) -> StoreResult<Vec<Id>> {
    let rows = sqlx::query("SELECT DISTINCT person_id FROM voice_samples WHERE speaker_id = ?").bind(speaker_id.to_string()).fetch_all(&mut **tx).await?;
    sqlx::query("DELETE FROM voice_samples WHERE speaker_id = ?").bind(speaker_id.to_string()).execute(&mut **tx).await?;
    rows.iter().map(|r| r.try_get::<String, _>("person_id").map_err(StoreError::from).and_then(|p| parse_id(&p))).collect()
}

/// Give the speaker's voiceprint to their person as a sample, if someone confirmed who they are
/// and there is a voiceprint. Returns that person.
async fn give_sample(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, speaker_id: Id) -> StoreResult<Option<Id>> {
    let Some(row) = sqlx::query("SELECT person_id, voice_confirmed, embedding, voice_ms FROM speakers WHERE id = ?")
        .bind(speaker_id.to_string())
        .fetch_optional(&mut **tx)
        .await?
    else {
        return Ok(None);
    };
    let person: Option<String> = row.try_get("person_id")?;
    let confirmed: bool = row.try_get("voice_confirmed")?;
    let embedding: Option<Vec<u8>> = row.try_get("embedding")?;
    let speech_ms: i64 = row.try_get("voice_ms")?;
    let (Some(person), true, Some(embedding)) = (person, confirmed, embedding) else { return Ok(None) };
    let person = parse_id(&person)?;
    insert_sample(tx, person, Some(speaker_id), &blob_to_vec(&embedding), speech_ms.clamp(0, u32::MAX as i64) as u32).await?;
    Ok(Some(person))
}

/// Make the samples a speaker gives match who they are now: exactly one, to their person, when
/// confirmed and they have a voiceprint; none otherwise. Every person affected is refreshed.
async fn sync_sample(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, speaker_id: Id) -> StoreResult<()> {
    let mut touched = take_samples(tx, speaker_id).await?;
    touched.extend(give_sample(tx, speaker_id).await?);
    refresh_voiceprints(tx, touched).await
}

/// Refresh each person once.
async fn refresh_voiceprints(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, people: Vec<Id>) -> StoreResult<()> {
    let mut seen = HashSet::new();
    for person in people {
        if seen.insert(person) {
            refresh_voiceprint(tx, person).await?;
        }
    }
    Ok(())
}

/// Recompute a person's voiceprint (the normalised mean of their samples) and sample count,
/// keeping only their [`MAX_SAMPLES`] most recent samples.
async fn refresh_voiceprint(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, person_id: Id) -> StoreResult<()> {
    sqlx::query(
        "DELETE FROM voice_samples WHERE person_id = ?1 AND rowid NOT IN \
         (SELECT rowid FROM voice_samples WHERE person_id = ?1 ORDER BY rowid DESC LIMIT ?2)",
    )
    .bind(person_id.to_string())
    .bind(MAX_SAMPLES as i64)
    .execute(&mut **tx)
    .await?;
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
    use cilica_domain::new_id;

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

    /// Cilica's own guesses link without teaching; recognition picks the closest sample.
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
    async fn merged_speakers_and_suggestions() {
        let db = Db::open_in_memory().await.unwrap();
        let sam = db.create_person("Sam", Some(&[1.0, 0.0, 0.0])).await.unwrap();
        let _kim = db.create_person("Kim", Some(&[0.0, 0.0, 1.0])).await.unwrap();
        let n = note(&db).await;
        let (you, _) = db.ensure_channel_speakers(n).await.unwrap();
        let new = |label: &str, embedding: Option<Vec<f32>>| NewSpeaker { label: label.into(), color_index: 3, is_self: false, embedding };
        let p1 = db.create_speaker(n, new("Person 1", None)).await.unwrap();
        let p2 = db.create_speaker(n, new("Person 2", Some(vec![0.9, 0.1, 0.0]))).await.unwrap();
        let p3 = db.create_speaker(n, new("Person 3", Some(vec![0.0, 1.0, 0.0]))).await.unwrap();
        let s1 = seg(n, Some(p1.id), AudioChannel::System, 0, "One");
        let s2 = seg(n, Some(p2.id), AudioChannel::System, 2000, "Two");
        let m1 = seg(n, Some(p3.id), AudioChannel::Mic, 1000, "Mine");
        for s in [&s1, &s2, &m1] {
            db.upsert_segment(s).await.unwrap();
        }

        // Person 2 was Person 1 all along: one speaker left, with Person 2's voiceprint.
        assert_eq!(db.merge_speakers(p2.id, p1.id).await.unwrap(), p1.id);
        // "You" is kept whichever way round.
        assert_eq!(db.merge_speakers(you.id, p3.id).await.unwrap(), you.id);
        let t = db.get_transcript(n).await.unwrap();
        let by_id: HashMap<Id, Option<Id>> = t.segments.iter().map(|s| (s.id, s.speaker_id)).collect();
        assert_eq!((by_id[&s1.id], by_id[&s2.id], by_id[&m1.id]), (Some(p1.id), Some(p1.id), Some(you.id)));
        assert!(t.speakers.iter().all(|s| s.id != p2.id && s.id != p3.id));
        assert!(db.speaker_embedding(p1.id).await.unwrap().is_some());

        let sugg = db.speaker_suggestions(n, 0.7).await.unwrap();
        assert_eq!(sugg.len(), 1);
        assert_eq!((sugg[0].speaker_id, sugg[0].person_id, sugg[0].person_name.as_str()), (p1.id, sam.id, "Sam"));
        assert!(sugg[0].similarity > 0.9);

        // Once linked, no more suggestions for that person or speaker.
        db.link_speaker(p1.id, Some(sam.id)).await.unwrap();
        assert!(db.speaker_suggestions(n, 0.7).await.unwrap().is_empty());
    }

    #[test]
    fn blob_roundtrip() {
        let v = vec![0.25f32, -1.5, 3.0e-7];
        assert_eq!(blob_to_vec(&vec_to_blob(&v)), v);
        assert_eq!(vec_to_blob(&[1.0]), 1.0f32.to_le_bytes().to_vec());
    }

    /// Marking a third voice as someone already in the meeting makes them one speaker, and a
    /// line live recording still writes for the merged speaker lands on the one that is left.
    #[tokio::test]
    async fn linking_to_someone_already_here_merges_the_speakers() {
        let db = Db::open_in_memory().await.unwrap();
        let marcus = db.create_person("Marcus", None).await.unwrap();
        let n = note(&db).await;
        let first = db.create_speaker(n, voice("Person 2", &[1.0, 0.0])).await.unwrap();
        let third = db.create_speaker(n, voice("Person 3", &[0.9, 0.1])).await.unwrap();
        db.upsert_segment(&seg(n, Some(first.id), AudioChannel::System, 0, "Hi")).await.unwrap();
        db.upsert_segment(&seg(n, Some(third.id), AudioChannel::System, 2000, "Me again")).await.unwrap();

        db.link_speaker(first.id, Some(marcus.id)).await.unwrap();
        let left = db.link_speaker(third.id, Some(marcus.id)).await.unwrap();
        assert_eq!((left.id, left.person_id), (first.id, Some(marcus.id)));
        let t = db.get_transcript(n).await.unwrap();
        assert_eq!(t.speakers.len(), 1);
        assert!(t.segments.iter().all(|s| s.speaker_id == Some(first.id)));
        // Both voices taught Marcus.
        assert_eq!(db.get_person(marcus.id).await.unwrap().sample_count, 2);

        // Live recording still knows the old id.
        db.upsert_segment(&seg(n, Some(third.id), AudioChannel::System, 4000, "Later")).await.unwrap();
        let t = db.get_transcript(n).await.unwrap();
        assert!(t.segments.iter().all(|s| s.speaker_id == Some(first.id)));
    }

    /// "You" can be unlinked (they become someone else) and merging keeps the speaker picked.
    #[tokio::test]
    async fn you_can_be_unlinked_and_merged_into_someone() {
        let db = Db::open_in_memory().await.unwrap();
        let me = db.ensure_self_person().await.unwrap();
        let n = note(&db).await;
        let (you, _) = db.ensure_channel_speakers(n).await.unwrap();
        db.link_self_speakers(n).await.unwrap();
        db.upsert_segment(&seg(n, Some(you.id), AudioChannel::Mic, 0, "Mine?")).await.unwrap();

        let unlinked = db.link_speaker(you.id, None).await.unwrap();
        assert!(!unlinked.is_self && unlinked.person_id.is_none());
        let relinked = db.link_speaker(you.id, Some(me.id)).await.unwrap();
        assert!(relinked.is_self);

        // Marking another voice as "You" folds it into the "You" speaker.
        let echo = db.create_speaker(n, voice("Person 4", &[1.0, 0.0])).await.unwrap();
        db.upsert_segment(&seg(n, Some(echo.id), AudioChannel::System, 900, "Also mine")).await.unwrap();
        assert_eq!(db.link_speaker(echo.id, Some(me.id)).await.unwrap().id, you.id);

        // "You" was really Person 2: Person 2 is kept as it was.
        let p2 = db.create_speaker(n, voice("Person 2", &[0.0, 1.0])).await.unwrap();
        let left = db.merge_speaker_into(you.id, p2.id).await.unwrap();
        assert_eq!((left.id, left.is_self, left.person_id, left.label.as_str()), (p2.id, false, None, "Person 2"));
        let t = db.get_transcript(n).await.unwrap();
        assert!(t.segments.iter().all(|s| s.speaker_id == Some(p2.id)));
    }

    /// The sample a speaker gave, if any: (person, embedding).
    async fn sample_of(db: &Db, speaker: Id) -> Vec<(Id, Vec<f32>)> {
        sqlx::query_as::<_, (String, Vec<u8>)>("SELECT person_id, embedding FROM voice_samples WHERE speaker_id = ?")
            .bind(speaker.to_string())
            .fetch_all(db.pool())
            .await
            .unwrap()
            .into_iter()
            .map(|(p, e)| (parse_id(&p).unwrap(), blob_to_vec(&e)))
            .collect()
    }

    /// Someone picked during the meeting, before Cilica had heard enough of them: their voice
    /// teaches as soon as it has a voiceprint, and each sharper one replaces the sample.
    #[tokio::test]
    async fn a_pick_before_the_voiceprint_teaches_once_it_comes_and_follows_it() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let sam = db.create_person("Sam", None).await.unwrap();
        let s = db.create_speaker(n, NewSpeaker { label: "Person 1".into(), color_index: 3, is_self: false, embedding: None }).await.unwrap();
        db.link_speaker(s.id, Some(sam.id)).await.unwrap();
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 0, "nothing to learn yet");
        assert!(db.speaker_teaches(s.id).await.unwrap());

        db.set_speaker_embedding(s.id, Some(&[1.0, 0.0]), 1_000).await.unwrap();
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 1);
        db.set_speaker_embedding(s.id, Some(&[0.0, 2.0]), 1_000).await.unwrap();
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 1, "replaced, not added");
        assert_eq!(sample_of(&db, s.id).await, vec![(sam.id, vec![0.0, 1.0])]);
        let print = db.list_voiceprints().await.unwrap().into_iter().find(|v| v.person_id == sam.id).unwrap();
        assert_eq!(print.vector, vec![0.0, 1.0], "the mean follows the new sample");
    }

    /// Picking someone else or unlinking mid-meeting moves or drops the sample, and later
    /// voiceprints follow: the wrong person never learns from them again.
    #[tokio::test]
    async fn relinking_and_unlinking_keep_later_voiceprints_with_the_right_person() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let sam = db.create_person("Sam", None).await.unwrap();
        let kim = db.create_person("Kim", None).await.unwrap();
        let s = db.create_speaker(n, voice("Person 1", &[1.0, 0.0])).await.unwrap();

        db.link_speaker(s.id, Some(sam.id)).await.unwrap();
        db.link_speaker(s.id, Some(kim.id)).await.unwrap();
        db.set_speaker_embedding(s.id, Some(&[0.0, 1.0]), 1_000).await.unwrap();
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 0);
        assert_eq!(sample_of(&db, s.id).await, vec![(kim.id, vec![0.0, 1.0])]);

        db.link_speaker(s.id, None).await.unwrap();
        db.set_speaker_embedding(s.id, Some(&[0.6, 0.8]), 1_000).await.unwrap();
        assert!(sample_of(&db, s.id).await.is_empty());
        assert_eq!(db.get_person(kim.id).await.unwrap().sample_count, 0);
        assert!(db.list_voiceprints().await.unwrap().is_empty(), "nobody keeps a voiceprint with no samples");
        assert!(!db.speaker_teaches(s.id).await.unwrap());

        // A new person from this speaker ("Add Lee as a new person") takes the latest voice.
        let lee = db.save_speaker_as_person(s.id, "Lee").await.unwrap();
        assert_eq!(lee.sample_count, 1);
        assert_eq!(sample_of(&db, s.id).await, vec![(lee.id, vec![0.6, 0.8])]);
        // Renaming the person changes their name only.
        db.rename_person(lee.id, "Leigh").await.unwrap();
        assert_eq!(db.get_person(lee.id).await.unwrap().sample_count, 1);
    }

    /// A pick, an unlink or a one-meeting name is the person's answer: guesses and suggestions
    /// stay off that speaker.
    #[tokio::test]
    async fn guesses_never_go_over_someones_choice() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let sam = db.create_person("Sam", Some(&[1.0, 0.0])).await.unwrap();
        let kim = db.create_person("Kim", None).await.unwrap();

        let picked = db.create_speaker(n, voice("Person 1", &[1.0, 0.0])).await.unwrap();
        db.link_speaker(picked.id, Some(kim.id)).await.unwrap();
        assert!(!db.assign_speaker(picked.id, sam.id).await.unwrap());
        assert_eq!(db.get_speaker(picked.id).await.unwrap().person_id, Some(kim.id));

        let unlinked = db.create_speaker(n, voice("Person 2", &[1.0, 0.0])).await.unwrap();
        assert!(db.assign_speaker(unlinked.id, sam.id).await.unwrap(), "a guess");
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 1, "a guess teaches nothing");
        db.link_speaker(unlinked.id, None).await.unwrap();
        assert!(!db.assign_speaker(unlinked.id, sam.id).await.unwrap(), "not Sam, said the person");
        assert!(db.get_speaker(unlinked.id).await.unwrap().person_id.is_none());

        let named = db.create_speaker(n, voice("Person 3", &[1.0, 0.0])).await.unwrap();
        db.rename_speaker(named.id, "The caterer").await.unwrap();
        assert!(!db.assign_speaker(named.id, sam.id).await.unwrap());

        assert!(db.speaker_suggestions(n, 0.5).await.unwrap().is_empty(), "every speaker here has an answer");
        let fresh = db.create_speaker(n, voice("Person 4", &[1.0, 0.0])).await.unwrap();
        assert_eq!(db.speaker_suggestions(n, 0.5).await.unwrap().iter().map(|s| s.speaker_id).collect::<Vec<_>>(), vec![fresh.id]);
    }

    /// Forgetting a voice or deleting a person keeps old meetings from teaching it back.
    #[tokio::test]
    async fn a_forgotten_voice_is_not_taught_back_by_old_speakers() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let sam = db.create_person("Sam", None).await.unwrap();
        let s = db.create_speaker(n, voice("Person 1", &[1.0, 0.0])).await.unwrap();
        db.link_speaker(s.id, Some(sam.id)).await.unwrap();
        db.forget_voice(sam.id).await.unwrap();
        db.set_speaker_embedding(s.id, Some(&[0.0, 1.0]), 1_000).await.unwrap();
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 0);
        assert_eq!(db.get_speaker(s.id).await.unwrap().person_id, Some(sam.id), "still linked");
        // Confirming again teaches again.
        db.link_speaker(s.id, Some(sam.id)).await.unwrap();
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 1);

        db.delete_person(sam.id).await.unwrap();
        db.set_speaker_embedding(s.id, Some(&[1.0, 0.0]), 1_000).await.unwrap();
        assert!(sample_of(&db, s.id).await.is_empty());
    }

    /// Removing one meeting from a voice keeps the speaker named but stops it teaching; forgetting
    /// every voice empties them all and keeps everyone.
    #[tokio::test]
    async fn removing_a_sample_and_forgetting_every_voice() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let sam = db.create_person("Sam", None).await.unwrap();
        let a = db.create_speaker(n, voice("Person 1", &[1.0, 0.0])).await.unwrap();
        let other = note(&db).await;
        let b = db.create_speaker(other, voice("Person 1", &[0.9, 0.1])).await.unwrap();
        db.link_speaker(a.id, Some(sam.id)).await.unwrap();
        db.link_speaker(b.id, Some(sam.id)).await.unwrap();
        let shape = db.person_voice(sam.id).await.unwrap().shape.unwrap();
        assert_eq!(shape.samples.len(), 2);
        assert!(shape.samples.iter().all(|s| s.similarity > 0.9));
        let gone = shape.samples.iter().find(|s| s.note_id == Some(n)).unwrap().id;

        let after = db.remove_voice_sample(gone).await.unwrap();
        assert_eq!(after.sample_count, 1);
        assert_eq!(db.get_speaker(a.id).await.unwrap().person_id, Some(sam.id), "still named");
        db.set_speaker_embedding(a.id, Some(&[1.0, 0.0]), 1_000).await.unwrap();
        assert!(sample_of(&db, a.id).await.is_empty(), "not learnt back");
        assert!(db.remove_voice_sample(gone).await.is_err());

        let me = db.ensure_self_person().await.unwrap();
        let c = db.create_speaker(n, voice("Person 2", &[0.0, 1.0])).await.unwrap();
        db.link_speaker(c.id, Some(me.id)).await.unwrap();
        let people = db.forget_all_voices().await.unwrap();
        assert_eq!(people.len(), 2);
        assert!(people.iter().all(|p| p.sample_count == 0));
        assert!(db.list_voiceprints().await.unwrap().is_empty());
        db.set_speaker_embedding(c.id, Some(&[0.0, 1.0]), 1_000).await.unwrap();
        assert!(sample_of(&db, c.id).await.is_empty());
    }

    /// A person's page lists their meetings with how long they spoke and who named them.
    #[tokio::test]
    async fn a_persons_meetings_say_who_named_them() {
        let db = Db::open_in_memory().await.unwrap();
        let sam = db.create_person("Sam", None).await.unwrap();
        assert!(db.person_meetings(sam.id).await.unwrap().is_empty());
        assert_eq!(db.get_person(sam.id).await.unwrap().last_met, None);

        let picked = note(&db).await;
        let a = db.create_speaker(picked, voice("Person 1", &[1.0, 0.0])).await.unwrap();
        db.link_speaker(a.id, Some(sam.id)).await.unwrap();
        db.upsert_segment(&seg(picked, Some(a.id), AudioChannel::System, 0, "Hi")).await.unwrap();
        db.upsert_segment(&seg(picked, Some(a.id), AudioChannel::System, 2_000, "Again")).await.unwrap();
        let heard = note(&db).await;
        let b = db.create_speaker(heard, voice("Person 1", &[1.0, 0.0])).await.unwrap();
        assert!(db.assign_speaker(b.id, sam.id).await.unwrap());

        let meetings = db.person_meetings(sam.id).await.unwrap();
        assert_eq!(meetings.len(), 2);
        let first = meetings.iter().find(|m| m.note_id == picked).unwrap();
        assert_eq!((first.named_by, first.lines, first.speech_ms), (NamedBy::You, 2, 3_000));
        let second = meetings.iter().find(|m| m.note_id == heard).unwrap();
        assert_eq!((second.named_by, second.lines), (NamedBy::Cilica, 0));
        assert!(db.get_person(sam.id).await.unwrap().last_met.is_some());
    }

    /// Joining a speaker into one confirmed as someone else takes back what it taught.
    #[tokio::test]
    async fn merging_into_someone_else_untrains_the_wrong_person() {
        let db = Db::open_in_memory().await.unwrap();
        let n = note(&db).await;
        let sam = db.create_person("Sam", None).await.unwrap();
        let kim = db.create_person("Kim", None).await.unwrap();
        let a = db.create_speaker(n, voice("Person 1", &[1.0, 0.0])).await.unwrap();
        let b = db.create_speaker(n, voice("Person 2", &[0.0, 1.0])).await.unwrap();
        db.link_speaker(a.id, Some(sam.id)).await.unwrap();
        db.link_speaker(b.id, Some(kim.id)).await.unwrap();

        // "Person 1 is Person 2": Sam was wrong.
        db.merge_speaker_into(a.id, b.id).await.unwrap();
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count, 0);
        assert_eq!(db.get_person(kim.id).await.unwrap().sample_count, 1);
        assert_eq!(sample_of(&db, b.id).await, vec![(kim.id, vec![0.0, 1.0])]);
        // A later voiceprint for the old id lands on the speaker that is left.
        db.set_speaker_embedding(a.id, Some(&[0.6, 0.8]), 1_000).await.unwrap();
        assert_eq!(sample_of(&db, b.id).await, vec![(kim.id, vec![0.6, 0.8])]);
    }

    /// Only the most recent samples are kept.
    #[tokio::test]
    async fn a_person_keeps_their_most_recent_samples() {
        let db = Db::open_in_memory().await.unwrap();
        let sam = db.create_person("Sam", None).await.unwrap();
        let mut speakers = Vec::new();
        for i in 0..MAX_SAMPLES + 3 {
            let other = db.create_note(crate::NewNote { title: format!("Call {i}"), kind: cilica_domain::NoteKind::Meeting, ..Default::default() }).await.unwrap().id;
            let s = db.create_speaker(other, voice("Person 1", &[1.0, i as f32])).await.unwrap();
            db.link_speaker(s.id, Some(sam.id)).await.unwrap();
            speakers.push(s.id);
        }
        assert_eq!(db.get_person(sam.id).await.unwrap().sample_count as usize, MAX_SAMPLES);
        assert!(sample_of(&db, speakers[0]).await.is_empty(), "the oldest went");
        assert_eq!(sample_of(&db, *speakers.last().unwrap()).await.len(), 1);
    }
}
