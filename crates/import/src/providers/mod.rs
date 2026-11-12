//! Every app Margin imports from, by id. Each provider says what it can find on this computer,
//! and turns an [`ImportInput`] into a [`Scan`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use margin_domain::{DetectedLocation, ImportFidelity, ImportInput, Timestamp, UnsupportedItem};

use crate::formats::files::{Collected, Tree, collect};
use crate::http::WaitNotice;
use crate::model::ImportedNote;
use crate::text::plural;
use crate::{ImportError, KnownPaths};

pub mod api;
pub mod local;

/// What a provider found without being asked to read anything heavy.
#[derive(Debug, Default, Clone)]
pub struct Detect {
    pub app_installed: bool,
    pub locations: Vec<DetectedLocation>,
    pub note: Option<String>,
}

/// An item listed by an account, read in full only while importing.
#[derive(Debug, Clone)]
pub struct RemoteEntry {
    pub external_id: String,
    pub title: String,
    pub date: Option<Timestamp>,
}

/// Fetches one listed item in full.
#[async_trait]
pub trait Loader: Send + Sync {
    async fn load(&self, entry: &RemoteEntry, notice: Option<WaitNotice>) -> Result<ImportedNote, ImportError>;
}

/// What a provider read, before anything is written.
#[derive(Default)]
pub struct Scan {
    pub origin: String,
    /// Notes read in full.
    pub notes: Vec<ImportedNote>,
    /// Items still to fetch, with the loader that fetches them.
    pub remote: Vec<RemoteEntry>,
    pub loader: Option<Arc<dyn Loader>>,
    pub unsupported: Vec<UnsupportedItem>,
    pub warnings: Vec<String>,
    /// What remote items carry, as the provider documents it.
    pub declared: Option<ImportFidelity>,
}

impl Scan {
    pub fn from_collected(origin: String, c: Collected) -> Scan {
        Scan {
            origin,
            notes: c.notes,
            unsupported: c.unsupported.into_iter().map(|(kind, (count, reason))| UnsupportedItem { kind: kind.to_string(), count, reason: reason.to_string() }).collect(),
            warnings: c.warnings,
            ..Default::default()
        }
    }
}

/// Handed to providers while scanning.
pub struct Cx<'a> {
    pub paths: &'a KnownPaths,
    /// The saved API key, for [`ImportInput::Account`].
    pub key: Option<String>,
    pub notice: Option<WaitNotice>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    /// As people know it: "Granola".
    fn name(&self) -> &'static str;
    /// App bundles whose presence means the app is installed ("Granola.app").
    fn apps(&self) -> &'static [&'static str] {
        &[]
    }

    /// Data on this computer, with counts. Must be quick and must not trigger permission prompts.
    async fn detect(&self, paths: &KnownPaths) -> Detect {
        Detect { app_installed: app_installed(paths, self.apps()), ..Default::default() }
    }

    /// Read what the input points at. The default reads files, folders, zips and pasted text.
    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        match input {
            ImportInput::Paths { paths } => scan_paths(paths).await,
            ImportInput::Paste { text, title } => Ok(scan_paste(text, title.as_deref())),
            ImportInput::Local { .. } => Err(ImportError::invalid(format!("Margin found no {} data on this computer. Export your notes from {} and choose the export instead.", self.name(), self.name()))),
            ImportInput::Account => {
                let _ = cx;
                Err(ImportError::invalid(format!("{} has no API key import. Export your notes from {} and choose the export instead.", self.name(), self.name())))
            }
        }
    }

    /// Check a key with one cheap call. Returns a line about the account when the provider says.
    async fn check_key(&self, _key: &str) -> Result<Option<String>, ImportError> {
        Err(ImportError::invalid(format!("{} has no API key import.", self.name())))
    }
}

/// Is any of these app bundles in an Applications folder?
pub fn app_installed(paths: &KnownPaths, apps: &[&str]) -> bool {
    apps.iter().any(|app| paths.applications.iter().any(|dir| dir.join(app).exists()))
}

/// Read chosen files and folders: folders and zips as trees, loose files together.
pub async fn scan_paths(paths: &[String]) -> Result<Scan, ImportError> {
    let paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    if paths.is_empty() {
        return Err(ImportError::invalid("Choose a file or folder to import."));
    }
    tokio::task::spawn_blocking(move || {
        let mut all = Collected::default();
        let mut loose = Vec::new();
        let mut labels = Vec::new();
        for path in &paths {
            if path.is_dir() {
                let tree = Tree::from_dir(path);
                labels.push(tree.label.clone());
                let prefix = if paths.len() > 1 { format!("{}/", tree.label) } else { String::new() };
                all.merge(collect(&tree, &prefix));
            } else if path.is_file() {
                loose.push(path.clone());
            } else {
                all.warnings.push(format!("{} could not be found. It may have been moved.", path.display()));
            }
        }
        if !loose.is_empty() {
            let tree = Tree::from_files(&loose);
            labels.push(if loose.len() == 1 { loose[0].file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() } else { tree.label.clone() });
            all.merge(collect(&tree, ""));
        }
        let origin = match labels.as_slice() {
            [one] => one.clone(),
            _ => plural(paths.len(), "item", "items"),
        };
        Ok(Scan::from_collected(origin, all))
    })
    .await
    .map_err(|_| ImportError::internal("Reading the files stopped unexpectedly. Try again."))?
}

/// Pasted text: a transcript if it reads like one, otherwise a note.
pub fn scan_paste(text: &str, title: Option<&str>) -> Scan {
    use crate::formats::transcript::{parse_cues, parse_text, split_transcript_section};
    let text = text.trim();
    let title = title.map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    let now = margin_domain::now();
    let id = format!("paste:{}", crate::model::note_hash(title.as_deref().unwrap_or(""), text));
    let mut note = ImportedNote { external_id: id, created_at: Some(now), ..Default::default() };
    if let Some(t) = parse_text(text).or_else(|| text.contains("-->").then(|| parse_cues(text)).flatten()) {
        note.attendees = t.names();
        note.transcript = Some(t);
        note.meeting = true;
        note.title = title.unwrap_or_else(|| "Pasted transcript".into());
    } else {
        let (h1, rest) = crate::text::take_h1(text);
        note.title = title.or(h1).unwrap_or_else(|| text.lines().next().unwrap_or("Pasted note").chars().take(60).collect());
        match split_transcript_section(&rest) {
            Some((t, body)) => {
                note.notes_md = Some(crate::text::tidy(&body));
                note.transcript = Some(t);
                note.meeting = true;
            }
            None => note.notes_md = Some(rest),
        }
    }
    Scan { origin: "Pasted text".into(), notes: vec![note], ..Default::default() }
}

/// A provider whose notes arrive as exported files (and pasted text).
pub struct Files {
    pub id: &'static str,
    pub name: &'static str,
    pub apps: &'static [&'static str],
}

#[async_trait]
impl Provider for Files {
    fn id(&self) -> &'static str {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn apps(&self) -> &'static [&'static str] {
        self.apps
    }
}

/// Every provider, by id. The UI's catalogue lists the same ids.
pub fn all() -> Vec<Arc<dyn Provider>> {
    let files = |id, name, apps| -> Arc<dyn Provider> { Arc::new(Files { id, name, apps }) };
    vec![
        // Notes apps.
        Arc::new(local::Obsidian),
        Arc::new(local::AppleNotes),
        files("notion", "Notion", &["Notion.app"]),
        files("evernote", "Evernote", &["Evernote.app"]),
        files("bear", "Bear", &["Bear.app"]),
        files("logseq", "Logseq", &["Logseq.app"]),
        files("google_keep", "Google Keep", &[]),
        files("google_docs", "Google Docs", &[]),
        files("onenote", "OneNote", &["Microsoft OneNote.app"]),
        files("craft", "Craft", &["Craft.app"]),
        files("reflect", "Reflect", &["Reflect.app"]),
        files("joplin", "Joplin", &["Joplin.app"]),
        files("mem", "Mem", &["Mem.app"]),
        // Meeting note takers.
        Arc::new(api::Granola),
        Arc::new(local::Anarlog),
        Arc::new(local::Meetily),
        Arc::new(api::Fireflies),
        Arc::new(api::Fathom),
        Arc::new(api::Otter),
        Arc::new(api::Tldv),
        Arc::new(api::Circleback),
        Arc::new(api::MeetGeek),
        Arc::new(api::Jamie),
        Arc::new(api::Limitless),
        files("read_ai", "Read AI", &[]),
        files("grain", "Grain", &[]),
        files("sembly", "Sembly", &[]),
        files("tactiq", "Tactiq", &[]),
        files("krisp", "Krisp", &["Krisp.app"]),
        files("bluedot", "Bluedot", &[]),
        files("avoma", "Avoma", &[]),
        files("fellow", "Fellow", &["Fellow.app"]),
        files("supernormal", "Supernormal", &[]),
        files("notta", "Notta", &[]),
        files("plaud", "Plaud", &["Plaud.app", "PLAUD.app"]),
        files("macwhisper", "MacWhisper", &["MacWhisper.app"]),
        files("superwhisper", "Superwhisper", &["superwhisper.app", "Superwhisper.app"]),
        files("shadow", "Shadow", &["Shadow.app"]),
        files("notion_meetings", "Notion AI Meeting Notes", &["Notion.app"]),
        files("voice_memos", "Voice Memos", &[]),
        // Meeting platforms.
        files("zoom", "Zoom", &["zoom.us.app"]),
        files("google_meet", "Google Meet", &[]),
        files("teams", "Microsoft Teams", &["Microsoft Teams.app", "Microsoft Teams (work or school).app"]),
        files("webex", "Webex", &["Webex.app"]),
        files("slack", "Slack huddles", &["Slack.app"]),
        files("microsoft_loop", "Microsoft Loop", &[]),
        // Anything else.
        files("files", "Files and folders", &[]),
    ]
}

/// The provider for an id.
pub fn find(id: &str) -> Option<Arc<dyn Provider>> {
    all().into_iter().find(|p| p.id() == id)
}

/// A path for people: `~/Documents/Vault`.
pub fn display_path(paths: &KnownPaths, path: &Path) -> String {
    match path.strip_prefix(&paths.home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    /// The UI's catalogue (apps/desktop/src/features/import/providers.ts) lists exactly the
    /// providers this crate knows, so no app in the picker answers "cannot import from that app".
    #[test]
    fn the_ui_catalogue_matches_the_providers() {
        let ts = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/desktop/src/features/import/providers.ts")).unwrap();
        let re = regex::Regex::new(r#"(?m)^    id: "([a-z_]+)","#).unwrap();
        let ui: BTreeSet<String> = re.captures_iter(&ts).map(|c| c[1].to_string()).collect();
        let rust: BTreeSet<String> = super::all().iter().map(|p| p.id().to_string()).collect();
        assert!(ui.len() > 40, "found only {} ids in providers.ts", ui.len());
        assert_eq!(ui, rust, "only in the UI: {:?}; only in Rust: {:?}", ui.difference(&rust).collect::<Vec<_>>(), rust.difference(&ui).collect::<Vec<_>>());
    }
}
