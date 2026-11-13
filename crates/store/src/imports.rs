//! Notes brought in from other apps (docs/import.md). Owner: Import.
//!
//! An imported note is written in one transaction with its speakers, segments, search row and
//! first version, so a cancelled or crashed import never leaves half a note behind. The note keeps
//! the provider and the item id it came from (`import_provider`, `import_id`), so importing the same
//! source again skips items that are already a note. A note in Trash or deleted no longer counts:
//! importing again brings it back as a new note.

use std::collections::HashSet;

use margin_domain::{AudioChannel, Category, Id, NoteKind, Timestamp, new_id, now};

use crate::notes::{index_text, kind_to_sql, markdown_to_plain, single_line, snippet_from_markdown, top_position};
use crate::{Db, StoreError, StoreResult, codec};

/// Speaker colours rotate through the `speaker-N` tokens.
const COLOR_COUNT: u8 = 8;

/// One imported note, ready to store.
#[derive(Debug, Clone)]
pub struct ImportWrite {
    pub provider: String,
    pub external_id: String,
    pub title: String,
    pub body_md: String,
    pub kind: NoteKind,
    pub meeting_app: Option<String>,
    pub started_at: Option<Timestamp>,
    pub ended_at: Option<Timestamp>,
    pub duration_ms: Option<u32>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub category_id: Option<Id>,
    /// False when the transcript had no times and segment times only keep the order.
    pub timed: bool,
    pub speakers: Vec<ImportSpeaker>,
    pub segments: Vec<ImportSegment>,
}

#[derive(Debug, Clone)]
pub struct ImportSpeaker {
    pub label: String,
    pub is_self: bool,
}

#[derive(Debug, Clone)]
pub struct ImportSegment {
    /// Index into [`ImportWrite::speakers`].
    pub speaker: Option<usize>,
    pub channel: AudioChannel,
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
}

impl Db {
    // ── Sources ──────────────────────────────────────────────────────────

    /// Item ids from this provider that are already notes outside Trash.
    pub async fn live_import_ids(&self, provider: &str) -> StoreResult<HashSet<String>> {
        let ids: Vec<String> = sqlx::query_scalar("SELECT import_id FROM notes WHERE import_provider = ? AND import_id IS NOT NULL AND deleted_at IS NULL")
            .bind(provider)
            .fetch_all(self.pool())
            .await?;
        Ok(ids.into_iter().collect())
    }

    // ── Writing notes ────────────────────────────────────────────────────

    /// Store a new imported note at the top of its category, with its transcript, search row,
    /// first version and source. Returns the note id.
    pub async fn insert_imported_note(&self, w: &ImportWrite) -> StoreResult<Id> {
        let mut tx = self.begin_write().await?;
        let id = new_id();
        let title = single_line(&w.title).trim().to_string();
        let position = top_position(&mut *tx, w.category_id).await?;
        sqlx::query(
            "INSERT INTO notes (id, title, body_md, snippet, category_id, kind, meeting_app, started_at, ended_at, position, \
             duration_ms, transcript_timed, created_at, updated_at, import_provider, import_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(codec::id(id))
        .bind(&title)
        .bind(&w.body_md)
        .bind(snippet_from_markdown(&w.body_md))
        .bind(codec::opt_id(w.category_id))
        .bind(kind_to_sql(w.kind))
        .bind(&w.meeting_app)
        .bind(codec::opt_ts(w.started_at))
        .bind(codec::opt_ts(w.ended_at))
        .bind(position)
        .bind(w.duration_ms.map(i64::from))
        .bind(w.timed)
        .bind(codec::ts(w.created_at))
        .bind(codec::ts(w.updated_at))
        .bind(&w.provider)
        .bind(&w.external_id)
        .execute(&mut *tx)
        .await?;
        insert_transcript(&mut tx, id, w).await?;
        sqlx::query("INSERT INTO notes_fts (note_id, title, body_md, transcript) VALUES (?, ?, ?, ?)")
            .bind(codec::id(id))
            .bind(index_text(&title))
            .bind(markdown_to_plain(&w.body_md))
            .bind(transcript_text(w))
            .execute(&mut *tx)
            .await?;
        insert_version(&mut tx, id, &title, &w.body_md).await?;
        tx.commit().await?;
        Ok(id)
    }

    /// The category with this name (ignoring case), created at the end of the list if missing.
    pub async fn category_named(&self, name: &str) -> StoreResult<Category> {
        let name = single_line(name).trim().to_string();
        if name.is_empty() {
            return Err(StoreError::Invalid("Give the category a name.".into()));
        }
        let found: Option<String> = sqlx::query_scalar("SELECT id FROM categories WHERE lower(name) = lower(?) ORDER BY position LIMIT 1")
            .bind(&name)
            .fetch_optional(self.pool())
            .await?;
        match found {
            Some(id) => self.get_category(codec::parse_id(&id)?).await,
            None => self.create_category(&name).await,
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

async fn insert_transcript(tx: &mut sqlx::Transaction<'static, sqlx::Sqlite>, note_id: Id, w: &ImportWrite) -> StoreResult<()> {
    let mut speaker_ids = Vec::with_capacity(w.speakers.len());
    let mut next_color = 2u8;
    for speaker in &w.speakers {
        let id = new_id();
        let color = if speaker.is_self {
            1
        } else {
            let c = next_color;
            next_color = if next_color >= COLOR_COUNT { 2 } else { next_color + 1 };
            c
        };
        sqlx::query("INSERT INTO speakers (id, note_id, label, color_index, is_self) VALUES (?, ?, ?, ?, ?)")
            .bind(codec::id(id))
            .bind(codec::id(note_id))
            .bind(single_line(&speaker.label).trim())
            .bind(i64::from(color))
            .bind(speaker.is_self)
            .execute(&mut **tx)
            .await?;
        speaker_ids.push(id);
    }
    for segment in &w.segments {
        let speaker_id = segment.speaker.and_then(|i| speaker_ids.get(i).copied());
        sqlx::query("INSERT INTO segments (id, note_id, speaker_id, channel, start_ms, end_ms, text, is_final) VALUES (?, ?, ?, ?, ?, ?, ?, 1)")
            .bind(codec::id(new_id()))
            .bind(codec::id(note_id))
            .bind(codec::opt_id(speaker_id))
            .bind(match segment.channel {
                AudioChannel::Mic => "mic",
                AudioChannel::System => "system",
            })
            .bind(i64::from(segment.start_ms))
            .bind(i64::from(segment.end_ms.max(segment.start_ms)))
            .bind(&segment.text)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

fn transcript_text(w: &ImportWrite) -> String {
    w.segments.iter().map(|s| index_text(s.text.trim())).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n")
}

async fn insert_version(tx: &mut sqlx::Transaction<'static, sqlx::Sqlite>, note_id: Id, title: &str, body_md: &str) -> StoreResult<()> {
    sqlx::query("INSERT INTO note_versions (id, note_id, title, body_md, author, ai_action_id, created_at) VALUES (?, ?, ?, ?, 'user', NULL, ?)")
        .bind(codec::id(new_id()))
        .bind(codec::id(note_id))
        .bind(title)
        .bind(body_md)
        .bind(codec::ts(now()))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use margin_domain::chrono::TimeZone;
    use margin_domain::chrono::Utc;

    use super::*;

    fn write(external_id: &str, body: &str) -> ImportWrite {
        ImportWrite {
            provider: "test".into(),
            external_id: external_id.into(),
            title: format!("Note {external_id}"),
            body_md: body.into(),
            kind: NoteKind::Meeting,
            meeting_app: Some("Zoom".into()),
            started_at: Some(Utc.with_ymd_and_hms(2025, 3, 4, 9, 0, 0).unwrap()),
            ended_at: None,
            duration_ms: Some(60_000),
            created_at: Utc.with_ymd_and_hms(2025, 3, 4, 9, 0, 0).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2025, 3, 4, 10, 0, 0).unwrap(),
            category_id: None,
            timed: false,
            speakers: vec![ImportSpeaker { label: "You".into(), is_self: true }, ImportSpeaker { label: "Sam".into(), is_self: false }],
            segments: vec![
                ImportSegment { speaker: Some(0), channel: AudioChannel::Mic, start_ms: 0, end_ms: 1000, text: "Hello there".into() },
                ImportSegment { speaker: Some(1), channel: AudioChannel::System, start_ms: 1000, end_ms: 2000, text: "Budget is approved".into() },
            ],
        }
    }

    #[tokio::test]
    async fn imported_notes_keep_their_dates_transcript_and_source() {
        let db = Db::open_in_memory().await.unwrap();
        let id = db.insert_imported_note(&write("a", "Summary")).await.unwrap();
        let note = db.get_note(id).await.unwrap();
        assert_eq!(note.created_at, Utc.with_ymd_and_hms(2025, 3, 4, 9, 0, 0).unwrap());
        assert_eq!(note.kind, NoteKind::Meeting);
        let transcript = db.get_transcript(id).await.unwrap();
        assert_eq!(transcript.segments.len(), 2);
        assert!(!transcript.timed);
        assert!(transcript.speakers.iter().any(|s| s.is_self && s.label == "You"));
        let hits = db.search_notes("budget", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(db.list_note_versions(id).await.unwrap().len(), 1);
        assert!(db.live_import_ids("test").await.unwrap().contains("a"));
        assert!(db.live_import_ids("other").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn notes_in_trash_or_deleted_are_not_live_sources() {
        let db = Db::open_in_memory().await.unwrap();
        let a = db.insert_imported_note(&write("a", "A")).await.unwrap();
        let b = db.insert_imported_note(&write("b", "B")).await.unwrap();
        db.insert_imported_note(&write("c", "C")).await.unwrap();
        db.trash_note(a).await.unwrap();
        db.trash_note(b).await.unwrap();
        db.delete_note_forever(b).await.unwrap();
        let live = db.live_import_ids("test").await.unwrap();
        assert_eq!(live, HashSet::from(["c".to_string()]));
    }

    #[tokio::test]
    async fn categories_are_found_by_name_whatever_the_case() {
        let db = Db::open_in_memory().await.unwrap();
        let made = db.category_named("From Obsidian").await.unwrap();
        let again = db.category_named("from obsidian").await.unwrap();
        assert_eq!(made.id, again.id);
        assert_eq!(db.list_categories().await.unwrap().len(), 1);
    }
}
