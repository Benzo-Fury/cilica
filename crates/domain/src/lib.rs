//! Shared types for Cilica. This crate is the contract between the Rust crates and, through
//! tauri-specta, the TypeScript UI. It depends on nothing app-specific (no tauri, no sqlx).
//!
//! Rules:
//! - Every type that crosses IPC derives `Serialize, Deserialize, specta::Type`.
//! - No `i64`/`u64` in IPC types (they do not fit a JS number): use `u32`, `i32` or `f64`.
//! - Enums are `snake_case` on the wire; data-carrying enums are internally tagged with `type`.

mod assistant;
mod error;
mod events;
mod graph;
mod home;
mod import;
mod note;
mod recording;
mod settings;
mod swears;
mod tasks;
mod transcript;

pub use assistant::*;
pub use error::*;
pub use events::*;
pub use graph::*;
pub use home::*;
pub use import::*;
pub use note::*;
pub use recording::*;
pub use settings::*;
pub use swears::*;
pub use tasks::*;
pub use transcript::*;

pub use chrono;
pub use uuid;

/// All ids are UUID v7 (time-ordered), serialised as strings.
pub type Id = uuid::Uuid;
/// All timestamps are UTC, serialised as RFC 3339 strings.
pub type Timestamp = chrono::DateTime<chrono::Utc>;

/// A new time-ordered id.
pub fn new_id() -> Id {
    uuid::Uuid::now_v7()
}

/// The current time.
pub fn now() -> Timestamp {
    chrono::Utc::now()
}
