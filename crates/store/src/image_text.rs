//! Text read from note images (on-device OCR): `attachment_text` (one row per image file, by
//! content hash) and `image_text_fts` (one search row per image a note points at).
//!
//! `image_text_fts` is kept in step by the store, like `notes_fts`: every write of a note body
//! rebuilds that note's rows from the images it points at ([`sync_note_image_text`]), recording an
//! image's text adds rows for every note that points at it, and deleting a note drops its rows (a
//! trigger in the migration).

use std::collections::HashSet;

use cilica_domain::{Id, now};
use sqlx::{Row, Sqlite, SqliteExecutor, Transaction};

use crate::notes::index_text;
use crate::{Db, StoreResult, codec};

/// The folder part of an image reference in note Markdown.
const PREFIX: &str = "attachments/";

/// Hex digits in an attachment's content hash.
const HASH_LEN: usize = 32;

/// File extensions the attachments folder stores.
const EXTENSIONS: &[&str] = &["png", "jpg", "gif", "webp", "avif", "bmp"];

/// Where reading the images referenced by notes stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageTextBacklog {
    /// Distinct images referenced by notes (Trash too).
    pub total: u32,
    /// Referenced images not read yet, as file names (`<hash>.<ext>`), oldest notes first.
    pub pending: Vec<String>,
}

impl Db {
    // ── Reading the backlog ────────────────────────────────────────────

    /// Every image file referenced by a note body (Trash too), and which of them have no
    /// `attachment_text` row yet.
    pub async fn image_text_backlog(&self) -> StoreResult<ImageTextBacklog> {
        // Every distinct reference, in note order
        let bodies = sqlx::query_scalar::<_, String>("SELECT body_md FROM notes WHERE instr(body_md, ?) > 0 ORDER BY created_at")
            .bind(PREFIX)
            .fetch_all(self.pool())
            .await?;
        let mut seen = HashSet::new();
        let mut names = Vec::new();
        for body in &bodies {
            for name in image_names(body) {
                if seen.insert(hash_of(&name).to_string()) {
                    names.push(name);
                }
            }
        }

        // Drop the ones already read
        let read: HashSet<String> = sqlx::query_scalar::<_, String>("SELECT hash FROM attachment_text").fetch_all(self.pool()).await?.into_iter().collect();
        let total = names.len() as u32;
        let pending = names.into_iter().filter(|n| !read.contains(hash_of(n))).collect();
        Ok(ImageTextBacklog { total, pending })
    }

    // ── Recording text ─────────────────────────────────────────────────

    /// Record what was read in the image `name` (`<hash>.<ext>`): its text (empty when it has
    /// none), or the reason it couldn't be read (`error`, text stored empty). Either way it isn't
    /// read again. Notes that point at the image get search rows for its text.
    pub async fn set_attachment_text(&self, name: &str, text: &str, error: Option<&str>) -> StoreResult<()> {
        let hash = hash_of(name);
        let text = if error.is_some() { "" } else { text.trim() };
        let mut tx = self.begin_write().await?;

        // The image's row
        sqlx::query(
            "INSERT INTO attachment_text (hash, text, error, recognized_at) VALUES (?, ?, ?, ?) \
             ON CONFLICT(hash) DO UPDATE SET text = excluded.text, error = excluded.error, recognized_at = excluded.recognized_at",
        )
        .bind(hash)
        .bind(text)
        .bind(error)
        .bind(codec::ts(now()))
        .execute(&mut *tx)
        .await?;

        // Search rows for every note that points at it
        let notes = sqlx::query("SELECT id, body_md FROM notes WHERE instr(body_md, ?) > 0")
            .bind(format!("{PREFIX}{hash}."))
            .fetch_all(&mut *tx)
            .await?;
        for row in &notes {
            let id = codec::parse_id(&row.try_get::<String, _>("id")?)?;
            let body: String = row.try_get("body_md")?;
            sync_note_image_text(&mut tx, id, &body).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The text read in a note's images, as `(attachments/<name>, text)` in the order the body
    /// shows them. Images with no text are left out.
    pub async fn note_image_texts(&self, note_id: Id) -> StoreResult<Vec<(String, String)>> {
        let rows = sqlx::query("SELECT path, text FROM image_text_fts WHERE note_id = ? ORDER BY rowid")
            .bind(codec::id(note_id))
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(|r| Ok((r.try_get("path")?, r.try_get("text")?))).collect()
    }
}

// ── Keeping the search rows in step ────────────────────────────────────────

/// Rebuild a note's `image_text_fts` rows from the images `body_md` points at, using the text
/// already read for them. Called inside every transaction that writes a note body.
pub(crate) async fn sync_note_image_text(tx: &mut Transaction<'static, Sqlite>, note_id: Id, body_md: &str) -> StoreResult<()> {
    let id = codec::id(note_id);
    sqlx::query("DELETE FROM image_text_fts WHERE note_id = ?").bind(&id).execute(&mut **tx).await?;

    // One row per distinct image with text, in body order
    let mut seen = HashSet::new();
    for name in image_names(body_md) {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(text) = recognized_text(&mut **tx, hash_of(&name)).await? else { continue };
        sqlx::query("INSERT INTO image_text_fts (note_id, path, text) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(format!("{PREFIX}{name}"))
            .bind(index_text(&text))
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// The non-empty text read for an image, by hash.
async fn recognized_text<'e>(ex: impl SqliteExecutor<'e>, hash: &str) -> StoreResult<Option<String>> {
    Ok(sqlx::query_scalar::<_, String>("SELECT text FROM attachment_text WHERE hash = ? AND text != ''").bind(hash).fetch_optional(ex).await?)
}

// ── Finding references ─────────────────────────────────────────────────────

/// The attachment file names (`<32 hex>.<ext>`) a Markdown body points at, in order, repeats
/// included. Same rule as `cilica_attachments::referenced_names`, without a regex.
pub fn image_names(markdown: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = markdown;
    while let Some(at) = rest.find(PREFIX) {
        let after = &rest[at + PREFIX.len()..];
        if let Some(name) = name_at(after) {
            names.push(name.to_string());
        }
        rest = after;
    }
    names
}

/// `<32 lowercase hex>.<known ext>` at the start of `text`, ending at a word boundary.
fn name_at(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.len() < HASH_LEN + 2 || !bytes[..HASH_LEN].iter().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)) || bytes[HASH_LEN] != b'.' {
        return None;
    }
    let ext_start = HASH_LEN + 1;
    let ext_len = bytes[ext_start..].iter().take_while(|b| b.is_ascii_alphanumeric() || **b == b'_').count();
    let ext = &text[ext_start..ext_start + ext_len];
    EXTENSIONS.contains(&ext).then(|| &text[..ext_start + ext_len])
}

/// The hash part of a file name (`<hash>.<ext>` → `<hash>`).
fn hash_of(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use cilica_domain::{Author, NoteQuery, SearchSource};

    use super::*;
    use crate::NewNote;

    const A: &str = "0123456789abcdef0123456789abcdef";
    const B: &str = "fedcba9876543210fedcba9876543210";

    #[test]
    fn finds_image_names() {
        let body = format!("![x|center|300](attachments/{A}.png \"Cap\") attachments/{B}.jpg attachments/{A}.svg attachments/short.png attachments/{B}.pngx");
        assert_eq!(image_names(&body), vec![format!("{A}.png"), format!("{B}.jpg")]);
    }

    /// Text is stored once per image, found by search with the image, and follows body edits.
    #[tokio::test]
    async fn image_text_is_indexed_and_follows_the_body() {
        let db = Db::open_in_memory().await.unwrap();
        let body = format!("Receipt below\n\n![Receipt](attachments/{A}.png)");
        let note = db.create_note(NewNote { title: "Lunch".into(), body_md: body.clone(), ..Default::default() }).await.unwrap();
        let other = db.create_note(NewNote { title: "Other".into(), body_md: format!("![b](attachments/{B}.jpg)"), ..Default::default() }).await.unwrap();

        // Both images wait to be read
        let backlog = db.image_text_backlog().await.unwrap();
        assert_eq!((backlog.total, backlog.pending.clone()), (2, vec![format!("{A}.png"), format!("{B}.jpg")]));

        // Record text for one, nothing for the other
        db.set_attachment_text(&format!("{A}.png"), "Trattoria Bellavista\nTotal 42.50", None).await.unwrap();
        db.set_attachment_text(&format!("{B}.jpg"), "", Some("unreadable")).await.unwrap();
        assert_eq!(db.image_text_backlog().await.unwrap(), ImageTextBacklog { total: 2, pending: vec![] });

        // Search finds the note through its image
        let hits = db.search_notes("bellavista", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].note.id, note.id);
        assert_eq!(hits[0].source, SearchSource::Image);
        assert_eq!(hits[0].image.as_deref(), Some(format!("attachments/{A}.png").as_str()));
        assert!(hits[0].excerpt.contains("<mark>Bellavista</mark>"), "{}", hits[0].excerpt);
        assert_eq!(db.note_image_texts(note.id).await.unwrap(), vec![(format!("attachments/{A}.png"), "Trattoria Bellavista\nTotal 42.50".into())]);
        assert!(db.note_image_texts(other.id).await.unwrap().is_empty());

        // A title or body match ranks first, and is not repeated as an image hit
        let third = db.create_note(NewNote { title: "Bellavista".into(), body_md: format!("Again ![r](attachments/{A}.png)"), ..Default::default() }).await.unwrap();
        let hits = db.search_notes("bellavista", 10).await.unwrap();
        assert_eq!(hits.iter().map(|h| (h.note.id, h.source)).collect::<Vec<_>>(), vec![(third.id, SearchSource::Title), (note.id, SearchSource::Image)]);
        assert_eq!(hits[0].image, None);

        // Filters apply to image hits too
        let q = NoteQuery { text: "bellavista".into(), trash: cilica_domain::TrashScope::Only, ..Default::default() };
        assert!(db.find_notes(&q, 10).await.unwrap().is_empty());

        // Taking the image out of the body takes its text out of search
        db.set_note_body(note.id, "Receipt below", Author::User, None).await.unwrap();
        assert_eq!(db.search_notes("trattoria", 10).await.unwrap().iter().map(|h| h.note.id).collect::<Vec<_>>(), vec![third.id]);
        assert!(db.note_image_texts(note.id).await.unwrap().is_empty());

        // Putting it back brings it back without reading it again
        db.set_note_body(note.id, &body, Author::User, None).await.unwrap();
        assert_eq!(db.search_notes("trattoria", 10).await.unwrap().len(), 2);
        assert!(db.image_text_backlog().await.unwrap().pending.is_empty());
    }

    /// Deleting a note removes its image rows; the image's text stays for other notes.
    #[tokio::test]
    async fn deleting_a_note_removes_its_rows() {
        let db = Db::open_in_memory().await.unwrap();
        db.set_attachment_text(&format!("{A}.png"), "Quarterly roadmap", None).await.unwrap();
        let note = db.create_note(NewNote { title: "Plan".into(), body_md: format!("![p](attachments/{A}.png)"), ..Default::default() }).await.unwrap();
        assert_eq!(db.search_notes("roadmap", 10).await.unwrap().len(), 1);

        // Trash hides it, deleting it forever removes the rows
        db.trash_note(note.id).await.unwrap();
        assert!(db.search_notes("roadmap", 10).await.unwrap().is_empty());
        db.delete_note_forever(note.id).await.unwrap();
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM image_text_fts").fetch_one(db.pool()).await.unwrap();
        assert_eq!(rows, 0);
        let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM attachment_text").fetch_one(db.pool()).await.unwrap();
        assert_eq!(kept, 1);
    }
}
