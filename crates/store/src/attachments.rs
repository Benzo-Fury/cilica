//! Reads and in-place rewrites for image attachments (`cilica-attachments`). No tables of its own:
//! it finds note bodies and versions that still carry inline `data:` images, swaps them for
//! `attachments/<name>` links without making a version or moving `updated_at`, and lists every
//! stored text that might point at an attachment so unused files can be cleaned up.

use cilica_domain::Id;
use sqlx::Row;

use crate::notes::{aliases_column, snippet_from_markdown};
use crate::{Db, StoreResult, codec};

/// One query per column that can hold note Markdown, or text copied from it: note bodies (Trash
/// too), versions, the assistant's proposals, its audit rows (arguments and undo snapshots), chat
/// messages (tool results quote note bodies) and scheduled task summaries. Each takes the needle.
const MARKDOWN_COLUMNS: &[&str] = &[
    "SELECT body_md FROM notes WHERE instr(body_md, ?1) > 0",
    "SELECT body_md FROM note_versions WHERE instr(body_md, ?1) > 0",
    "SELECT payload_json FROM proposals WHERE instr(payload_json, ?1) > 0",
    "SELECT args_json FROM ai_actions WHERE instr(args_json, ?1) > 0",
    "SELECT undo_json FROM ai_actions WHERE undo_json IS NOT NULL AND instr(undo_json, ?1) > 0",
    "SELECT parts_json FROM messages WHERE instr(parts_json, ?1) > 0",
    "SELECT raw_json FROM messages WHERE raw_json IS NOT NULL AND instr(raw_json, ?1) > 0",
    "SELECT summary FROM task_runs WHERE instr(summary, ?1) > 0",
];

impl Db {
    // ── Finding inline images ──────────────────────────────────────────

    /// Notes whose body contains `needle`, in Trash or not.
    pub async fn note_ids_with_text(&self, needle: &str) -> StoreResult<Vec<Id>> {
        let ids = sqlx::query_scalar::<_, String>("SELECT id FROM notes WHERE instr(body_md, ?) > 0").bind(needle).fetch_all(self.pool()).await?;
        ids.iter().map(|id| codec::parse_id(id)).collect()
    }

    /// Versions whose body contains `needle`.
    pub async fn version_ids_with_text(&self, needle: &str) -> StoreResult<Vec<Id>> {
        let ids =
            sqlx::query_scalar::<_, String>("SELECT id FROM note_versions WHERE instr(body_md, ?) > 0").bind(needle).fetch_all(self.pool()).await?;
        ids.iter().map(|id| codec::parse_id(id)).collect()
    }

    /// One note's body, in Trash or not. `None` when the note is gone.
    pub async fn note_body_raw(&self, id: Id) -> StoreResult<Option<String>> {
        Ok(sqlx::query_scalar("SELECT body_md FROM notes WHERE id = ?").bind(codec::id(id)).fetch_optional(self.pool()).await?)
    }

    /// One version's body. `None` when the version is gone.
    pub async fn version_body_raw(&self, id: Id) -> StoreResult<Option<String>> {
        Ok(sqlx::query_scalar("SELECT body_md FROM note_versions WHERE id = ?").bind(codec::id(id)).fetch_optional(self.pool()).await?)
    }

    // ── Rewriting in place ─────────────────────────────────────────────

    /// Swap a note's body for `new_body` when it still equals `old_body`, without snapshotting a
    /// version or moving `updated_at`: the note reads the same, only its storage changed. Refreshes
    /// the snippet (manual notes), aliases and search row; the link index follows through its
    /// trigger. Returns false when the body changed in between (nothing is written then).
    pub async fn replace_note_body_in_place(&self, id: Id, old_body: &str, new_body: &str) -> StoreResult<bool> {
        let mut tx = self.begin_write().await?;

        // Read the note's kind and snippet, only if the body is still the one we rewrote
        let row = sqlx::query("SELECT kind, snippet FROM notes WHERE id = ? AND body_md = ?")
            .bind(codec::id(id))
            .bind(old_body)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(false);
        };
        let kind: String = row.try_get("kind")?;
        let snippet: String = row.try_get("snippet")?;

        // Manual notes take their snippet from the body; meeting notes keep the summariser's
        let snippet = if kind == "manual" || snippet.trim().is_empty() || snippet.contains("data:image/") {
            snippet_from_markdown(new_body)
        } else {
            snippet
        };
        sqlx::query("UPDATE notes SET body_md = ?, aliases = ?, snippet = ? WHERE id = ?")
            .bind(new_body)
            .bind(aliases_column(new_body))
            .bind(&snippet)
            .bind(codec::id(id))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        // Rebuild the search row from the new body
        self.reindex_note(id).await?;
        Ok(true)
    }

    /// Swap a version's body for `new_body` when it still equals `old_body`. Returns false when it
    /// changed or is gone.
    pub async fn replace_version_body_in_place(&self, id: Id, old_body: &str, new_body: &str) -> StoreResult<bool> {
        let done = sqlx::query("UPDATE note_versions SET body_md = ? WHERE id = ? AND body_md = ?")
            .bind(new_body)
            .bind(codec::id(id))
            .bind(old_body)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    // ── References ─────────────────────────────────────────────────────

    /// Every stored text that contains `needle`, across every column that can hold note Markdown
    /// (see [`MARKDOWN_COLUMNS`]). Calls `visit` once per matching value, so large tables are not
    /// held in memory all at once beyond one column's matches.
    pub async fn for_each_text_containing(&self, needle: &str, mut visit: impl FnMut(&str)) -> StoreResult<()> {
        for sql in MARKDOWN_COLUMNS {
            let texts = sqlx::query_scalar::<_, String>(*sql).bind(needle).fetch_all(self.pool()).await?;
            for text in &texts {
                visit(text);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use cilica_domain::Author;

    use super::*;
    use crate::NewNote;

    /// A note with an inline image is rewritten without a version or a new `updated_at`.
    #[tokio::test]
    async fn rewrites_a_body_in_place() {
        let db = Db::open_in_memory().await.unwrap();
        let body = "Look ![pic](data:image/png;base64,AAAA) here";
        let note = db.create_note(NewNote { title: "Pics".into(), body_md: body.into(), ..Default::default() }).await.unwrap();

        // Find it, then swap the body
        assert_eq!(db.note_ids_with_text("data:image/").await.unwrap(), vec![note.id]);
        let new_body = "Look ![pic](attachments/0123456789abcdef0123456789abcdef.png) here";
        assert!(db.replace_note_body_in_place(note.id, body, new_body).await.unwrap());

        // Same timestamps, no version, new body and search row
        let after = db.get_note(note.id).await.unwrap();
        assert_eq!(after.body_md, new_body);
        assert_eq!(after.updated_at, note.updated_at);
        assert!(db.list_note_versions(note.id).await.unwrap().is_empty());
        assert!(db.note_ids_with_text("data:image/").await.unwrap().is_empty());

        // A stale old body writes nothing
        assert!(!db.replace_note_body_in_place(note.id, body, "x").await.unwrap());
        assert_eq!(db.get_note(note.id).await.unwrap().body_md, new_body);
    }

    /// Versions are found and rewritten too, and references are read from every body column.
    #[tokio::test]
    async fn rewrites_versions_and_lists_references() {
        let db = Db::open_in_memory().await.unwrap();
        let inline = "![a](data:image/png;base64,AAAA)";
        let note = db.create_note(NewNote { title: "V".into(), body_md: inline.into(), ..Default::default() }).await.unwrap();
        db.set_note_body(note.id, "next", Author::Ai, None).await.unwrap();

        // The AI write snapshotted the inline body
        let versions = db.version_ids_with_text("data:image/").await.unwrap();
        assert_eq!(versions.len(), 1);
        let linked = "![a](attachments/0123456789abcdef0123456789abcdef.png)";
        assert!(db.replace_version_body_in_place(versions[0], inline, linked).await.unwrap());
        assert_eq!(db.version_body_raw(versions[0]).await.unwrap().as_deref(), Some(linked));

        // The rewritten version is a reference
        let mut seen = Vec::new();
        db.for_each_text_containing("attachments/", |t| seen.push(t.to_string())).await.unwrap();
        assert_eq!(seen, vec![linked.to_string()]);
    }
}
