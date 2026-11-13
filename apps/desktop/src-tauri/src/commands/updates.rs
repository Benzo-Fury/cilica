//! App updates from GitHub Releases, through the Tauri updater plugin.
//!
//! The updater reads `latest.json` from the endpoint in `tauri.conf.json` and only accepts
//! packages signed with the updater key whose public half is in that file.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use cilica_domain::*;

use crate::core::Core;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::{Error as UpdaterError, Update, UpdaterExt};

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
        check_error(&e)
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
pub async fn install_update(core: State<'_, Core>, pending: State<'_, PendingUpdate>) -> AppResult<()> {
    // Take the update found by the last check
    let update = pending
        .0
        .lock()
        .map_err(|_| AppError::internal("Update state is unavailable."))?
        .take()
        .ok_or_else(|| AppError::invalid("There is no update to install. Check for updates first."))?;

    // Download, reporting progress at most every 100 ms, then verify the signature and install
    let events = core.events.clone();
    let mut downloaded: u64 = 0;
    let mut last: Option<Instant> = None;
    let on_chunk = |chunk: usize, total: Option<u64>| {
        downloaded += chunk as u64;
        let finished = total.is_some_and(|t| downloaded >= t);
        if !finished && last.is_some_and(|t| t.elapsed() < Duration::from_millis(100)) {
            return;
        }
        last = Some(Instant::now());
        events.emit(AppEvent::UpdateDownload {
            downloaded_bytes: downloaded as f64,
            total_bytes: total.map(|t| t as f64),
        });
    };
    update.download_and_install(on_chunk, || {}).await.map_err(|e| {
        tracing::warn!("update install failed: {e}");
        install_error(&e)
    })
}

/// Restart Cilica, finishing an installed update.
#[tauri::command]
#[specta::specta]
pub async fn restart_app(app: AppHandle) -> AppResult<()> {
    app.restart()
}

// ───────── Errors ─────────

/// Says why a check failed. A release that is still being published is the common case: its
/// `latest.json` is missing, or lists other platforms before this one is uploaded.
fn check_error(error: &UpdaterError) -> AppError {
    match error {
        UpdaterError::TargetNotFound(_) | UpdaterError::TargetsNotFound(_) => AppError::unavailable(
            "A new version is out, but the download for this computer is not ready yet. Try again in a few minutes.",
        ),
        UpdaterError::ReleaseNotFound => AppError::unavailable(
            "The latest release is still being published. Try again in a few minutes.",
        ),
        UpdaterError::Reqwest(e) if e.is_timeout() => {
            AppError::upstream("GitHub took too long to answer. Check your connection and try again.")
        }
        UpdaterError::Reqwest(e) if e.is_connect() || e.is_request() => {
            AppError::upstream("Cilica could not reach GitHub. Check your connection and try again.")
        }
        UpdaterError::Reqwest(e) if e.status().is_some() => AppError::upstream(format!(
            "GitHub answered with an error ({}). Try again later.",
            e.status().map(|s| s.as_u16()).unwrap_or_default()
        )),
        UpdaterError::Serialization(_) | UpdaterError::Semver(_) => AppError::upstream(
            "The latest release has update details Cilica cannot read. Try again later, or download it from GitHub.",
        ),
        UpdaterError::UnsupportedArch | UpdaterError::UnsupportedOs => {
            AppError::unavailable("Updates are not available for this computer. Download new versions from GitHub.")
        }
        other => AppError::upstream(format!("Cilica could not check for updates: {other}.")),
    }
}

/// Says why downloading or installing an update failed.
fn install_error(error: &UpdaterError) -> AppError {
    match error {
        UpdaterError::Reqwest(_) | UpdaterError::Network(_) => AppError::upstream(
            "The update did not download. Check your connection and try again.",
        ),
        UpdaterError::Minisign(_)
        | UpdaterError::Base64(_)
        | UpdaterError::SignatureUtf8(_)
        | UpdaterError::SignedVersionMismatch { .. }
        | UpdaterError::MissingSignedVersion => AppError::upstream(
            "The update's signature did not match, so it was not installed. Download it from GitHub instead.",
        ),
        UpdaterError::Io(_)
        | UpdaterError::TempDirNotFound
        | UpdaterError::TempDirNotOnSameMountPoint
        | UpdaterError::FailedToDetermineExtractPath => AppError::upstream(
            "Cilica could not replace itself. Check it is in Applications and you can write there, then try again.",
        ),
        UpdaterError::AuthenticationFailed => {
            AppError::invalid("The update needs your password to install. Try again and allow it.")
        }
        other => AppError::upstream(format!(
            "The update did not install: {other}. Try again, or download it from GitHub."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_platform_says_the_build_is_not_ready() {
        let err = check_error(&UpdaterError::TargetNotFound("darwin-aarch64".into()));
        assert!(err.message.contains("not ready yet"), "{}", err.message);
    }

    #[test]
    fn missing_release_json_says_it_is_being_published() {
        let err = check_error(&UpdaterError::ReleaseNotFound);
        assert!(err.message.contains("still being published"), "{}", err.message);
    }

    #[test]
    fn bad_signature_is_named_on_install() {
        let err = install_error(&UpdaterError::MissingSignedVersion);
        assert!(err.message.contains("signature"), "{}", err.message);
    }
}
