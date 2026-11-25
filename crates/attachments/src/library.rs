//! Attachments across the whole database: moving inline images out of every note and version, and
//! finding which attachment files are still pointed at so the rest can be cleaned up.

use std::collections::HashSet;
use std::time::Duration;

use cilica_domain::{AppResult, Id};
use cilica_store::Db;

use crate::markdown::{externalize_inline_images, referenced_names};
use crate::store::{FOLDER, ImageStore};

/// What marks a body that still carries an inline image.
const INLINE_MARK: &str = "data:image/";

/// What one [`NoteImages::externalize_all`] pass changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExternalizeReport {
    /// Notes whose body was rewritten (the UI should reload them).
    pub notes: Vec<Id>,
    /// Versions rewritten.
    pub versions: u32,
    /// Inline images saved as files.
    pub images: u32,
    /// Inline images that couldn't be kept as files and stay inline.
    pub failed: u32,
}

/// The database and the attachments folder together. Cheap to clone.
#[derive(Clone)]
pub struct NoteImages {
    db: Db,
    store: ImageStore,
}

impl NoteImages {
    /// Work on `db`'s notes with images saved into `store`.
    pub fn new(db: Db, store: ImageStore) -> Self {
        Self { db, store }
    }

    /// The attachments folder.
    pub fn store(&self) -> &ImageStore {
        &self.store
    }

    /// Rewrite every note body (Trash included) and every version that still carries inline
    /// `data:` images so they point at files instead. In place: no new version, `updated_at`
    /// unchanged; snippet, search and links follow. A body edited meanwhile is left for next time.
    pub async fn externalize_all(&self) -> AppResult<ExternalizeReport> {
        let mut report = ExternalizeReport::default();

        // Note bodies
        for id in self.db.note_ids_with_text(INLINE_MARK).await? {
            let Some(body) = self.db.note_body_raw(id).await? else { continue };
            let done = externalize_inline_images(&body, &self.store).await;
            report.images += done.converted;
            report.failed += done.failed;
            if done.converted > 0 && self.db.replace_note_body_in_place(id, &body, &done.markdown).await? {
                report.notes.push(id);
            }
        }

        // Versions
        for id in self.db.version_ids_with_text(INLINE_MARK).await? {
            let Some(body) = self.db.version_body_raw(id).await? else { continue };
            let done = externalize_inline_images(&body, &self.store).await;
            report.images += done.converted;
            report.failed += done.failed;
            if done.converted > 0 && self.db.replace_version_body_in_place(id, &body, &done.markdown).await? {
                report.versions += 1;
            }
        }
        Ok(report)
    }

    /// Every attachment file name pointed at from anything that stores note Markdown: notes
    /// (Trash too), versions, proposals, the assistant's audit and undo records, chat messages and
    /// task summaries.
    pub async fn collect_references(&self) -> AppResult<HashSet<String>> {
        let mut names = HashSet::new();
        self.db.for_each_text_containing(&format!("{FOLDER}/"), |text| names.extend(referenced_names(text))).await?;
        Ok(names)
    }

    /// Delete attachment files nothing points at and older than `min_age`. Returns how many went.
    pub async fn clean_up(&self, min_age: Duration) -> AppResult<u32> {
        let referenced = self.collect_references().await?;
        self.store.gc(&referenced, min_age).await
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use cilica_domain::Author;
    use cilica_store::NewNote;
    use image::ImageFormat;

    use super::*;
    use crate::format::tests::encoded;

    #[tokio::test]
    async fn moves_every_inline_image_out_and_keeps_what_is_used() {
        let temp = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().await.unwrap();
        let store = ImageStore::new(temp.path().join(FOLDER));
        let images = NoteImages::new(db.clone(), store.clone());

        // A note with an inline image, edited by the AI so a version keeps the old body too
        let png = base64::engine::general_purpose::STANDARD.encode(encoded(3, 3, ImageFormat::Png));
        let body = format!("Before ![pic](data:image/png;base64,{png}) after");
        let note = db.create_note(NewNote { title: "Pics".into(), body_md: body.clone(), ..Default::default() }).await.unwrap();
        db.set_note_body(note.id, &format!("{body}\n\nMore"), Author::Ai, None).await.unwrap();
        let before = db.get_note(note.id).await.unwrap();

        // One pass rewrites the note and the version, in place
        let report = images.externalize_all().await.unwrap();
        assert_eq!((report.notes.clone(), report.versions, report.images, report.failed), (vec![note.id], 1, 2, 0));
        let after = db.get_note(note.id).await.unwrap();
        assert!(after.body_md.starts_with("Before ![pic](attachments/") && after.body_md.ends_with(" after\n\nMore"));
        assert_eq!(after.updated_at, before.updated_at);
        assert_eq!(db.list_note_versions(note.id).await.unwrap().len(), 1);

        // A second pass has nothing to do
        assert_eq!(images.externalize_all().await.unwrap(), ExternalizeReport::default());

        // The one file is referenced; an orphan is cleaned up
        let refs = images.collect_references().await.unwrap();
        assert_eq!(refs.len(), 1);
        let orphan = store.save_bytes(encoded(5, 5, ImageFormat::Png), None, Default::default()).await.unwrap();
        assert_eq!(images.clean_up(Duration::ZERO).await.unwrap(), 1);
        assert!(!store.absolute(&orphan.path).unwrap().exists());
        let kept = refs.into_iter().next().unwrap();
        assert!(store.dir().join(kept).is_file());
    }
}
