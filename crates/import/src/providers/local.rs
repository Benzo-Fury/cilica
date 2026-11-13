//! Data already on this computer: Obsidian vaults, Anarlog's and Meetily's databases, Apple Notes.
//!
//! Other apps' databases are copied to a private temp folder (with their WAL files) and opened
//! read-only, so the running app is never locked or changed. Each reader names the tables it reads
//! and nothing else; tables holding other apps' API keys are never opened.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use cilica_domain::{DetectedLocation, ImportInput, UnsupportedItem};
use cilica_store::sqlx::{self, Row, SqlitePool, sqlite::SqliteConnectOptions};
use serde_json::Value;

use super::{Cx, Detect, Provider, Scan, app_installed, display_path, scan_paste, scan_paths};
use crate::formats::documents::{html_to_markdown, prosemirror_to_markdown};
use crate::formats::files::Tree;
use crate::model::{ImportedNote, SpeakerRef, Turn};
use crate::text::{parse_date, plural, secs_to_ms};
use crate::{ImportError, KnownPaths};

/// Paths and pasted text work the same for every local provider.
async fn files_or_paste(input: &ImportInput) -> Option<Result<Scan, ImportError>> {
    match input {
        ImportInput::Paths { paths } => Some(scan_paths(paths).await),
        ImportInput::Paste { text, title } => Some(Ok(scan_paste(text, title.as_deref()))),
        _ => None,
    }
}

/// The detected location matching `path`, or the first. Only detected places can be read as Local.
fn pick<'a>(found: &'a [DetectedLocation], path: Option<&str>) -> Result<&'a DetectedLocation, ImportError> {
    match path {
        Some(p) => found.iter().find(|l| l.path == p),
        None => found.first(),
    }
    .ok_or_else(|| ImportError::not_found("Cilica could not find that data on this computer any more. Check that the app is still installed."))
}

// ── Obsidian ─────────────────────────────────────────────────────────────

pub struct Obsidian;

/// Vaults Obsidian knows about, from its own settings file.
fn obsidian_vaults(paths: &KnownPaths) -> Vec<PathBuf> {
    let candidates = [paths.config_dir.join("obsidian/obsidian.json"), paths.data_dir.join("obsidian/obsidian.json")];
    let mut vaults: Vec<PathBuf> = Vec::new();
    for file in candidates {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let Ok(json) = serde_json::from_str::<Value>(&text) else { continue };
        for v in json.get("vaults").and_then(Value::as_object).into_iter().flat_map(|m| m.values()) {
            if let Some(p) = v.get("path").and_then(Value::as_str).map(PathBuf::from)
                && p.is_dir()
                && !vaults.contains(&p)
            {
                vaults.push(p);
            }
        }
    }
    vaults
}

/// Counts for a vault from file names alone (nothing is read).
fn count_vault(paths: &KnownPaths, dir: &Path) -> DetectedLocation {
    let tree = Tree::from_dir(dir);
    let mut notes = 0;
    let mut canvases = 0;
    let mut bases = 0;
    let mut drawings = 0;
    let mut images = 0;
    let mut other = 0;
    for p in tree.paths() {
        let lower = p.to_lowercase();
        if lower.ends_with(".excalidraw.md") {
            drawings += 1;
        } else if lower.ends_with(".md") {
            notes += 1;
        } else if lower.ends_with(".canvas") {
            canvases += 1;
        } else if lower.ends_with(".base") {
            bases += 1;
        } else if crate::formats::markdown::is_image_name(&lower) {
            images += 1;
        } else {
            other += 1;
        }
    }
    let mut unsupported = Vec::new();
    let mut add = |kind: &str, count: u32, reason: &str| {
        if count > 0 {
            unsupported.push(UnsupportedItem { kind: kind.into(), count, reason: reason.into() });
        }
    };
    add("Canvases", canvases, "Cilica has no canvas view, so canvas boards are left out. The notes on them are imported on their own.");
    add("Bases", bases, "Obsidian Bases are database views. The notes they list are imported on their own.");
    add("Excalidraw drawings", drawings, "Drawings made with the Excalidraw plugin are left out.");
    add("Other files", other, "PDFs, recordings and other files that are not notes are left out.");
    DetectedLocation {
        label: dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| display_path(paths, dir)),
        path: dir.display().to_string(),
        notes,
        meetings: 0,
        transcripts: 0,
        attachments: images,
        unsupported,
    }
}

#[async_trait]
impl Provider for Obsidian {
    fn id(&self) -> &'static str {
        "obsidian"
    }
    fn name(&self) -> &'static str {
        "Obsidian"
    }
    fn apps(&self) -> &'static [&'static str] {
        &["Obsidian.app"]
    }

    async fn detect(&self, paths: &KnownPaths) -> Detect {
        let paths = paths.clone();
        let installed = app_installed(&paths, self.apps());
        tokio::task::spawn_blocking(move || {
            let locations: Vec<DetectedLocation> = obsidian_vaults(&paths).iter().map(|v| count_vault(&paths, v)).collect();
            Detect { app_installed: installed || !locations.is_empty(), locations, note: None }
        })
        .await
        .unwrap_or_default()
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let ImportInput::Local { path } = input else { return Err(ImportError::invalid("Choose your vault folder to import from Obsidian.")) };
        let found = self.detect(cx.paths).await.locations;
        if path.is_some() {
            let vault = pick(&found, path.as_deref())?;
            return scan_paths(std::slice::from_ref(&vault.path)).await;
        }
        // No vault named (onboarding): read every vault with notes, as the detected count does.
        // The first listed vault may be empty, or an `.obsidian` folder opened as a vault by mistake.
        let vaults: Vec<String> = found.iter().filter(|l| l.notes > 0).map(|l| l.path.clone()).collect();
        if vaults.is_empty() {
            return scan_paths(std::slice::from_ref(&pick(&found, None)?.path)).await;
        }
        scan_paths(&vaults).await
    }
}

// ── Database copies ───────────────────────────────────────────────────────

/// A private read-only copy of another app's SQLite database, removed when dropped.
struct DbCopy {
    dir: PathBuf,
    pool: SqlitePool,
}

impl DbCopy {
    async fn open(db: &Path) -> Result<DbCopy, ImportError> {
        let dir = std::env::temp_dir().join(format!("cilica-import-{}", cilica_domain::new_id()));
        std::fs::create_dir_all(&dir).map_err(|_| ImportError::internal("Cilica could not make a temporary folder to read from. Check free disk space."))?;
        let name = db.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "app.db".into());
        let target = dir.join(&name);
        std::fs::copy(db, &target).map_err(|_| ImportError::permission("Cilica could not read the other app's data. Quit that app and try again."))?;
        for suffix in ["-wal", "-shm"] {
            let side = db.with_file_name(format!("{name}{suffix}"));
            if side.exists() {
                let _ = std::fs::copy(&side, dir.join(format!("{name}{suffix}")));
            }
        }
        // Read-write on the private copy so SQLite can fold in the WAL; the original is never opened.
        let options = SqliteConnectOptions::new().filename(&target);
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(options).await.map_err(|_| ImportError::invalid("That database could not be opened. The app may be using a newer format; export your notes from it instead."))?;
        Ok(DbCopy { dir, pool })
    }

    async fn has_columns(&self, table: &str, columns: &[&str]) -> bool {
        let Ok(rows) = sqlx::query("SELECT name FROM pragma_table_info(?)").bind(table).fetch_all(&self.pool).await else { return false };
        let have: Vec<String> = rows.iter().filter_map(|r| r.try_get::<String, _>("name").ok()).collect();
        columns.iter().all(|c| have.iter().any(|h| h == c))
    }

    async fn count(&self, sql: &'static str) -> u32 {
        sqlx::query_scalar::<_, i64>(sql).fetch_one(&self.pool).await.map(|n| n.clamp(0, u32::MAX as i64) as u32).unwrap_or(0)
    }
}

impl Drop for DbCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn first_existing(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|p| p.is_file())
}

fn opt_str(r: &cilica_store::sqlx::sqlite::SqliteRow, col: &str) -> Option<String> {
    r.try_get::<Option<String>, _>(col).ok().flatten().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

// ── Anarlog (and Hyprnote) ────────────────────────────────────────────────

pub struct Anarlog;

fn anarlog_db(paths: &KnownPaths) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for base in [&paths.config_dir, &paths.data_dir, &paths.local_data_dir] {
        for app in ["anarlog", "hyprnote", "com.hyprnote.stable", "com.hyprnote.nightly", "char"] {
            candidates.push(base.join(app).join("app.db"));
        }
    }
    first_existing(candidates)
}

const ANARLOG_SCHEMA: [(&str, &[&str]); 3] = [
    ("sessions", &["id", "title", "created_at", "updated_at", "started_at", "ended_at", "folder_path", "deleted_at"]),
    ("session_documents", &["session_id", "kind", "body_format", "body", "deleted_at"]),
    ("transcripts", &["session_id", "started_at_ms", "words_json", "speaker_hints_json", "deleted_at"]),
];

async fn anarlog_check(db: &DbCopy) -> Result<(), ImportError> {
    for (table, cols) in ANARLOG_SCHEMA {
        if !db.has_columns(table, cols).await {
            return Err(ImportError::invalid("This version of Anarlog stores notes in a way Cilica does not know yet. In Anarlog, export your meetings as Markdown and import that folder."));
        }
    }
    Ok(())
}

#[async_trait]
impl Provider for Anarlog {
    fn id(&self) -> &'static str {
        "anarlog"
    }
    fn name(&self) -> &'static str {
        "Anarlog"
    }
    fn apps(&self) -> &'static [&'static str] {
        &["Anarlog.app", "Hyprnote.app", "Char.app"]
    }

    async fn detect(&self, paths: &KnownPaths) -> Detect {
        let installed = app_installed(paths, self.apps());
        let Some(path) = anarlog_db(paths) else { return Detect { app_installed: installed, ..Default::default() } };
        let Ok(db) = DbCopy::open(&path).await else { return Detect { app_installed: true, ..Default::default() } };
        if anarlog_check(&db).await.is_err() {
            return Detect { app_installed: true, note: Some("Cilica found Anarlog's data, but this version stores it in a way Cilica does not know yet. Export your meetings as Markdown from Anarlog and import that folder.".into()), ..Default::default() };
        }
        let sessions = db.count("SELECT count(*) FROM sessions WHERE deleted_at IS NULL").await;
        let transcripts = db.count("SELECT count(DISTINCT session_id) FROM transcripts WHERE deleted_at IS NULL AND json_array_length(words_json) > 0").await;
        Detect {
            app_installed: true,
            locations: vec![DetectedLocation {
                label: "Anarlog on this computer".into(),
                path: path.display().to_string(),
                notes: 0,
                meetings: sessions,
                transcripts,
                attachments: 0,
                unsupported: vec![UnsupportedItem { kind: "Recordings".into(), count: sessions, reason: "Audio stays in Anarlog. Cilica imports the notes, summaries and transcripts.".into() }]
                    .into_iter()
                    .filter(|u| u.count > 0)
                    .collect(),
            }],
            note: None,
        }
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let found = self.detect(cx.paths).await.locations;
        let location = pick(&found, if let ImportInput::Local { path } = input { path.as_deref() } else { None })?;
        let db = DbCopy::open(Path::new(&location.path)).await?;
        anarlog_check(&db).await?;
        let notes = read_anarlog(&db).await?;
        Ok(Scan {
            origin: "Anarlog on this computer".into(),
            notes,
            unsupported: location.unsupported.clone(),
            ..Default::default()
        })
    }
}

async fn read_anarlog(db: &DbCopy) -> Result<Vec<ImportedNote>, ImportError> {
    let fail = |_| ImportError::invalid("Cilica could not read Anarlog's meetings. Export them as Markdown from Anarlog and import that folder.");
    let sessions = sqlx::query("SELECT id, title, created_at, updated_at, started_at, ended_at, folder_path FROM sessions WHERE deleted_at IS NULL ORDER BY created_at")
        .fetch_all(&db.pool)
        .await
        .map_err(fail)?;
    let docs = sqlx::query("SELECT session_id, kind, body FROM session_documents WHERE deleted_at IS NULL AND body_format = 'prosemirror_json' ORDER BY sort_order, created_at")
        .fetch_all(&db.pool)
        .await
        .or(sqlx::query("SELECT session_id, kind, body FROM session_documents WHERE deleted_at IS NULL AND body_format = 'prosemirror_json'").fetch_all(&db.pool).await)
        .map_err(fail)?;
    let transcripts = sqlx::query("SELECT session_id, started_at_ms, words_json, speaker_hints_json FROM transcripts WHERE deleted_at IS NULL ORDER BY started_at_ms")
        .fetch_all(&db.pool)
        .await
        .map_err(fail)?;
    let humans: HashMap<String, String> = sqlx::query("SELECT id, name FROM humans WHERE deleted_at IS NULL")
        .fetch_all(&db.pool)
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|r| Some((opt_str(r, "id")?, opt_str(r, "name")?)))
        .collect();
    let participants: Vec<(String, String)> =
        sqlx::query("SELECT p.session_id, COALESCE(NULLIF(h.name, ''), p.display_name, p.email) AS name FROM session_participants p LEFT JOIN humans h ON h.id = p.human_id WHERE p.deleted_at IS NULL")
            .fetch_all(&db.pool)
            .await
            .unwrap_or_default()
            .iter()
            .filter_map(|r| Some((opt_str(r, "session_id")?, opt_str(r, "name")?)))
            .collect();

    let mut notes = Vec::new();
    for s in &sessions {
        let Some(id) = opt_str(s, "id") else { continue };
        let mut summary = Vec::new();
        let mut mine = Vec::new();
        for d in docs.iter().filter(|d| opt_str(d, "session_id").as_deref() == Some(&id)) {
            let Some(body) = opt_str(d, "body").and_then(|b| serde_json::from_str::<Value>(&b).ok()) else { continue };
            let md = prosemirror_to_markdown(&body);
            if md.trim().is_empty() {
                continue;
            }
            if opt_str(d, "kind").as_deref() == Some("summary") { summary.push(md) } else { mine.push(md) }
        }
        let session_start = opt_str(s, "started_at").and_then(|t| parse_date(&t));
        let mut turns = Vec::new();
        for t in transcripts.iter().filter(|t| opt_str(t, "session_id").as_deref() == Some(&id)) {
            let started_ms: Option<i64> = t.try_get("started_at_ms").ok();
            let offset = match (session_start, started_ms) {
                (Some(s), Some(t)) => (t - s.timestamp_millis()).max(0),
                _ => 0,
            };
            let words: Value = opt_str(t, "words_json").and_then(|w| serde_json::from_str(&w).ok()).unwrap_or(Value::Null);
            let hints: Value = opt_str(t, "speaker_hints_json").and_then(|w| serde_json::from_str(&w).ok()).unwrap_or(Value::Null);
            turns.extend(anarlog_turns(&words, &hints, &humans, offset));
        }
        let started = session_start.or_else(|| opt_str(s, "created_at").and_then(|t| parse_date(&t)));
        let ended = opt_str(s, "ended_at").and_then(|t| parse_date(&t));
        let transcript = (!turns.is_empty()).then(|| crate::formats::transcript::finish(turns));
        let duration = match (started, ended) {
            (Some(a), Some(b)) if b > a => Some((b - a).num_milliseconds().min(u32::MAX as i64) as u32),
            _ => transcript.as_ref().and_then(|t| t.turns.iter().filter_map(|x| x.end_ms).max()),
        };
        let mut attendees: Vec<String> = participants.iter().filter(|(sid, _)| *sid == id).map(|(_, n)| n.clone()).collect();
        attendees.dedup();
        notes.push(ImportedNote {
            external_id: id,
            title: opt_str(s, "title").unwrap_or_else(|| "Anarlog meeting".into()),
            created_at: opt_str(s, "created_at").and_then(|t| parse_date(&t)),
            updated_at: opt_str(s, "updated_at").and_then(|t| parse_date(&t)),
            started_at: started,
            ended_at: ended,
            duration_ms: duration,
            attendees,
            summary_md: (!summary.is_empty()).then(|| summary.join("\n\n")),
            notes_md: (!mine.is_empty()).then(|| mine.join("\n\n")),
            folder: opt_str(s, "folder_path").map(|f| f.split('/').map(str::trim).filter(|p| !p.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
            transcript,
            meeting: true,
            ..Default::default()
        });
    }
    Ok(notes)
}

/// Anarlog keeps words, not lines. Each word may carry a speaker hint (`provider_speaker_index`
/// by word id), and the person may have named a speaker (`user_speaker_assignment`). Words from the
/// same speaker with short gaps become one turn.
fn anarlog_turns(words: &Value, hints: &Value, humans: &HashMap<String, String>, offset_ms: i64) -> Vec<Turn> {
    let parse = |v: &Value| -> Value {
        match v {
            Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
            other => other.clone(),
        }
    };
    let mut word_speaker: HashMap<String, (i64, i64)> = HashMap::new();
    let mut named: HashMap<(i64, i64), String> = HashMap::new();
    for h in hints.as_array().into_iter().flatten() {
        let value = parse(h.get("value").unwrap_or(&Value::Null));
        let key = (value.get("channel").and_then(Value::as_i64).unwrap_or(-1), value.get("speaker_index").and_then(Value::as_i64).unwrap_or(-1));
        match h.get("type").and_then(Value::as_str) {
            Some("provider_speaker_index") => {
                if let Some(w) = h.get("word_id").and_then(Value::as_str) {
                    word_speaker.insert(w.to_string(), key);
                }
            }
            Some("user_speaker_assignment") => {
                if let Some(name) = value.get("human_id").and_then(Value::as_str).and_then(|id| humans.get(id)) {
                    named.insert(key, name.clone());
                }
            }
            _ => {}
        }
    }
    // Channels arrive interleaved word by word, so each speaker's words are grouped on their own
    // (a new turn after a pause), then all turns are put back in time order.
    let mut by_speaker: Vec<((i64, i64), Vec<Turn>)> = Vec::new();
    for w in words.as_array().into_iter().flatten() {
        let text = w.get("text").and_then(Value::as_str).unwrap_or("");
        if text.trim().is_empty() {
            continue;
        }
        let channel = w.get("channel").and_then(Value::as_i64).unwrap_or(0);
        let key = w.get("id").and_then(Value::as_str).and_then(|id| word_speaker.get(id).copied()).unwrap_or((channel, -1));
        let start = w.get("start_ms").and_then(Value::as_i64).map(|s| (s + offset_ms).clamp(0, u32::MAX as i64) as u32);
        let end = w.get("end_ms").and_then(Value::as_i64).map(|s| (s + offset_ms).clamp(0, u32::MAX as i64) as u32);
        let slot = match by_speaker.iter().position(|(k, _)| *k == key) {
            Some(i) => i,
            None => {
                by_speaker.push((key, Vec::new()));
                by_speaker.len() - 1
            }
        };
        let turns = &mut by_speaker[slot].1;
        let continues = turns.last().is_some_and(|t| {
            let gap_ok = t.end_ms.zip(start).is_none_or(|(e, s)| s.saturating_sub(e) < 1_500);
            gap_ok && t.text.len() < 1_200
        });
        if continues && let Some(turn) = turns.last_mut() {
            if !text.starts_with(char::is_whitespace) && !text.starts_with(['.', ',', '?', '!', ';', ':']) {
                turn.text.push(' ');
            }
            turn.text.push_str(text.trim_end());
            turn.end_ms = end.or(turn.end_ms);
            continue;
        }
        let speaker = match named.get(&key) {
            Some(name) => SpeakerRef::Named(name.clone()),
            None if key.0 == 0 => SpeakerRef::Me,
            None if key.1 >= 0 => SpeakerRef::Named(format!("Speaker {}", key.1 + 1)),
            None => SpeakerRef::Them,
        };
        turns.push(Turn { speaker, start_ms: start, end_ms: end, text: text.trim().to_string() });
    }
    let mut turns: Vec<Turn> = by_speaker.into_iter().flat_map(|(_, t)| t).collect();
    turns.sort_by_key(|t| t.start_ms.unwrap_or(0));
    turns
}

// ── Meetily ──────────────────────────────────────────────────────────────

pub struct Meetily;

fn meetily_db(paths: &KnownPaths) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for base in [&paths.data_dir, &paths.config_dir, &paths.local_data_dir] {
        for app in ["com.meetily.ai", "Meetily", "meetily", "com.zackriya.meetily"] {
            candidates.push(base.join(app).join("meeting_minutes.sqlite"));
            candidates.push(base.join(app).join("meeting_minutes.db"));
        }
    }
    first_existing(candidates)
}

#[async_trait]
impl Provider for Meetily {
    fn id(&self) -> &'static str {
        "meetily"
    }
    fn name(&self) -> &'static str {
        "Meetily"
    }
    fn apps(&self) -> &'static [&'static str] {
        &["Meetily.app", "meetily.app"]
    }

    async fn detect(&self, paths: &KnownPaths) -> Detect {
        let installed = app_installed(paths, self.apps());
        let Some(path) = meetily_db(paths) else { return Detect { app_installed: installed, ..Default::default() } };
        let Ok(db) = DbCopy::open(&path).await else { return Detect { app_installed: true, ..Default::default() } };
        if !db.has_columns("meetings", &["id", "title", "created_at"]).await {
            return Detect { app_installed: true, note: Some("Cilica found Meetily's data but could not read this version of it.".into()), ..Default::default() };
        }
        Detect {
            app_installed: true,
            locations: vec![DetectedLocation {
                label: "Meetily on this computer".into(),
                path: path.display().to_string(),
                notes: 0,
                meetings: db.count("SELECT count(*) FROM meetings").await,
                transcripts: db.count("SELECT count(DISTINCT meeting_id) FROM transcripts").await,
                attachments: 0,
                unsupported: Vec::new(),
            }],
            note: None,
        }
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let found = self.detect(cx.paths).await.locations;
        let location = pick(&found, if let ImportInput::Local { path } = input { path.as_deref() } else { None })?;
        let db = DbCopy::open(Path::new(&location.path)).await?;
        let fail = |_| ImportError::invalid("Cilica could not read Meetily's meetings. Export them from Meetily and import the files instead.");
        let meetings = sqlx::query("SELECT * FROM meetings ORDER BY created_at").fetch_all(&db.pool).await.map_err(fail)?;
        let has_speaker = db.has_columns("transcripts", &["speaker"]).await;
        let has_times = db.has_columns("transcripts", &["audio_start_time", "audio_end_time"]).await;
        // Older versions lack the speaker and audio-time columns; ask only for what exists.
        let sql: &'static str = match (has_speaker, has_times) {
            (true, true) => "SELECT meeting_id, transcript, speaker, audio_start_time AS start_s, audio_end_time AS end_s FROM transcripts ORDER BY audio_start_time, timestamp",
            (false, true) => "SELECT meeting_id, transcript, NULL AS speaker, audio_start_time AS start_s, audio_end_time AS end_s FROM transcripts ORDER BY audio_start_time, timestamp",
            (true, false) => "SELECT meeting_id, transcript, speaker, NULL AS start_s, NULL AS end_s FROM transcripts ORDER BY timestamp",
            (false, false) => "SELECT meeting_id, transcript, NULL AS speaker, NULL AS start_s, NULL AS end_s FROM transcripts ORDER BY timestamp",
        };
        let transcripts = sqlx::query(sql).fetch_all(&db.pool).await.map_err(fail)?;
        let summaries: HashMap<String, String> = if db.has_columns("summary_processes", &["meeting_id", "result"]).await {
            sqlx::query("SELECT meeting_id, result FROM summary_processes").fetch_all(&db.pool).await.unwrap_or_default().iter().filter_map(|r| Some((opt_str(r, "meeting_id")?, meetily_summary(&opt_str(r, "result")?)?))).collect()
        } else {
            HashMap::new()
        };
        let my_notes: HashMap<String, String> = if db.has_columns("meeting_notes", &["meeting_id", "notes_markdown"]).await {
            sqlx::query("SELECT meeting_id, notes_markdown FROM meeting_notes").fetch_all(&db.pool).await.unwrap_or_default().iter().filter_map(|r| Some((opt_str(r, "meeting_id")?, opt_str(r, "notes_markdown")?))).collect()
        } else {
            HashMap::new()
        };
        let mut notes = Vec::new();
        for m in &meetings {
            let Some(id) = opt_str(m, "id") else { continue };
            let turns: Vec<Turn> = transcripts
                .iter()
                .filter(|t| opt_str(t, "meeting_id").as_deref() == Some(&id))
                .filter_map(|t| {
                    let speaker = match opt_str(t, "speaker").as_deref() {
                        Some("mic") => SpeakerRef::Me,
                        Some("system") => SpeakerRef::Them,
                        _ => SpeakerRef::Unknown,
                    };
                    let start = t.try_get::<Option<f64>, _>("start_s").ok().flatten().and_then(secs_to_ms);
                    let end = t.try_get::<Option<f64>, _>("end_s").ok().flatten().and_then(secs_to_ms);
                    Some(Turn { speaker, start_ms: start, end_ms: end, text: opt_str(t, "transcript")? })
                })
                .collect();
            let created = opt_str(m, "created_at").and_then(|t| parse_date(&t));
            let transcript = (!turns.is_empty()).then(|| crate::formats::transcript::finish(turns));
            notes.push(ImportedNote {
                title: opt_str(m, "title").unwrap_or_else(|| "Meetily meeting".into()),
                created_at: created,
                updated_at: opt_str(m, "updated_at").and_then(|t| parse_date(&t)),
                started_at: created,
                duration_ms: transcript.as_ref().and_then(|t| t.turns.iter().filter_map(|x| x.end_ms).max()),
                summary_md: summaries.get(&id).cloned(),
                notes_md: my_notes.get(&id).cloned(),
                transcript,
                meeting: true,
                external_id: id,
                ..Default::default()
            });
        }
        Ok(Scan { origin: "Meetily on this computer".into(), notes, ..Default::default() })
    }
}

/// Meetily stores its summary as JSON; the Markdown sits under `markdown` when present.
fn meetily_summary(result: &str) -> Option<String> {
    match serde_json::from_str::<Value>(result) {
        Ok(Value::Object(map)) => map.get("markdown").and_then(Value::as_str).map(str::to_string).or_else(|| {
            let parts: Vec<String> = map
                .iter()
                .filter_map(|(k, v)| match v {
                    Value::String(s) if !s.trim().is_empty() => Some(format!("## {k}\n\n{s}")),
                    Value::Array(items) => {
                        let lines: Vec<String> = items.iter().filter_map(|i| i.as_str().map(|s| format!("- {s}")).or_else(|| i.get("text").and_then(Value::as_str).map(|s| format!("- {s}")))).collect();
                        (!lines.is_empty()).then(|| format!("## {k}\n\n{}", lines.join("\n")))
                    }
                    _ => None,
                })
                .collect();
            (!parts.is_empty()).then(|| parts.join("\n\n"))
        }),
        Ok(Value::String(s)) => Some(s),
        _ => Some(result.to_string()).filter(|s| !s.trim().is_empty()),
    }
}

// ── Apple Notes ──────────────────────────────────────────────────────────

pub struct AppleNotes;

/// Reads every note through Notes' own scripting interface: folders, dates and the HTML body.
/// Locked notes report only that they exist.
const NOTES_SCRIPT: &str = r#"
function run() {
  const Notes = Application('Notes');
  const out = [];
  function walk(folder, path) {
    const name = folder.name();
    if (name === 'Recently Deleted') return;
    const here = path.concat([name]);
    const notes = folder.notes;
    const ids = notes.id(), names = notes.name(), locked = notes.passwordProtected();
    const created = notes.creationDate(), modified = notes.modificationDate();
    for (let i = 0; i < ids.length; i++) {
      let body = null;
      if (!locked[i]) { try { body = notes[i].body(); } catch (e) { body = null; } }
      out.push({ id: ids[i], name: names[i], folder: here, locked: !!locked[i],
        created: created[i] ? created[i].toISOString() : null,
        modified: modified[i] ? modified[i].toISOString() : null, body: body });
    }
    try { folder.folders().forEach(f => walk(f, here)); } catch (e) {}
  }
  Notes.accounts().forEach(acc => acc.folders().forEach(f => {
    try { if (f.container().class() === 'account') walk(f, []); } catch (e) { walk(f, []); }
  }));
  return JSON.stringify(out);
}
"#;

#[async_trait]
impl Provider for AppleNotes {
    fn id(&self) -> &'static str {
        "apple_notes"
    }
    fn name(&self) -> &'static str {
        "Apple Notes"
    }

    async fn detect(&self, paths: &KnownPaths) -> Detect {
        let installed = paths.applications.iter().any(|d| d.join("Notes.app").exists()) || Path::new("/System/Applications/Notes.app").exists();
        Detect {
            app_installed: installed,
            note: installed.then(|| "Apple Notes keeps its notes where only it can read them. When you check, macOS asks whether Cilica may read Notes; choose OK.".into()),
            ..Default::default()
        }
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let _ = cx;
        let output = tokio::process::Command::new("osascript")
            .args(["-l", "JavaScript", "-e", NOTES_SCRIPT])
            .output()
            .await
            .map_err(|_| ImportError::unavailable("Reading Apple Notes directly works on a Mac. Export your notes as Markdown from Notes (File › Export) and choose those files instead."))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(if err.contains("-1743") || err.to_lowercase().contains("not authorized") || err.contains("-1744") {
                ImportError::permission("macOS did not let Cilica read Apple Notes. Open System Settings › Privacy & Security › Automation, turn on Notes under Cilica, then try again.")
            } else {
                ImportError::upstream("Apple Notes did not answer. Open Notes once, wait for it to finish syncing, then try again.")
            });
        }
        let json = parse_notes_json(&output.stdout).map_err(|e| {
            tracing::warn!(error = %e, bytes = output.stdout.len(), "could not parse Apple Notes output");
            ImportError::upstream("Apple Notes answered in a way Cilica did not understand. Try again.")
        })?;
        Ok(read_apple_notes(&json))
    }
}

/// Parse what the Notes script printed. JavaScript strings may hold half an emoji (a title Notes
/// cut mid-character), which `JSON.stringify` writes as a lone `\\uD83D` escape that serde rejects;
/// those halves become U+FFFD and the rest of the note survives.
fn parse_notes_json(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    match serde_json::from_slice(bytes) {
        Ok(v) => Ok(v),
        Err(first) => {
            let text = String::from_utf8_lossy(bytes);
            let repaired = repair_lone_surrogates(&text);
            if repaired == text { Err(first) } else { serde_json::from_str(&repaired) }
        }
    }
}

/// Replace `\uD800`–`\uDFFF` escapes that are not part of a high+low pair with `\uFFFD`.
fn repair_lone_surrogates(text: &str) -> String {
    fn escape_at(b: &[u8], i: usize) -> Option<u16> {
        // `\uXXXX` preceded by an even number of backslashes is a real escape.
        if b.get(i) != Some(&b'\\') || b.get(i + 1) != Some(&b'u') {
            return None;
        }
        let hex = std::str::from_utf8(b.get(i + 2..i + 6)?).ok()?;
        u16::from_str_radix(hex, 16).ok()
    }
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut last = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            match escape_at(b, i) {
                Some(hi @ 0xD800..=0xDBFF) if matches!(escape_at(b, i + 6), Some(0xDC00..=0xDFFF)) => {
                    let _ = hi;
                    i += 12;
                }
                Some(0xD800..=0xDFFF) => {
                    out.push_str(&text[last..i]);
                    out.push_str("\\uFFFD");
                    i += 6;
                    last = i;
                }
                // Any other escape, including `\\`, is two bytes we skip whole.
                _ => i += 2,
            }
        } else {
            i += 1;
        }
    }
    out.push_str(&text[last..]);
    out
}

fn read_apple_notes(json: &Value) -> Scan {
    let mut notes = Vec::new();
    let mut locked = 0u32;
    for n in json.as_array().into_iter().flatten() {
        if n.get("locked").and_then(Value::as_bool) == Some(true) {
            locked += 1;
            continue;
        }
        let Some(body) = n.get("body").and_then(Value::as_str) else { continue };
        let markdown = html_to_markdown(body);
        let title = n.get("name").and_then(Value::as_str).unwrap_or("Untitled").trim().to_string();
        // Notes repeats the title as the first line of the body.
        let (h1, rest) = crate::text::take_h1(&markdown);
        let body = match h1 {
            Some(h) if h.eq_ignore_ascii_case(&title) => rest,
            _ => match markdown.split_once('\n') {
                Some((first, rest)) if first.trim().trim_matches('*').eq_ignore_ascii_case(&title) => rest.trim().to_string(),
                _ => markdown.clone(),
            },
        };
        let images = body.matches("](data:image").count() as u32;
        notes.push(ImportedNote {
            external_id: n.get("id").and_then(Value::as_str).unwrap_or(&title).to_string(),
            title,
            created_at: n.get("created").and_then(Value::as_str).and_then(parse_date),
            updated_at: n.get("modified").and_then(Value::as_str).and_then(parse_date),
            notes_md: Some(body),
            folder: n.get("folder").and_then(Value::as_array).map(|f| f.iter().filter_map(Value::as_str).filter(|f| *f != "Notes").map(str::to_string).collect()).unwrap_or_default(),
            attachments: images,
            ..Default::default()
        });
    }
    let mut unsupported = vec![UnsupportedItem {
        kind: "Attachments".into(),
        count: 0,
        reason: "Apple Notes only shares the text and images of a note with other apps. Scans, drawings, PDFs and audio recordings stay in Notes.".into(),
    }];
    if locked > 0 {
        unsupported.insert(0, UnsupportedItem { kind: "Locked notes".into(), count: locked, reason: "Locked notes stay locked in Apple Notes. Unlock them there, then import again.".into() });
    }
    Scan { origin: format!("Apple Notes ({})", plural(notes.len(), "note", "notes")), notes, unsupported, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anarlog_words_become_turns_with_named_speakers() {
        let words: Value = serde_json::from_str(
            r#"[{"id":"w1","text":"Hello","start_ms":0,"end_ms":300,"channel":0},{"id":"w2","text":" there","start_ms":300,"end_ms":600,"channel":0},
                {"id":"w3","text":"Hi","start_ms":5000,"end_ms":5200,"channel":1},{"id":"w4","text":"back","start_ms":5300,"end_ms":5500,"channel":1}]"#,
        )
        .unwrap();
        let hints: Value = serde_json::from_str(
            r#"[{"id":"h1","word_id":"w3","type":"provider_speaker_index","value":"{\"channel\":1,\"speaker_index\":0}"},
                {"id":"h2","word_id":"w4","type":"provider_speaker_index","value":{"channel":1,"speaker_index":0}},
                {"id":"h3","type":"user_speaker_assignment","value":"{\"human_id\":\"H1\",\"scope\":\"speaker\",\"channel\":1,\"speaker_index\":0}"}]"#,
        )
        .unwrap();
        let humans = HashMap::from([("H1".to_string(), "Ana".to_string())]);
        let turns = anarlog_turns(&words, &hints, &humans, 1_000);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].speaker, SpeakerRef::Me);
        assert_eq!(turns[0].text, "Hello there");
        assert_eq!(turns[0].start_ms, Some(1_000));
        assert_eq!(turns[1].speaker, SpeakerRef::Named("Ana".into()));
        assert_eq!(turns[1].text, "Hi back");
    }

    /// Onboarding names no vault. The first one Obsidian lists here is an `.obsidian` folder with
    /// only settings in it, as on a friend's Windows machine; the real vault after it is still read.
    #[tokio::test]
    async fn obsidian_without_a_vault_reads_every_vault_with_notes() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config");
        let settings = tmp.path().join("Notes").join(".obsidian");
        let vault = tmp.path().join("Notes");
        std::fs::create_dir_all(&settings).unwrap();
        std::fs::create_dir_all(config.join("obsidian")).unwrap();
        std::fs::write(settings.join("app.json"), "{}").unwrap();
        std::fs::write(vault.join("Hello.md"), "# Hello\n\nWorld").unwrap();
        let json = serde_json::json!({ "vaults": {
            "a": { "path": settings.display().to_string() },
            "b": { "path": vault.display().to_string() },
        }});
        std::fs::write(config.join("obsidian/obsidian.json"), json.to_string()).unwrap();
        let paths = KnownPaths { home: tmp.path().into(), config_dir: config.clone(), data_dir: config.clone(), local_data_dir: config, applications: vec![] };
        let cx = Cx { paths: &paths, key: None, notice: None };

        let scan = Obsidian.scan(&ImportInput::Local { path: None }, &cx).await.unwrap();
        assert_eq!(scan.notes.len(), 1);
        assert_eq!(scan.notes[0].title, "Hello");

        let picked = Obsidian.scan(&ImportInput::Local { path: Some(settings.display().to_string()) }, &cx).await.unwrap();
        assert!(picked.notes.is_empty());
    }

    #[test]
    fn meetily_summaries() {
        assert_eq!(meetily_summary(r###"{"markdown":"## Done"}"###).as_deref(), Some("## Done"));
        assert!(meetily_summary(r#"{"Key points":["a","b"]}"#).unwrap().contains("- a"));
        assert_eq!(meetily_summary("plain").as_deref(), Some("plain"));
    }

    #[test]
    fn apple_notes_output_with_half_an_emoji() {
        // Exactly what osascript prints for a title cut mid-emoji, plus a whole emoji and an escaped backslash.
        let raw = br#"[{"id":"x-coredata://1","name":"Trip \ud83c","folder":["Notes"],"locked":false,"body":"<div>Fun \ud83c\udf89 \\ud800 \udc00 end</div>"}]"#;
        assert!(serde_json::from_slice::<Value>(raw).is_err());
        let json = parse_notes_json(raw).unwrap();
        let scan = read_apple_notes(&json);
        assert_eq!(scan.notes.len(), 1);
        assert_eq!(scan.notes[0].title, "Trip \u{FFFD}");
        let body = scan.notes[0].notes_md.as_deref().unwrap();
        assert!(body.starts_with("Fun \u{1F389} ") && body.contains("ud800") && body.ends_with(" \u{FFFD} end"), "{body}");
    }

    #[test]
    fn apple_notes_output() {
        let json: Value = serde_json::from_str(
            r#"[{"id":"x-coredata://1","name":"Groceries","folder":["Notes"],"locked":false,"created":"2025-01-02T03:04:05.000Z","modified":"2025-01-03T03:04:05.000Z","body":"<div><h1>Groceries</h1></div><div>Milk</div>"},
                {"id":"x-coredata://2","name":"Secret","folder":["Private"],"locked":true,"body":null}]"#,
        )
        .unwrap();
        let scan = read_apple_notes(&json);
        assert_eq!(scan.notes.len(), 1);
        assert_eq!(scan.notes[0].title, "Groceries");
        assert_eq!(scan.notes[0].notes_md.as_deref(), Some("Milk"));
        assert!(scan.notes[0].folder.is_empty());
        assert_eq!(scan.unsupported[0].kind, "Locked notes");
    }
}
