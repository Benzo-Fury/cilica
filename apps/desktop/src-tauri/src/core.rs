//! Shared state every command module can reach: the database, the event sink and the data directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cilica_domain::{AppError, AppEvent, AppResult, Events};
use cilica_store::Db;
use tauri::{AppHandle, Manager};

use crate::events::TauriSink;

/// Managed Tauri state. Cheap to clone; area-specific services are managed separately by each
/// `commands::*::setup`.
#[derive(Clone)]
pub struct Core {
    pub db: Db,
    pub events: Events,
    /// The app data directory (database, recordings, models).
    pub data_dir: PathBuf,
    sink: Arc<TauriSink>,
}

impl Core {
    pub async fn init(app: &AppHandle) -> AppResult<Self> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::internal(format!("Cilica could not find its data folder: {e}")))?;
        move_legacy_data(&data_dir)
            .map_err(|e| AppError::internal(format!("Cilica could not move your notes over from Margin: {e}")))?;
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| AppError::internal(format!("Cilica could not create its data folder: {e}")))?;
        let db = Db::open(&data_dir.join("cilica.db")).await?.with_data_dir(&data_dir);
        let sink = Arc::new(TauriSink::new(app.clone()));
        let events: Events = sink.clone();
        Ok(Self { db, events, data_dir, sink })
    }

    /// Where recordings live. Store `notes.audio_path` relative to [`Core::data_dir`]
    /// (`recordings/<file>`); the webview can only play files under this folder.
    pub fn recordings_dir(&self) -> PathBuf {
        self.data_dir.join("recordings")
    }

    /// Run `listener` for every event emitted through [`Core::events`], after the webview gets it.
    pub fn on_event(&self, listener: impl Fn(&AppEvent) + Send + Sync + 'static) {
        self.sink.subscribe(listener);
    }
}

/// Folder name the app kept its data under while it was called Margin.
const LEGACY_DATA_FOLDER: &str = "app.margin.desktop";

/// On the first launch after the rename, moves the Margin data folder to `data_dir` and renames
/// its database. Does nothing once `data_dir` exists or when there is no Margin folder.
fn move_legacy_data(data_dir: &Path) -> std::io::Result<()> {
    let Some(legacy) = data_dir.parent().map(|p| p.join(LEGACY_DATA_FOLDER)) else { return Ok(()) };
    if data_dir.exists() || !legacy.is_dir() {
        return Ok(());
    }
    std::fs::rename(&legacy, data_dir)?;
    for suffix in ["", "-wal", "-shm"] {
        let old = data_dir.join(format!("margin.db{suffix}"));
        if old.exists() {
            std::fs::rename(old, data_dir.join(format!("cilica.db{suffix}")))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_the_margin_folder_once() {
        let root = std::env::temp_dir().join(format!("cilica-core-test-{}", std::process::id()));
        let legacy = root.join(LEGACY_DATA_FOLDER);
        std::fs::create_dir_all(legacy.join("recordings")).unwrap();
        std::fs::write(legacy.join("margin.db"), b"db").unwrap();
        std::fs::write(legacy.join("margin.db-wal"), b"wal").unwrap();

        let data_dir = root.join("app.cilica.desktop");
        move_legacy_data(&data_dir).unwrap();
        assert!(!legacy.exists());
        assert_eq!(std::fs::read(data_dir.join("cilica.db")).unwrap(), b"db");
        assert_eq!(std::fs::read(data_dir.join("cilica.db-wal")).unwrap(), b"wal");
        assert!(data_dir.join("recordings").is_dir());

        // A second Margin folder appearing later is left alone.
        std::fs::create_dir_all(&legacy).unwrap();
        move_legacy_data(&data_dir).unwrap();
        assert!(legacy.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
