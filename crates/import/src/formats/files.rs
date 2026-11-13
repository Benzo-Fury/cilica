//! Files, folders and zips the person chose: an Obsidian vault, a Notion export, a Bear backup,
//! a Logseq graph, a Google Takeout, or loose transcripts and documents from any meeting app.
//!
//! Everything is read through a [`Tree`] (a folder on disk or a zip in memory), then each file is
//! turned into notes by what it is. Attachments are looked up in the same tree.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use cilica_domain::Timestamp;
use cilica_domain::chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::Value;

use super::documents::{docx_to_markdown, html_to_markdown, read_enex};
use super::markdown::{self, Embed, Rewrite, image_mime};
use super::transcript::{parse_cues, parse_text, speaker_ref, split_transcript_section};
use crate::model::{ImportedNote, ImportedTranscript, Turn};
use crate::text::{ext, has_notion_id, parse_clock, parse_date, plural, secs_to_ms, strip_notion_id, take_h1, tidy};

/// Files bigger than this are not read (a stray video in a vault).
const MAX_FILE: u64 = 64 * 1024 * 1024;

/// How a tree is laid out, which decides titles, folders and what to skip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Obsidian,
    Notion,
    Bear,
    Logseq,
    Takeout,
    Plain,
}

// ── Trees ─────────────────────────────────────────────────────────────────

/// A folder or a zip, flattened to relative paths with `/` separators.
pub struct Tree {
    pub label: String,
    files: BTreeMap<String, Source>,
    /// Lowercased file name → relative paths, to find attachments anywhere (Obsidian does).
    by_name: HashMap<String, Vec<String>>,
}

enum Source {
    Disk { path: PathBuf, created: Option<Timestamp>, modified: Option<Timestamp> },
    Memory(Vec<u8>),
}

impl Tree {
    /// A folder on disk. Hidden folders (`.obsidian`, `.git`, `.trash`) are skipped.
    pub fn from_dir(dir: &Path) -> Tree {
        let mut files = BTreeMap::new();
        let walker = walkdir::WalkDir::new(dir).follow_links(false).into_iter().filter_entry(|e| e.depth() == 0 || !is_hidden(&e.file_name().to_string_lossy()));
        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(rel) = entry.path().strip_prefix(dir) else { continue };
            let meta = entry.metadata().ok();
            if meta.as_ref().is_some_and(|m| m.len() > MAX_FILE) {
                continue;
            }
            let created = meta.as_ref().and_then(|m| m.created().ok()).map(DateTime::<Utc>::from);
            let modified = meta.as_ref().and_then(|m| m.modified().ok()).map(DateTime::<Utc>::from);
            files.insert(rel_string(rel), Source::Disk { path: entry.path().to_path_buf(), created, modified });
        }
        Self::build(dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| dir.display().to_string()), files)
    }

    /// A zip in memory. Zips inside it (Notion splits big exports) are opened too.
    pub fn from_zip(label: &str, bytes: &[u8]) -> Option<Tree> {
        let mut files = BTreeMap::new();
        read_zip_into(bytes, "", &mut files, 0)?;
        Some(Self::build(label.to_string(), files))
    }

    /// Single files chosen one by one, under their own names.
    pub fn from_files(paths: &[PathBuf]) -> Tree {
        let mut files = BTreeMap::new();
        for path in paths {
            let meta = std::fs::metadata(path).ok();
            if meta.as_ref().is_some_and(|m| m.len() > MAX_FILE) {
                continue;
            }
            let created = meta.as_ref().and_then(|m| m.created().ok()).map(DateTime::<Utc>::from);
            let modified = meta.as_ref().and_then(|m| m.modified().ok()).map(DateTime::<Utc>::from);
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            files.insert(name, Source::Disk { path: path.clone(), created, modified });
        }
        Self::build(plural(paths.len(), "file", "files"), files)
    }

    fn build(label: String, files: BTreeMap<String, Source>) -> Tree {
        let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
        for rel in files.keys() {
            let name = rel.rsplit('/').next().unwrap_or(rel).to_lowercase();
            by_name.entry(name).or_default().push(rel.clone());
        }
        Tree { label, files, by_name }
    }

    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.files.keys()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn contains(&self, rel: &str) -> bool {
        self.files.contains_key(rel)
    }

    pub fn read(&self, rel: &str) -> Option<Vec<u8>> {
        match self.files.get(rel)? {
            Source::Disk { path, .. } => std::fs::read(path).ok(),
            Source::Memory(bytes) => Some(bytes.clone()),
        }
    }

    pub fn read_text(&self, rel: &str) -> Option<String> {
        self.read(rel).map(|b| decode_text(&b))
    }

    /// When the file was created and last changed, for files on disk.
    pub fn times(&self, rel: &str) -> (Option<Timestamp>, Option<Timestamp>) {
        match self.files.get(rel) {
            Some(Source::Disk { created, modified, .. }) => (*created, *modified),
            _ => (None, None),
        }
    }

    /// Find an attachment written in a note at `from`: relative to the note, from the root, or
    /// anywhere by file name (Obsidian's shortest-path links).
    pub fn resolve(&self, from: &str, target: &str) -> Option<String> {
        let target = target.trim().trim_start_matches("./").replace('\\', "/");
        let dir = from.rsplit_once('/').map_or("", |(d, _)| d);
        let joined = normalise(&if dir.is_empty() { target.clone() } else { format!("{dir}/{target}") });
        if self.files.contains_key(&joined) {
            return Some(joined);
        }
        if self.files.contains_key(&target) {
            return Some(target);
        }
        let name = target.rsplit('/').next().unwrap_or(&target).to_lowercase();
        let candidates = self.by_name.get(&name)?;
        // Prefer the candidate whose path ends with what was written.
        candidates.iter().find(|c| c.to_lowercase().ends_with(&target.to_lowercase())).or_else(|| candidates.first()).cloned()
    }

    /// Guess the layout from what the tree holds.
    pub fn layout(&self) -> Layout {
        let keys: Vec<&String> = self.files.keys().collect();
        if keys.iter().any(|k| k.contains(".textbundle/")) {
            Layout::Bear
        } else if keys.iter().any(|k| k.starts_with("logseq/") || k.contains("/logseq/config.edn")) || (keys.iter().any(|k| k.starts_with("journals/")) && keys.iter().any(|k| k.starts_with("pages/"))) {
            Layout::Logseq
        } else if keys.iter().any(|k| k.starts_with("Takeout/") || k.contains("Keep/") && k.ends_with(".json")) {
            Layout::Takeout
        } else if keys.iter().filter(|k| k.ends_with(".md") || k.ends_with(".html")).take(20).any(|k| has_notion_id(stem(k))) {
            Layout::Notion
        } else if keys.iter().any(|k| k.ends_with(".canvas") || k.ends_with(".base")) {
            Layout::Obsidian
        } else {
            Layout::Plain
        }
    }
}

fn read_zip_into(bytes: &[u8], prefix: &str, files: &mut BTreeMap<String, Source>, depth: u8) -> Option<()> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    for i in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(i) else { continue };
        if entry.is_dir() || entry.size() > MAX_FILE {
            continue;
        }
        let Some(name) = entry.enclosed_name() else { continue };
        let rel = rel_string(&name);
        if rel.split('/').any(|part| is_hidden(part) || part == "__MACOSX") {
            continue;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        if entry.read_to_end(&mut data).is_err() {
            continue;
        }
        let full = if prefix.is_empty() { rel.clone() } else { format!("{prefix}/{rel}") };
        if rel.to_lowercase().ends_with(".zip") && depth < 2 {
            let inner_prefix = full.trim_end_matches(".zip").trim_end_matches(".ZIP").to_string();
            // Notion's "Part-1.zip" files hold the real export; flatten them into the parent.
            let inner_prefix = if depth == 0 { prefix.to_string() } else { inner_prefix };
            if read_zip_into(&data, &inner_prefix, files, depth + 1).is_some() {
                continue;
            }
        }
        files.insert(full, Source::Memory(data));
    }
    Some(())
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.') || name == "__MACOSX" || name == "node_modules"
}

fn rel_string(path: &Path) -> String {
    path.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>().join("/")
}

fn normalise(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

fn stem(rel: &str) -> &str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

/// UTF-8 (with or without BOM), else UTF-16 with a BOM, else Latin-1.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).to_string();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let le = bytes[0] == 0xFF;
        let units: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        return String::from_utf16_lossy(&units);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

// ── Reading a tree ────────────────────────────────────────────────────────

/// Notes found in a tree, plus what was left out.
#[derive(Debug, Default)]
pub struct Collected {
    pub notes: Vec<ImportedNote>,
    /// Kind → (count, reason), in a stable order.
    pub unsupported: BTreeMap<&'static str, (u32, &'static str)>,
    pub warnings: Vec<String>,
}

impl Collected {
    fn skip(&mut self, kind: &'static str, reason: &'static str) {
        self.skip_n(kind, 1, reason);
    }

    fn skip_n(&mut self, kind: &'static str, n: u32, reason: &'static str) {
        if n > 0 {
            self.unsupported.entry(kind).or_insert((0, reason)).0 += n;
        }
    }

    fn rewrite_report(&mut self, r: Rewrite) {
        self.skip_n("Attachments", r.other_files, "PDFs, audio and other files attached to notes are left out. Images come along inside the note.");
        self.skip_n("Large images", r.too_big, "Images over 2 MB are left out; a line in the note marks where each one was.");
        self.skip_n("Missing images", r.missing, "Some images a note links to are not in the folder; a line in the note marks where each one was.");
        self.skip_n("Embedded notes", r.embedded_notes, "A note embedded in another shows as its name in italics. The embedded note is imported on its own.");
    }

    pub fn merge(&mut self, other: Collected) {
        self.notes.extend(other.notes);
        for (k, (n, reason)) in other.unsupported {
            self.unsupported.entry(k).or_insert((0, reason)).0 += n;
        }
        self.warnings.extend(other.warnings);
    }
}

/// Read every note in a tree. `id_prefix` keeps ids apart when several trees are imported together.
pub fn collect(tree: &Tree, id_prefix: &str) -> Collected {
    let layout = tree.layout();
    let mut out = Collected::default();
    let mut used_images: std::collections::HashSet<String> = std::collections::HashSet::new();
    let paths: Vec<String> = tree.paths().cloned().collect();
    // Notion and Takeout export the same note twice (HTML and JSON, or MD and CSV): keep one.
    let has = |rel: &str| tree.contains(rel);
    for rel in &paths {
        let e = ext(Path::new(rel));
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let id = format!("{id_prefix}{rel}");
        match e.as_str() {
            "md" | "markdown" | "mdown" | "mkd" if layout == Layout::Bear && rel.contains(".textbundle/") => {
                if name.starts_with("text.") {
                    read_textbundle(tree, rel, &id, &mut out, &mut used_images);
                }
            }
            "md" | "markdown" | "mdown" | "mkd" => {
                if name.to_lowercase().ends_with(".excalidraw.md") {
                    out.skip("Excalidraw drawings", "Drawings made with the Excalidraw plugin are left out.");
                    continue;
                }
                if layout == Layout::Logseq && (rel.starts_with("logseq/") || rel.contains("/logseq/")) {
                    continue;
                }
                if let Some(text) = tree.read_text(rel) {
                    read_markdown_file(tree, rel, &id, &text, layout, &mut out, &mut used_images);
                }
            }
            "txt" | "text" => {
                if let Some(text) = tree.read_text(rel) {
                    read_plain_text(tree, rel, &id, &text, &mut out);
                }
            }
            "vtt" | "srt" | "sbv" => match tree.read_text(rel).and_then(|t| parse_cues(&t)) {
                Some(t) => out.notes.push(transcript_note(tree, rel, &id, t)),
                None => out.warnings.push(format!("{name} has no transcript Cilica could read.")),
            },
            "docx" => match tree.read(rel).and_then(|b| docx_to_markdown(&b)) {
                Some(md) => read_markdown_file(tree, rel, &id, &md, Layout::Plain, &mut out, &mut used_images),
                None => out.warnings.push(format!("{name} could not be opened as a Word document. Export it again as .docx or .txt.")),
            },
            "html" | "htm" => {
                // Takeout writes each Keep note as .json and .html; Notion can export either.
                if has(&rel.replace(&format!(".{e}"), ".json")) || has(&rel.replace(&format!(".{e}"), ".md")) {
                    continue;
                }
                if let Some(text) = tree.read_text(rel) {
                    let md = html_to_markdown(&text);
                    read_markdown_file(tree, rel, &id, &md, layout, &mut out, &mut used_images);
                }
            }
            "enex" => {
                if let Some(text) = tree.read_text(rel) {
                    let notebook = strip_notion_id(stem(rel));
                    let (mut notes, skipped) = read_enex(&text, &notebook);
                    for n in &mut notes {
                        n.external_id = format!("{id_prefix}{}", n.external_id);
                    }
                    if notes.is_empty() {
                        out.warnings.push(format!("{name} has no notes Cilica could read."));
                    }
                    out.notes.extend(notes);
                    out.skip_n("Attachments", skipped.files, "PDFs, audio and other files attached to notes are left out. Images come along inside the note.");
                    out.skip_n("Encrypted text", skipped.encrypted, "Text you encrypted in Evernote stays encrypted there; Cilica cannot read it.");
                    out.skip_n("Large images", skipped.too_big, "Images over 2 MB are left out; a line in the note marks where each one was.");
                }
            }
            "csv" | "tsv" => {
                if layout == Layout::Notion {
                    out.skip("Database views", "Notion databases come across as one note per row; table and board views are left out.");
                    continue;
                }
                if let Some(text) = tree.read_text(rel) {
                    read_csv(rel, &id, &text, e == "tsv", &mut out);
                }
            }
            "json" => {
                if let Some(text) = tree.read_text(rel) {
                    read_json(tree, rel, &id, &text, &mut out, &mut used_images);
                }
            }
            "zip" | "bear2bk" => match tree.read(rel).and_then(|b| Tree::from_zip(name, &b)) {
                Some(inner) => out.merge(collect(&inner, &format!("{id}/"))),
                None => out.warnings.push(format!("{name} could not be opened.")),
            },
            "canvas" => out.skip("Canvases", "Cilica has no canvas view, so canvas boards are left out. The notes on them are imported on their own."),
            "base" => out.skip("Bases", "Obsidian Bases are database views. The notes they list are imported on their own."),
            "edn" | "css" | "js" | "ds_store" | "ini" | "plist" => {}
            _ if image_mime(name).is_some() => {}
            "pdf" | "mp3" | "m4a" | "wav" | "mp4" | "mov" | "webm" | "ogg" | "opus" | "xlsx" | "pptx" | "key" | "pages" | "numbers" => {
                out.skip("Other files", "PDFs, recordings and other files that are not notes are left out.")
            }
            _ => out.skip("Other files", "PDFs, recordings and other files that are not notes are left out."),
        }
    }
    // Images nothing links to are not notes; say so only when there are some.
    let loose = paths.iter().filter(|p| image_mime(p).is_some() && !used_images.contains(*p) && !p.contains(".textbundle/")).count() as u32;
    if layout != Layout::Takeout {
        out.skip_n("Unlinked images", loose, "Images no note links to are left out.");
    }
    out
}

fn note_times(tree: &Tree, rel: &str) -> (Option<Timestamp>, Option<Timestamp>) {
    tree.times(rel)
}

/// The folder a note sits in, as category names, outermost first.
fn folder_of(rel: &str, layout: Layout) -> Vec<String> {
    let mut parts: Vec<String> = rel.split('/').map(str::to_string).collect();
    parts.pop();
    match layout {
        Layout::Notion => parts.into_iter().map(|p| strip_notion_id(&p)).filter(|p| !p.is_empty() && !p.starts_with("Export-")).collect(),
        Layout::Logseq => {
            if rel.starts_with("journals/") {
                vec!["Journals".into()]
            } else {
                Vec::new()
            }
        }
        _ => parts,
    }
}

static DAILY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}[-_.]\d{2}[-_.]\d{2}$").unwrap());
static LOGSEQ_PROP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*-?\s*([A-Za-z][\w-]*)::\s*(.*)$").unwrap());
static BLOCK_REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(\(([0-9a-f-]{36})\)\)").unwrap());

fn read_markdown_file(tree: &Tree, rel: &str, id: &str, text: &str, layout: Layout, out: &mut Collected, used: &mut std::collections::HashSet<String>) {
    let doc = markdown::read(text);
    let mut body = doc.body.clone();
    let mut properties = doc.properties.clone();
    if layout == Layout::Logseq {
        // `key:: value` properties; `id::` lines only anchor block references.
        let mut props = Vec::new();
        body = LOGSEQ_PROP
            .replace_all(&body, |c: &regex::Captures| {
                let key = c[1].to_string();
                if key != "id" && key != "collapsed" {
                    props.push((key, c[2].trim().to_string()));
                }
                String::new()
            })
            .to_string();
        properties.extend(props);
        let refs = BLOCK_REF.find_iter(&body).count() as u32;
        out.skip_n("Block references", refs, "Logseq block references become “(block reference)”; the block itself stays in its own page.");
        body = BLOCK_REF.replace_all(&body, "(block reference)").to_string();
    }
    let dataview = body.matches("```dataview").count() as u32;
    out.skip_n("Dataview queries", dataview, "Dataview queries are kept as code blocks. Cilica does not run them.");
    let (rewritten, report) = markdown::rewrite(&body, &mut |target: &str| {
        let found = tree.resolve(rel, target)?;
        let mime = image_mime(&found)?;
        used.insert(found.clone());
        Some(Embed { bytes: tree.read(&found)?, mime })
    });
    out.rewrite_report(report);
    let (h1, rest) = take_h1(&rewritten);
    let file_title = match layout {
        Layout::Notion => strip_notion_id(stem(rel)),
        Layout::Logseq => logseq_title(stem(rel)),
        _ => stem(rel).to_string(),
    };
    // A first heading that repeats the file name is the title, not content.
    let (title, body) = match (&doc.title, h1) {
        (Some(t), Some(h)) if h.eq_ignore_ascii_case(t) => (t.clone(), rest),
        (Some(t), _) => (t.clone(), rewritten),
        (None, Some(h)) if layout != Layout::Obsidian || h.eq_ignore_ascii_case(&file_title) => (h, rest),
        (None, _) => (file_title.clone(), rewritten),
    };
    let (created_fs, modified_fs) = note_times(tree, rel);
    let daily = DAILY.is_match(stem(rel)).then(|| parse_date(stem(rel))).flatten();
    let mut note = ImportedNote {
        external_id: id.to_string(),
        title,
        created_at: doc.created.or(daily).or(created_fs),
        updated_at: doc.updated.or(modified_fs),
        attendees: doc.attendees.clone(),
        folder: folder_of(rel, layout),
        tags: doc.tags.clone(),
        properties,
        attachments: report.images,
        ..Default::default()
    };
    // Meeting exports (Grain, Sembly, Shadow, Fireflies) keep the transcript in a section.
    match split_transcript_section(&body) {
        Some((transcript, rest)) => {
            note.notes_md = Some(tidy(&rest));
            note.transcript = Some(transcript);
            note.meeting = true;
        }
        None => note.notes_md = Some(body),
    }
    out.notes.push(note);
}

fn logseq_title(stem: &str) -> String {
    let decoded = markdown::percent_decode(stem).replace("___", "/");
    match parse_date(&decoded) {
        Some(d) if DAILY.is_match(&decoded) => d.with_timezone(&cilica_domain::chrono::Local).format("%-d %b %Y").to_string(),
        _ => decoded,
    }
}

fn read_plain_text(tree: &Tree, rel: &str, id: &str, text: &str, out: &mut Collected) {
    if let Some(t) = parse_text(text).or_else(|| text.contains("-->").then(|| parse_cues(text)).flatten()) {
        out.notes.push(transcript_note(tree, rel, id, t));
        return;
    }
    let (created, modified) = note_times(tree, rel);
    let mut note = ImportedNote {
        external_id: id.to_string(),
        title: stem(rel).to_string(),
        created_at: created,
        updated_at: modified,
        folder: folder_of(rel, Layout::Plain),
        ..Default::default()
    };
    match split_transcript_section(text) {
        Some((t, rest)) => {
            note.notes_md = Some(tidy(&rest));
            note.transcript = Some(t);
            note.meeting = true;
        }
        None => note.notes_md = Some(tidy(text)),
    }
    out.notes.push(note);
}

fn transcript_note(tree: &Tree, rel: &str, id: &str, transcript: ImportedTranscript) -> ImportedNote {
    let (created, modified) = note_times(tree, rel);
    let title = stem(rel).replace(['_'], " ");
    ImportedNote {
        external_id: id.to_string(),
        title: title.trim().to_string(),
        created_at: created,
        updated_at: modified,
        started_at: created,
        duration_ms: transcript.turns.iter().filter_map(|t| t.end_ms).max(),
        folder: folder_of(rel, Layout::Plain),
        attendees: transcript.names(),
        transcript: Some(transcript),
        meeting: true,
        ..Default::default()
    }
}

fn read_textbundle(tree: &Tree, rel: &str, id: &str, out: &mut Collected, used: &mut std::collections::HashSet<String>) {
    let Some(text) = tree.read_text(rel) else { return };
    let bundle = rel.rsplit_once('/').map_or("", |(d, _)| d);
    let info: Value = tree.read_text(&format!("{bundle}/info.json")).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let bear = info.get("net.shinyfrog.bear").unwrap_or(&Value::Null);
    let date = |k: &str| bear.get(k).and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_f64().map(|n| n.to_string()))).and_then(|s| parse_date(&s));
    if bear.get("trashed").and_then(Value::as_u64) == Some(1) {
        return;
    }
    let (rewritten, report) = markdown::rewrite(&text, &mut |target: &str| {
        let found = tree.resolve(rel, target)?;
        let mime = image_mime(&found)?;
        used.insert(found.clone());
        Some(Embed { bytes: tree.read(&found)?, mime })
    });
    out.rewrite_report(report);
    let (h1, rest) = take_h1(&rewritten);
    let bundle_name = stem(bundle).to_string();
    out.notes.push(ImportedNote {
        external_id: id.trim_end_matches(&format!("/{}", rel.rsplit('/').next().unwrap_or(""))).to_string(),
        title: h1.unwrap_or(bundle_name),
        created_at: date("creationDate"),
        updated_at: date("modificationDate"),
        notes_md: Some(rest),
        // Bear's tags already sit in the text as #tags; listing them again would repeat them.
        attachments: report.images,
        ..Default::default()
    });
}

fn read_csv(rel: &str, id: &str, text: &str, tabs: bool, out: &mut Collected) {
    let mut reader = csv::ReaderBuilder::new().delimiter(if tabs { b'\t' } else { b',' }).flexible(true).from_reader(text.as_bytes());
    let Ok(headers) = reader.headers().map(|h| h.iter().map(|s| s.trim().to_string()).collect::<Vec<_>>()) else {
        out.warnings.push(format!("{} could not be read as a table.", rel.rsplit('/').next().unwrap_or(rel)));
        return;
    };
    let find = |names: &[&str]| headers.iter().position(|h| names.iter().any(|n| h.eq_ignore_ascii_case(n))).or_else(|| headers.iter().position(|h| names.iter().any(|n| h.to_lowercase().contains(n))));
    let title_col = find(&["title", "name", "meeting", "subject", "meeting title", "note title"]);
    let date_col = find(&["date", "created", "created at", "start", "start time", "meeting date", "timestamp"]);
    let end_col = find(&["end", "end time", "ended"]);
    let summary_col = find(&["summary", "enhanced notes", "ai notes", "overview", "notes summary"]);
    let notes_col = find(&["my notes", "private notes", "notes", "content", "body", "text"]).filter(|c| Some(*c) != summary_col);
    let transcript_col = find(&["transcript", "full transcript"]);
    let attendees_col = find(&["attendees", "participants", "people"]);
    let tags_col = find(&["tags", "labels", "folder"]);
    if title_col.is_none() && summary_col.is_none() && notes_col.is_none() && transcript_col.is_none() {
        out.warnings.push(format!("{} does not look like notes: Cilica found no title, notes or transcript column.", rel.rsplit('/').next().unwrap_or(rel)));
        return;
    }
    let used = [title_col, date_col, end_col, summary_col, notes_col, transcript_col, attendees_col, tags_col];
    for (i, row) in reader.records().flatten().enumerate() {
        let get = |c: Option<usize>| c.and_then(|c| row.get(c)).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        let transcript = get(transcript_col).map(|t| parse_text(&t).unwrap_or_else(|| ImportedTranscript::new(vec![Turn { speaker: crate::model::SpeakerRef::Unknown, start_ms: None, end_ms: None, text: t }])));
        let extra: Vec<(String, String)> = headers.iter().enumerate().filter(|(c, _)| !used.contains(&Some(*c))).filter_map(|(c, h)| row.get(c).map(str::trim).filter(|v| !v.is_empty()).map(|v| (h.clone(), v.to_string()))).collect();
        let started = get(date_col).and_then(|d| parse_date(&d));
        let note = ImportedNote {
            external_id: format!("{id}#{}", get(title_col).map_or_else(|| i.to_string(), |t| format!("{i}:{t}"))),
            title: get(title_col).unwrap_or_else(|| format!("{} {}", stem(rel), i + 1)),
            created_at: started,
            started_at: transcript.as_ref().and(started),
            ended_at: get(end_col).and_then(|d| parse_date(&d)),
            summary_md: get(summary_col),
            notes_md: get(notes_col),
            attendees: get(attendees_col).map(|a| a.split([',', ';']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default(),
            tags: get(tags_col).map(|a| a.split([',', ';']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default(),
            properties: extra,
            meeting: transcript.is_some(),
            transcript,
            ..Default::default()
        };
        out.notes.push(note);
    }
}

fn read_json(tree: &Tree, rel: &str, id: &str, text: &str, out: &mut Collected, used: &mut std::collections::HashSet<String>) {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        out.warnings.push(format!("{} is not valid JSON.", rel.rsplit('/').next().unwrap_or(rel)));
        return;
    };
    // Google Keep (Takeout): one note per file.
    if v.get("textContent").is_some() || v.get("listContent").is_some() || v.get("createdTimestampUsec").is_some() {
        if v.get("isTrashed").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let mut body = v.get("textContent").and_then(Value::as_str).unwrap_or("").to_string();
        if let Some(items) = v.get("listContent").and_then(Value::as_array) {
            let list: Vec<String> = items
                .iter()
                .map(|i| format!("- [{}] {}", if i.get("isChecked").and_then(Value::as_bool) == Some(true) { "x" } else { " " }, i.get("text").and_then(Value::as_str).unwrap_or("")))
                .collect();
            body = format!("{body}\n\n{}", list.join("\n"));
        }
        let mut images = 0;
        for a in v.get("attachments").and_then(Value::as_array).into_iter().flatten() {
            let Some(path) = a.get("filePath").and_then(Value::as_str) else { continue };
            match tree.resolve(rel, path).and_then(|f| tree.read(&f).map(|b| (f, b))) {
                Some((f, bytes)) if bytes.len() <= markdown::MAX_INLINE_IMAGE && image_mime(&f).is_some() => {
                    used.insert(f.clone());
                    images += 1;
                    body.push_str(&format!("\n\n![{path}](data:{};base64,{})", image_mime(&f).unwrap_or("image/png"), base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)));
                }
                _ => out.skip("Attachments", "PDFs, audio and other files attached to notes are left out. Images come along inside the note."),
            }
        }
        let usec = |k: &str| v.get(k).and_then(|n| n.as_f64().or_else(|| n.as_str().and_then(|s| s.parse().ok()))).and_then(|n| parse_date(&n.to_string()));
        let title = v.get("title").and_then(Value::as_str).filter(|t| !t.trim().is_empty()).map(str::to_string).unwrap_or_else(|| body.lines().find(|l| !l.trim().is_empty()).unwrap_or("Untitled").chars().take(60).collect());
        let mut tags: Vec<String> = v.get("labels").and_then(Value::as_array).into_iter().flatten().filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_string)).collect();
        if v.get("isArchived").and_then(Value::as_bool) == Some(true) {
            tags.push("archived".into());
        }
        out.notes.push(ImportedNote {
            external_id: id.to_string(),
            title,
            created_at: usec("createdTimestampUsec"),
            updated_at: usec("userEditedTimestampUsec"),
            notes_md: Some(tidy(&body)),
            tags,
            folder: vec!["Google Keep".into()],
            attachments: images,
            ..Default::default()
        });
        return;
    }
    // A transcript as JSON: an array of segments, or an object holding one.
    let list = v.as_array().or_else(|| ["sentences", "segments", "transcript", "utterances", "entries", "data", "results"].iter().find_map(|k| v.get(*k).and_then(Value::as_array)));
    if let Some(items) = list
        && let Some(transcript) = json_transcript(items)
    {
        let title = v.get("title").or_else(|| v.get("name")).and_then(Value::as_str).map(str::to_string);
        let mut note = transcript_note(tree, rel, id, transcript);
        if let Some(t) = title {
            note.title = t;
        }
        if let Some(d) = ["date", "dateString", "created_at", "startTime", "start_time"].iter().find_map(|k| v.get(*k)).and_then(|d| d.as_str().map(str::to_string).or_else(|| d.as_f64().map(|n| n.to_string()))).and_then(|s| parse_date(&s)) {
            note.started_at = Some(d);
            note.created_at = Some(d);
        }
        if let Some(summary) = v.get("summary").and_then(|s| s.as_str().map(str::to_string).or_else(|| s.get("overview").and_then(Value::as_str).map(str::to_string))) {
            note.summary_md = Some(summary);
        }
        out.notes.push(note);
        return;
    }
    out.skip("Other files", "PDFs, recordings and other files that are not notes are left out.");
}

/// Segments in JSON with any of the usual key names. Number times are seconds unless the key says ms.
pub fn json_transcript(items: &[Value]) -> Option<ImportedTranscript> {
    let mut turns = Vec::new();
    for item in items {
        let text = ["text", "content", "transcript", "sentence", "words", "raw_text"].iter().find_map(|k| item.get(*k).and_then(Value::as_str))?;
        let speaker = ["speaker_name", "speakerName", "speaker", "name", "participant", "displayName"]
            .iter()
            .find_map(|k| item.get(*k).and_then(|s| s.as_str().map(str::to_string).or_else(|| s.get("name").or_else(|| s.get("display_name")).and_then(Value::as_str).map(str::to_string))))
            .unwrap_or_default();
        let time = |keys: &[&str]| -> Option<u32> {
            keys.iter().find_map(|k| {
                let v = item.get(*k)?;
                let ms_key = k.to_lowercase().ends_with("ms");
                match v {
                    Value::Number(n) => n.as_f64().and_then(|f| if ms_key { Some(f.round() as u32) } else { secs_to_ms(f) }),
                    Value::String(s) => parse_clock(s).or_else(|| s.parse::<f64>().ok().and_then(|f| if ms_key { Some(f as u32) } else { secs_to_ms(f) })),
                    _ => None,
                }
            })
        };
        turns.push(Turn {
            speaker: speaker_ref(&speaker),
            start_ms: time(&["start_ms", "startMs", "startOffsetMs", "start_time", "startTime", "start", "timestamp", "offset"]),
            end_ms: time(&["end_ms", "endMs", "endOffsetMs", "end_time", "endTime", "end"]),
            text: text.trim().to_string(),
        });
    }
    (!turns.is_empty()).then(|| super::transcript::finish(turns))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SpeakerRef;

    fn write(dir: &Path, rel: &str, content: &[u8]) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    #[test]
    fn an_obsidian_vault() {
        let dir = tempfile::tempdir().unwrap();
        let v = dir.path();
        write(v, ".obsidian/app.json", b"{}");
        write(v, "Welcome.md", b"# Welcome\n\nHello [[Projects/Acme|Acme]].\n\n![[pic.png]]");
        write(v, "Projects/Acme.md", b"---\ntags: [client]\ncreated: 2025-01-02\n---\nAcme notes\n\n```dataview\nLIST\n```");
        write(v, "2026-11-12.md", b"Daily stuff");
        write(v, "assets/pic.png", &[137, 80, 78, 71]);
        write(v, "assets/unused.png", &[1]);
        write(v, "Board.canvas", b"{}");
        write(v, "Tasks.base", b"");
        write(v, "Sketch.excalidraw.md", b"x");
        write(v, "doc.pdf", b"%PDF");
        let tree = Tree::from_dir(v);
        assert_eq!(tree.layout(), Layout::Obsidian);
        let got = collect(&tree, "");
        assert_eq!(got.notes.len(), 3, "{:?}", got.notes.iter().map(|n| &n.title).collect::<Vec<_>>());
        let welcome = got.notes.iter().find(|n| n.title == "Welcome").unwrap();
        let body = welcome.notes_md.as_deref().unwrap();
        assert!(body.contains("Hello [[Projects/Acme|Acme]]."), "{body}");
        assert!(body.contains("data:image/png;base64,"));
        assert!(!body.starts_with("# Welcome"));
        let acme = got.notes.iter().find(|n| n.title == "Acme").unwrap();
        assert_eq!(acme.folder, vec!["Projects"]);
        assert_eq!(acme.tags, vec!["client"]);
        assert!(acme.created_at.is_some());
        let daily = got.notes.iter().find(|n| n.title == "2026-11-12").unwrap();
        assert_eq!(daily.created_at.unwrap().with_timezone(&cilica_domain::chrono::Local).format("%Y-%m-%d").to_string(), "2026-11-12");
        for kind in ["Canvases", "Bases", "Excalidraw drawings", "Dataview queries", "Other files", "Unlinked images"] {
            assert_eq!(got.unsupported.get(kind).map(|u| u.0), Some(1), "{kind}: {:?}", got.unsupported);
        }
    }

    #[test]
    fn a_notion_zip_with_nested_parts() {
        let mut inner = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        inner.start_file("Team 0123456789abcdef0123456789abcdef/Kickoff 1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d.md", opts).unwrap();
        std::io::Write::write_all(&mut inner, b"# Kickoff\n\nCreated: March 4, 2025 9:00 AM\n\nAgenda").unwrap();
        inner.start_file("Team 0123456789abcdef0123456789abcdef.md", opts).unwrap();
        std::io::Write::write_all(&mut inner, b"# Team\n\nHome").unwrap();
        inner.start_file("Tasks 0123456789abcdef0123456789abcde0.csv", opts).unwrap();
        std::io::Write::write_all(&mut inner, b"Name,Status\nA,Done").unwrap();
        let inner = inner.finish().unwrap().into_inner();
        let mut outer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        outer.start_file("Export-abc-Part-1.zip", opts).unwrap();
        std::io::Write::write_all(&mut outer, &inner).unwrap();
        let outer = outer.finish().unwrap().into_inner();
        let tree = Tree::from_zip("Export.zip", &outer).unwrap();
        assert_eq!(tree.layout(), Layout::Notion);
        let got = collect(&tree, "");
        assert_eq!(got.notes.len(), 2);
        let kickoff = got.notes.iter().find(|n| n.title == "Kickoff").unwrap();
        assert_eq!(kickoff.folder, vec!["Team"]);
        assert_eq!(got.unsupported.get("Database views").map(|u| u.0), Some(1));
    }

    #[test]
    fn loose_meeting_exports() {
        let dir = tempfile::tempdir().unwrap();
        let v = dir.path();
        write(v, "Weekly sync.txt", b"Jane Doe  0:00\nHello all.\n\nSam Lee  0:05\nHi Jane.\n");
        write(v, "Teams call.vtt", b"WEBVTT\n\n00:00:00.000 --> 00:00:02.000\n<v Ana>Morning.</v>\n\n00:00:03.000 --> 00:00:04.000\n<v Bo>Hi.</v>\n");
        write(v, "Grain.md", b"# Design review\n\n## Summary\n\nShip it.\n\n## Transcript\n\n**Ana** (00:01): Looks good\n**Bo** (00:05): Agreed\n");
        write(v, "granola.csv", b"Title,Date,Summary,Transcript,Attendees\nKickoff,2025-03-04 09:00,We agreed,\"Me: hi\nThem: hello\",\"Sam, Ana\"\n");
        write(v, "fireflies.json", br#"{"title":"Standup","date":1706715000000,"sentences":[{"speaker_name":"Ana","text":"Done","start_time":1.5,"end_time":2.0},{"speaker_name":"Bo","text":"Ok","start_time":"3.0","end_time":"4"}]}"#);
        let tree = Tree::from_dir(v);
        let got = collect(&tree, "");
        assert_eq!(got.notes.len(), 5, "{:?} {:?}", got.notes.iter().map(|n| &n.title).collect::<Vec<_>>(), got.warnings);
        let otter = got.notes.iter().find(|n| n.title == "Weekly sync").unwrap();
        assert_eq!(otter.transcript.as_ref().unwrap().turns.len(), 2);
        assert!(otter.is_meeting());
        let teams = got.notes.iter().find(|n| n.title == "Teams call").unwrap();
        assert_eq!(teams.attendees, vec!["Ana", "Bo"]);
        let grain = got.notes.iter().find(|n| n.title == "Design review").unwrap();
        assert!(grain.notes_md.as_deref().unwrap().contains("Ship it."));
        assert_eq!(grain.transcript.as_ref().unwrap().turns[1].start_ms, Some(5_000));
        let csv = got.notes.iter().find(|n| n.title == "Kickoff").unwrap();
        assert_eq!(csv.summary_md.as_deref(), Some("We agreed"));
        assert_eq!(csv.attendees, vec!["Sam", "Ana"]);
        assert_eq!(csv.transcript.as_ref().unwrap().turns[0].speaker, SpeakerRef::Me);
        let ff = got.notes.iter().find(|n| n.title == "Standup").unwrap();
        assert_eq!(ff.transcript.as_ref().unwrap().turns[1].start_ms, Some(3_000));
        assert_eq!(ff.started_at.unwrap().to_rfc3339(), "2024-01-31T15:30:00+00:00");
    }

    #[test]
    fn bear_backups_and_keep_takeout() {
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        z.start_file("Recipe.textbundle/text.md", opts).unwrap();
        std::io::Write::write_all(&mut z, b"# Pancakes\n\nFlour #food\n\n![](assets/p.jpg)").unwrap();
        z.start_file("Recipe.textbundle/info.json", opts).unwrap();
        std::io::Write::write_all(&mut z, br#"{"net.shinyfrog.bear":{"creationDate":"2024-02-01T10:00:00Z","modificationDate":"2024-02-02T10:00:00Z"}}"#).unwrap();
        z.start_file("Recipe.textbundle/assets/p.jpg", opts).unwrap();
        std::io::Write::write_all(&mut z, &[0xff, 0xd8]).unwrap();
        let bytes = z.finish().unwrap().into_inner();
        let tree = Tree::from_zip("Notes.bear2bk", &bytes).unwrap();
        assert_eq!(tree.layout(), Layout::Bear);
        let got = collect(&tree, "");
        assert_eq!(got.notes.len(), 1);
        assert_eq!(got.notes[0].title, "Pancakes");
        assert_eq!(got.notes[0].attachments, 1);
        assert!(got.notes[0].created_at.is_some());

        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Takeout/Keep/Shopping.json", br#"{"title":"Shopping","listContent":[{"text":"Milk","isChecked":true},{"text":"Eggs","isChecked":false}],"createdTimestampUsec":1706715000000000,"userEditedTimestampUsec":1706715000000000,"labels":[{"name":"home"}],"isTrashed":false}"#);
        write(dir.path(), "Takeout/Keep/Shopping.html", b"<p>dup</p>");
        write(dir.path(), "Takeout/Keep/Old.json", br#"{"title":"Old","textContent":"x","isTrashed":true}"#);
        let got = collect(&Tree::from_dir(dir.path()), "");
        assert_eq!(got.notes.len(), 1);
        let keep = &got.notes[0];
        assert!(keep.notes_md.as_deref().unwrap().contains("- [x] Milk"));
        assert_eq!(keep.tags, vec!["home"]);
        assert_eq!(keep.created_at.unwrap().to_rfc3339(), "2024-01-31T15:30:00+00:00");
    }

    #[test]
    fn logseq_graphs() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "logseq/config.edn", b"{}");
        write(dir.path(), "journals/2024_01_31.md", b"- Met with Sam\n  id:: 64b1c2d3-0000-0000-0000-000000000000\n- See ((64b1c2d3-0000-0000-0000-000000000000))");
        write(dir.path(), "pages/Project%2FAlpha.md", b"status:: active\n- First block");
        let got = collect(&Tree::from_dir(dir.path()), "");
        assert_eq!(got.notes.len(), 2);
        let journal = got.notes.iter().find(|n| n.folder == vec!["Journals"]).unwrap();
        assert_eq!(journal.title, "31 Jan 2024");
        assert!(journal.notes_md.as_deref().unwrap().contains("(block reference)"));
        let page = got.notes.iter().find(|n| n.title == "Project/Alpha").unwrap();
        assert_eq!(page.properties, vec![("status".to_string(), "active".to_string())]);
    }
}
