//! SQLite storage for Margin: the only crate that talks SQL.
//!
//! [`Db`] is a cheap-to-clone handle to the connection pool. Each area of the schema has its own
//! module that adds methods to `Db` (`impl Db { ... }`), so owners do not edit each other's files:
//!
//! | module        | tables                                         |
//! | ---           | ---                                            |
//! | `notes`       | notes, categories, note_versions, notes_fts    |
//! | `settings`    | settings                                       |
//! | `transcripts` | segments, speakers, people                     |
//! | `assistant`   | threads, messages, proposals, ai_actions       |
//! | `tasks`       | tasks, task_runs                               |
//! | `home`        | reads notes and segments for Home (no tables)  |
//! | `near`        | reads segments for close search matches (none) |
//! | `written`     | reads notes and versions for Home (no tables)  |
//! | `swears`      | swear_coins, swear_jar_emptied; reads segments |
//! | `imports`     | writes imported notes (import_* on notes)      |
//! | `links`       | note_links (the link index behind the graph)   |
//! | `welcome`     | writes the welcome note after setup (none)     |

use std::path::{Path, PathBuf};
use std::sync::Arc;

use margin_domain::{AppError, ErrorCode, Id, Timestamp};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

mod assistant;
mod home;
mod imports;
pub mod links;
mod near;
pub mod notes;
mod settings;
mod swears;
mod tasks;
mod transcripts;
mod welcome;
mod written;

pub use imports::{ImportSegment, ImportSpeaker, ImportWrite};
pub use notes::NewNote;
pub use swears::week_start;
pub use sqlx;
pub use welcome::{ShortcutStyle, WELCOME_TITLE, welcome_body};
pub use transcripts::{NewSpeaker, OTHERS_LABEL, SELF_LABEL, Voiceprint, blob_to_vec, cosine, normalized, vec_to_blob};

/// How this crate writes ids and timestamps. The schema stores both as TEXT; other modules in this
/// crate should use these helpers so joins and comparisons match (binding a raw `Uuid` with sqlx
/// writes a BLOB, which never equals the TEXT ids in `notes`).
pub(crate) mod codec {
    use margin_domain::chrono::{DateTime, SecondsFormat, Utc};

    use super::{Id, StoreError, StoreResult, Timestamp};

    /// Lowercase hyphenated UUID text.
    pub fn id(id: Id) -> String {
        id.hyphenated().to_string()
    }

    pub fn opt_id(id: Option<Id>) -> Option<String> {
        id.map(self::id)
    }

    pub fn parse_id(text: &str) -> StoreResult<Id> {
        Id::parse_str(text).map_err(|_| StoreError::Invalid(format!("A stored id is not valid: {text}")))
    }

    pub fn parse_opt_id(text: Option<String>) -> StoreResult<Option<Id>> {
        text.as_deref().map(parse_id).transpose()
    }

    /// Fixed-width RFC 3339 in UTC with microseconds (`2026-11-11T09:30:00.000000Z`), so TEXT
    /// comparison orders timestamps correctly.
    pub fn ts(ts: Timestamp) -> String {
        ts.to_rfc3339_opts(SecondsFormat::Micros, true)
    }

    pub fn opt_ts(ts: Option<Timestamp>) -> Option<String> {
        ts.map(self::ts)
    }

    pub fn parse_ts(text: &str) -> StoreResult<Timestamp> {
        DateTime::parse_from_rfc3339(text)
            .map(|t| t.with_timezone(&Utc))
            .map_err(|_| StoreError::Invalid(format!("A stored date is not valid: {text}")))
    }

    pub fn parse_opt_ts(text: Option<String>) -> StoreResult<Option<Timestamp>> {
        text.as_deref().map(parse_ts).transpose()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0} was not found.")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("database error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("stored data could not be read: {0}")]
    Decode(#[from] serde_json::Error),
}

pub type StoreResult<T> = Result<T, StoreError>;

impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        let code = match &e {
            StoreError::NotFound(_) => ErrorCode::NotFound,
            StoreError::Invalid(_) => ErrorCode::Invalid,
            _ => ErrorCode::Internal,
        };
        AppError::new(code, e.to_string())
    }
}

/// Handle to the database. Clone freely.
#[derive(Clone)]
pub struct Db {
    pool: sqlx::SqlitePool,
    /// The app data directory. Recording paths stored in `notes.audio_path` are relative to it, so
    /// purging a note can remove its audio. `None` in tests that do not care about files.
    data_dir: Option<Arc<PathBuf>>,
}

impl Db {
    /// Open (creating if needed) the database at `path` and run pending migrations.
    pub async fn open(path: &Path) -> StoreResult<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);
        let pool = SqlitePoolOptions::new().max_connections(4).connect_with(options).await?;
        Self::migrate(pool).await
    }

    /// A private in-memory database, for tests.
    pub async fn open_in_memory() -> StoreResult<Self> {
        let options = SqliteConnectOptions::new().in_memory(true).foreign_keys(true);
        // One connection: every connection to `:memory:` is its own database.
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await?;
        Self::migrate(pool).await
    }

    async fn migrate(pool: sqlx::SqlitePool) -> StoreResult<Self> {
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool, data_dir: None })
    }

    /// Tell the store where the app data directory is, so deleting a note for good also deletes
    /// its recording.
    pub fn with_data_dir(mut self, data_dir: impl Into<PathBuf>) -> Self {
        self.data_dir = Some(Arc::new(data_dir.into()));
        self
    }

    /// The app data directory, when known.
    pub fn data_dir(&self) -> Option<&Path> {
        self.data_dir.as_deref().map(PathBuf::as_path)
    }

    /// The pool, for the modules in this crate.
    pub(crate) fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_apply() {
        let db = Db::open_in_memory().await.unwrap();
        let (count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'notes'").fetch_one(db.pool()).await.unwrap();
        assert_eq!(count, 1);
    }
}
