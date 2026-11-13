//! Notes, categories, versions, trash, search. Owner: Core agent.
//!
//! Every command here is part of the IPC contract in `src/ipc/bindings.ts`. Keep names, arguments and
//! return types as they are unless the orchestrator agrees to a change.
//!
//! UI-initiated writes do not emit `data_changed` (the UI updates its own cache). The exceptions are
//! side effects the UI did not ask for: the Trash purge, and renumbering siblings after a move.

use std::path::PathBuf;
use std::time::Duration;

use cilica_domain::*;
use cilica_store::NewNote;
use tauri::{AppHandle, State};

use crate::core::Core;

/// Trash keeps notes this long.
const TRASH_DAYS: u32 = 30;

/// Called once at startup, after the database is open: purge old Trash now and about daily, and
/// repair any search rows a crash left behind.
pub fn setup(app: &AppHandle, core: &Core) -> AppResult<()> {
    let _ = app;
    let db = core.db.clone();
    let events = core.events.clone();
    tauri::async_runtime::spawn(async move {
        match db.repair_search_index().await {
            Ok(0) => {}
            Ok(n) => tracing::info!(notes = n, "rebuilt missing search rows"),
            Err(error) => tracing::warn!(%error, "could not check the search index"),
        }
        // Read links from notes written before the link index existed (once), so the first graph opens fast.
        match db.refresh_link_index().await {
            Ok(0) => {}
            Ok(n) => tracing::info!(notes = n, "indexed links"),
            Err(error) => tracing::warn!(%error, "could not index links"),
        }
        // Wall-clock check every hour, so a laptop that sleeps through the night still purges.
        let mut last_purge: Option<Timestamp> = None;
        loop {
            if last_purge.is_none_or(|t| now() - t >= chrono::TimeDelta::hours(24)) {
                match db.purge_trash_older_than(TRASH_DAYS).await {
                    Ok(0) => {}
                    Ok(n) => {
                        tracing::info!(notes = n, "purged old Trash");
                        events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Trash], note_id: None });
                    }
                    Err(error) => tracing::warn!(%error, "could not purge old Trash"),
                }
                last_purge = Some(now());
            }
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
    Ok(())
}

/// Facts for Home from the last seven days of meetings: the word said most and lines worth
/// hearing again. Counted locally; nothing leaves the machine.
#[tauri::command]
#[specta::specta]
pub async fn get_home_glance(core: State<'_, Core>) -> AppResult<HomeGlance> {
    Ok(core.db.home_glance(now() - chrono::TimeDelta::days(7)).await?)
}

/// Every note that is not in Trash, newest first within its category.
#[tauri::command]
#[specta::specta]
pub async fn list_notes(core: State<'_, Core>) -> AppResult<Vec<NoteSummary>> {
    Ok(core.db.list_notes().await?)
}

/// Notes in Trash, most recently deleted first.
#[tauri::command]
#[specta::specta]
pub async fn list_trash(core: State<'_, Core>) -> AppResult<Vec<NoteSummary>> {
    Ok(core.db.list_trash().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn get_note(core: State<'_, Core>, note_id: Id) -> AppResult<Note> {
    Ok(core.db.get_note(note_id).await?)
}

/// A new empty manual note at the top of the category (or of Notes).
#[tauri::command]
#[specta::specta]
pub async fn create_note(core: State<'_, Core>, category_id: Option<Id>) -> AppResult<Note> {
    Ok(core.db.create_note(NewNote { category_id, ..Default::default() }).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn rename_note(core: State<'_, Core>, note_id: Id, title: String) -> AppResult<NoteSummary> {
    let before = core.db.get_note_summary(note_id).await?.title;
    let summary = core.db.rename_note(note_id, &title).await?;
    // Links to the old title follow the note, as in Obsidian. Those notes changed without the UI asking.
    let relinked = core.db.relink_after_rename(note_id, &before, &summary.title).await?;
    if !relinked.is_empty() {
        core.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Notes], note_id: None });
    }
    Ok(summary)
}

/// The link graph: every note, plus the people, tags and notes not created yet they link to.
/// With `focus`, only what is within `depth` links (1–3) of that note: the local graph.
#[tauri::command]
#[specta::specta]
pub async fn get_link_graph(core: State<'_, Core>, focus: Option<Id>, depth: u32) -> AppResult<LinkGraph> {
    Ok(core.db.link_graph(focus, depth).await?)
}

/// Notes that link to this one, and notes that mention its title without a link.
#[tauri::command]
#[specta::specta]
pub async fn get_backlinks(core: State<'_, Core>, note_id: Id) -> AppResult<Backlinks> {
    Ok(core.db.backlinks(note_id).await?)
}

/// Turn the first unlinked mention of `target_note_id`'s title in `source_note_id` into a `[[link]]`.
#[tauri::command]
#[specta::specta]
pub async fn link_mention(core: State<'_, Core>, source_note_id: Id, target_note_id: Id) -> AppResult<NoteSummary> {
    Ok(core.db.link_mention(source_note_id, target_note_id).await?)
}

/// Save the editor's Markdown. Called debounced while typing; the store decides when to snapshot a version.
#[tauri::command]
#[specta::specta]
pub async fn update_note_body(core: State<'_, Core>, note_id: Id, body_md: String) -> AppResult<NoteSummary> {
    Ok(core.db.set_note_body(note_id, &body_md, Author::User, None).await?)
}

/// Move a note into a category (or out, with `None`) at a fractional position.
#[tauri::command]
#[specta::specta]
pub async fn move_note(core: State<'_, Core>, note_id: Id, category_id: Option<Id>, position: f64) -> AppResult<NoteSummary> {
    let (note, renumbered) = core.db.move_note_rebalanced(note_id, category_id, position).await?;
    if renumbered {
        core.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Notes], note_id: None });
    }
    Ok(note)
}

/// Soft delete. Trash keeps notes for 30 days.
#[tauri::command]
#[specta::specta]
pub async fn trash_note(core: State<'_, Core>, note_id: Id) -> AppResult<()> {
    Ok(core.db.trash_note(note_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn restore_note(core: State<'_, Core>, note_id: Id) -> AppResult<NoteSummary> {
    Ok(core.db.restore_note(note_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_note_forever(core: State<'_, Core>, note_id: Id) -> AppResult<()> {
    Ok(core.db.delete_note_forever(note_id).await?)
}

/// Move several notes to Trash at once. Returns how many were moved.
#[tauri::command]
#[specta::specta]
pub async fn trash_notes(core: State<'_, Core>, note_ids: Vec<Id>) -> AppResult<u32> {
    Ok(core.db.trash_notes(&note_ids).await?)
}

/// Bring several notes back from Trash, as a block on top of their categories in the order given.
#[tauri::command]
#[specta::specta]
pub async fn restore_notes(core: State<'_, Core>, note_ids: Vec<Id>) -> AppResult<Vec<NoteSummary>> {
    Ok(core.db.restore_notes(&note_ids).await?)
}

/// Delete several notes from Trash for good. Notes not in Trash are left alone. Returns how many.
#[tauri::command]
#[specta::specta]
pub async fn delete_notes_forever(core: State<'_, Core>, note_ids: Vec<Id>) -> AppResult<u32> {
    Ok(core.db.delete_notes_forever(&note_ids).await?)
}

/// Move several notes into a category (or out, with `None`) as one block in the order given:
/// from `position` up to the next note, or to the top when `position` is `None`.
#[tauri::command]
#[specta::specta]
pub async fn move_notes(core: State<'_, Core>, note_ids: Vec<Id>, category_id: Option<Id>, position: Option<f64>) -> AppResult<Vec<NoteSummary>> {
    let (notes, renumbered) = core.db.move_notes(&note_ids, category_id, position).await?;
    if renumbered {
        core.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Notes], note_id: None });
    }
    Ok(notes)
}

/// Drop a note that has nothing in it (no title, body, recording or transcript). Returns whether
/// it was removed.
#[tauri::command]
#[specta::specta]
pub async fn discard_empty_note(core: State<'_, Core>, note_id: Id) -> AppResult<bool> {
    Ok(core.db.discard_empty_note(note_id).await?)
}

/// Returns how many notes were removed.
#[tauri::command]
#[specta::specta]
pub async fn empty_trash(core: State<'_, Core>) -> AppResult<u32> {
    Ok(core.db.empty_trash().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn list_categories(core: State<'_, Core>) -> AppResult<Vec<Category>> {
    Ok(core.db.list_categories().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn create_category(core: State<'_, Core>, name: String) -> AppResult<Category> {
    Ok(core.db.create_category(&name).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn rename_category(core: State<'_, Core>, category_id: Id, name: String) -> AppResult<Category> {
    Ok(core.db.rename_category(category_id, &name).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn reorder_category(core: State<'_, Core>, category_id: Id, position: f64) -> AppResult<Category> {
    let (category, renumbered) = core.db.reorder_category_rebalanced(category_id, position).await?;
    if renumbered {
        core.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Categories], note_id: None });
    }
    Ok(category)
}

#[tauri::command]
#[specta::specta]
pub async fn set_category_collapsed(core: State<'_, Core>, category_id: Id, collapsed: bool) -> AppResult<Category> {
    Ok(core.db.set_category_collapsed(category_id, collapsed).await?)
}

/// Deletes the category; its notes move to uncategorised.
#[tauri::command]
#[specta::specta]
pub async fn delete_category(core: State<'_, Core>, category_id: Id) -> AppResult<()> {
    Ok(core.db.delete_category(category_id).await?)
}

/// Full-text search over titles, bodies and transcripts.
#[tauri::command]
#[specta::specta]
pub async fn search_notes(core: State<'_, Core>, query: String, limit: u32) -> AppResult<Vec<SearchHit>> {
    Ok(core.db.search_notes(&query, limit).await?)
}

/// Things said in meetings that are close to a search with no results.
#[tauri::command]
#[specta::specta]
pub async fn near_words(core: State<'_, Core>, query: String) -> AppResult<Vec<NearWord>> {
    Ok(core.db.near_words(&query).await?)
}

/// Newest first.
#[tauri::command]
#[specta::specta]
pub async fn list_note_versions(core: State<'_, Core>, note_id: Id) -> AppResult<Vec<NoteVersion>> {
    Ok(core.db.list_note_versions(note_id).await?)
}

/// Snapshots the current body first, then restores.
#[tauri::command]
#[specta::specta]
pub async fn restore_note_version(core: State<'_, Core>, version_id: Id) -> AppResult<Note> {
    Ok(core.db.restore_note_version(version_id, Author::User).await?)
}

/// Write the note as a .md file to `path`: the title as an H1, then the body.
#[tauri::command]
#[specta::specta]
pub async fn export_note_markdown(core: State<'_, Core>, note_id: Id, path: String) -> AppResult<()> {
    let note = core.db.get_note(note_id).await?;
    if path.trim().is_empty() {
        return Err(AppError::invalid("Choose where to save the file."));
    }
    let mut path = PathBuf::from(path);
    if path.extension().is_none() {
        path.set_extension("md");
    }
    tokio::fs::write(&path, note_markdown(&note)).await.map_err(|e| {
        AppError::internal(format!("Cilica could not save {}: {e}. Check that the folder exists and you can write to it.", path.display()))
    })
}

/// `# Title` then the body, unless the body already opens with that heading.
fn note_markdown(note: &Note) -> String {
    let title = if note.title.trim().is_empty() { "Untitled" } else { note.title.trim() };
    let heading = format!("# {title}");
    let body = note.body_md.trim_matches('\n');
    let starts_with_heading = body.lines().find(|l| !l.trim().is_empty()).is_some_and(|l| l.trim() == heading);
    let mut out = if starts_with_heading { String::new() } else { format!("{heading}\n\n") };
    if !body.trim().is_empty() {
        out.push_str(body);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(title: &str, body: &str) -> Note {
        Note {
            id: new_id(),
            title: title.into(),
            body_md: body.into(),
            snippet: String::new(),
            category_id: None,
            kind: NoteKind::Manual,
            meeting_app: None,
            started_at: None,
            ended_at: None,
            position: 0.0,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        }
    }

    #[test]
    fn markdown_export() {
        assert_eq!(note_markdown(&note("Plan", "- one\n- two\n")), "# Plan\n\n- one\n- two\n");
        assert_eq!(note_markdown(&note("", "")), "# Untitled\n\n");
        assert_eq!(note_markdown(&note("Plan", "# Plan\n\nBody")), "# Plan\n\nBody\n");
    }
}
