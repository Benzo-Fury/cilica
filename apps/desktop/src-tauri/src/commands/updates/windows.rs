//! Updates on Windows, through Cilica Setup.
//!
//! The update package on Windows is Cilica Setup itself (`Cilica_<version>_x64_Setup.exe`, see
//! `apps/setup`), the same file people download. Tauri's updater would run it the moment the
//! download finishes and quit Cilica to do it. Instead the verified package is kept in the
//! temporary folder until Restart, which starts it with `--update` to install the Cilica it
//! carries under Setup's own window and open Cilica again. If it can't be started that way,
//! Restart hands the bytes to Tauri's updater, which starts it with `/P /UPDATE /R`, and Setup
//! answers that with the same window.

use std::path::PathBuf;
use std::process::Command;

use cilica_domain::{AppError, AppResult};
use tauri_plugin_updater::Update;

use super::install_error;

/// Setup's flags for an update (`UPDATE_FLAG`, `VERSION_FLAG` in apps/setup/src/plan.rs).
const UPDATE_FLAG: &str = "--update";
const VERSION_FLAG: &str = "--version";
/// The temporary folder's subfolder the package waits in. Setup deletes it after it exits
/// (`downloaded_from` in apps/setup/src/lib.rs).
const STAGING_DIR: &str = "cilica-update";

/// A downloaded, verified update waiting for Restart.
pub struct Staged {
    update: Update,
    setup: PathBuf,
}

/// Keep the downloaded `bytes` (already checked against the update's signature) for Restart.
pub fn stage(update: Update, bytes: Vec<u8>) -> AppResult<Staged> {
    let setup = save(&update.version, &bytes)?;
    Ok(Staged { update, setup })
}

/// Install the staged update. Returns once Cilica Setup has started, and Cilica should quit; the
/// updater's fallback quits Cilica itself.
pub fn apply(staged: Staged) -> AppResult<()> {
    let Staged { update, setup } = staged;
    // Check the file again: it sat in the temporary folder since the download.
    let bytes = std::fs::read(&setup).map_err(|e| {
        AppError::upstream(format!("The downloaded update is missing ({e}). Check for updates again in Settings › General."))
    })?;
    update.verify(&bytes).map_err(|e| install_error(&e))?;
    let started = Command::new(&setup).args([UPDATE_FLAG, VERSION_FLAG, update.version.as_str()]).spawn();
    match started {
        Ok(_) => Ok(()),
        Err(error) => {
            tracing::warn!(%error, "Cilica Setup didn't start; installing with the updater instead");
            update.install(bytes).map_err(|e| install_error(&e))
        }
    }
}

/// Write the package to the temporary folder, replacing any earlier download that never ran.
fn save(version: &str, bytes: &[u8]) -> AppResult<PathBuf> {
    let dir = std::env::temp_dir().join(STAGING_DIR);
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join(format!("Cilica_{version}_x64_Setup.exe"));
    std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, bytes)).map_err(|e| {
        AppError::upstream(format!("Cilica couldn't save the update to the temporary folder: {e}. Free up some space and try again."))
    })?;
    Ok(path)
}
