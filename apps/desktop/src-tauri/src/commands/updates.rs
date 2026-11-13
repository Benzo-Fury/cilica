//! App updates from GitHub Releases, through the Tauri updater plugin.
//!
//! The updater reads `latest.json` from the endpoint in `tauri.conf.json` and only accepts
//! packages signed with the updater key whose public half is in that file.

use std::sync::Mutex;

use cilica_domain::*;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

// ───────── State ─────────

/// The update found by the last check, kept so `install_update` installs exactly that one.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

/// A newer version that can be installed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct UpdateInfo {
    pub version: String,
    /// Release notes from the GitHub release, as Markdown.
    pub notes: Option<String>,
}

/// Registers the pending-update slot. Called once at startup.
pub fn setup(app: &AppHandle) {
    app.manage(PendingUpdate::default());
}

// ───────── Commands ─────────

/// Ask GitHub whether a newer version exists. Returns `None` when Cilica is up to date.
#[tauri::command]
#[specta::specta]
pub async fn check_for_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> AppResult<Option<UpdateInfo>> {
    // Look up the latest release
    let updater = app.updater().map_err(|e| {
        tracing::warn!("updater unavailable: {e}");
        AppError::unavailable("Updates are not available in this build.")
    })?;
    let update = updater.check().await.map_err(|e| {
        tracing::warn!("update check failed: {e}");
        AppError::upstream("Cilica could not check for updates. Check your connection and try again.")
    })?;

    // Remember it for install_update
    let info = update
        .as_ref()
        .map(|u| UpdateInfo { version: u.version.clone(), notes: u.body.clone() });
    *pending.0.lock().map_err(|_| AppError::internal("Update state is unavailable."))? = update;
    Ok(info)
}

/// Download and install the update found by the last check. Takes effect after a restart.
#[tauri::command]
#[specta::specta]
pub async fn install_update(pending: State<'_, PendingUpdate>) -> AppResult<()> {
    // Take the update found by the last check
    let update = pending
        .0
        .lock()
        .map_err(|_| AppError::internal("Update state is unavailable."))?
        .take()
        .ok_or_else(|| AppError::invalid("There is no update to install. Check for updates first."))?;

    // Download, verify the signature and install
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| {
        tracing::warn!("update install failed: {e}");
        AppError::upstream("The update did not install. Try again, or download it from GitHub.")
    })
}

/// Restart Cilica, finishing an installed update.
#[tauri::command]
#[specta::specta]
pub async fn restart_app(app: AppHandle) -> AppResult<()> {
    app.restart()
}
