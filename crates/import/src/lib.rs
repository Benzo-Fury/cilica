//! Importing notes from other apps (docs/import.md): Obsidian, Notion, Evernote, Bear, Apple Notes,
//! Granola, Otter, Fireflies, Fathom and many more.
//!
//! Every provider turns what it reads into [`model::ImportedNote`]s. The [`Importer`] previews
//! them, writes them through the store one note per transaction, and reports progress through the
//! event sink. Importing the same source again skips items that are already notes in Cilica;
//! anything deleted, undone or in Trash comes back as a new note. Imports keep no history: the done
//! toast can undo the run while the app is open, and after that the notes are ordinary notes.
//! This crate never depends on Tauri.

pub mod formats;
pub mod http;
pub mod model;
pub mod providers;
pub mod text;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cilica_domain::{
    AppError, AppEvent, AudioChannel, DataScope, ErrorCode, Events, Id, ImportCategoryMode, ImportDetection, ImportFidelity, ImportInput, ImportKeyStatus, ImportOptions,
    ImportPhase, ImportPreview, ImportProgress, ImportRun, ImportRunStatus, ImportSample, NoteKind, SpeakerCount, new_id, now,
};
use cilica_store::{Db, ImportSegment, ImportSpeaker, ImportWrite};

use crate::http::WaitNotice;
use crate::model::{ImportedNote, SpeakerRef};
use crate::providers::{Cx, Provider, RemoteEntry, Scan};

// ── Errors and setup ─────────────────────────────────────────────────────

/// An import problem, written for people.
#[derive(Debug, Clone)]
pub struct ImportError {
    pub code: ErrorCode,
    pub message: String,
    /// The provider said the item is too large for one answer (Granola transcripts).
    pub too_large: bool,
}

impl ImportError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), too_large: false }
    }
    pub fn invalid(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Invalid, m)
    }
    pub fn not_found(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, m)
    }
    pub fn upstream(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Upstream, m)
    }
    pub fn unavailable(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unavailable, m)
    }
    pub fn permission(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::PermissionDenied, m)
    }
    pub fn internal(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, m)
    }
    pub fn too_large() -> Self {
        Self { too_large: true, ..Self::upstream("That item is too large to fetch in one go.") }
    }
}

impl From<ImportError> for AppError {
    fn from(e: ImportError) -> Self {
        AppError::new(e.code, e.message)
    }
}

/// Where other apps keep their data on this computer. Built by the shell, which knows the OS.
#[derive(Debug, Clone, Default)]
pub struct KnownPaths {
    pub home: PathBuf,
    /// macOS: ~/Library/Application Support; Windows: %APPDATA%; Linux: ~/.config.
    pub config_dir: PathBuf,
    /// macOS: ~/Library/Application Support; Windows: %APPDATA%; Linux: ~/.local/share.
    pub data_dir: PathBuf,
    /// Windows: %LOCALAPPDATA%; the same as `data_dir` elsewhere.
    pub local_data_dir: PathBuf,
    /// Folders apps are installed in (macOS: /Applications and ~/Applications).
    pub applications: Vec<PathBuf>,
}

/// API keys for account imports, from the OS keychain. Never logged, never sent over IPC.
pub trait Secrets: Send + Sync {
    fn key(&self, provider: &str) -> Option<String>;
}

/// Most problems listed in a run's report; the rest are counted.
const MAX_REPORT: usize = 40;

struct Pending {
    provider: Arc<dyn Provider>,
    scan: Scan,
    origin: String,
}

/// The import service: one per app, managed by the shell.
pub struct Importer {
    db: Db,
    events: Events,
    paths: KnownPaths,
    secrets: Arc<dyn Secrets>,
    scans: Mutex<HashMap<Id, Arc<Pending>>>,
    running: Mutex<HashMap<Id, Arc<AtomicBool>>>,
    /// Notes each import of this session created, for Undo. Kept in memory only.
    created: Mutex<HashMap<Id, Vec<Id>>>,
}

impl Importer {
    pub fn new(db: Db, events: Events, paths: KnownPaths, secrets: Arc<dyn Secrets>) -> Arc<Self> {
        Arc::new(Self { db, events, paths, secrets, scans: Mutex::new(HashMap::new()), running: Mutex::new(HashMap::new()), created: Mutex::new(HashMap::new()) })
    }

    fn provider(id: &str) -> Result<Arc<dyn Provider>, ImportError> {
        providers::find(id).ok_or_else(|| ImportError::new(ErrorCode::NotImplemented, "Cilica cannot import from that app yet."))
    }

    // ── Detecting ────────────────────────────────────────────────────────

    /// What Cilica can find for one provider without reading anything heavy.
    pub async fn detect(&self, provider: &str) -> Result<ImportDetection, ImportError> {
        let p = Self::provider(provider)?;
        let d = p.detect(&self.paths).await;
        Ok(ImportDetection { provider: provider.to_string(), app_installed: d.app_installed, locations: d.locations, has_key: self.secrets.key(provider).is_some(), note: d.note })
    }

    /// Providers with something on this computer (an installed app or data), for the picker's badges.
    pub async fn detect_all(&self) -> Vec<ImportDetection> {
        let all = providers::all();
        let found = futures::future::join_all(all.iter().map(|p| async {
            let d = p.detect(&self.paths).await;
            ImportDetection { provider: p.id().to_string(), app_installed: d.app_installed, locations: d.locations, has_key: self.secrets.key(p.id()).is_some(), note: d.note }
        }))
        .await;
        found.into_iter().filter(|d| d.app_installed || !d.locations.is_empty() || d.has_key).collect()
    }

    /// Check an API key with one cheap call.
    pub async fn check_key(&self, provider: &str, key: &str) -> ImportKeyStatus {
        match Self::provider(provider) {
            Ok(p) => match p.check_key(key).await {
                Ok(account) => ImportKeyStatus { ok: true, problem: None, account },
                Err(e) => ImportKeyStatus { ok: false, problem: Some(e.message), account: None },
            },
            Err(e) => ImportKeyStatus { ok: false, problem: Some(e.message), account: None },
        }
    }

    // ── Previewing ───────────────────────────────────────────────────────

    /// Read the source and say what an import would do. Nothing is written.
    pub async fn scan(&self, provider_id: &str, input: ImportInput) -> Result<ImportPreview, ImportError> {
        let provider = Self::provider(provider_id)?;
        let key = matches!(input, ImportInput::Account).then(|| self.secrets.key(provider_id)).flatten();
        if matches!(input, ImportInput::Account) && key.is_none() {
            return Err(ImportError::invalid(format!("Paste your {} API key first.", provider.name())));
        }
        let cx = Cx { paths: &self.paths, key, notice: None };
        let mut scan = provider.scan(&input, &cx).await?;
        scan.notes.retain(|n| !n.external_id.is_empty());
        if scan.notes.is_empty() && scan.remote.is_empty() {
            let mut message = format!("Cilica found nothing to import in {}.", scan.origin);
            if let Some(w) = scan.warnings.first() {
                message = format!("{message} {w}");
            } else if let Some(u) = scan.unsupported.first() {
                message = format!("{message} {}", u.reason);
            }
            return Err(ImportError::not_found(message));
        }
        let existing = self.db.live_import_ids(provider_id).await.map_err(store_error)?;
        let scan_id = new_id();
        let preview = preview(scan_id, provider_id, &scan, &existing);
        let origin = scan.origin.clone();
        let mut scans = self.scans.lock().unwrap_or_else(|e| e.into_inner());
        // Keep only a few previews around; each can hold a whole vault.
        if scans.len() > 4 {
            scans.clear();
        }
        scans.insert(scan_id, Arc::new(Pending { provider, scan, origin }));
        Ok(preview)
    }

    // ── Importing ────────────────────────────────────────────────────────

    /// Start importing a previewed scan in the background. Progress arrives as `import_progress` events.
    pub async fn start(self: &Arc<Self>, scan_id: Id, options: ImportOptions) -> Result<ImportRun, ImportError> {
        let pending = self
            .scans
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&scan_id)
            .ok_or_else(|| ImportError::not_found("That preview has expired. Check the source again, then import."))?;
        let run = ImportRun {
            id: new_id(),
            provider: pending.provider.id().to_string(),
            origin: pending.origin.clone(),
            status: ImportRunStatus::Running,
            imported: 0,
            skipped: 0,
            failed: 0,
            report: Vec::new(),
            started_at: now(),
            finished_at: None,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.running.lock().unwrap_or_else(|e| e.into_inner()).insert(run.id, cancel.clone());
        let this = self.clone();
        let started = run.clone();
        tokio::spawn(async move {
            let run_id = started.id;
            this.run(started, pending, options, cancel).await;
            this.running.lock().unwrap_or_else(|e| e.into_inner()).remove(&run_id);
        });
        Ok(run)
    }

    /// Stop a running import after the note in flight. Notes already written stay.
    pub fn cancel(&self, run_id: Id) {
        if let Some(flag) = self.running.lock().unwrap_or_else(|e| e.into_inner()).get(&run_id) {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// Move every note an import of this session created to Trash. Importing again brings them back.
    pub async fn undo(&self, run_id: Id) -> Result<u32, ImportError> {
        self.cancel(run_id);
        let ids = self
            .created
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&run_id)
            .ok_or_else(|| ImportError::not_found("That import can no longer be undone. Select its notes in the sidebar and delete them instead."))?;
        let moved = self.db.trash_notes(&ids).await.map_err(store_error)?;
        self.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Notes, DataScope::Trash, DataScope::Categories], note_id: None });
        Ok(moved)
    }

    async fn run(&self, mut run: ImportRun, pending: Arc<Pending>, options: ImportOptions, cancel: Arc<AtomicBool>) {
        let provider = pending.provider.clone();
        let provider_id = provider.id().to_string();
        let existing = match self.db.live_import_ids(&provider_id).await {
            Ok(ids) => ids,
            Err(e) => {
                run.status = ImportRunStatus::Failed;
                report(&mut run, format!("Cilica could not check which notes are already here: {e}. Try again."));
                self.finish(run, &provider_id, 0);
                return;
            }
        };
        // Items that are already notes are skipped up front; account items are then not fetched
        // again, which saves the provider's rate limit.
        let all = pending.scan.notes.len() + pending.scan.remote.len();
        // Oldest first, so the newest note ends up at the top of its category.
        let mut items: Vec<Item> = pending.scan.notes.iter().filter(|n| !existing.contains(&n.external_id)).map(|n| Item::Ready(Box::new(n.clone()))).collect();
        items.extend(pending.scan.remote.iter().filter(|e| !existing.contains(&e.external_id)).map(|e| Item::Remote(e.clone())));
        run.skipped += (all - items.len()) as u32;
        items.sort_by_key(|i| i.date());
        let total = items.len() as u32;
        let waiting: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let notice: WaitNotice = {
            let waiting = waiting.clone();
            let events = self.events.clone();
            let run_id = run.id;
            let provider_id = provider_id.clone();
            Arc::new(move |why: Option<String>| {
                *waiting.lock().unwrap_or_else(|e| e.into_inner()) = why.clone();
                if let Some(why) = why {
                    events.emit(AppEvent::ImportProgress {
                        progress: ImportProgress { run_id, provider: provider_id.clone(), phase: ImportPhase::Waiting, done: 0, total, current: Some(why), run: None },
                    });
                }
            })
        };
        let mut writer = Writer { db: &self.db, provider: provider.as_ref(), options: &options, categories: HashMap::new(), existing };
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut since_refresh = 0;
        for (i, item) in items.into_iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                run.status = ImportRunStatus::Cancelled;
                break;
            }
            let title = item.title();
            if last_emit.elapsed() > Duration::from_millis(120) || matches!(item, Item::Remote(_)) {
                self.progress(&run, &provider_id, ImportPhase::Writing, i as u32, total, Some(title.clone()));
                last_emit = Instant::now();
            }
            let note = match item {
                Item::Ready(n) => *n,
                Item::Remote(entry) => match pending.scan.loader.as_ref() {
                    Some(loader) => match loader.load(&entry, Some(notice.clone())).await {
                        Ok(n) => n,
                        Err(e) => {
                            run.failed += 1;
                            report(&mut run, format!("“{title}” could not be read: {}", e.message));
                            // A refused key or a used-up daily limit stops everything after it.
                            if e.code == ErrorCode::Invalid {
                                report(&mut run, "The import stopped there. Fix the key, then import again; finished notes stay.".into());
                                run.status = ImportRunStatus::Failed;
                                break;
                            }
                            continue;
                        }
                    },
                    None => continue,
                },
            };
            match writer.write(&note).await {
                Ok(Some(id)) => {
                    run.imported += 1;
                    self.created.lock().unwrap_or_else(|e| e.into_inner()).entry(run.id).or_default().push(id);
                }
                Ok(None) => run.skipped += 1,
                Err(message) => {
                    run.failed += 1;
                    report(&mut run, format!("“{title}” was not saved: {message}"));
                }
            }
            since_refresh += 1;
            if since_refresh >= 25 {
                since_refresh = 0;
                self.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Notes, DataScope::Categories], note_id: None });
            }
        }
        for w in &pending.scan.warnings {
            report(&mut run, w.clone());
        }
        if run.status == ImportRunStatus::Running {
            run.status = if run.failed > 0 { ImportRunStatus::Partial } else { ImportRunStatus::Succeeded };
        }
        self.finish(run, &provider_id, total);
    }

    /// Say the run is over: refresh the lists and send the final counts for the done toast.
    fn finish(&self, mut run: ImportRun, provider_id: &str, total: u32) {
        run.finished_at = Some(now());
        self.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Notes, DataScope::Categories, DataScope::Transcript, DataScope::Versions], note_id: None });
        let done = run.imported + run.skipped + run.failed;
        self.events.emit(AppEvent::ImportProgress {
            progress: ImportProgress { run_id: run.id, provider: provider_id.to_string(), phase: ImportPhase::Done, done, total: done.max(total), current: None, run: Some(run) },
        });
    }

    fn progress(&self, run: &ImportRun, provider: &str, phase: ImportPhase, done: u32, total: u32, current: Option<String>) {
        self.events.emit(AppEvent::ImportProgress { progress: ImportProgress { run_id: run.id, provider: provider.to_string(), phase, done, total, current, run: None } });
    }
}

fn store_error(e: cilica_store::StoreError) -> ImportError {
    let app: AppError = e.into();
    ImportError::new(app.code, app.message)
}

fn report(run: &mut ImportRun, line: String) {
    if run.report.len() < MAX_REPORT {
        run.report.push(line);
    } else if run.report.len() == MAX_REPORT {
        run.report.push("More items had problems; only the first ones are listed.".into());
    }
}

enum Item {
    Ready(Box<ImportedNote>),
    Remote(RemoteEntry),
}

impl Item {
    fn date(&self) -> Option<cilica_domain::Timestamp> {
        match self {
            Item::Ready(n) => n.date(),
            Item::Remote(e) => e.date,
        }
    }
    fn title(&self) -> String {
        match self {
            Item::Ready(n) => n.title.clone(),
            Item::Remote(e) => e.title.clone(),
        }
    }
}

// ── Writing one note ──────────────────────────────────────────────────────

struct Writer<'a> {
    db: &'a Db,
    provider: &'a dyn Provider,
    options: &'a ImportOptions,
    categories: HashMap<String, Id>,
    /// Item ids that are already notes outside Trash, including ones this run wrote.
    existing: HashSet<String>,
}

impl Writer<'_> {
    /// Write one note. `None` when the item is already a note (a source listing it twice).
    async fn write(&mut self, note: &ImportedNote) -> Result<Option<Id>, String> {
        if self.existing.contains(&note.external_id) {
            return Ok(None);
        }
        let category_id = self.category(note).await?;
        let w = self.to_write(note, category_id);
        let id = self.db.insert_imported_note(&w).await.map_err(|e| e.to_string())?;
        self.existing.insert(note.external_id.clone());
        Ok(Some(id))
    }

    async fn category(&mut self, note: &ImportedNote) -> Result<Option<Id>, String> {
        let fallback = format!("From {}", self.provider.name());
        let name = match self.options.category_mode {
            ImportCategoryMode::None => return Ok(None),
            ImportCategoryMode::OneCategory => fallback,
            ImportCategoryMode::KeepFolders => {
                let path: Vec<&str> = note.folder.iter().map(|f| f.trim()).filter(|f| !f.is_empty()).collect();
                if path.is_empty() { fallback } else { path.join(" / ") }
            }
        };
        if let Some(id) = self.categories.get(&name.to_lowercase()) {
            return Ok(Some(*id));
        }
        let category = self.db.category_named(&name).await.map_err(|e| e.to_string())?;
        self.categories.insert(name.to_lowercase(), category.id);
        Ok(Some(category.id))
    }

    fn to_write(&self, note: &ImportedNote, category_id: Option<Id>) -> ImportWrite {
        let title: String = {
            let t = note.title.chars().map(|c| if matches!(c, '\n' | '\r' | '\t') { ' ' } else { c }).collect::<String>();
            let t = t.trim().to_string();
            if t.is_empty() { format!("{} note", self.provider.name()) } else { t }
        };
        let body = note.body_md();
        let is_meeting = note.is_meeting();
        let is_self = |name: &str| self.options.self_names.iter().any(|n| n.trim().eq_ignore_ascii_case(name.trim()));

        // Speakers in order of first appearance; "You" for the person using Cilica.
        let mut speakers: Vec<ImportSpeaker> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut segments = Vec::new();
        let transcript = note.transcript.as_ref().filter(|t| !t.turns.is_empty());
        let timed = transcript.is_none_or(|t| t.timed);
        let mut cursor = 0u32;
        for (i, turn) in transcript.map(|t| t.turns.as_slice()).unwrap_or(&[]).iter().enumerate() {
            let (key, label, me) = match &turn.speaker {
                SpeakerRef::Me => ("\u{0}me".to_string(), cilica_store::SELF_LABEL.to_string(), true),
                SpeakerRef::Named(n) if is_self(n) => ("\u{0}me".to_string(), cilica_store::SELF_LABEL.to_string(), true),
                SpeakerRef::Named(n) => (n.to_lowercase(), n.clone(), false),
                SpeakerRef::Them => ("\u{0}them".to_string(), cilica_store::OTHERS_LABEL.to_string(), false),
                SpeakerRef::Unknown => (String::new(), String::new(), false),
            };
            let speaker = (!key.is_empty()).then(|| {
                *index.entry(key).or_insert_with(|| {
                    speakers.push(ImportSpeaker { label, is_self: me });
                    speakers.len() - 1
                })
            });
            let (start, end) = if timed {
                let start = turn.start_ms.unwrap_or(cursor);
                let end = turn.end_ms.filter(|e| *e >= start).unwrap_or(start + 1_000);
                (start, end)
            } else {
                let start = i as u32 * 1_000;
                (start, start + 1_000)
            };
            cursor = end;
            segments.push(ImportSegment { speaker, channel: if me { AudioChannel::Mic } else { AudioChannel::System }, start_ms: start, end_ms: end, text: turn.text.clone() });
        }

        let started_at = note.started_at.or(if is_meeting { note.created_at } else { None });
        let created_at = note.created_at.or(started_at).or(note.updated_at).unwrap_or_else(now);
        let updated_at = note.updated_at.filter(|u| *u >= created_at).unwrap_or(created_at);
        let duration_ms = note.duration_ms.or_else(|| match (started_at, note.ended_at) {
            (Some(a), Some(b)) if b > a => Some((b - a).num_milliseconds().min(u32::MAX as i64) as u32),
            _ => None,
        });
        let duration_ms = duration_ms.or_else(|| timed.then(|| segments.iter().map(|s| s.end_ms).max()).flatten());
        ImportWrite {
            provider: self.provider.id().to_string(),
            external_id: note.external_id.clone(),
            title,
            body_md: body,
            kind: if is_meeting { NoteKind::Meeting } else { NoteKind::Manual },
            meeting_app: note.meeting_app.clone(),
            started_at,
            ended_at: note.ended_at,
            duration_ms,
            created_at,
            updated_at,
            category_id,
            timed,
            speakers,
            segments,
        }
    }
}

// ── The preview ─────────────────────────────────────────────────────────

fn preview(scan_id: Id, provider: &str, scan: &Scan, existing: &HashSet<String>) -> ImportPreview {
    let notes = &scan.notes;
    let meetings = notes.iter().filter(|n| n.is_meeting()).count() + scan.remote.len();
    let transcripts = notes.iter().filter(|n| n.transcript.as_ref().is_some_and(|t| !t.turns.is_empty())).count();
    let dates: Vec<_> = notes.iter().filter_map(|n| n.date()).chain(scan.remote.iter().filter_map(|e| e.date)).collect();
    let mut folders: Vec<String> = Vec::new();
    for n in notes {
        let f = n.folder.iter().map(|f| f.trim()).filter(|f| !f.is_empty()).collect::<Vec<_>>().join(" / ");
        if !f.is_empty() && !folders.contains(&f) {
            folders.push(f);
        }
    }
    let mut speakers: HashMap<String, (String, u32)> = HashMap::new();
    for n in notes {
        if let Some(t) = &n.transcript {
            for name in t.names() {
                speakers.entry(name.to_lowercase()).or_insert((name.clone(), 0)).1 += 1;
            }
        }
    }
    let mut speakers: Vec<SpeakerCount> = speakers.into_values().map(|(name, meetings)| SpeakerCount { name, meetings }).collect();
    speakers.sort_by(|a, b| b.meetings.cmp(&a.meetings).then(a.name.cmp(&b.name)));
    speakers.truncate(24);
    let already = notes.iter().filter(|n| existing.contains(&n.external_id)).count() + scan.remote.iter().filter(|e| existing.contains(&e.external_id)).count();
    let includes = scan.declared.unwrap_or(ImportFidelity {
        summary: notes.iter().any(|n| n.summary_md.as_deref().is_some_and(|s| !s.trim().is_empty())),
        my_notes: notes.iter().any(|n| n.notes_md.as_deref().is_some_and(|s| !s.trim().is_empty())),
        transcript: transcripts > 0,
        speakers: notes.iter().any(|n| n.transcript.as_ref().is_some_and(|t| t.turns.iter().any(|x| x.speaker != SpeakerRef::Unknown))),
        timestamps: notes.iter().any(|n| n.transcript.as_ref().is_some_and(|t| t.timed)),
        attachments: notes.iter().any(|n| n.attachments > 0),
    });
    let mut recent: Vec<&ImportedNote> = notes.iter().collect();
    recent.sort_by_key(|n| std::cmp::Reverse(n.date()));
    let mut samples: Vec<ImportSample> = recent.iter().take(5).map(|n| ImportSample { title: n.title.clone(), date: n.date(), is_meeting: n.is_meeting(), excerpt: excerpt(n) }).collect();
    if samples.len() < 5 {
        let mut remote: Vec<&RemoteEntry> = scan.remote.iter().collect();
        remote.sort_by_key(|e| std::cmp::Reverse(e.date));
        samples.extend(remote.into_iter().take(5 - samples.len()).map(|e| ImportSample { title: e.title.clone(), date: e.date, is_meeting: true, excerpt: String::new() }));
    }
    ImportPreview {
        scan_id,
        provider: provider.to_string(),
        origin: scan.origin.clone(),
        total: (notes.len() + scan.remote.len()) as u32,
        notes: (notes.len() + scan.remote.len() - meetings) as u32,
        meetings: meetings as u32,
        transcripts: transcripts as u32,
        attachments: notes.iter().map(|n| n.attachments).sum(),
        earliest: dates.iter().min().copied(),
        latest: dates.iter().max().copied(),
        includes,
        folders,
        speakers,
        existing: already as u32,
        unsupported: scan.unsupported.clone(),
        warnings: scan.warnings.clone(),
        samples,
        details_pending: !scan.remote.is_empty(),
    }
}

/// A plain-text glimpse of a note for the preview.
fn excerpt(n: &ImportedNote) -> String {
    let body = n.summary_md.as_deref().or(n.notes_md.as_deref()).unwrap_or("");
    let plain = cilica_store::notes::markdown_to_plain(body);
    let text = if plain.trim().is_empty() {
        n.transcript.as_ref().map(|t| t.turns.iter().take(3).map(|x| x.text.as_str()).collect::<Vec<_>>().join(" ")).unwrap_or_default()
    } else {
        plain
    };
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > 160 { format!("{}…", one_line.chars().take(159).collect::<String>()) } else { one_line }
}

#[cfg(test)]
mod tests {
    use cilica_domain::RecordingSink;

    use super::*;

    struct NoSecrets;
    impl Secrets for NoSecrets {
        fn key(&self, _: &str) -> Option<String> {
            None
        }
    }

    fn write(dir: &std::path::Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    async fn wait_done(sink: &Arc<RecordingSink>) -> ImportRun {
        for _ in 0..200 {
            let found = sink.0.lock().unwrap().iter().find_map(|e| match e {
                AppEvent::ImportProgress { progress } if progress.phase == ImportPhase::Done => progress.run.clone(),
                _ => None,
            });
            if let Some(run) = found {
                sink.0.lock().unwrap().clear();
                return run;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("import did not finish");
    }

    #[tokio::test]
    async fn importing_again_skips_live_notes_and_brings_back_removed_ones() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("Vault");
        write(&vault, ".obsidian/app.json", "{}");
        write(&vault, "Ideas.md", "# Ideas\n\nFirst idea");
        write(&vault, "Work/Standup.md", "---\ncreated: 2025-01-02\n---\nNotes\n\n## Transcript\n\nSam: Hello\nAna: Hi Sam\nSam: Bye");
        write(&vault, "Board.canvas", "{}");
        let db = Db::open_in_memory().await.unwrap();
        let sink = Arc::new(RecordingSink(Default::default()));
        let importer = Importer::new(db.clone(), sink.clone(), KnownPaths::default(), Arc::new(NoSecrets));
        let options = ImportOptions { category_mode: ImportCategoryMode::KeepFolders, self_names: vec!["Sam".into()] };

        let input = ImportInput::Paths { paths: vec![vault.display().to_string()] };
        let preview = importer.scan("obsidian", input.clone()).await.unwrap();
        assert_eq!(preview.total, 2);
        assert_eq!(preview.meetings, 1);
        assert_eq!(preview.folders, vec!["Work"]);
        assert!(preview.unsupported.iter().any(|u| u.kind == "Canvases"));
        let first = importer.start(preview.scan_id, options.clone()).await.unwrap();
        let run = wait_done(&sink).await;
        assert_eq!((run.imported, run.status), (2, ImportRunStatus::Succeeded));
        let cats = db.list_categories().await.unwrap();
        assert!(cats.iter().any(|c| c.name == "Work") && cats.iter().any(|c| c.name == "From Obsidian"));
        let notes = db.list_notes().await.unwrap();
        let standup = notes.iter().find(|n| n.title == "Standup").unwrap();
        assert_eq!(standup.kind, NoteKind::Meeting);
        let t = db.get_transcript(standup.id).await.unwrap();
        assert!(!t.timed);
        assert!(t.speakers.iter().any(|s| s.is_self), "Sam is the person importing");

        // Everything is already here: everything is skipped, even notes edited in Cilica.
        let ideas = notes.iter().find(|n| n.title == "Ideas").unwrap();
        db.set_note_body(ideas.id, "My own edit", cilica_domain::Author::User, None).await.unwrap();
        let again = importer.scan("obsidian", input.clone()).await.unwrap();
        assert_eq!(again.existing, 2);
        let second = importer.start(again.scan_id, options.clone()).await.unwrap();
        let run = wait_done(&sink).await;
        assert_eq!((run.imported, run.skipped), (0, 2));
        assert!(importer.undo(second.id).await.is_err(), "an import that added nothing has nothing to undo");

        // Undo the first import: both notes go to Trash.
        assert_eq!(importer.undo(first.id).await.unwrap(), 2);
        assert!(db.list_notes().await.unwrap().is_empty());

        // Importing again brings them back, whether they were undone, deleted or emptied from Trash.
        let back = importer.scan("obsidian", input.clone()).await.unwrap();
        assert_eq!(back.existing, 0);
        importer.start(back.scan_id, options.clone()).await.unwrap();
        let run = wait_done(&sink).await;
        assert_eq!((run.imported, run.skipped), (2, 0));
        let standup = db.list_notes().await.unwrap().into_iter().find(|n| n.title == "Standup").unwrap();
        db.trash_note(standup.id).await.unwrap();
        db.empty_trash().await.unwrap();
        let after = importer.scan("obsidian", input).await.unwrap();
        assert_eq!(after.existing, 1);
        importer.start(after.scan_id, options).await.unwrap();
        let run = wait_done(&sink).await;
        assert_eq!((run.imported, run.skipped), (1, 1));
        assert_eq!(db.list_notes().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn pasted_transcripts_and_empty_sources() {
        let db = Db::open_in_memory().await.unwrap();
        let sink = Arc::new(RecordingSink(Default::default()));
        let importer = Importer::new(db.clone(), sink.clone(), KnownPaths::default(), Arc::new(NoSecrets));
        let preview = importer.scan("tldv", ImportInput::Paste { text: "Ana: Morning\nBo: Hi Ana\nAna: Shall we start?".into(), title: Some("Planning".into()) }).await.unwrap();
        assert_eq!(preview.transcripts, 1);
        assert_eq!(preview.speakers.len(), 2);
        importer.start(preview.scan_id, ImportOptions { category_mode: ImportCategoryMode::None, self_names: vec![] }).await.unwrap();
        let run = wait_done(&sink).await;
        assert_eq!(run.imported, 1);
        let note = &db.list_notes().await.unwrap()[0];
        assert_eq!(note.title, "Planning");
        assert!(note.category_id.is_none());

        let empty = tempfile::tempdir().unwrap();
        let err = importer.scan("obsidian", ImportInput::Paths { paths: vec![empty.path().display().to_string()] }).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
        assert!(importer.scan("nope", ImportInput::Account).await.is_err());
        assert!(importer.scan("granola", ImportInput::Account).await.unwrap_err().message.contains("API key"));
    }
}
