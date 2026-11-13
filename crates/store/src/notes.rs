//! notes, categories, note_versions, notes_fts. Owner: Core agent.
//!
//! The signatures marked "contract" are the cross-area contract (the agent and the recording
//! pipeline call them). Do not change them; add methods freely.
//!
//! Conventions:
//! - Ordering is fractional: lower `position` first. New notes go to the top of their category,
//!   new categories go last. When two neighbours get closer than [`MIN_GAP`] the container is
//!   renumbered 1, 2, 3… (the `*_rebalanced` variants report when that happened).
//! - `updated_at` moves only when the title or body really changes.
//! - The FTS row of a note is kept in sync here, in the same transaction as the write.

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use cilica_domain::chrono::TimeDelta;
use cilica_domain::{Author, Category, Id, Note, NoteKind, NoteQuery, NoteSummary, NoteVersion, SearchHit, Timestamp, new_id, now};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, Sqlite, SqliteExecutor, Transaction};

use crate::{Db, StoreError, StoreResult, codec};

/// Fields for a new note. Everything else gets a default.
#[derive(Debug, Clone)]
pub struct NewNote {
    pub title: String,
    pub body_md: String,
    pub category_id: Option<Id>,
    pub kind: NoteKind,
    pub meeting_app: Option<String>,
    pub started_at: Option<Timestamp>,
}

impl Default for NewNote {
    fn default() -> Self {
        Self { title: String::new(), body_md: String::new(), category_id: None, kind: NoteKind::Manual, meeting_app: None, started_at: None }
    }
}

// ── Policy ─────────────────────────────────────────────────────────────────

/// A user edit snapshots the previous body when the newest version is older than this.
pub const USER_SNAPSHOT_INTERVAL: TimeDelta = TimeDelta::minutes(5);
/// Versions kept per note, newest first. AI-authored versions younger than [`AI_VERSION_KEEP`]
/// are kept on top of this and never count against it.
pub const MAX_VERSIONS_PER_NOTE: i64 = 50;
/// AI-authored versions are never pruned while younger than this (turn undo relies on them).
pub const AI_VERSION_KEEP: TimeDelta = TimeDelta::days(30);
/// Sidebar snippets are cut to about this many characters.
pub const SNIPPET_MAX_CHARS: usize = 120;
/// Two siblings closer than this trigger a renumber of their container.
pub const MIN_GAP: f64 = 1e-6;
/// The most search results one call returns.
pub const MAX_SEARCH_RESULTS: u32 = 200;

/// Private-use characters that mark matches inside FTS snippets before HTML escaping.
pub(crate) const MARK_START: char = '\u{E000}';
const MARK_END: char = '\u{E001}';

macro_rules! note_columns {
    () => {
        "id, title, body_md, snippet, category_id, kind, meeting_app, started_at, ended_at, position, created_at, updated_at, deleted_at"
    };
}
macro_rules! summary_columns {
    () => {
        "id, title, snippet, category_id, kind, meeting_app, position, created_at, updated_at, deleted_at"
    };
}
macro_rules! version_columns {
    () => {
        "id, note_id, title, body_md, author, ai_action_id, created_at"
    };
}
macro_rules! category_columns {
    () => {
        "id, name, position, collapsed"
    };
}

impl Db {
    // ── Notes ────────────────────────────────────────────────────────────

    /// Notes not in Trash. Contract.
    pub async fn list_notes(&self) -> StoreResult<Vec<NoteSummary>> {
        let rows = sqlx::query(concat!("SELECT ", summary_columns!(), " FROM notes WHERE deleted_at IS NULL ORDER BY position ASC, created_at DESC"))
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(summary_from_row).collect()
    }

    /// Notes in Trash, most recently deleted first. Contract.
    pub async fn list_trash(&self) -> StoreResult<Vec<NoteSummary>> {
        let rows = sqlx::query(concat!("SELECT ", summary_columns!(), " FROM notes WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC"))
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(summary_from_row).collect()
    }

    /// One note, whether or not it is in Trash. Contract.
    pub async fn get_note(&self, id: Id) -> StoreResult<Note> {
        fetch_note(self.pool(), id).await
    }

    /// One note without its body.
    pub async fn get_note_summary(&self, id: Id) -> StoreResult<NoteSummary> {
        fetch_summary(self.pool(), id).await
    }

    /// Insert a note at the top of its category. Contract.
    pub async fn create_note(&self, new: NewNote) -> StoreResult<Note> {
        let mut tx = self.begin_write().await?;
        if let Some(category_id) = new.category_id {
            ensure_category(&mut *tx, category_id).await?;
        }
        let position = top_position(&mut *tx, new.category_id).await?;
        let id = new_id();
        let at = codec::ts(now());
        let title = single_line(&new.title);
        let snippet = snippet_from_markdown(&new.body_md);
        sqlx::query(
            "INSERT INTO notes (id, title, body_md, snippet, category_id, kind, meeting_app, started_at, position, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(codec::id(id))
        .bind(&title)
        .bind(&new.body_md)
        .bind(&snippet)
        .bind(codec::opt_id(new.category_id))
        .bind(kind_to_sql(new.kind))
        .bind(&new.meeting_app)
        .bind(codec::opt_ts(new.started_at))
        .bind(position)
        .bind(&at)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO notes_fts (note_id, title, body_md, transcript) VALUES (?, ?, ?, '')")
            .bind(codec::id(id))
            .bind(index_text(&title))
            .bind(markdown_to_plain(&new.body_md))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.get_note(id).await
    }

    /// Contract. Newlines are folded into spaces; renaming to the same title changes nothing.
    pub async fn rename_note(&self, id: Id, title: &str) -> StoreResult<NoteSummary> {
        let title = single_line(title);
        let mut tx = self.begin_write().await?;
        let current = fetch_note(&mut *tx, id).await?;
        if current.title != title {
            sqlx::query("UPDATE notes SET title = ?, updated_at = ? WHERE id = ?")
                .bind(&title)
                .bind(codec::ts(now()))
                .bind(codec::id(id))
                .execute(&mut *tx)
                .await?;
            fts_set_text(&mut tx, id, &title, &current.body_md).await?;
        }
        let summary = fetch_summary(&mut *tx, id).await?;
        tx.commit().await?;
        Ok(summary)
    }

    /// Replace the body. Snapshots the previous body into `note_versions` first: always when
    /// `author` is `Ai` (tagged with `ai_action_id`), and for `User` when the last snapshot is
    /// older than a few minutes (so typing does not create a version per keystroke).
    /// Refreshes the snippet for manual notes and the FTS row. Contract.
    ///
    /// Also: a `User` write snapshots when the body changed a lot in one go (a paste or a
    /// select-all delete); an unchanged body is a no-op; a meeting note's snippet is only filled
    /// from the body while it is still empty (the AI writes the real one).
    pub async fn set_note_body(&self, id: Id, body_md: &str, author: Author, ai_action_id: Option<Id>) -> StoreResult<NoteSummary> {
        let mut tx = self.begin_write().await?;
        let current = fetch_note(&mut *tx, id).await?;
        if current.body_md == body_md {
            let summary = fetch_summary(&mut *tx, id).await?;
            tx.commit().await?;
            return Ok(summary);
        }
        let at = now();

        // Snapshot the body as it was before this write.
        let snapshot = match author {
            Author::Ai => true,
            Author::User => {
                !current.body_md.trim().is_empty()
                    && (latest_version_at(&mut *tx, id).await?.is_none_or(|t| at - t >= USER_SNAPSHOT_INTERVAL)
                        || changed_a_lot(&current.body_md, body_md))
            }
        };
        if snapshot {
            insert_version(&mut tx, &current, author, ai_action_id, at).await?;
            prune_versions(&mut *tx, id, at).await?;
        }

        // Write the body, the snippet and the search row.
        let snippet = match current.kind {
            NoteKind::Manual => snippet_from_markdown(body_md),
            NoteKind::Meeting if current.snippet.trim().is_empty() => snippet_from_markdown(body_md),
            NoteKind::Meeting => current.snippet.clone(),
        };
        sqlx::query("UPDATE notes SET body_md = ?, snippet = ?, updated_at = ? WHERE id = ?")
            .bind(body_md)
            .bind(&snippet)
            .bind(codec::ts(at))
            .bind(codec::id(id))
            .execute(&mut *tx)
            .await?;
        fts_set_text(&mut tx, id, &current.title, body_md).await?;
        let summary = fetch_summary(&mut *tx, id).await?;
        tx.commit().await?;
        Ok(summary)
    }

    /// Contract. Folded to one line; does not move `updated_at`.
    pub async fn set_note_snippet(&self, id: Id, snippet: &str) -> StoreResult<()> {
        let done = sqlx::query("UPDATE notes SET snippet = ? WHERE id = ?")
            .bind(single_line(snippet).trim())
            .bind(codec::id(id))
            .execute(self.pool())
            .await?;
        expect_one(done.rows_affected(), "That note")
    }

    /// Set when a meeting started and ended. Contract.
    pub async fn set_note_meeting_times(&self, id: Id, started_at: Option<Timestamp>, ended_at: Option<Timestamp>) -> StoreResult<()> {
        let done = sqlx::query("UPDATE notes SET started_at = ?, ended_at = ? WHERE id = ?")
            .bind(codec::opt_ts(started_at))
            .bind(codec::opt_ts(ended_at))
            .bind(codec::id(id))
            .execute(self.pool())
            .await?;
        expect_one(done.rows_affected(), "That note")
    }

    /// Record (or clear) where the note's audio lives, relative to the data dir, and its length. Contract.
    pub async fn set_note_audio(&self, id: Id, audio_path: Option<&str>, duration_ms: Option<u32>) -> StoreResult<()> {
        let done = sqlx::query("UPDATE notes SET audio_path = ?, duration_ms = ? WHERE id = ?")
            .bind(audio_path)
            .bind(duration_ms.map(i64::from))
            .bind(codec::id(id))
            .execute(self.pool())
            .await?;
        expect_one(done.rows_affected(), "That note")
    }

    /// `(audio_path, duration_ms)` for a note. Contract.
    pub async fn get_note_audio(&self, id: Id) -> StoreResult<(Option<String>, Option<u32>)> {
        let row = sqlx::query("SELECT audio_path, duration_ms FROM notes WHERE id = ?")
            .bind(codec::id(id))
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("That note".into()))?;
        let path: Option<String> = row.try_get("audio_path")?;
        let duration: Option<i64> = row.try_get("duration_ms")?;
        Ok((path, duration.map(|d| u32::try_from(d.max(0)).unwrap_or(u32::MAX))))
    }

    /// Contract. Does not move `updated_at` (moving is not editing).
    pub async fn move_note(&self, id: Id, category_id: Option<Id>, position: f64) -> StoreResult<NoteSummary> {
        Ok(self.move_note_rebalanced(id, category_id, position).await?.0)
    }

    /// [`Db::move_note`], also saying whether the target category was renumbered (so other notes'
    /// positions changed and lists should be refetched).
    pub async fn move_note_rebalanced(&self, id: Id, category_id: Option<Id>, position: f64) -> StoreResult<(NoteSummary, bool)> {
        check_position(position)?;
        let mut tx = self.begin_write().await?;
        fetch_summary(&mut *tx, id).await?;
        if let Some(category_id) = category_id {
            ensure_category(&mut *tx, category_id).await?;
        }
        sqlx::query("UPDATE notes SET category_id = ?, position = ? WHERE id = ?")
            .bind(codec::opt_id(category_id))
            .bind(position)
            .bind(codec::id(id))
            .execute(&mut *tx)
            .await?;
        let rebalanced = rebalance_notes_if_crowded(&mut tx, id, category_id, position).await?;
        let summary = fetch_summary(&mut *tx, id).await?;
        tx.commit().await?;
        Ok((summary, rebalanced))
    }

    /// Soft delete. Contract. Trashing a note that is already in Trash changes nothing.
    pub async fn trash_note(&self, id: Id) -> StoreResult<()> {
        let done = sqlx::query("UPDATE notes SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL")
            .bind(codec::ts(now()))
            .bind(codec::id(id))
            .execute(self.pool())
            .await?;
        if done.rows_affected() == 0 {
            fetch_summary(self.pool(), id).await?;
        }
        Ok(())
    }

    /// Contract. The note comes back at the top of its category (or of Notes, if its category was deleted).
    pub async fn restore_note(&self, id: Id) -> StoreResult<NoteSummary> {
        let mut tx = self.begin_write().await?;
        let current = fetch_summary(&mut *tx, id).await?;
        if current.deleted_at.is_some() {
            let position = top_position(&mut *tx, current.category_id).await?;
            sqlx::query("UPDATE notes SET deleted_at = NULL, position = ? WHERE id = ?").bind(position).bind(codec::id(id)).execute(&mut *tx).await?;
        }
        let summary = fetch_summary(&mut *tx, id).await?;
        tx.commit().await?;
        Ok(summary)
    }

    /// Contract. Also removes the note's versions, transcript, search row and recording file.
    pub async fn delete_note_forever(&self, id: Id) -> StoreResult<()> {
        fetch_summary(self.pool(), id).await?;
        self.delete_notes(&[id]).await?;
        Ok(())
    }

    // ── Several notes at once (sidebar and Trash selection) ─────────────
    //
    // Each runs in one transaction, so a failure leaves nothing half done. Ids that no longer exist
    // (removed by Cilica or another window since the selection was made) are skipped, not errors.

    /// Move live notes to Trash. Returns how many were moved; ones already in Trash are not counted.
    pub async fn trash_notes(&self, ids: &[Id]) -> StoreResult<u32> {
        let at = codec::ts(now());
        let mut moved = 0u32;
        let mut tx = self.begin_write().await?;
        for &id in ids {
            let done = sqlx::query("UPDATE notes SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL")
                .bind(&at)
                .bind(codec::id(id))
                .execute(&mut *tx)
                .await?;
            moved += u32::try_from(done.rows_affected()).unwrap_or(0);
        }
        tx.commit().await?;
        Ok(moved)
    }

    /// Bring notes back from Trash, each at the top of its category, keeping the order given
    /// (the first id ends up highest). Returns the restored notes in that order.
    pub async fn restore_notes(&self, ids: &[Id]) -> StoreResult<Vec<NoteSummary>> {
        let mut tx = self.begin_write().await?;
        let mut restored = Vec::new();
        // Last first, so each next one lands above it.
        for &id in ids.iter().rev() {
            let Some(current) = fetch_summary_opt(&mut *tx, id).await? else { continue };
            if current.deleted_at.is_none() {
                continue;
            }
            let position = top_position(&mut *tx, current.category_id).await?;
            sqlx::query("UPDATE notes SET deleted_at = NULL, position = ? WHERE id = ?")
                .bind(position)
                .bind(codec::id(id))
                .execute(&mut *tx)
                .await?;
            restored.push(fetch_summary(&mut *tx, id).await?);
        }
        tx.commit().await?;
        restored.reverse();
        Ok(restored)
    }

    /// Delete notes for good. Only notes already in Trash are removed; returns how many.
    pub async fn delete_notes_forever(&self, ids: &[Id]) -> StoreResult<u32> {
        let mut trashed = Vec::with_capacity(ids.len());
        for &id in ids {
            if fetch_summary_opt(self.pool(), id).await?.is_some_and(|n| n.deleted_at.is_some()) {
                trashed.push(id);
            }
        }
        self.delete_notes(&trashed).await
    }

    /// Move live notes into a category (or out, with `None`) as one block, in the order given.
    ///
    /// With `position`, the block starts there and fills the gap up to the next note that is not
    /// moving; without one it goes to the top. Does not move `updated_at`. Returns the moved notes
    /// and whether the category had to be renumbered (so other notes' positions changed too).
    pub async fn move_notes(&self, ids: &[Id], category_id: Option<Id>, position: Option<f64>) -> StoreResult<(Vec<NoteSummary>, bool)> {
        if let Some(position) = position {
            check_position(position)?;
        }
        let mut tx = self.begin_write().await?;
        if let Some(category_id) = category_id {
            ensure_category(&mut *tx, category_id).await?;
        }
        let mut moving: Vec<Id> = Vec::with_capacity(ids.len());
        for &id in ids {
            if moving.contains(&id) {
                continue;
            }
            if fetch_summary_opt(&mut *tx, id).await?.is_some_and(|n| n.deleted_at.is_none()) {
                moving.push(id);
            }
        }
        if moving.is_empty() {
            tx.commit().await?;
            return Ok((Vec::new(), false));
        }

        // Positions of the notes staying where they are in the target.
        let staying = sqlx::query_as::<_, (String, Option<f64>)>("SELECT id, position FROM notes WHERE category_id IS ? AND deleted_at IS NULL")
            .bind(codec::opt_id(category_id))
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .filter(|(id, _)| !moving.iter().any(|m| codec::id(*m) == *id))
            .filter_map(|(_, p)| p)
            .collect::<Vec<f64>>();
        let positions = block_positions(&staying, position, moving.len());

        for (&id, &p) in moving.iter().zip(&positions) {
            sqlx::query("UPDATE notes SET category_id = ?, position = ? WHERE id = ?")
                .bind(codec::opt_id(category_id))
                .bind(p)
                .bind(codec::id(id))
                .execute(&mut *tx)
                .await?;
        }

        let rebalanced = rebalance_category_if_crowded(&mut tx, category_id).await?;
        let mut moved = Vec::with_capacity(moving.len());
        for &id in &moving {
            moved.push(fetch_summary(&mut *tx, id).await?);
        }
        tx.commit().await?;
        Ok((moved, rebalanced))
    }

    /// Remove a note nothing was ever put in: no title, no body, no recording, no transcript.
    /// Returns whether it was removed. Anything else is left alone.
    pub async fn discard_empty_note(&self, id: Id) -> StoreResult<bool> {
        let mut tx = self.begin_write().await?;
        let removed = sqlx::query(
            "DELETE FROM notes WHERE id = ? AND kind = 'manual' AND deleted_at IS NULL AND audio_path IS NULL \
             AND trim(title) = '' AND trim(body_md, ' ' || char(9) || char(10) || char(13)) = '' \
             AND NOT EXISTS (SELECT 1 FROM segments WHERE segments.note_id = notes.id)",
        )
        .bind(codec::id(id))
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;
        if removed {
            sqlx::query("DELETE FROM notes_fts WHERE note_id = ?").bind(codec::id(id)).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(removed)
    }

    /// Returns how many notes were removed. Contract.
    pub async fn empty_trash(&self) -> StoreResult<u32> {
        let ids = sqlx::query_scalar::<_, String>("SELECT id FROM notes WHERE deleted_at IS NOT NULL").fetch_all(self.pool()).await?;
        let ids = ids.iter().map(|s| codec::parse_id(s)).collect::<StoreResult<Vec<_>>>()?;
        self.delete_notes(&ids).await
    }

    /// Remove notes that have been in Trash longer than `days`. Returns how many. Contract.
    pub async fn purge_trash_older_than(&self, days: u32) -> StoreResult<u32> {
        let cutoff = now() - TimeDelta::days(i64::from(days));
        let ids = sqlx::query_scalar::<_, String>("SELECT id FROM notes WHERE deleted_at IS NOT NULL AND deleted_at < ?")
            .bind(codec::ts(cutoff))
            .fetch_all(self.pool())
            .await?;
        let ids = ids.iter().map(|s| codec::parse_id(s)).collect::<StoreResult<Vec<_>>>()?;
        self.delete_notes(&ids).await
    }

    /// Hard-delete notes in one transaction, then remove their recordings from disk.
    async fn delete_notes(&self, ids: &[Id]) -> StoreResult<u32> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut audio_paths = Vec::new();
        let mut removed = 0u32;
        let mut tx = self.begin_write().await?;
        for &id in ids {
            let audio: Option<Option<String>> =
                sqlx::query_scalar("SELECT audio_path FROM notes WHERE id = ?").bind(codec::id(id)).fetch_optional(&mut *tx).await?;
            let Some(audio) = audio else { continue };
            sqlx::query("DELETE FROM notes_fts WHERE note_id = ?").bind(codec::id(id)).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM notes WHERE id = ?").bind(codec::id(id)).execute(&mut *tx).await?;
            audio_paths.extend(audio);
            removed += 1;
        }
        tx.commit().await?;
        for path in audio_paths {
            self.remove_recording(&path).await;
        }
        Ok(removed)
    }

    /// Delete a recording (file or folder) under the data dir. Never touches anything outside it.
    async fn remove_recording(&self, stored: &str) {
        let Some(path) = self.data_dir().and_then(|root| resolve_inside(root, stored)) else {
            if self.data_dir().is_some() {
                tracing::warn!(path = stored, "not removing a recording path outside the data folder");
            }
            return;
        };
        let result = match tokio::fs::metadata(&path).await {
            Ok(meta) if meta.is_dir() => tokio::fs::remove_dir_all(&path).await,
            Ok(_) => tokio::fs::remove_file(&path).await,
            Err(e) => Err(e),
        };
        match result {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "could not remove a recording")
            }
        }
    }

    // ── Categories ───────────────────────────────────────────────────────

    /// Contract.
    pub async fn list_categories(&self) -> StoreResult<Vec<Category>> {
        let rows =
            sqlx::query(concat!("SELECT ", category_columns!(), " FROM categories ORDER BY position ASC, name ASC")).fetch_all(self.pool()).await?;
        rows.iter().map(category_from_row).collect()
    }

    /// Contract.
    pub async fn get_category(&self, id: Id) -> StoreResult<Category> {
        fetch_category(self.pool(), id).await
    }

    /// Appended after the last category. Contract.
    pub async fn create_category(&self, name: &str) -> StoreResult<Category> {
        let name = category_name(name)?;
        let mut tx = self.begin_write().await?;
        let last: Option<f64> = sqlx::query_scalar("SELECT MAX(position) FROM categories").fetch_one(&mut *tx).await?;
        let id = new_id();
        sqlx::query("INSERT INTO categories (id, name, position, collapsed) VALUES (?, ?, ?, 0)")
            .bind(codec::id(id))
            .bind(&name)
            .bind(last.map_or(1.0, |p| p.floor() + 1.0))
            .execute(&mut *tx)
            .await?;
        let category = fetch_category(&mut *tx, id).await?;
        tx.commit().await?;
        Ok(category)
    }

    /// Contract.
    pub async fn rename_category(&self, id: Id, name: &str) -> StoreResult<Category> {
        let name = category_name(name)?;
        let done = sqlx::query("UPDATE categories SET name = ? WHERE id = ?").bind(&name).bind(codec::id(id)).execute(self.pool()).await?;
        expect_one(done.rows_affected(), "That category")?;
        self.get_category(id).await
    }

    /// Contract.
    pub async fn reorder_category(&self, id: Id, position: f64) -> StoreResult<Category> {
        Ok(self.reorder_category_rebalanced(id, position).await?.0)
    }

    /// [`Db::reorder_category`], also saying whether all categories were renumbered.
    pub async fn reorder_category_rebalanced(&self, id: Id, position: f64) -> StoreResult<(Category, bool)> {
        check_position(position)?;
        let mut tx = self.begin_write().await?;
        let done = sqlx::query("UPDATE categories SET position = ? WHERE id = ?").bind(position).bind(codec::id(id)).execute(&mut *tx).await?;
        expect_one(done.rows_affected(), "That category")?;
        let crowded: i64 = sqlx::query_scalar("SELECT count(*) FROM categories WHERE id != ? AND abs(position - ?) < ?")
            .bind(codec::id(id))
            .bind(position)
            .bind(MIN_GAP)
            .fetch_one(&mut *tx)
            .await?;
        if crowded > 0 {
            renumber(&mut tx, "SELECT id FROM categories ORDER BY position ASC, name ASC", None, "UPDATE categories SET position = ? WHERE id = ?")
                .await?;
        }
        let category = fetch_category(&mut *tx, id).await?;
        tx.commit().await?;
        Ok((category, crowded > 0))
    }

    /// Contract.
    pub async fn set_category_collapsed(&self, id: Id, collapsed: bool) -> StoreResult<Category> {
        let done = sqlx::query("UPDATE categories SET collapsed = ? WHERE id = ?").bind(collapsed).bind(codec::id(id)).execute(self.pool()).await?;
        expect_one(done.rows_affected(), "That category")?;
        self.get_category(id).await
    }

    /// Delete a category; its notes become uncategorised. Contract.
    pub async fn delete_category(&self, id: Id) -> StoreResult<()> {
        let mut tx = self.begin_write().await?;
        // Explicit rather than relying on ON DELETE SET NULL, so it holds even with foreign keys off.
        sqlx::query("UPDATE notes SET category_id = NULL WHERE category_id = ?").bind(codec::id(id)).execute(&mut *tx).await?;
        let done = sqlx::query("DELETE FROM categories WHERE id = ?").bind(codec::id(id)).execute(&mut *tx).await?;
        expect_one(done.rows_affected(), "That category")?;
        tx.commit().await?;
        Ok(())
    }

    // ── Versions ─────────────────────────────────────────────────────────

    /// Snapshot the note as it is now. Contract.
    pub async fn snapshot_note(&self, id: Id, author: Author, ai_action_id: Option<Id>) -> StoreResult<NoteVersion> {
        let mut tx = self.begin_write().await?;
        let note = fetch_note(&mut *tx, id).await?;
        let at = now();
        let version = insert_version(&mut tx, &note, author, ai_action_id, at).await?;
        prune_versions(&mut *tx, id, at).await?;
        tx.commit().await?;
        Ok(version)
    }

    /// Newest first. Contract.
    pub async fn list_note_versions(&self, note_id: Id) -> StoreResult<Vec<NoteVersion>> {
        let rows = sqlx::query(concat!("SELECT ", version_columns!(), " FROM note_versions WHERE note_id = ? ORDER BY created_at DESC, id DESC"))
            .bind(codec::id(note_id))
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(version_from_row).collect()
    }

    /// Contract.
    pub async fn get_note_version(&self, version_id: Id) -> StoreResult<NoteVersion> {
        fetch_version(self.pool(), version_id).await
    }

    /// Snapshot the current state (as `author`), then restore title and body from the version. Contract.
    pub async fn restore_note_version(&self, version_id: Id, author: Author) -> StoreResult<Note> {
        let mut tx = self.begin_write().await?;
        let version = fetch_version(&mut *tx, version_id).await?;
        let current = fetch_note(&mut *tx, version.note_id).await?;
        if current.title != version.title || current.body_md != version.body_md {
            let at = now();
            insert_version(&mut tx, &current, author, None, at).await?;
            let snippet = match current.kind {
                NoteKind::Manual => snippet_from_markdown(&version.body_md),
                NoteKind::Meeting => current.snippet.clone(),
            };
            sqlx::query("UPDATE notes SET title = ?, body_md = ?, snippet = ?, updated_at = ? WHERE id = ?")
                .bind(&version.title)
                .bind(&version.body_md)
                .bind(&snippet)
                .bind(codec::ts(at))
                .bind(codec::id(current.id))
                .execute(&mut *tx)
                .await?;
            fts_set_text(&mut tx, current.id, &version.title, &version.body_md).await?;
            prune_versions(&mut *tx, current.id, at).await?;
        }
        let note = fetch_note(&mut *tx, version.note_id).await?;
        tx.commit().await?;
        Ok(note)
    }

    // ── Search ───────────────────────────────────────────────────────────

    /// Full-text search over titles, bodies and transcripts of notes not in Trash. Contract.
    ///
    /// Any input is safe: it is split into words (punctuation, operators and emoji are dropped),
    /// each word is quoted, and the last one matches as a prefix so results appear while typing.
    /// Ranked by bm25 with the title weighted highest. `excerpt` is HTML-escaped text with the
    /// matches wrapped in `<mark>`; `source` is the first of title, body, transcript that matched.
    pub async fn search_notes(&self, query: &str, limit: u32) -> StoreResult<Vec<SearchHit>> {
        self.find_notes(&NoteQuery { text: query.to_string(), ..NoteQuery::default() }, limit).await
    }

    /// Start of the first final segment that contains the first highlighted term of a snippet.
    pub(crate) async fn segment_at_for_match(&self, note_id: Id, raw_snippet: &str) -> StoreResult<Option<u32>> {
        let Some(term) = first_marked_term(raw_snippet) else {
            return Ok(None);
        };
        let needle = term.to_lowercase();
        let rows = sqlx::query("SELECT start_ms, text FROM segments WHERE note_id = ? AND is_final = 1 ORDER BY start_ms ASC")
            .bind(codec::id(note_id))
            .fetch_all(self.pool())
            .await?;
        for row in rows {
            let text: String = row.try_get("text")?;
            if text.to_lowercase().contains(&needle) {
                let start: i64 = row.try_get("start_ms")?;
                return Ok(Some(u32::try_from(start.max(0)).unwrap_or(u32::MAX)));
            }
        }
        Ok(None)
    }

    /// Rebuild the FTS row for one note from its title, body and final transcript segments.
    /// The recording pipeline calls this after a meeting. Contract.
    pub async fn reindex_note(&self, id: Id) -> StoreResult<()> {
        let mut tx = self.begin_write().await?;
        let note = fetch_note(&mut *tx, id).await?;
        let transcript = transcript_index_text(&mut *tx, id).await?;
        sqlx::query("DELETE FROM notes_fts WHERE note_id = ?").bind(codec::id(id)).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO notes_fts (note_id, title, body_md, transcript) VALUES (?, ?, ?, ?)")
            .bind(codec::id(id))
            .bind(index_text(&note.title))
            .bind(markdown_to_plain(&note.body_md))
            .bind(transcript)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Add search rows for notes that have none (for example after a crash between writes) and
    /// drop rows whose note is gone. Returns how many notes were reindexed. Cheap when in sync.
    pub async fn repair_search_index(&self) -> StoreResult<u32> {
        sqlx::query("DELETE FROM notes_fts WHERE note_id NOT IN (SELECT id FROM notes)").execute(self.pool()).await?;
        let missing =
            sqlx::query_scalar::<_, String>("SELECT id FROM notes WHERE id NOT IN (SELECT note_id FROM notes_fts)").fetch_all(self.pool()).await?;
        for id in &missing {
            self.reindex_note(codec::parse_id(id)?).await?;
        }
        Ok(missing.len() as u32)
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// A write transaction that takes the write lock up front, so a read-then-write never fails
    /// with "database is locked" when another connection wrote in between.
    pub(crate) async fn begin_write(&self) -> StoreResult<Transaction<'static, Sqlite>> {
        Ok(self.pool().begin_with("BEGIN IMMEDIATE").await?)
    }
}

// ── Row access ─────────────────────────────────────────────────────────────

async fn fetch_note<'e>(ex: impl SqliteExecutor<'e>, id: Id) -> StoreResult<Note> {
    let row = sqlx::query(concat!("SELECT ", note_columns!(), " FROM notes WHERE id = ?"))
        .bind(codec::id(id))
        .fetch_optional(ex)
        .await?
        .ok_or_else(|| StoreError::NotFound("That note".into()))?;
    note_from_row(&row)
}

async fn fetch_summary<'e>(ex: impl SqliteExecutor<'e>, id: Id) -> StoreResult<NoteSummary> {
    let row = sqlx::query(concat!("SELECT ", summary_columns!(), " FROM notes WHERE id = ?"))
        .bind(codec::id(id))
        .fetch_optional(ex)
        .await?
        .ok_or_else(|| StoreError::NotFound("That note".into()))?;
    summary_from_row(&row)
}

async fn fetch_summary_opt<'e>(ex: impl SqliteExecutor<'e>, id: Id) -> StoreResult<Option<NoteSummary>> {
    let row = sqlx::query(concat!("SELECT ", summary_columns!(), " FROM notes WHERE id = ?")).bind(codec::id(id)).fetch_optional(ex).await?;
    row.as_ref().map(summary_from_row).transpose()
}

/// Positions for `count` notes placed as one block among `staying` (the target's other notes).
/// From `start` up to the next staying note above it, or above everything when `start` is `None`.
fn block_positions(staying: &[f64], start: Option<f64>, count: usize) -> Vec<f64> {
    let n = count as f64;
    match start {
        None => {
            let top = staying.iter().copied().fold(f64::INFINITY, f64::min);
            let top = if top.is_finite() { top.ceil() } else { n };
            (0..count).map(|i| top - n + i as f64).collect()
        }
        Some(start) => {
            let next = staying.iter().copied().filter(|p| *p > start).fold(f64::INFINITY, f64::min);
            let step = if next.is_finite() { (next - start) / n } else { 1.0 };
            (0..count).map(|i| start + step * i as f64).collect()
        }
    }
}

/// Renumber a category's live notes 1, 2, 3… when any two sit closer than [`MIN_GAP`].
async fn rebalance_category_if_crowded(tx: &mut Transaction<'static, Sqlite>, category_id: Option<Id>) -> StoreResult<bool> {
    let positions: Vec<Option<f64>> = sqlx::query_scalar("SELECT position FROM notes WHERE category_id IS ? AND deleted_at IS NULL ORDER BY position ASC")
        .bind(codec::opt_id(category_id))
        .fetch_all(&mut **tx)
        .await?;
    let positions: Vec<f64> = positions.into_iter().flatten().collect();
    if !positions.windows(2).any(|w| w[1] - w[0] < MIN_GAP) {
        return Ok(false);
    }
    renumber(
        tx,
        "SELECT id FROM notes WHERE category_id IS ? AND deleted_at IS NULL ORDER BY position ASC, created_at DESC",
        Some(codec::opt_id(category_id)),
        "UPDATE notes SET position = ? WHERE id = ?",
    )
    .await?;
    Ok(true)
}

async fn fetch_category<'e>(ex: impl SqliteExecutor<'e>, id: Id) -> StoreResult<Category> {
    let row = sqlx::query(concat!("SELECT ", category_columns!(), " FROM categories WHERE id = ?"))
        .bind(codec::id(id))
        .fetch_optional(ex)
        .await?
        .ok_or_else(|| StoreError::NotFound("That category".into()))?;
    category_from_row(&row)
}

async fn fetch_version<'e>(ex: impl SqliteExecutor<'e>, id: Id) -> StoreResult<NoteVersion> {
    let row = sqlx::query(concat!("SELECT ", version_columns!(), " FROM note_versions WHERE id = ?"))
        .bind(codec::id(id))
        .fetch_optional(ex)
        .await?
        .ok_or_else(|| StoreError::NotFound("That version".into()))?;
    version_from_row(&row)
}

async fn ensure_category<'e>(ex: impl SqliteExecutor<'e>, id: Id) -> StoreResult<()> {
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM categories WHERE id = ?").bind(codec::id(id)).fetch_optional(ex).await?;
    found.map(|_| ()).ok_or_else(|| StoreError::NotFound("That category".into()))
}

/// The position that puts a note above every other live note in the category.
pub(crate) async fn top_position<'e>(ex: impl SqliteExecutor<'e>, category_id: Option<Id>) -> StoreResult<f64> {
    let top: Option<f64> = sqlx::query_scalar("SELECT MIN(position) FROM notes WHERE category_id IS ? AND deleted_at IS NULL")
        .bind(codec::opt_id(category_id))
        .fetch_one(ex)
        .await?;
    Ok(top.map_or(0.0, |p| p.ceil() - 1.0))
}

async fn latest_version_at<'e>(ex: impl SqliteExecutor<'e>, note_id: Id) -> StoreResult<Option<Timestamp>> {
    let latest: Option<String> =
        sqlx::query_scalar("SELECT MAX(created_at) FROM note_versions WHERE note_id = ?").bind(codec::id(note_id)).fetch_one(ex).await?;
    codec::parse_opt_ts(latest)
}

async fn insert_version(
    tx: &mut Transaction<'static, Sqlite>,
    note: &Note,
    author: Author,
    ai_action_id: Option<Id>,
    at: Timestamp,
) -> StoreResult<NoteVersion> {
    let version = NoteVersion {
        id: new_id(),
        note_id: note.id,
        title: note.title.clone(),
        body_md: note.body_md.clone(),
        author,
        ai_action_id,
        created_at: at,
    };
    sqlx::query("INSERT INTO note_versions (id, note_id, title, body_md, author, ai_action_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(codec::id(version.id))
        .bind(codec::id(version.note_id))
        .bind(&version.title)
        .bind(&version.body_md)
        .bind(author_to_sql(author))
        .bind(codec::opt_id(ai_action_id))
        .bind(codec::ts(at))
        .execute(&mut **tx)
        .await?;
    Ok(version)
}

/// Keep the newest [`MAX_VERSIONS_PER_NOTE`] versions, plus every AI version younger than [`AI_VERSION_KEEP`].
async fn prune_versions<'e>(ex: impl SqliteExecutor<'e>, note_id: Id, at: Timestamp) -> StoreResult<()> {
    sqlx::query(
        "DELETE FROM note_versions WHERE id IN ( \
           SELECT id FROM ( \
             SELECT id, ROW_NUMBER() OVER (ORDER BY created_at DESC, id DESC) AS rn \
             FROM note_versions WHERE note_id = ?1 AND NOT (author = 'ai' AND created_at >= ?2) \
           ) WHERE rn > ?3 \
         )",
    )
    .bind(codec::id(note_id))
    .bind(codec::ts(at - AI_VERSION_KEEP))
    .bind(MAX_VERSIONS_PER_NOTE)
    .execute(ex)
    .await?;
    Ok(())
}

/// Update the title and body of a note's search row, creating the row if it is missing.
async fn fts_set_text(tx: &mut Transaction<'static, Sqlite>, id: Id, title: &str, body_md: &str) -> StoreResult<()> {
    let title = index_text(title);
    let body = markdown_to_plain(body_md);
    let done = sqlx::query("UPDATE notes_fts SET title = ?, body_md = ? WHERE note_id = ?")
        .bind(&title)
        .bind(&body)
        .bind(codec::id(id))
        .execute(&mut **tx)
        .await?;
    if done.rows_affected() == 0 {
        let transcript = transcript_index_text(&mut **tx, id).await?;
        sqlx::query("INSERT INTO notes_fts (note_id, title, body_md, transcript) VALUES (?, ?, ?, ?)")
            .bind(codec::id(id))
            .bind(&title)
            .bind(&body)
            .bind(transcript)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Final segments of a note, one per line, read straight from the `segments` table.
async fn transcript_index_text<'e>(ex: impl SqliteExecutor<'e>, note_id: Id) -> StoreResult<String> {
    let texts = sqlx::query_scalar::<_, String>("SELECT text FROM segments WHERE note_id = ? AND is_final = 1 ORDER BY start_ms ASC")
        .bind(codec::id(note_id))
        .fetch_all(ex)
        .await?;
    Ok(texts.iter().map(|t| index_text(t.trim())).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n"))
}

/// When the note just placed at `position` sits too close to a sibling, renumber its category.
async fn rebalance_notes_if_crowded(tx: &mut Transaction<'static, Sqlite>, id: Id, category_id: Option<Id>, position: f64) -> StoreResult<bool> {
    let crowded: i64 =
        sqlx::query_scalar("SELECT count(*) FROM notes WHERE category_id IS ? AND deleted_at IS NULL AND id != ? AND abs(position - ?) < ?")
            .bind(codec::opt_id(category_id))
            .bind(codec::id(id))
            .bind(position)
            .bind(MIN_GAP)
            .fetch_one(&mut **tx)
            .await?;
    if crowded == 0 {
        return Ok(false);
    }
    renumber(
        tx,
        "SELECT id FROM notes WHERE category_id IS ? AND deleted_at IS NULL ORDER BY position ASC, created_at DESC",
        Some(codec::opt_id(category_id)),
        "UPDATE notes SET position = ? WHERE id = ?",
    )
    .await?;
    Ok(true)
}

/// Give the rows `select` returns positions 1, 2, 3… in that order.
async fn renumber(
    tx: &mut Transaction<'static, Sqlite>,
    select: &'static str,
    param: Option<Option<String>>,
    update: &'static str,
) -> StoreResult<()> {
    let mut query = sqlx::query_scalar::<_, String>(select);
    if let Some(param) = param {
        query = query.bind(param);
    }
    let ids = query.fetch_all(&mut **tx).await?;
    for (i, id) in ids.iter().enumerate() {
        sqlx::query(update).bind((i + 1) as f64).bind(id).execute(&mut **tx).await?;
    }
    Ok(())
}

fn note_from_row(row: &SqliteRow) -> StoreResult<Note> {
    Ok(Note {
        id: codec::parse_id(&row.try_get::<String, _>("id")?)?,
        title: row.try_get("title")?,
        body_md: row.try_get("body_md")?,
        snippet: row.try_get("snippet")?,
        category_id: codec::parse_opt_id(row.try_get("category_id")?)?,
        kind: kind_from_sql(&row.try_get::<String, _>("kind")?),
        meeting_app: row.try_get("meeting_app")?,
        started_at: codec::parse_opt_ts(row.try_get("started_at")?)?,
        ended_at: codec::parse_opt_ts(row.try_get("ended_at")?)?,
        position: row.try_get("position")?,
        created_at: codec::parse_ts(&row.try_get::<String, _>("created_at")?)?,
        updated_at: codec::parse_ts(&row.try_get::<String, _>("updated_at")?)?,
        deleted_at: codec::parse_opt_ts(row.try_get("deleted_at")?)?,
    })
}

pub(crate) fn summary_from_row(row: &SqliteRow) -> StoreResult<NoteSummary> {
    Ok(NoteSummary {
        id: codec::parse_id(&row.try_get::<String, _>("id")?)?,
        title: row.try_get("title")?,
        snippet: row.try_get("snippet")?,
        category_id: codec::parse_opt_id(row.try_get("category_id")?)?,
        kind: kind_from_sql(&row.try_get::<String, _>("kind")?),
        meeting_app: row.try_get("meeting_app")?,
        position: row.try_get("position")?,
        created_at: codec::parse_ts(&row.try_get::<String, _>("created_at")?)?,
        updated_at: codec::parse_ts(&row.try_get::<String, _>("updated_at")?)?,
        deleted_at: codec::parse_opt_ts(row.try_get("deleted_at")?)?,
    })
}

fn category_from_row(row: &SqliteRow) -> StoreResult<Category> {
    Ok(Category {
        id: codec::parse_id(&row.try_get::<String, _>("id")?)?,
        name: row.try_get("name")?,
        position: row.try_get("position")?,
        collapsed: row.try_get::<i64, _>("collapsed")? != 0,
    })
}

fn version_from_row(row: &SqliteRow) -> StoreResult<NoteVersion> {
    Ok(NoteVersion {
        id: codec::parse_id(&row.try_get::<String, _>("id")?)?,
        note_id: codec::parse_id(&row.try_get::<String, _>("note_id")?)?,
        title: row.try_get("title")?,
        body_md: row.try_get("body_md")?,
        author: if row.try_get::<String, _>("author")? == "ai" { Author::Ai } else { Author::User },
        ai_action_id: codec::parse_opt_id(row.try_get("ai_action_id")?)?,
        created_at: codec::parse_ts(&row.try_get::<String, _>("created_at")?)?,
    })
}

pub(crate) fn kind_to_sql(kind: NoteKind) -> &'static str {
    match kind {
        NoteKind::Manual => "manual",
        NoteKind::Meeting => "meeting",
    }
}

fn kind_from_sql(kind: &str) -> NoteKind {
    if kind == "meeting" { NoteKind::Meeting } else { NoteKind::Manual }
}

fn author_to_sql(author: Author) -> &'static str {
    match author {
        Author::User => "user",
        Author::Ai => "ai",
    }
}

fn expect_one(rows: u64, what: &str) -> StoreResult<()> {
    if rows == 0 { Err(StoreError::NotFound(what.into())) } else { Ok(()) }
}

fn check_position(position: f64) -> StoreResult<()> {
    if position.is_finite() { Ok(()) } else { Err(StoreError::Invalid("That position is not a number. Try the move again.".into())) }
}

fn category_name(name: &str) -> StoreResult<String> {
    let name = single_line(name).trim().to_string();
    if name.is_empty() {
        return Err(StoreError::Invalid("Give the category a name.".into()));
    }
    Ok(name)
}

/// Join a stored relative recording path to the data dir, refusing anything that escapes it.
fn resolve_inside(root: &Path, stored: &str) -> Option<PathBuf> {
    let path = Path::new(stored);
    if stored.trim().is_empty() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    if path.is_absolute() {
        return path.starts_with(root).then(|| path.to_path_buf());
    }
    if path.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir)) {
        return Some(root.join(path));
    }
    None
}

// ── Text ───────────────────────────────────────────────────────────────────

/// Fold line breaks and tabs into single spaces.
pub(crate) fn single_line(text: &str) -> String {
    text.chars().map(|c| if matches!(c, '\n' | '\r' | '\t') { ' ' } else { c }).collect()
}

/// Text as stored in the search index: the snippet markers can never appear in it.
pub(crate) fn index_text(text: &str) -> String {
    text.replace([MARK_START, MARK_END], " ")
}

/// Whether a user edit replaced so much of the body that it deserves its own version.
fn changed_a_lot(old: &str, new: &str) -> bool {
    let (old_len, new_len) = (old.chars().count(), new.chars().count());
    let delta = old_len.abs_diff(new_len);
    if delta >= 400 {
        return true;
    }
    if old_len.max(new_len) < 80 {
        return false;
    }
    let ratio = similar::TextDiff::configure().timeout(Duration::from_millis(20)).diff_words(old, new).ratio();
    ratio < 0.5
}

/// The sidebar preview of a Markdown body: its first line with words in it, Markdown removed,
/// cut to about [`SNIPPET_MAX_CHARS`] characters on a word boundary.
pub fn snippet_from_markdown(markdown: &str) -> String {
    let mut in_fence = false;
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let plain = strip_markdown_line(trimmed);
        if plain.chars().any(char::is_alphanumeric) {
            return truncate_words(&plain, SNIPPET_MAX_CHARS);
        }
    }
    String::new()
}

/// The whole body as plain text for the search index (code blocks kept, syntax removed).
pub fn markdown_to_plain(markdown: &str) -> String {
    let mut out = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            continue;
        }
        let plain = strip_markdown_line(trimmed);
        if !plain.is_empty() {
            out.push(plain);
        }
    }
    index_text(&out.join("\n"))
}

fn truncate_words(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max - 1).collect();
    let cut = match cut.rfind(char::is_whitespace) {
        Some(i) if cut[..i].chars().count() >= max * 2 / 3 => &cut[..i],
        _ => cut.as_str(),
    };
    format!("{}…", cut.trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | '-')))
}

/// Remove block syntax (quotes, headings, list markers, task boxes) then inline syntax.
pub(crate) fn strip_markdown_line(line: &str) -> String {
    let mut rest = line.trim();
    loop {
        let before = rest;
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix('>') {
            rest = r;
        } else if rest.starts_with('#') {
            let r = rest.trim_start_matches('#');
            if r.is_empty() || r.starts_with(' ') {
                rest = r;
            }
        } else if let Some(r) = ["- ", "* ", "+ "].iter().find_map(|m| rest.strip_prefix(m)) {
            rest = r;
        } else if let Some(r) = ["[ ] ", "[x] ", "[X] "].iter().find_map(|m| rest.strip_prefix(m)) {
            rest = r;
        } else {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            if digits > 0 && digits <= 9 {
                let r = &rest[digits..];
                if let Some(r) = r.strip_prefix(". ").or_else(|| r.strip_prefix(") ")) {
                    rest = r;
                }
            }
        }
        if rest == before {
            break;
        }
    }
    let plain = strip_inline(rest);
    plain.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An Obsidian wiki link or embed starting at `i`: (shown text, index after it).
fn wiki_link_at(chars: &[char], i: usize) -> Option<(String, usize)> {
    let open = if chars.get(i) == Some(&'!') { i + 1 } else { i };
    if chars.get(open) != Some(&'[') || chars.get(open + 1) != Some(&'[') {
        return None;
    }
    let start = open + 2;
    let close = (start..chars.len().saturating_sub(1)).find(|&j| chars[j] == ']' && chars[j + 1] == ']')?;
    let inner: String = chars[start..close].iter().collect();
    if inner.is_empty() || inner.contains("[[") {
        return None;
    }
    let shown = match inner.split_once('|') {
        Some((_, alias)) => alias.to_string(),
        None => {
            let page = inner.split('#').next().unwrap_or(&inner);
            let page = page.rsplit('/').next().unwrap_or(page);
            page.strip_suffix(".md").unwrap_or(page).to_string()
        }
    };
    Some((shown.trim().to_string(), close + 2))
}

/// Remove inline Markdown: emphasis, code ticks, links (keep the text), images (keep the alt
/// text), HTML tags and backslash escapes.
fn strip_inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() => {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            // [[Note|shown]] → shown, [[Note#Heading]] → Note, ![[Note]] → Note
            '!' | '[' if wiki_link_at(&chars, i).is_some() => {
                let (label, next) = wiki_link_at(&chars, i).unwrap_or_default();
                out.push_str(&label);
                i = next;
                continue;
            }
            '!' if chars.get(i + 1) == Some(&'[') => {
                if let Some((label, next)) = parse_link(&chars, i + 1) {
                    out.push_str(&strip_inline(&label));
                    i = next;
                    continue;
                }
            }
            '[' => {
                if let Some((label, next)) = parse_link(&chars, i) {
                    out.push_str(&strip_inline(&label));
                    i = next;
                    continue;
                }
            }
            '<' => {
                if let Some(end) = chars[i + 1..].iter().position(|&c| c == '>').map(|p| i + 1 + p) {
                    let inner: String = chars[i + 1..end].iter().collect();
                    if inner.contains("://") || (inner.contains('@') && !inner.contains(' ')) {
                        out.push_str(&inner);
                        i = end + 1;
                        continue;
                    }
                    if is_html_tag(&inner) {
                        i = end + 1;
                        continue;
                    }
                }
            }
            '*' | '`' => {
                i += 1;
                continue;
            }
            '~' if chars.get(i + 1) == Some(&'~') => {
                i += 2;
                continue;
            }
            '=' if chars.get(i + 1) == Some(&'=') => {
                i += 2;
                continue;
            }
            '_' => {
                let before = i.checked_sub(1).and_then(|p| chars.get(p)).is_some_and(|c| c.is_alphanumeric());
                let after = chars.get(i + 1).is_some_and(|c| c.is_alphanumeric());
                if !(before && after) {
                    i += 1;
                    continue;
                }
            }
            _ => {}
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Whether `<inner>` looks like a common HTML tag (so `<Q3>` or `a < b > c` survive as text).
fn is_html_tag(inner: &str) -> bool {
    const TAGS: &[&str] = &[
        "a", "abbr", "b", "br", "code", "del", "details", "div", "em", "hr", "i", "img", "ins", "kbd", "mark", "p", "s", "small", "span", "strong",
        "sub", "summary", "sup", "u",
    ];
    if inner.starts_with("!--") {
        return true;
    }
    let name: String = inner.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    let rest = &inner.trim_start_matches('/')[name.len()..];
    TAGS.contains(&name.as_str()) && (rest.is_empty() || rest.starts_with([' ', '/']))
}

/// `[label](url)` or `[label]` starting at `start` (the `[`). Returns the label and the index after it.
fn parse_link(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut depth = 0;
    let mut close = None;
    for (offset, &c) in chars[start..].iter().enumerate() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(start + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let label: String = chars[start + 1..close].iter().collect();
    if chars.get(close + 1) == Some(&'(') {
        let end = chars[close + 1..].iter().position(|&c| c == ')').map(|p| close + 1 + p)?;
        return Some((label, end + 1));
    }
    // Not a link: keep task boxes like "[x]" as they are, unwrap reference-style labels.
    if label.chars().count() <= 1 {
        return None;
    }
    Some((label, close + 1))
}

/// Turn arbitrary input into a safe FTS5 query: quoted words joined by AND, the last a prefix.
/// `None` when the input has no words (empty, only punctuation or emoji).
pub fn fts_query(input: &str) -> Option<String> {
    let words: Vec<&str> = input.split(|c: char| !is_word_char(c)).filter(|w| !w.is_empty()).take(16).collect();
    let (last, rest) = words.split_last()?;
    let mut query: Vec<String> = rest.iter().map(|w| format!("\"{w}\"")).collect();
    query.push(format!("\"{last}\"*"));
    Some(query.join(" "))
}

/// Characters the `unicode61` tokenizer keeps inside a word (letters, numbers, combining marks).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// HTML-escape an FTS snippet and turn its private markers into `<mark>` tags.
pub(crate) fn excerpt_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 16);
    let mut last_space = false;
    for c in raw.chars() {
        let is_space = c.is_whitespace();
        if is_space && last_space {
            continue;
        }
        last_space = is_space;
        match c {
            MARK_START => out.push_str("<mark>"),
            MARK_END => out.push_str("</mark>"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ if is_space => out.push(' '),
            c => out.push(c),
        }
    }
    out.trim().to_string()
}

fn first_marked_term(raw: &str) -> Option<&str> {
    let start = raw.find(MARK_START)? + MARK_START.len_utf8();
    let end = raw[start..].find(MARK_END)? + start;
    let term = raw[start..end].trim();
    (!term.is_empty()).then_some(term)
}

#[cfg(test)]
mod tests {
    use cilica_domain::SearchSource;
    use cilica_domain::chrono::Utc;

    use super::*;

    async fn db() -> Db {
        Db::open_in_memory().await.unwrap()
    }

    async fn note(db: &Db, title: &str, body: &str) -> Note {
        db.create_note(NewNote { title: title.into(), body_md: body.into(), ..Default::default() }).await.unwrap()
    }

    /// Pretend every version of a note was taken `minutes` ago.
    async fn age_versions(db: &Db, note_id: Id, minutes: i64) {
        sqlx::query("UPDATE note_versions SET created_at = ? WHERE note_id = ?")
            .bind(codec::ts(Utc::now() - TimeDelta::minutes(minutes)))
            .bind(codec::id(note_id))
            .execute(db.pool())
            .await
            .unwrap();
    }

    async fn insert_segment(db: &Db, note_id: Id, start_ms: i64, text: &str, is_final: bool) {
        sqlx::query("INSERT INTO segments (id, note_id, channel, start_ms, end_ms, text, is_final) VALUES (?, ?, 'system', ?, ?, ?, ?)")
            .bind(codec::id(new_id()))
            .bind(codec::id(note_id))
            .bind(start_ms)
            .bind(start_ms + 1000)
            .bind(text)
            .bind(is_final)
            .execute(db.pool())
            .await
            .unwrap();
    }

    // ── Ordering ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn new_notes_go_to_the_top_of_their_category() {
        let db = db().await;
        let work = db.create_category("Work").await.unwrap();
        let a = note(&db, "A", "").await;
        let b = note(&db, "B", "").await;
        let c = db.create_note(NewNote { title: "C".into(), category_id: Some(work.id), ..Default::default() }).await.unwrap();
        let d = db.create_note(NewNote { title: "D".into(), category_id: Some(work.id), ..Default::default() }).await.unwrap();
        assert!(b.position < a.position);
        assert!(d.position < c.position);

        let titles: Vec<_> = db.list_notes().await.unwrap().into_iter().filter(|n| n.category_id.is_none()).map(|n| n.title).collect();
        assert_eq!(titles, ["B", "A"]);

        // Moving between categories keeps updated_at and honours the given position.
        let moved = db.move_note(a.id, Some(work.id), (d.position + c.position) / 2.0).await.unwrap();
        assert_eq!(moved.category_id, Some(work.id));
        assert_eq!(moved.updated_at, a.updated_at);
        let in_work: Vec<_> = db.list_notes().await.unwrap().into_iter().filter(|n| n.category_id == Some(work.id)).map(|n| n.title).collect();
        assert_eq!(in_work, ["D", "A", "C"]);
    }

    #[tokio::test]
    async fn crowded_positions_are_renumbered() {
        let db = db().await;
        let a = note(&db, "A", "").await;
        let b = note(&db, "B", "").await; // B above A
        let c = note(&db, "C", "").await; // C above B
        let (_, rebalanced) = db.move_note_rebalanced(c.id, None, (b.position + a.position) / 2.0).await.unwrap();
        assert!(!rebalanced);
        let (moved, rebalanced) = db.move_note_rebalanced(c.id, None, b.position + 1e-9).await.unwrap();
        assert!(rebalanced);
        let list = db.list_notes().await.unwrap();
        assert_eq!(list.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), ["B", "C", "A"]);
        assert_eq!(list.iter().map(|n| n.position).collect::<Vec<_>>(), [1.0, 2.0, 3.0]);
        assert_eq!(moved.position, 2.0);
        assert!(db.move_note(a.id, None, f64::NAN).await.is_err());
    }

    #[tokio::test]
    async fn categories_append_reorder_collapse_and_delete() {
        let db = db().await;
        let one = db.create_category("  One ").await.unwrap();
        let two = db.create_category("Two").await.unwrap();
        let three = db.create_category("Three").await.unwrap();
        assert_eq!(one.name, "One");
        assert!(one.position < two.position && two.position < three.position);
        assert!(db.create_category("   ").await.is_err());

        db.reorder_category(three.id, one.position - 1.0).await.unwrap();
        let names: Vec<_> = db.list_categories().await.unwrap().into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["Three", "One", "Two"]);

        let (_, rebalanced) = db.reorder_category_rebalanced(two.id, one.position + 1e-9).await.unwrap();
        assert!(rebalanced);
        let positions: Vec<_> = db.list_categories().await.unwrap().into_iter().map(|c| c.position).collect();
        assert_eq!(positions, [1.0, 2.0, 3.0]);

        assert!(db.set_category_collapsed(one.id, true).await.unwrap().collapsed);
        assert_eq!(db.rename_category(one.id, "Uno").await.unwrap().name, "Uno");

        let n = db.create_note(NewNote { title: "In one".into(), category_id: Some(one.id), ..Default::default() }).await.unwrap();
        db.delete_category(one.id).await.unwrap();
        assert_eq!(db.get_note(n.id).await.unwrap().category_id, None);
        assert!(matches!(db.get_category(one.id).await, Err(StoreError::NotFound(_))));
        assert!(matches!(db.delete_category(one.id).await, Err(StoreError::NotFound(_))));
        assert!(matches!(db.create_note(NewNote { category_id: Some(one.id), ..Default::default() }).await, Err(StoreError::NotFound(_))));
    }

    // ── Bodies, snippets, snapshots ──────────────────────────────────────

    #[tokio::test]
    async fn user_edits_snapshot_sparingly() {
        let db = db().await;
        let n = note(&db, "Plan", "").await;

        // First text in an empty note: nothing worth keeping yet.
        db.set_note_body(n.id, "first draft", Author::User, None).await.unwrap();
        assert_eq!(db.list_note_versions(n.id).await.unwrap().len(), 0);

        // Non-empty body with no version yet: snapshot once.
        db.set_note_body(n.id, "first draft, more", Author::User, None).await.unwrap();
        let versions = db.list_note_versions(n.id).await.unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].body_md, "first draft");
        assert_eq!(versions[0].author, Author::User);

        // Typing soon after: no new versions.
        for i in 0..5 {
            db.set_note_body(n.id, &format!("first draft, more {i}"), Author::User, None).await.unwrap();
        }
        assert_eq!(db.list_note_versions(n.id).await.unwrap().len(), 1);

        // After the interval: one more.
        age_versions(&db, n.id, 6).await;
        db.set_note_body(n.id, "first draft, more 5", Author::User, None).await.unwrap();
        assert_eq!(db.list_note_versions(n.id).await.unwrap().len(), 2);

        // A big replacement right away still snapshots.
        let long = "word ".repeat(200);
        db.set_note_body(n.id, &long, Author::User, None).await.unwrap();
        assert_eq!(db.list_note_versions(n.id).await.unwrap().len(), 3);
        db.set_note_body(n.id, "", Author::User, None).await.unwrap();
        let versions = db.list_note_versions(n.id).await.unwrap();
        assert_eq!(versions.len(), 4);
        assert_eq!(versions[0].body_md, long);
    }

    #[tokio::test]
    async fn no_op_writes_change_nothing() {
        let db = db().await;
        let n = note(&db, "Same", "body").await;
        let after = db.set_note_body(n.id, "body", Author::Ai, None).await.unwrap();
        assert_eq!(after.updated_at, n.updated_at);
        assert!(db.list_note_versions(n.id).await.unwrap().is_empty());
        let renamed = db.rename_note(n.id, "Same").await.unwrap();
        assert_eq!(renamed.updated_at, n.updated_at);
        let renamed = db.rename_note(n.id, "Other\nline").await.unwrap();
        assert_eq!(renamed.title, "Other line");
        assert!(renamed.updated_at > n.updated_at);
    }

    #[tokio::test]
    async fn ai_writes_always_snapshot_with_their_action() {
        let db = db().await;
        let n = note(&db, "Notes", "").await;
        let action = new_id();
        db.set_note_body(n.id, "summary v1", Author::Ai, Some(action)).await.unwrap();
        db.set_note_body(n.id, "summary v2", Author::Ai, Some(action)).await.unwrap();
        let versions = db.list_note_versions(n.id).await.unwrap();
        assert_eq!(versions.len(), 2);
        assert!(versions.iter().all(|v| v.author == Author::Ai && v.ai_action_id == Some(action)));
        assert_eq!(versions[0].body_md, "summary v1");
        assert_eq!(versions[1].body_md, "");
    }

    #[tokio::test]
    async fn restore_snapshots_then_restores() {
        let db = db().await;
        let n = note(&db, "Title one", "body one").await;
        let v1 = db.snapshot_note(n.id, Author::User, None).await.unwrap();
        db.rename_note(n.id, "Title two").await.unwrap();
        db.set_note_body(n.id, "body two", Author::Ai, None).await.unwrap();

        let restored = db.restore_note_version(v1.id, Author::User).await.unwrap();
        assert_eq!(restored.title, "Title one");
        assert_eq!(restored.body_md, "body one");
        assert_eq!(restored.snippet, "body one");
        let versions = db.list_note_versions(n.id).await.unwrap();
        assert_eq!(versions[0].title, "Title two");
        assert_eq!(versions[0].body_md, "body two");
        assert_eq!(db.get_note_version(v1.id).await.unwrap(), v1);
        assert!(matches!(db.get_note_version(new_id()).await, Err(StoreError::NotFound(_))));

        // Search follows the restore.
        assert_eq!(db.search_notes("one", 10).await.unwrap().len(), 1);
        assert!(db.search_notes("two", 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn versions_are_bounded_but_recent_ai_versions_stay() {
        let db = db().await;
        let n = note(&db, "Busy", "start").await;
        for i in 0..(MAX_VERSIONS_PER_NOTE + 10) {
            db.snapshot_note(n.id, Author::User, None).await.unwrap();
            db.set_note_body(n.id, &format!("user {i}"), Author::User, None).await.unwrap();
        }
        assert_eq!(db.list_note_versions(n.id).await.unwrap().len() as i64, MAX_VERSIONS_PER_NOTE);

        for i in 0..20 {
            db.set_note_body(n.id, &format!("ai {i}"), Author::Ai, Some(new_id())).await.unwrap();
        }
        let versions = db.list_note_versions(n.id).await.unwrap();
        assert_eq!(versions.iter().filter(|v| v.author == Author::Ai).count(), 20);
        assert_eq!(versions.iter().filter(|v| v.author == Author::User).count() as i64, MAX_VERSIONS_PER_NOTE);

        // Once old, AI versions count against the cap like any other.
        sqlx::query("UPDATE note_versions SET created_at = ? WHERE author = 'ai'")
            .bind(codec::ts(Utc::now() - TimeDelta::days(31)))
            .execute(db.pool())
            .await
            .unwrap();
        db.snapshot_note(n.id, Author::User, None).await.unwrap();
        assert_eq!(db.list_note_versions(n.id).await.unwrap().len() as i64, MAX_VERSIONS_PER_NOTE);
    }

    #[tokio::test]
    async fn snippets_follow_manual_notes_but_not_meeting_summaries() {
        let db = db().await;
        let n = note(&db, "Manual", "").await;
        let s = db.set_note_body(n.id, "\n\n# Heading with **bold** and [a link](https://x.y)\nmore", Author::User, None).await.unwrap();
        assert_eq!(s.snippet, "Heading with bold and a link");

        let meeting = db
            .create_note(NewNote { title: "Zoom meeting".into(), kind: NoteKind::Meeting, meeting_app: Some("Zoom".into()), ..Default::default() })
            .await
            .unwrap();
        let s = db.set_note_body(meeting.id, "jotting", Author::User, None).await.unwrap();
        assert_eq!(s.snippet, "jotting", "an empty meeting snippet is filled from the body");
        db.set_note_snippet(meeting.id, "Agreed the Q3 plan\nand budget").await.unwrap();
        let s = db.set_note_body(meeting.id, "## Summary\nSomething else", Author::Ai, None).await.unwrap();
        assert_eq!(s.snippet, "Agreed the Q3 plan and budget");
    }

    #[test]
    fn snippet_extraction() {
        assert_eq!(snippet_from_markdown(""), "");
        assert_eq!(snippet_from_markdown("---\n\n***\n"), "");
        assert_eq!(snippet_from_markdown("```\ncode only\n```\nAfter code"), "After code");
        assert_eq!(snippet_from_markdown("- [ ] Call _Sam_ about `budget`"), "Call Sam about budget");
        assert_eq!(snippet_from_markdown("1. First ~~old~~ item"), "First old item");
        assert_eq!(snippet_from_markdown("> quoted <b>html</b> text<br/>"), "quoted html text");
        assert_eq!(snippet_from_markdown("Plan for <Q3> when a < b > c"), "Plan for <Q3> when a < b > c");
        assert_eq!(snippet_from_markdown("![diagram](a.png) of snake_case_names"), "diagram of snake_case_names");
        assert_eq!(snippet_from_markdown("Price \\*not bold\\*"), "Price *not bold*");
        let long = "lorem ipsum ".repeat(30);
        let s = snippet_from_markdown(&long);
        assert!(s.chars().count() <= SNIPPET_MAX_CHARS);
        assert!(s.ends_with('…'));
        assert!(!s.contains("ipsu…"), "cut on a word boundary: {s}");
    }

    // ── Trash ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn trash_restore_and_delete() {
        let db = db().await;
        let a = note(&db, "Keep", "alpha").await;
        let b = note(&db, "Bin", "beta").await;
        db.trash_note(b.id).await.unwrap();
        db.trash_note(b.id).await.unwrap(); // idempotent
        assert!(matches!(db.trash_note(new_id()).await, Err(StoreError::NotFound(_))));

        assert_eq!(db.list_notes().await.unwrap().iter().map(|n| n.id).collect::<Vec<_>>(), [a.id]);
        assert_eq!(db.list_trash().await.unwrap().iter().map(|n| n.id).collect::<Vec<_>>(), [b.id]);
        assert!(db.get_note(b.id).await.unwrap().deleted_at.is_some());
        assert!(db.search_notes("beta", 10).await.unwrap().is_empty(), "trashed notes are not searchable");

        let restored = db.restore_note(b.id).await.unwrap();
        assert!(restored.deleted_at.is_none());
        assert!(restored.position < a.position, "restored notes come back on top");
        assert_eq!(db.search_notes("beta", 10).await.unwrap().len(), 1);

        db.snapshot_note(b.id, Author::User, None).await.unwrap();
        db.delete_note_forever(b.id).await.unwrap();
        assert!(matches!(db.get_note(b.id).await, Err(StoreError::NotFound(_))));
        assert!(db.list_note_versions(b.id).await.unwrap().is_empty());
        let fts_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM notes_fts").fetch_one(db.pool()).await.unwrap();
        assert_eq!(fts_rows, 1);

        db.trash_note(a.id).await.unwrap();
        assert_eq!(db.empty_trash().await.unwrap(), 1);
        assert!(db.list_trash().await.unwrap().is_empty());
        assert_eq!(db.empty_trash().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn trash_restore_and_delete_several() {
        let db = db().await;
        let a = note(&db, "A", "").await;
        let b = note(&db, "B", "").await;
        let c = note(&db, "C", "").await;
        let gone = new_id();

        assert_eq!(db.trash_notes(&[a.id, b.id, gone]).await.unwrap(), 2);
        assert_eq!(db.trash_notes(&[a.id]).await.unwrap(), 0, "already in Trash");
        assert_eq!(db.list_notes().await.unwrap().iter().map(|n| n.id).collect::<Vec<_>>(), [c.id]);

        // Restored as a block on top, in the order given.
        let restored = db.restore_notes(&[b.id, a.id, gone, c.id]).await.unwrap();
        assert_eq!(restored.iter().map(|n| n.id).collect::<Vec<_>>(), [b.id, a.id]);
        let live = byposition(db.list_notes().await.unwrap());
        assert_eq!(live, [b.id, a.id, c.id]);

        // Only notes in Trash are deleted for good.
        db.trash_notes(&[a.id]).await.unwrap();
        assert_eq!(db.delete_notes_forever(&[a.id, b.id, gone]).await.unwrap(), 1);
        assert!(matches!(db.get_note(a.id).await, Err(StoreError::NotFound(_))));
        assert!(db.get_note(b.id).await.is_ok());
    }

    fn byposition(mut notes: Vec<NoteSummary>) -> Vec<Id> {
        notes.sort_by(|x, y| x.position.partial_cmp(&y.position).unwrap());
        notes.into_iter().map(|n| n.id).collect()
    }

    #[tokio::test]
    async fn move_several_keeps_their_order() {
        let db = db().await;
        let cat = db.create_category("Work").await.unwrap();
        let x = note(&db, "X", "").await;
        let y = note(&db, "Y", "").await;
        let a = note(&db, "A", "").await;
        let b = note(&db, "B", "").await;
        db.move_note(x.id, Some(cat.id), 1.0).await.unwrap();
        db.move_note(y.id, Some(cat.id), 2.0).await.unwrap();
        let in_cat = |db: Db| async move {
            byposition(db.list_notes().await.unwrap().into_iter().filter(|n| n.category_id == Some(cat.id)).collect())
        };

        // To the top, in the order given.
        let (moved, _) = db.move_notes(&[b.id, a.id], Some(cat.id), None).await.unwrap();
        assert_eq!(moved.iter().map(|n| n.id).collect::<Vec<_>>(), [b.id, a.id]);
        assert_eq!(in_cat(db.clone()).await, [b.id, a.id, x.id, y.id]);

        // Between X and Y.
        db.move_notes(&[a.id, b.id], Some(cat.id), Some(1.5)).await.unwrap();
        assert_eq!(in_cat(db.clone()).await, [x.id, a.id, b.id, y.id]);

        // To the end.
        db.move_notes(&[x.id, a.id], Some(cat.id), Some(3.0)).await.unwrap();
        assert_eq!(in_cat(db.clone()).await, [b.id, y.id, x.id, a.id]);

        // A crowded gap gets the category renumbered, order intact.
        let b_at = db.get_note(b.id).await.unwrap().position;
        let (_, rebalanced) = db.move_notes(&[x.id, a.id], Some(cat.id), Some(b_at - 1e-7)).await.unwrap();
        assert!(rebalanced);
        assert_eq!(in_cat(db.clone()).await, [x.id, a.id, b.id, y.id]);
        let x_at = db.get_note(x.id).await.unwrap().position;
        assert_eq!(x_at, 1.0, "renumbered 1, 2, 3…");

        // Out of the category; trashed and missing notes are skipped.
        db.trash_note(y.id).await.unwrap();
        let (moved, _) = db.move_notes(&[y.id, x.id, new_id()], None, None).await.unwrap();
        assert_eq!(moved.iter().map(|n| n.id).collect::<Vec<_>>(), [x.id]);
        assert_eq!(moved[0].category_id, None);

        assert!(matches!(db.move_notes(&[a.id], Some(new_id()), None).await, Err(StoreError::NotFound(_))));
        assert!(matches!(db.move_notes(&[a.id], None, Some(f64::NAN)).await, Err(StoreError::Invalid(_))));
    }

    #[tokio::test]
    async fn discard_removes_only_notes_with_nothing_in_them() {
        let db = db().await;
        let blank = note(&db, "", "").await;
        let spaces = note(&db, "  ", "\n \n").await;
        let titled = note(&db, "Plan", "").await;
        let written = note(&db, "", "Hello").await;
        let trashed = note(&db, "", "").await;
        db.trash_note(trashed.id).await.unwrap();

        assert!(db.discard_empty_note(blank.id).await.unwrap());
        assert!(db.discard_empty_note(spaces.id).await.unwrap());
        assert!(!db.discard_empty_note(titled.id).await.unwrap());
        assert!(!db.discard_empty_note(written.id).await.unwrap());
        assert!(!db.discard_empty_note(trashed.id).await.unwrap());
        assert!(!db.discard_empty_note(new_id()).await.unwrap());

        assert!(matches!(db.get_note(blank.id).await, Err(StoreError::NotFound(_))));
        assert_eq!(db.list_notes().await.unwrap().len(), 2);
        assert_eq!(db.list_trash().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn purge_removes_old_trash_and_its_recordings() {
        let dir = std::env::temp_dir().join(format!("cilica-store-test-{}", new_id()));
        std::fs::create_dir_all(dir.join("recordings")).unwrap();
        let outside = std::env::temp_dir().join(format!("cilica-outside-{}.opus", new_id()));
        std::fs::write(&outside, b"keep me").unwrap();
        let db = db().await.with_data_dir(&dir);

        let old = note(&db, "Old", "").await;
        let recent = note(&db, "Recent", "").await;
        let live = note(&db, "Live", "").await;
        let sneaky = note(&db, "Sneaky", "").await;
        std::fs::write(dir.join("recordings/old.opus"), b"audio").unwrap();
        db.set_note_audio(old.id, Some("recordings/old.opus"), Some(61_000)).await.unwrap();
        assert_eq!(db.get_note_audio(old.id).await.unwrap(), (Some("recordings/old.opus".into()), Some(61_000)));
        db.set_note_audio(sneaky.id, Some(outside.to_str().unwrap()), None).await.unwrap();

        for id in [old.id, recent.id, sneaky.id] {
            db.trash_note(id).await.unwrap();
        }
        sqlx::query("UPDATE notes SET deleted_at = ? WHERE id IN (?, ?)")
            .bind(codec::ts(Utc::now() - TimeDelta::days(31)))
            .bind(codec::id(old.id))
            .bind(codec::id(sneaky.id))
            .execute(db.pool())
            .await
            .unwrap();

        assert_eq!(db.purge_trash_older_than(30).await.unwrap(), 2);
        assert!(!dir.join("recordings/old.opus").exists(), "the recording is deleted with the note");
        assert!(outside.exists(), "paths outside the data dir are never touched");
        assert!(db.get_note(recent.id).await.is_ok());
        assert!(db.get_note(live.id).await.is_ok());
        assert_eq!(db.purge_trash_older_than(30).await.unwrap(), 0);

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_file(&outside).ok();
    }

    #[test]
    fn recording_paths_stay_inside_the_data_dir() {
        let root = Path::new("/data/cilica");
        assert_eq!(resolve_inside(root, "recordings/a.opus"), Some(root.join("recordings/a.opus")));
        assert_eq!(resolve_inside(root, "/data/cilica/recordings/a.opus"), Some(root.join("recordings/a.opus")));
        assert_eq!(resolve_inside(root, "../a.opus"), None);
        assert_eq!(resolve_inside(root, "recordings/../../a.opus"), None);
        assert_eq!(resolve_inside(root, "/etc/passwd"), None);
        assert_eq!(resolve_inside(root, ""), None);
    }

    // ── Search ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn search_ranks_titles_first_and_marks_matches() {
        let db = db().await;
        let in_body = note(&db, "Weekly sync", "We discussed the roadmap for <Q3> & beyond.").await;
        let in_title = note(&db, "Roadmap review", "Nothing else here.").await;

        let hits = db.search_notes("roadmap", 10).await.unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].note.id, in_title.id);
        assert_eq!(hits[0].source, SearchSource::Title);
        assert_eq!(hits[0].excerpt, "<mark>Roadmap</mark> review");
        assert_eq!(hits[1].note.id, in_body.id);
        assert_eq!(hits[1].source, SearchSource::Body);
        assert!(hits[1].excerpt.contains("<mark>roadmap</mark>"), "{}", hits[1].excerpt);
        assert!(hits[1].excerpt.contains("&lt;Q3&gt; &amp; beyond"), "surrounding text is escaped: {}", hits[1].excerpt);
        assert!(hits.iter().all(|h| h.at_ms.is_none()));
    }

    #[tokio::test]
    async fn search_matches_prefixes_while_typing_and_ignores_diacritics() {
        let db = db().await;
        note(&db, "Café notes", "Résumé of the kickoff").await;
        assert_eq!(db.search_notes("kick", 10).await.unwrap().len(), 1);
        assert_eq!(db.search_notes("cafe", 10).await.unwrap().len(), 1);
        assert_eq!(db.search_notes("resume kick", 10).await.unwrap().len(), 1);
        assert_eq!(db.search_notes("resume kickx", 10).await.unwrap().len(), 0);
        assert_eq!(db.search_notes("kickoff resume", 10).await.unwrap().len(), 1);
        assert_eq!(db.search_notes("kick resume", 10).await.unwrap().len(), 0, "earlier words must match whole");
        assert_eq!(db.search_notes("ki resume", 10).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn search_survives_hostile_input() {
        let db = db().await;
        note(&db, "Budget \"final\" (v2)", "AND OR NOT NEAR title:body col* ^start 🎉 party").await;
        for query in [
            "",
            "   ",
            "\"",
            "\"\"",
            "'",
            "*",
            "-",
            "(",
            ")",
            "AND",
            "OR NOT",
            "NEAR(",
            "title:",
            "body:x",
            "^",
            "🎉",
            "🎉🎉 ",
            "a\"b",
            "\u{0}",
            "\\",
            "%",
            "_",
            "budget\"",
            "\"final",
            "col*",
            "{title}: x",
            "x OR",
            "NOT budget",
            "'; DROP TABLE notes; --",
        ] {
            let result = db.search_notes(query, 10).await;
            assert!(result.is_ok(), "query {query:?} failed: {result:?}");
        }
        assert_eq!(db.search_notes("\"final", 10).await.unwrap().len(), 1);
        assert_eq!(db.search_notes("NOT budget", 10).await.unwrap().len(), 1, "operators are plain words");
        assert_eq!(db.search_notes("party 🎉", 10).await.unwrap().len(), 1);
        assert!(db.search_notes("🎉", 10).await.unwrap().is_empty());
        assert!(db.search_notes("", 10).await.unwrap().is_empty());
        assert_eq!(db.list_notes().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn search_finds_transcripts_with_their_time() {
        let db = db().await;
        let meeting = db.create_note(NewNote { title: "Acme kickoff".into(), kind: NoteKind::Meeting, ..Default::default() }).await.unwrap();
        insert_segment(&db, meeting.id, 0, "Hello everyone", true).await;
        insert_segment(&db, meeting.id, 65_000, "Let us talk about the invoice schedule", true).await;
        insert_segment(&db, meeting.id, 90_000, "partial invoice words", false).await;
        assert!(db.search_notes("invoice", 10).await.unwrap().is_empty(), "not indexed until reindex_note");

        db.reindex_note(meeting.id).await.unwrap();
        let hits = db.search_notes("invoi", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, SearchSource::Transcript);
        assert_eq!(hits[0].at_ms, Some(65_000));
        assert!(hits[0].excerpt.contains("<mark>invoice</mark>"), "{}", hits[0].excerpt);
        assert!(!hits[0].excerpt.contains("partial"), "only final segments are indexed");

        // Body edits keep the transcript column.
        db.set_note_body(meeting.id, "Summary", Author::Ai, None).await.unwrap();
        assert_eq!(db.search_notes("invoice", 10).await.unwrap().len(), 1);
        assert_eq!(db.search_notes("summary", 10).await.unwrap()[0].source, SearchSource::Body);
    }

    #[tokio::test]
    async fn search_respects_the_limit_and_repair_fills_gaps() {
        let db = db().await;
        for i in 0..30 {
            note(&db, &format!("Standup {i}"), "").await;
        }
        assert_eq!(db.search_notes("standup", 5).await.unwrap().len(), 5);
        assert_eq!(db.search_notes("standup", 0).await.unwrap().len(), 1);

        sqlx::query("DELETE FROM notes_fts").execute(db.pool()).await.unwrap();
        assert!(db.search_notes("standup", 50).await.unwrap().is_empty());
        assert_eq!(db.repair_search_index().await.unwrap(), 30);
        assert_eq!(db.search_notes("standup", 50).await.unwrap().len(), 30);
        assert_eq!(db.repair_search_index().await.unwrap(), 0);
    }

    #[test]
    fn query_building() {
        assert_eq!(fts_query("hello world"), Some("\"hello\" \"world\"*".into()));
        assert_eq!(fts_query("  \"quoted\" OR (x) "), Some("\"quoted\" \"OR\" \"x\"*".into()));
        assert_eq!(fts_query("🎉 -- *"), None);
        assert_eq!(fts_query("naïve"), Some("\"naïve\"*".into()));
    }

    #[test]
    fn excerpt_escaping() {
        let raw = format!("a <b> & {MARK_START}c{MARK_END}\n\n'd'");
        assert_eq!(excerpt_html(&raw), "a &lt;b&gt; &amp; <mark>c</mark> &#39;d&#39;");
    }

    // ── Misc ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn meeting_fields_round_trip() {
        let db = db().await;
        let started = Utc::now() - TimeDelta::minutes(30);
        let n = db
            .create_note(NewNote {
                title: "Zoom meeting".into(),
                kind: NoteKind::Meeting,
                meeting_app: Some("Zoom".into()),
                started_at: Some(started),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(n.kind, NoteKind::Meeting);
        assert_eq!(n.meeting_app.as_deref(), Some("Zoom"));
        assert_eq!(n.started_at.map(|t| t.timestamp_micros()), Some(started.timestamp_micros()));
        let ended = Utc::now();
        db.set_note_meeting_times(n.id, Some(started), Some(ended)).await.unwrap();
        let n = db.get_note(n.id).await.unwrap();
        assert_eq!(n.ended_at.map(|t| t.timestamp_micros()), Some(ended.timestamp_micros()));
        assert!(matches!(db.set_note_snippet(new_id(), "x").await, Err(StoreError::NotFound(_))));
        assert!(matches!(db.get_note_audio(new_id()).await, Err(StoreError::NotFound(_))));
    }
}
