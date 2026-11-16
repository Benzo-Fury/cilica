//! Synonyms for the selection toolbar, from the thesaurus bundled with the app. Nothing leaves the
//! machine.

use cilica_domain::*;
use cilica_thesaurus::Thesaurus;

/// Synonyms for a selected word or short phrase, grouped by meaning. Empty groups when there are none.
#[tauri::command]
#[specta::specta]
pub async fn get_synonyms(text: String) -> AppResult<Synonyms> {
    // The first lookup reads the thesaurus (a fraction of a second); keep it off the async runtime.
    tauri::async_runtime::spawn_blocking(move || Thesaurus::shared().lookup(&text))
        .await
        .map_err(|e| AppError::internal(format!("Cilica could not look up synonyms: {e}")))
}
