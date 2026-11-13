//! The swear jar: its counts, its word list, coins typed into notes, and emptying it by hand.
//!
//! Swears said in meetings need no command: the store reads them from transcript segments each time
//! the jar is counted, so they are counted whether or not the window is open.

use cilica_domain::chrono::Local;
use cilica_domain::*;
use tauri::State;

use crate::core::Core;

/// This week's jar (Monday at local midnight) and the eight weeks before, counted the way
/// Settings › Swear jar says.
#[tauri::command]
#[specta::specta]
pub async fn get_swear_jar(core: State<'_, Core>) -> AppResult<SwearJar> {
    let prefs = core.db.get_settings().await?.swear_jar;
    Ok(core.db.swear_jar(cilica_store::week_start(Local::now()), &prefs).await?)
}

/// Every word the jar listens for, so the note editor can spot one as it is typed.
#[tauri::command]
#[specta::specta]
pub async fn list_swear_words() -> AppResult<Vec<SwearWord>> {
    Ok(Swears::words())
}

/// A swear typed into a note. Counted only while the jar and "Notes you write" are on.
#[tauri::command]
#[specta::specta]
pub async fn add_swear_coin(core: State<'_, Core>, note_id: Option<Id>, word: String) -> AppResult<()> {
    let prefs = core.db.get_settings().await?.swear_jar;
    if !prefs.enabled || !prefs.notes {
        return Ok(());
    }
    core.db.add_swear_coin(note_id, &word, now()).await?;
    core.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::SwearJar], note_id: None });
    Ok(())
}

/// Empty this week's jar. Past weeks keep their coins.
#[tauri::command]
#[specta::specta]
pub async fn empty_swear_jar(core: State<'_, Core>) -> AppResult<()> {
    core.db.empty_swear_jar(now()).await?;
    core.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::SwearJar], note_id: None });
    Ok(())
}
