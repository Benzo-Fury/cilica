//! Updates on Windows, through Cilica Setup.
//!
//! Tauri's updater would run the new NSIS installer with its own progress window the moment the
//! download finishes, and quit Cilica to do it. Instead the verified installer is kept in the
//! temporary folder until Restart, which starts the copy of Cilica Setup installed next to Cilica
//! (`apps/setup`, bundled by `tauri.windows-setup.conf.json`) to install it under Setup's own
//! window and open Cilica again. Without that copy (a build made without Setup) or for a package
//! Setup can't run (an MSI), Restart hands the bytes to Tauri's updater as before.

use std::path::PathBuf;
use std::process::Command;

use cilica_domain::{AppError, AppResult};
use tauri_plugin_updater::Update;

use super::install_error;

/// The copy of Cilica Setup next to Cilica.
const SETUP_EXE: &str = "cilica-setup.exe";
/// Setup's flags for an update (`UPDATE_FLAG`, `VERSION_FLAG` in apps/setup/src/plan.rs).
const UPDATE_FLAG: &str = "--update";
const VERSION_FLAG: &str = "--version";
/// The temporary folder's subfolder the installer waits in. Setup deletes it when it is done.
const STAGING_DIR: &str = "cilica-update";

/// A downloaded, verified update waiting for Restart.
pub struct Staged {
    update: Update,
    how: How,
}

enum How {
    /// Cilica Setup installs the NSIS installer saved at `installer`.
    Setup { setup: PathBuf, installer: PathBuf },
    /// Tauri's updater installs these bytes.
    Updater(Vec<u8>),
}

/// Keep the downloaded `bytes` (already checked against the update's signature) for Restart.
pub fn stage(update: Update, bytes: Vec<u8>) -> AppResult<Staged> {
    // An NSIS installer is a plain program ("MZ"); an MSI or a zipped package stays with the updater.
    let how = match setup_exe() {
        Some(setup) if bytes.starts_with(b"MZ") => How::Setup { installer: save(&update.version, &bytes)?, setup },
        _ => How::Updater(bytes),
    };
    Ok(Staged { update, how })
}

/// Install the staged update. Returns once Cilica Setup has started, and Cilica should quit; the
/// updater's fallback quits Cilica itself.
pub fn apply(staged: Staged) -> AppResult<()> {
    let Staged { update, how } = staged;
    match how {
        How::Setup { setup, installer } => {
            // Check the file again: it sat in the temporary folder since the download.
            let bytes = std::fs::read(&installer).map_err(|e| {
                AppError::upstream(format!("The downloaded update is missing ({e}). Check for updates again in Settings › General."))
            })?;
            update.verify(&bytes).map_err(|e| install_error(&e))?;
            let started = Command::new(&setup).arg(UPDATE_FLAG).arg(&installer).args([VERSION_FLAG, update.version.as_str()]).spawn();
            match started {
                Ok(_) => Ok(()),
                Err(error) => {
                    tracing::warn!(%error, "Cilica Setup didn't start; installing with the updater instead");
                    update.install(bytes).map_err(|e| install_error(&e))
                }
            }
        }
        How::Updater(bytes) => update.install(bytes).map_err(|e| install_error(&e)),
    }
}

/// Cilica Setup next to Cilica's program file, when this install has it.
fn setup_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(SETUP_EXE)).filter(|path| path.is_file())
}

/// Write the installer to the temporary folder, replacing any earlier download that never ran.
fn save(version: &str, bytes: &[u8]) -> AppResult<PathBuf> {
    let dir = std::env::temp_dir().join(STAGING_DIR);
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join(format!("Cilica_{version}_x64-setup.exe"));
    std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, bytes)).map_err(|e| {
        AppError::upstream(format!("Cilica couldn't save the update to the temporary folder: {e}. Free up some space and try again."))
    })?;
    Ok(path)
}
