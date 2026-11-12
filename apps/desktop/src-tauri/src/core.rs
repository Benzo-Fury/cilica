//! Shared state every command module can reach: the database, the event sink and the data directory.

use std::path::PathBuf;
use std::sync::Arc;

use margin_domain::{AppError, AppEvent, AppResult, Events};
use margin_store::Db;
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
            .map_err(|e| AppError::internal(format!("Margin could not find its data folder: {e}")))?;
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| AppError::internal(format!("Margin could not create its data folder: {e}")))?;
        let db = Db::open(&data_dir.join("margin.db")).await?.with_data_dir(&data_dir);
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
