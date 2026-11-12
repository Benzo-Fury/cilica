//! import_runs, import_links: notes brought in from other apps (docs/import.md). Owner: Import.
//!
//! An imported note is written in one transaction with its speakers, segments, search row, first
//! version and link, so a cancelled or crashed import never leaves half a note behind. The link
//! remembers what the source looked like (`source_hash`) and what Margin wrote (`note_hash`), so a
//! second import of the same source skips what is unchanged and never overwrites the person's edits.
//! Hashing is the caller's job; this module only stores the strings.

use margin_domain::{AudioChannel, Category, Id, ImportRun, ImportRunStatus, NoteKind, Timestamp, new_id, now};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::notes::{index_text, kind_to_sql, markdown_to_plain, single_line, snippet_from_markdown, top_position};
use crate::{Db, StoreError, StoreResult, codec};

/// Speaker colours rotate through the `speaker-N` tokens.
const COLOR_COUNT: u8 = 8;

/// One imported note, ready to store.
#[derive(Debug, Clone)]
pub struct ImportWrite {
    pub provider: String,
    pub external_id: String,
    pub source_hash: String,
    pub note_hash: String,
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

/// What Margin already holds for one source item.
#[derive(Debug, Clone)]
pub struct ImportLink {
    pub note_id: Id,
    pub run_id: Id,
    pub source_hash: String,
    pub note_hash: String,
    /// The note as it is now, so the caller can tell whether the person edited it.
    pub title: String,
    pub body_md: String,
    pub in_trash: bool,
}

impl Db {
    // ── Links ────────────────────────────────────────────────────────────

    /// The note an earlier import made from this item, if it still exists.
    pub async fn import_link(&self, provider: &str, external_id: &str) -> StoreResult<Option<ImportLink>> {
        let row = sqlx::query(
            "SELECT l.note_id, l.run_id, l.source_hash, l.note_hash, n.title, n.body_md, n.deleted_at \
             FROM import_links l JOIN notes n ON n.id = l.note_id WHERE l.provider = ? AND l.external_id = ?",
        )
        .bind(provider)
        .bind(external_id)
        .fetch_optional(self.pool())
        .await?;
        row.map(|r| {
            let deleted: Option<String> = r.try_get("deleted_at")?;
            Ok(ImportLink {
                note_id: codec::parse_id(&r.try_get::<String, _>("note_id")?)?,
                run_id: codec::parse_id(&r.try_get::<String, _>("run_id")?)?,
                source_hash: r.try_get("source_hash")?,
                note_hash: r.try_get("note_hash")?,
                title: r.try_get("title")?,
                body_md: r.try_get("body_md")?,
                in_trash: deleted.is_some(),
            })
        })
        .transpose()
    }

    /// Every external id already imported from a provider, with its source hash.
    pub async fn import_links_for(&self, provider: &str) -> StoreResult<Vec<(String, String)>> {
        let rows = sqlx::query("SELECT external_id, source_hash FROM import_links WHERE provider = ?").bind(provider).fetch_all(self.pool()).await?;
        rows.iter().map(|r| Ok((r.try_get("external_id")?, r.try_get("source_hash")?))).collect()
    }

    /// The provider a note was imported from, and when.
    pub async fn imported_from(&self, note_id: Id) -> StoreResult<Option<(String, Timestamp)>> {
        let row = sqlx::query("SELECT provider, imported_at FROM import_links WHERE note_id = ? ORDER BY imported_at DESC LIMIT 1")
            .bind(codec::id(note_id))
            .fetch_optional(self.pool())
            .await?;
        row.map(|r| Ok((r.try_get("provider")?, codec::parse_ts(&r.try_get::<String, _>("imported_at")?)?))).transpose()
    }

    // ── Writing notes ────────────────────────────────────────────────────

    /// Store a new imported note at the top of its category, with its transcript, search row,
    /// first version and link. Returns the note id.
    pub async fn insert_imported_note(&self, run_id: Id, w: &ImportWrite) -> StoreResult<Id> {
        let mut tx = self.begin_write().await?;
        let id = new_id();
        let title = single_line(&w.title).trim().to_string();
        let position = top_position(&mut *tx, w.category_id).await?;
        sqlx::query(
            "INSERT INTO notes (id, title, body_md, snippet, category_id, kind, meeting_app, started_at, ended_at, position, \
             duration_ms, transcript_timed, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        sqlx::query(
            "INSERT INTO import_links (provider, external_id, note_id, run_id, source_hash, note_hash, imported_at) VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(provider, external_id) DO UPDATE SET note_id = excluded.note_id, run_id = excluded.run_id, \
             source_hash = excluded.source_hash, note_hash = excluded.note_hash, imported_at = excluded.imported_at",
        )
        .bind(&w.provider)
        .bind(&w.external_id)
        .bind(codec::id(id))
        .bind(codec::id(run_id))
        .bind(&w.source_hash)
        .bind(&w.note_hash)
        .bind(codec::ts(now()))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Replace an imported note with a newer copy from its source. The current body is kept as a
    /// version first, so History can bring it back. Category and position stay where the person put them.
    pub async fn update_imported_note(&self, note_id: Id, w: &ImportWrite) -> StoreResult<()> {
        let mut tx = self.begin_write().await?;
        let old: Option<(String, String)> =
            sqlx::query_as("SELECT title, body_md FROM notes WHERE id = ?").bind(codec::id(note_id)).fetch_optional(&mut *tx).await?;
        let (old_title, old_body) = old.ok_or_else(|| StoreError::NotFound("The imported note".into()))?;
        insert_version(&mut tx, note_id, &old_title, &old_body).await?;
        let title = single_line(&w.title).trim().to_string();
        sqlx::query(
            "UPDATE notes SET title = ?, body_md = ?, snippet = ?, kind = ?, meeting_app = ?, started_at = ?, ended_at = ?, \
             duration_ms = ?, transcript_timed = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&title)
        .bind(&w.body_md)
        .bind(snippet_from_markdown(&w.body_md))
        .bind(kind_to_sql(w.kind))
        .bind(&w.meeting_app)
        .bind(codec::opt_ts(w.started_at))
        .bind(codec::opt_ts(w.ended_at))
        .bind(w.duration_ms.map(i64::from))
        .bind(w.timed)
        .bind(codec::ts(now()))
        .bind(codec::id(note_id))
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM segments WHERE note_id = ?").bind(codec::id(note_id)).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM speakers WHERE note_id = ?").bind(codec::id(note_id)).execute(&mut *tx).await?;
        insert_transcript(&mut tx, note_id, w).await?;
        sqlx::query("DELETE FROM notes_fts WHERE note_id = ?").bind(codec::id(note_id)).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO notes_fts (note_id, title, body_md, transcript) VALUES (?, ?, ?, ?)")
            .bind(codec::id(note_id))
            .bind(index_text(&title))
            .bind(markdown_to_plain(&w.body_md))
            .bind(transcript_text(w))
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE import_links SET source_hash = ?, note_hash = ?, imported_at = ? WHERE provider = ? AND external_id = ?")
            .bind(&w.source_hash)
            .bind(&w.note_hash)
            .bind(codec::ts(now()))
            .bind(&w.provider)
            .bind(&w.external_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
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

    // ── Runs ─────────────────────────────────────────────────────────────

    pub async fn create_import_run(&self, provider: &str, origin: &str) -> StoreResult<ImportRun> {
        let run = ImportRun {
            id: new_id(),
            provider: provider.to_string(),
            origin: origin.to_string(),
            status: ImportRunStatus::Running,
            imported: 0,
            updated: 0,
            skipped: 0,
            failed: 0,
            report: Vec::new(),
            started_at: now(),
            finished_at: None,
        };
        sqlx::query("INSERT INTO import_runs (id, provider, origin, status, started_at) VALUES (?, ?, ?, ?, ?)")
            .bind(codec::id(run.id))
            .bind(&run.provider)
            .bind(&run.origin)
            .bind(status_to_sql(run.status))
            .bind(codec::ts(run.started_at))
            .execute(self.pool())
            .await?;
        Ok(run)
    }

    /// Save a run's counts, report and status.
    pub async fn save_import_run(&self, run: &ImportRun) -> StoreResult<()> {
        sqlx::query(
            "UPDATE import_runs SET status = ?, imported = ?, updated = ?, skipped = ?, failed = ?, report_json = ?, finished_at = ? WHERE id = ?",
        )
        .bind(status_to_sql(run.status))
        .bind(i64::from(run.imported))
        .bind(i64::from(run.updated))
        .bind(i64::from(run.skipped))
        .bind(i64::from(run.failed))
        .bind(serde_json::to_string(&run.report)?)
        .bind(codec::opt_ts(run.finished_at))
        .bind(codec::id(run.id))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_import_run(&self, id: Id) -> StoreResult<ImportRun> {
        let row = sqlx::query("SELECT * FROM import_runs WHERE id = ?")
            .bind(codec::id(id))
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("That import".into()))?;
        run_from_row(&row)
    }

    /// Newest first.
    pub async fn list_import_runs(&self, limit: u32) -> StoreResult<Vec<ImportRun>> {
        let rows = sqlx::query("SELECT * FROM import_runs ORDER BY started_at DESC LIMIT ?").bind(i64::from(limit)).fetch_all(self.pool()).await?;
        rows.iter().map(run_from_row).collect()
    }

    /// A run left `running` by a crash or quit is marked cancelled at the next launch.
    pub async fn close_stale_import_runs(&self) -> StoreResult<()> {
        sqlx::query("UPDATE import_runs SET status = 'cancelled', finished_at = ? WHERE status = 'running'")
            .bind(codec::ts(now()))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Notes a run created that are not in Trash.
    pub async fn import_run_note_ids(&self, run_id: Id) -> StoreResult<Vec<Id>> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT l.note_id FROM import_links l JOIN notes n ON n.id = l.note_id WHERE l.run_id = ? AND n.deleted_at IS NULL",
        )
        .bind(codec::id(run_id))
        .fetch_all(self.pool())
        .await?;
        ids.iter().map(|s| codec::parse_id(s)).collect()
    }

    /// Move every note a run created to Trash and mark the run undone. Returns how many moved.
    /// The links stay, so restoring from Trash keeps the note linked to its source.
    pub async fn undo_import_run(&self, run_id: Id) -> StoreResult<u32> {
        let mut tx = self.begin_write().await?;
        let at = codec::ts(now());
        let done = sqlx::query(
            "UPDATE notes SET deleted_at = ? WHERE deleted_at IS NULL AND id IN (SELECT note_id FROM import_links WHERE run_id = ?)",
        )
        .bind(&at)
        .bind(codec::id(run_id))
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE import_runs SET status = 'undone' WHERE id = ?").bind(codec::id(run_id)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(done.rows_affected() as u32)
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

fn status_to_sql(status: ImportRunStatus) -> &'static str {
    match status {
        ImportRunStatus::Running => "running",
        ImportRunStatus::Succeeded => "succeeded",
        ImportRunStatus::Partial => "partial",
        ImportRunStatus::Failed => "failed",
        ImportRunStatus::Cancelled => "cancelled",
        ImportRunStatus::Undone => "undone",
    }
}

fn status_from_sql(status: &str) -> ImportRunStatus {
    match status {
        "running" => ImportRunStatus::Running,
        "succeeded" => ImportRunStatus::Succeeded,
        "partial" => ImportRunStatus::Partial,
        "failed" => ImportRunStatus::Failed,
        "undone" => ImportRunStatus::Undone,
        _ => ImportRunStatus::Cancelled,
    }
}

fn run_from_row(r: &SqliteRow) -> StoreResult<ImportRun> {
    let count = |name: &str| -> StoreResult<u32> { Ok(r.try_get::<i64, _>(name)?.clamp(0, u32::MAX as i64) as u32) };
    let report: String = r.try_get("report_json")?;
    Ok(ImportRun {
        id: codec::parse_id(&r.try_get::<String, _>("id")?)?,
        provider: r.try_get("provider")?,
        origin: r.try_get("origin")?,
        status: status_from_sql(&r.try_get::<String, _>("status")?),
        imported: count("imported")?,
        updated: count("updated")?,
        skipped: count("skipped")?,
        failed: count("failed")?,
        report: serde_json::from_str(&report).unwrap_or_default(),
        started_at: codec::parse_ts(&r.try_get::<String, _>("started_at")?)?,
        finished_at: codec::parse_opt_ts(r.try_get("finished_at")?)?,
    })
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
            source_hash: format!("src-{body}"),
            note_hash: format!("note-{body}"),
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
    async fn imported_notes_keep_their_dates_transcript_and_link() {
        let db = Db::open_in_memory().await.unwrap();
        let run = db.create_import_run("test", "fixtures").await.unwrap();
        let id = db.insert_imported_note(run.id, &write("a", "Summary")).await.unwrap();
        let note = db.get_note(id).await.unwrap();
        assert_eq!(note.created_at, Utc.with_ymd_and_hms(2025, 3, 4, 9, 0, 0).unwrap());
        assert_eq!(note.kind, NoteKind::Meeting);
        let transcript = db.get_transcript(id).await.unwrap();
        assert_eq!(transcript.segments.len(), 2);
        assert!(!transcript.timed);
        assert!(transcript.speakers.iter().any(|s| s.is_self && s.label == "You"));
        let hits = db.search_notes("budget", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        let link = db.import_link("test", "a").await.unwrap().unwrap();
        assert_eq!(link.note_id, id);
        assert_eq!(db.list_note_versions(id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn updating_keeps_the_old_body_as_a_version() {
        let db = Db::open_in_memory().await.unwrap();
        let run = db.create_import_run("test", "fixtures").await.unwrap();
        let id = db.insert_imported_note(run.id, &write("a", "First")).await.unwrap();
        db.update_imported_note(id, &write("a", "Second")).await.unwrap();
        assert_eq!(db.get_note(id).await.unwrap().body_md, "Second");
        assert_eq!(db.get_transcript(id).await.unwrap().segments.len(), 2);
        let versions = db.list_note_versions(id).await.unwrap();
        assert!(versions.iter().any(|v| v.body_md == "First"));
        assert_eq!(db.import_link("test", "a").await.unwrap().unwrap().source_hash, "src-Second");
    }

    #[tokio::test]
    async fn undo_moves_only_that_runs_notes_to_trash() {
        let db = Db::open_in_memory().await.unwrap();
        let first = db.create_import_run("test", "one").await.unwrap();
        let second = db.create_import_run("test", "two").await.unwrap();
        let a = db.insert_imported_note(first.id, &write("a", "A")).await.unwrap();
        let b = db.insert_imported_note(second.id, &write("b", "B")).await.unwrap();
        assert_eq!(db.undo_import_run(second.id).await.unwrap(), 1);
        assert!(db.get_note(a).await.unwrap().deleted_at.is_none());
        assert!(db.get_note(b).await.unwrap().deleted_at.is_some());
        assert_eq!(db.get_import_run(second.id).await.unwrap().status, ImportRunStatus::Undone);
        assert!(db.import_link("test", "b").await.unwrap().unwrap().in_trash);
    }

    #[tokio::test]
    async fn categories_are_found_by_name_whatever_the_case() {
        let db = Db::open_in_memory().await.unwrap();
        let made = db.category_named("From Obsidian").await.unwrap();
        let again = db.category_named("from obsidian").await.unwrap();
        assert_eq!(made.id, again.id);
        assert_eq!(db.list_categories().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn runs_round_trip() {
        let db = Db::open_in_memory().await.unwrap();
        let mut run = db.create_import_run("obsidian", "Vault").await.unwrap();
        run.status = ImportRunStatus::Partial;
        run.imported = 3;
        run.report = vec!["One file could not be read.".into()];
        run.finished_at = Some(now());
        db.save_import_run(&run).await.unwrap();
        let back = db.list_import_runs(10).await.unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].imported, 3);
        assert_eq!(back[0].report, run.report);
    }
}
