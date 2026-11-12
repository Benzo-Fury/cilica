# margin-store

SQLite storage for Margin (sqlx, migrations in `migrations/`, FTS5 search). The only crate that talks SQL.

## Public API

`Db` is a cheap-to-clone handle. Each schema area adds methods in its own module (`impl Db`):

| Module | Owner | Tables |
| --- | --- | --- |
| `notes.rs` | Core | notes, categories, note_versions, notes_fts |
| `settings.rs` | Core | settings |
| `transcripts.rs` | Audio | segments, speakers, people |
| `assistant.rs`, `tasks.rs` | AI | threads, messages, proposals, ai_actions, tasks, task_runs |

- `Db::open(path)`, `Db::open_in_memory()` (tests), `.with_data_dir(dir)` so deleting a note for good also deletes its recording (`notes.audio_path`, relative to the data dir).
- Notes: fractional `position` (lower first; new notes on top, new categories last; crowded siblings are renumbered, see the `*_rebalanced` variants), soft delete, `purge_trash_older_than(days)`.
- `set_note_body(id, body, author, ai_action_id)`: snapshots before AI writes always; before user writes when the newest version is over 5 minutes old or the body changed a lot. Versions are capped at 50 per note; AI versions younger than 30 days are never pruned.
- `search_notes(query, limit)`: any input is safe; words are quoted, the last matches as a prefix; bm25 with title weighted highest; `excerpt` is HTML-escaped with `<mark>`. `reindex_note(id)` rebuilds one note's row including final transcript segments; call it after writing segments.
- Settings: `get_settings()` (defaults for anything missing or unreadable; unknown fields kept on write), `set_settings(&Settings)`, and `get_value / set_value / delete_value / get_json / set_json` for small caches (prefix keys with your area).

## Conventions

- Ids are lowercase hyphenated UUID TEXT (`id.to_string()`); never bind a raw `Uuid` (sqlx writes a BLOB).
- Timestamps are RFC 3339 UTC TEXT. `crate::codec` writes them fixed-width (`…T09:30:00.000000Z`) so TEXT comparison orders them.
- Write transactions that read first use `BEGIN IMMEDIATE`.

## Must not depend on

`tauri` or any other app crate except `margin-domain`.

## Tests

`cargo test -p margin-store`: everything runs against `Db::open_in_memory()`.
