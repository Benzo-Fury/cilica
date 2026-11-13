//! Settings › Import: bring notes in from other apps (docs/import.md). The work lives in
//! `cilica-import`; this module wires it to commands, the keychain and the OS's folders.
//!
//! API keys for account imports are kept in the OS keychain (service `app.cilica.desktop`, account
//! `import.<provider>`). They go in through `import_save_key` and never come back over IPC.

use std::path::PathBuf;
use std::sync::Arc;

use cilica_domain::*;
use cilica_import::{Importer, KnownPaths, Secrets};
use tauri::{AppHandle, Manager, State};

use crate::core::Core;

fn account(provider: &str) -> String {
    format!("import.{provider}")
}

/// Import keys, read from the keychain when an import needs one.
struct Keychain;

impl Secrets for Keychain {
    fn key(&self, provider: &str) -> Option<String> {
        crate::keychain::read(&account(provider)).ok().flatten().filter(|k| !k.trim().is_empty())
    }
}

/// Where other apps keep their data on this OS.
fn known_paths(app: &AppHandle) -> KnownPaths {
    let p = app.path();
    let home = p.home_dir().unwrap_or_default();
    let config_dir = p.config_dir().unwrap_or_else(|_| home.clone());
    let data_dir = p.data_dir().unwrap_or_else(|_| config_dir.clone());
    let local_data_dir = p.local_data_dir().unwrap_or_else(|_| data_dir.clone());
    #[cfg(target_os = "macos")]
    let applications = vec![PathBuf::from("/Applications"), home.join("Applications"), PathBuf::from("/System/Applications")];
    #[cfg(not(target_os = "macos"))]
    let applications: Vec<PathBuf> = Vec::new();
    KnownPaths { home, config_dir, data_dir, local_data_dir, applications }
}

pub fn setup(app: &AppHandle, core: &Core) -> AppResult<()> {
    let importer = Importer::new(core.db.clone(), core.events.clone(), known_paths(app), Arc::new(Keychain));
    app.manage(importer);
    Ok(())
}

type Svc<'a> = State<'a, Arc<Importer>>;

/// What Cilica can find on this computer for one app: its data with counts, whether the app is
/// installed, and whether an API key is saved.
#[tauri::command]
#[specta::specta]
pub async fn import_detect(importer: Svc<'_>, provider: String) -> AppResult<ImportDetection> {
    Ok(importer.detect(&provider).await?)
}

/// Every app with something on this computer, for the picker's "Found" badges.
#[tauri::command]
#[specta::specta]
pub async fn import_detect_all(importer: Svc<'_>) -> AppResult<Vec<ImportDetection>> {
    Ok(importer.detect_all().await)
}

/// Check an API key and, if it works, keep it in the keychain. A key that does not work is not saved.
#[tauri::command]
#[specta::specta]
pub async fn import_save_key(importer: Svc<'_>, provider: String, key: String) -> AppResult<ImportKeyStatus> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(AppError::invalid("Paste the API key first."));
    }
    let status = importer.check_key(&provider, &key).await;
    if status.ok {
        crate::keychain::write(&account(&provider), &key)
            .map_err(|_| AppError::unavailable("Cilica could not save the key in your keychain. Check that the keychain is unlocked and try again."))?;
    }
    Ok(status)
}

/// Remove a saved import key.
#[tauri::command]
#[specta::specta]
pub async fn import_clear_key(provider: String) -> AppResult<()> {
    match crate::keychain::delete(&account(&provider)) {
        Ok(()) => Ok(()),
        Err(_) => Err(AppError::unavailable("Cilica could not remove the key from your keychain. Try again.")),
    }
}

/// Read a source and say what importing it would do. Nothing is written.
#[tauri::command]
#[specta::specta]
pub async fn import_scan(importer: Svc<'_>, provider: String, input: ImportInput) -> AppResult<ImportPreview> {
    Ok(importer.scan(&provider, input).await?)
}

/// Start importing a preview in the background. Progress arrives as `import_progress` events.
#[tauri::command]
#[specta::specta]
pub async fn import_start(importer: Svc<'_>, scan_id: Id, options: ImportOptions) -> AppResult<ImportRun> {
    Ok(importer.inner().start(scan_id, options).await?)
}

/// Stop an import after the note in flight. Notes already imported stay.
#[tauri::command]
#[specta::specta]
pub async fn import_cancel(importer: Svc<'_>, run_id: Id) -> AppResult<()> {
    importer.cancel(run_id);
    Ok(())
}

/// Move every note an import of this session created to Trash. Returns how many moved.
#[tauri::command]
#[specta::specta]
pub async fn import_undo(importer: Svc<'_>, run_id: Id) -> AppResult<u32> {
    Ok(importer.undo(run_id).await?)
}
