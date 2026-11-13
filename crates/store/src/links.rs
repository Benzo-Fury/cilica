//! note_links: the link index behind the graph and backlinks.
//!
//! Links use Obsidian's Markdown so imported vaults keep working: `[[Title]]`, `[[Title|text]]`,
//! `[[Title#Heading]]`, `[[Title#^block]]`, `[[folder/Title]]`, `![[Title]]` embeds,
//! `[text](Title.md)` and `#tags` (nested `#area/topic` too). Code, `%%comments%%` and links to
//! attachments are skipped.
//!
//! Cost: a save does no link work. A trigger marks the note stale (`links_indexed = 0`) and
//! [`Db::refresh_link_index`] re-reads only the stale notes, at the start of every read here.
//! A link resolves at read time, to the note with that title (case-insensitive), else to a saved
//! person with that name, else to a note not created yet.

use std::collections::{HashMap, HashSet, VecDeque};

use cilica_domain::{Author, Backlinks, GraphEdge, GraphEdgeKind, GraphNode, GraphNodeKind, Id, LinkGraph, Mention, NoteKind, NoteRef, NoteSummary};
use sqlx::Row;

use crate::notes::strip_markdown_line;
use crate::{Db, StoreError, StoreResult, codec};

/// Local graphs go at most this many links out.
pub const MAX_DEPTH: u32 = 3;
/// Most mentions returned per list.
const MAX_MENTIONS: usize = 50;
/// Excerpts are cut to about this many characters.
const EXCERPT_CHARS: usize = 180;

/// What a reference in a note points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefKind {
    Link,
    Embed,
    Tag,
}

impl RefKind {
    fn as_sql(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Embed => "embed",
            Self::Tag => "tag",
        }
    }

    fn from_sql(s: &str) -> Self {
        match s {
            "embed" => Self::Embed,
            "tag" => Self::Tag,
            _ => Self::Link,
        }
    }
}

/// One reference read from a note body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteRefLink {
    pub kind: RefKind,
    /// As written: the note title (no path, heading or alias) or the tag (no `#`).
    pub target: String,
    /// [`link_key`] of `target`.
    pub key: String,
}

/// How titles, names and tags compare: trimmed, whitespace folded, lowercased.
pub fn link_key(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

// ── Parsing ─────────────────────────────────────────────────────────────────

/// Every link, embed and tag in a note body, first occurrence of each kept.
pub fn parse_refs(markdown: &str) -> Vec<NoteRefLink> {
    let text = markdown.replace("\r\n", "\n");
    let mut out = Refs::default();
    let body = match split_frontmatter(&text) {
        Some((yaml, body)) => {
            frontmatter_refs(yaml, &mut out);
            body
        }
        None => text.as_str(),
    };
    let mut fence: Option<(char, usize)> = None;
    let mut in_comment = false;
    for line in body.lines() {
        if let Some(open) = fence_marker(line) {
            match fence {
                None => fence = Some(open),
                Some((c, n)) if open.0 == c && open.1 >= n && line.trim()[open.1..].trim().is_empty() => fence = None,
                Some(_) => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        for span in scan_line(line, &mut in_comment) {
            match span.what {
                Span::Wiki { page, embed } => out.push(if embed { RefKind::Embed } else { RefKind::Link }, page),
                Span::MdLink { page } => out.push(RefKind::Link, page),
                Span::Tag { tag } => out.push(RefKind::Tag, tag),
                Span::Protected => {}
            }
        }
    }
    out.list
}

#[derive(Default)]
struct Refs {
    list: Vec<NoteRefLink>,
    seen: HashSet<(RefKind, String)>,
}

impl Refs {
    fn push(&mut self, kind: RefKind, target: String) {
        let key = link_key(&target);
        if key.is_empty() || !self.seen.insert((kind, key.clone())) {
            return;
        }
        self.list.push(NoteRefLink { kind, target: target.split_whitespace().collect::<Vec<_>>().join(" "), key });
    }
}

/// `---\n…\n---` at the very top: (yaml, rest).
fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    let after = &rest[end + 4..];
    if !(after.is_empty() || after.starts_with('\n')) {
        return None;
    }
    Some((&rest[..end], after.strip_prefix('\n').unwrap_or(after)))
}

/// `tags:` in any of YAML's list forms, and any `[[link]]` in a property value.
fn frontmatter_refs(yaml: &str, out: &mut Refs) {
    let mut in_tags = false;
    for line in yaml.lines() {
        let trimmed = line.trim();
        let mut no_comment = false;
        for span in scan_line(line, &mut no_comment) {
            if let Span::Wiki { page, .. } = span.what {
                out.push(RefKind::Link, page);
            }
        }
        if let Some(item) = trimmed.strip_prefix("- ").filter(|_| in_tags && line.starts_with([' ', '-'])) {
            push_yaml_tags(item, out);
            continue;
        }
        in_tags = false;
        let Some((key, value)) = trimmed.split_once(':') else { continue };
        if matches!(key.trim().to_ascii_lowercase().as_str(), "tags" | "tag") {
            let value = value.trim();
            if value.is_empty() {
                in_tags = true;
            } else {
                push_yaml_tags(value.trim_start_matches('[').trim_end_matches(']'), out);
            }
        }
    }
}

fn push_yaml_tags(value: &str, out: &mut Refs) {
    for tag in value.split([',', ' ']) {
        let tag = tag.trim().trim_matches(['"', '\'']).trim_start_matches('#');
        if is_tag(tag) {
            out.push(RefKind::Tag, tag.to_string());
        }
    }
}

/// A fence opener or closer: its character and length.
fn fence_marker(line: &str) -> Option<(char, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = t.chars().take_while(|x| *x == c).count();
    (n >= 3).then_some((c, n))
}

#[derive(Debug, Clone, PartialEq)]
enum Span {
    Wiki { page: String, embed: bool },
    MdLink { page: String },
    Tag { tag: String },
    /// Code, comments, URLs and links to the web: never a link or a mention.
    Protected,
}

#[derive(Debug, Clone, PartialEq)]
struct Found {
    /// Byte range in the line.
    start: usize,
    end: usize,
    what: Span,
}

/// The links, tags and protected ranges on one line, in order. `in_comment` carries `%%` across lines.
fn scan_line(line: &str, in_comment: &mut bool) -> Vec<Found> {
    let bytes = line.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    let mut comment_start = if *in_comment { Some(0) } else { None };
    while i < bytes.len() {
        if line[i..].starts_with("%%") {
            match comment_start.take() {
                Some(start) => {
                    found.push(Found { start, end: i + 2, what: Span::Protected });
                    *in_comment = false;
                }
                None => {
                    comment_start = Some(i);
                    *in_comment = true;
                }
            }
            i += 2;
            continue;
        }
        if comment_start.is_some() {
            i += 1;
            continue;
        }
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'`' => {
                let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
                let ticks = &line[i..i + run];
                let close = line[i + run..].find(ticks).map(|p| i + run + p + run);
                let end = close.unwrap_or(i + run);
                found.push(Found { start: i, end, what: Span::Protected });
                i = end;
                continue;
            }
            b'!' if line[i..].starts_with("![[") => {
                if let Some((end, inner)) = wiki_at(line, i + 1) {
                    found.push(Found { start: i, end, what: wiki_page(inner).map_or(Span::Protected, |page| Span::Wiki { page, embed: true }) });
                    i = end;
                    continue;
                }
            }
            b'[' if line[i..].starts_with("[[") => {
                if let Some((end, inner)) = wiki_at(line, i) {
                    found.push(Found { start: i, end, what: wiki_page(inner).map_or(Span::Protected, |page| Span::Wiki { page, embed: false }) });
                    i = end;
                    continue;
                }
            }
            b'[' | b'!' => {
                let open = if bytes[i] == b'!' { i + 1 } else { i };
                if bytes.get(open) == Some(&b'[')
                    && let Some((end, dest)) = md_link_at(line, open)
                {
                    let what = md_link_page(dest).filter(|_| bytes[i] == b'[').map_or(Span::Protected, |page| Span::MdLink { page });
                    found.push(Found { start: i, end, what });
                    i = end;
                    continue;
                }
            }
            b'h' | b'w' if line[i..].starts_with("http://") || line[i..].starts_with("https://") || line[i..].starts_with("www.") => {
                let end = line[i..].find(char::is_whitespace).map_or(line.len(), |p| i + p);
                found.push(Found { start: i, end, what: Span::Protected });
                i = end;
                continue;
            }
            b'#' if i == 0 || line[..i].chars().next_back().is_some_and(char::is_whitespace) => {
                let len = line[i + 1..].find(|c: char| !is_tag_char(c)).unwrap_or(line.len() - i - 1);
                let tag = line[i + 1..i + 1 + len].trim_end_matches('/');
                if is_tag(tag) {
                    found.push(Found { start: i, end: i + 1 + tag.len(), what: Span::Tag { tag: tag.to_string() } });
                    i += 1 + tag.len();
                    continue;
                }
            }
            _ => {}
        }
        i += line[i..].chars().next().map_or(1, char::len_utf8);
    }
    if let Some(start) = comment_start {
        found.push(Found { start, end: line.len(), what: Span::Protected });
    }
    found
}

/// `[[…]]` starting at `i`: (end, inner).
fn wiki_at(line: &str, i: usize) -> Option<(usize, &str)> {
    let rest = line.get(i + 2..)?;
    let close = rest.find("]]")?;
    let inner = &rest[..close];
    if inner.is_empty() || inner.contains("[[") {
        return None;
    }
    Some((i + 2 + close + 2, inner))
}

/// The note a wiki link points at: no path, `.md`, heading, block or alias. `None` for a link
/// inside the same note (`[[#Heading]]`) or to an attachment.
fn wiki_page(inner: &str) -> Option<String> {
    let target = inner.split('|').next().unwrap_or(inner).trim_end_matches('\\');
    let page = target.split('#').next().unwrap_or(target);
    note_page(page)
}

fn note_page(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path).trim();
    let name = strip_suffix_ci(name, ".md").trim();
    if name.is_empty() || is_attachment(name) {
        return None;
    }
    Some(name.to_string())
}

/// `[text](dest)` with `[` at `i`: (end, dest).
fn md_link_at(line: &str, i: usize) -> Option<(usize, &str)> {
    let bytes = line.as_bytes();
    let mut depth = 0usize;
    let mut j = i;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    if bytes.get(j) != Some(&b']') || bytes.get(j + 1) != Some(&b'(') {
        return None;
    }
    let start = j + 2;
    let close = line[start..].find(')')? + start;
    Some((close + 1, line[start..close].trim()))
}

/// A Markdown link to another note: a relative `.md` path (`Note%20name.md`, `<Note name.md>`).
fn md_link_page(dest: &str) -> Option<String> {
    let dest = dest.trim_start_matches('<').trim_end_matches('>');
    let dest = dest.split_once(" \"").map_or(dest, |(d, _)| d).trim();
    if dest.contains("://") || dest.starts_with('#') || dest.starts_with("mailto:") || dest.starts_with("data:") {
        return None;
    }
    let path = dest.split('#').next().unwrap_or(dest);
    if !path.to_ascii_lowercase().ends_with(".md") {
        return None;
    }
    note_page(&percent_decode(path))
}

fn strip_suffix_ci<'a>(s: &'a str, suffix: &str) -> &'a str {
    if s.len() >= suffix.len() && s.is_char_boundary(s.len() - suffix.len()) && s[s.len() - suffix.len()..].eq_ignore_ascii_case(suffix) {
        &s[..s.len() - suffix.len()]
    } else {
        s
    }
}

fn is_attachment(name: &str) -> bool {
    const EXTENSIONS: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif", "heic", "pdf", "mp3", "m4a", "wav", "ogg", "flac", "webm", "mp4", "mov", "mkv",
        "canvas", "excalidraw", "base", "zip", "csv", "docx", "xlsx", "pptx", "txt",
    ];
    name.rsplit_once('.').is_some_and(|(_, ext)| EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}

/// Obsidian's rule: letters, numbers, `_`, `-` and `/`, with at least one character that is not a number.
fn is_tag(tag: &str) -> bool {
    !tag.is_empty() && !tag.starts_with('/') && tag.chars().all(is_tag_char) && tag.chars().any(|c| !c.is_numeric())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

// ── Mentions and rewriting ──────────────────────────────────────────────────

/// Byte range of the first place `title` appears in `markdown` as a whole word, outside code,
/// links, URLs, tags and comments, plus the line it is on.
fn find_unlinked(markdown: &str, title: &str) -> Option<(usize, usize, String)> {
    let needle: Vec<char> = title.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return None;
    }
    let body_start = split_frontmatter(markdown).map_or(0, |(_, body)| markdown.len() - body.len());
    let mut fence: Option<(char, usize)> = None;
    let mut in_comment = false;
    let mut offset = 0;
    for line in markdown.split_inclusive('\n') {
        let start_of_line = offset;
        offset += line.len();
        let line = line.trim_end_matches('\n');
        if start_of_line < body_start {
            continue;
        }
        if let Some(open) = fence_marker(line) {
            fence = match fence {
                None => Some(open),
                Some((c, n)) if open.0 == c && open.1 >= n => None,
                keep => keep,
            };
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let protected = scan_line(line, &mut in_comment);
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        'at: for (ci, &(at, _)) in chars.iter().enumerate() {
            if protected.iter().any(|f| at >= f.start && at < f.end) {
                continue;
            }
            if ci > 0 && chars[ci - 1].1.is_alphanumeric() {
                continue;
            }
            let mut k = ci;
            let mut lowered = Vec::with_capacity(needle.len());
            while lowered.len() < needle.len() {
                let Some(&(_, c)) = chars.get(k) else { continue 'at };
                lowered.extend(c.to_lowercase());
                k += 1;
            }
            if lowered != needle {
                continue;
            }
            let end = chars.get(k).map_or(line.len(), |&(e, _)| e);
            if chars.get(k).is_some_and(|&(_, c)| c.is_alphanumeric()) || protected.iter().any(|f| end > f.start && at < f.end) {
                continue;
            }
            return Some((start_of_line + at, start_of_line + end, line.to_string()));
        }
    }
    None
}

/// Point every wiki link to `old_key` at `new_title`, keeping headings, blocks and shown text.
/// Returns the new body when something changed.
fn rewrite_wiki_links(markdown: &str, old_key: &str, new_title: &str) -> Option<String> {
    let mut out = String::with_capacity(markdown.len());
    let mut changed = false;
    let mut fence: Option<(char, usize)> = None;
    let mut in_comment = false;
    for line in markdown.split_inclusive('\n') {
        let bare = line.trim_end_matches('\n');
        if let Some(open) = fence_marker(bare) {
            fence = match fence {
                None => Some(open),
                Some((c, n)) if open.0 == c && open.1 >= n => None,
                keep => keep,
            };
        }
        if fence.is_some() || fence_marker(bare).is_some() {
            out.push_str(line);
            continue;
        }
        let mut last = 0;
        for f in scan_line(bare, &mut in_comment) {
            let Span::Wiki { page, .. } = &f.what else { continue };
            if link_key(page) != old_key {
                continue;
            }
            let open = if bare[f.start..].starts_with('!') { f.start + 3 } else { f.start + 2 };
            let inner = &bare[open..f.end - 2];
            let (target, alias) = match inner.split_once('|') {
                Some((t, a)) => (t, Some(a)),
                None => (inner, None),
            };
            let anchor = target.find('#').map_or("", |p| &target[p..]);
            out.push_str(&bare[last..open]);
            out.push_str(new_title);
            out.push_str(anchor);
            if let Some(alias) = alias {
                out.push('|');
                out.push_str(alias);
            }
            out.push_str("]]");
            last = f.end;
            changed = true;
        }
        out.push_str(&line[last..]);
    }
    changed.then_some(out)
}

fn excerpt(line: &str) -> String {
    let plain = strip_markdown_line(line);
    if plain.chars().count() <= EXCERPT_CHARS {
        return plain;
    }
    let cut: String = plain.chars().take(EXCERPT_CHARS - 1).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end())
}

/// The first line of `markdown` with a link or embed to `key`.
fn linking_line(markdown: &str, key: &str) -> Option<String> {
    let mut in_comment = false;
    markdown.lines().find_map(|line| {
        scan_line(line, &mut in_comment)
            .iter()
            .any(|f| matches!(&f.what, Span::Wiki { page, .. } | Span::MdLink { page } if link_key(page) == key))
            .then(|| line.to_string())
    })
}

// ── Database ────────────────────────────────────────────────────────────────

struct GraphNote {
    id: Id,
    title: String,
    key: String,
    kind: NoteKind,
    category_id: Option<Id>,
    created_at: cilica_domain::Timestamp,
}

impl Db {
    /// Re-read the links of notes whose body changed since they were last read. Returns how many
    /// notes were read. Cheap when nothing changed (one indexed lookup).
    pub async fn refresh_link_index(&self) -> StoreResult<u32> {
        let stale = sqlx::query("SELECT id, body_md FROM notes WHERE links_indexed = 0").fetch_all(self.pool()).await?;
        if stale.is_empty() {
            return Ok(0);
        }
        let mut tx = self.begin_write().await?;
        for row in &stale {
            let id: String = row.try_get("id")?;
            let body: String = row.try_get("body_md")?;
            sqlx::query("DELETE FROM note_links WHERE note_id = ?").bind(&id).execute(&mut *tx).await?;
            for r in parse_refs(&body) {
                sqlx::query("INSERT INTO note_links (note_id, kind, target_key, target) VALUES (?, ?, ?, ?)")
                    .bind(&id)
                    .bind(r.kind.as_sql())
                    .bind(&r.key)
                    .bind(&r.target)
                    .execute(&mut *tx)
                    .await?;
            }
            // Only if the body is still the one read (a save in between leaves it stale).
            sqlx::query("UPDATE notes SET links_indexed = 1 WHERE id = ? AND body_md = ?").bind(&id).bind(&body).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(stale.len() as u32)
    }

    async fn graph_notes(&self) -> StoreResult<Vec<GraphNote>> {
        let rows = sqlx::query("SELECT id, title, kind, category_id, created_at FROM notes WHERE deleted_at IS NULL ORDER BY updated_at DESC").fetch_all(self.pool()).await?;
        rows.iter()
            .map(|r| {
                let title: String = r.try_get("title")?;
                let kind: String = r.try_get("kind")?;
                Ok(GraphNote {
                    id: codec::parse_id(&r.try_get::<String, _>("id")?)?,
                    key: link_key(&title),
                    title,
                    kind: if kind == "meeting" { NoteKind::Meeting } else { NoteKind::Manual },
                    category_id: codec::parse_opt_id(r.try_get("category_id")?)?,
                    created_at: codec::parse_ts(&r.try_get::<String, _>("created_at")?)?,
                })
            })
            .collect()
    }

    /// Every note, the people and tags they reach, and links to notes not created yet. With
    /// `focus`, only what is within `depth` links of that note (1 to [`MAX_DEPTH`]); tags are
    /// shown but not walked through, so one tag does not pull in every note that has it.
    pub async fn link_graph(&self, focus: Option<Id>, depth: u32) -> StoreResult<LinkGraph> {
        self.refresh_link_index().await?;
        let notes = self.graph_notes().await?;
        let refs = sqlx::query("SELECT l.note_id, l.kind, l.target_key, l.target FROM note_links l JOIN notes n ON n.id = l.note_id WHERE n.deleted_at IS NULL")
            .fetch_all(self.pool())
            .await?;
        let people = sqlx::query("SELECT id, name, color_index FROM people").fetch_all(self.pool()).await?;
        let heard = sqlx::query(
            "SELECT DISTINCT s.note_id, s.person_id FROM speakers s JOIN notes n ON n.id = s.note_id \
             WHERE s.person_id IS NOT NULL AND s.is_self = 0 AND n.deleted_at IS NULL",
        )
        .fetch_all(self.pool())
        .await?;

        let mut b = GraphBuilder::default();
        // Notes come newest first, so a title used twice resolves to the newest note.
        let mut by_title: HashMap<&str, usize> = HashMap::new();
        let mut by_note: HashMap<Id, usize> = HashMap::new();
        for n in &notes {
            let i = b.node(format!("n:{}", n.id), || GraphNode {
                key: String::new(),
                kind: GraphNodeKind::Note,
                label: n.title.clone(),
                note_id: Some(n.id),
                note_kind: Some(n.kind),
                category_id: n.category_id,
                person_id: None,
                color_index: None,
                degree: 0,
                created_at: Some(n.created_at),
            });
            by_note.insert(n.id, i);
            if !n.key.is_empty() {
                by_title.entry(n.key.as_str()).or_insert(i);
            }
        }
        let mut by_name: HashMap<String, (Id, String, u32)> = HashMap::new();
        for p in &people {
            let id = codec::parse_id(&p.try_get::<String, _>("id")?)?;
            let name: String = p.try_get("name")?;
            let color: i64 = p.try_get("color_index")?;
            by_name.entry(link_key(&name)).or_insert((id, name, u32::try_from(color).unwrap_or(1)));
        }
        let person_node = |b: &mut GraphBuilder, id: Id, name: &str, color: u32| {
            b.node(format!("p:{id}"), || GraphNode {
                key: String::new(),
                kind: GraphNodeKind::Person,
                label: name.to_string(),
                note_id: None,
                note_kind: None,
                category_id: None,
                person_id: Some(id),
                color_index: Some(color),
                degree: 0,
                created_at: None,
            })
        };

        for r in &refs {
            let Some(&from) = codec::parse_id(&r.try_get::<String, _>("note_id")?).ok().and_then(|id| by_note.get(&id)) else { continue };
            let kind = RefKind::from_sql(&r.try_get::<String, _>("kind")?);
            let key: String = r.try_get("target_key")?;
            let target: String = r.try_get("target")?;
            let (to, edge) = match kind {
                RefKind::Tag => (
                    b.node(format!("t:{key}"), || plain_node(GraphNodeKind::Tag, format!("#{target}"))),
                    GraphEdgeKind::Tag,
                ),
                RefKind::Link | RefKind::Embed => {
                    let edge = if kind == RefKind::Embed { GraphEdgeKind::Embed } else { GraphEdgeKind::Link };
                    if let Some(&to) = by_title.get(key.as_str()) {
                        (to, edge)
                    } else if let Some((id, name, color)) = by_name.get(&key) {
                        (person_node(&mut b, *id, name, *color), edge)
                    } else {
                        (b.node(format!("m:{key}"), || plain_node(GraphNodeKind::Missing, target.clone())), edge)
                    }
                }
            };
            b.edge(from, to, edge);
        }
        let names: HashMap<Id, (String, u32)> = by_name.values().map(|(id, name, color)| (*id, (name.clone(), *color))).collect();
        for h in &heard {
            let Some(&from) = codec::parse_id(&h.try_get::<String, _>("note_id")?).ok().and_then(|id| by_note.get(&id)) else { continue };
            let person = codec::parse_id(&h.try_get::<String, _>("person_id")?)?;
            let Some((name, color)) = names.get(&person) else { continue };
            let to = person_node(&mut b, person, name, *color);
            b.edge(from, to, GraphEdgeKind::Transcript);
        }

        let mut graph = b.finish();
        if let Some(focus) = focus {
            let start = graph.nodes.iter().position(|n| n.note_id == Some(focus)).ok_or_else(|| StoreError::NotFound("That note".into()))?;
            graph = neighbourhood(graph, start, depth.clamp(1, MAX_DEPTH));
        }
        Ok(graph)
    }

    /// Notes that link to this one, and notes that name its title without a link.
    pub async fn backlinks(&self, id: Id) -> StoreResult<Backlinks> {
        self.refresh_link_index().await?;
        let note = self.get_note_summary(id).await?;
        let key = link_key(&note.title);
        if key.is_empty() {
            return Ok(Backlinks::default());
        }
        // Linked: through the index, then one body read per linking note for its excerpt.
        let rows = sqlx::query(
            "SELECT DISTINCT n.id, n.title, n.body_md FROM note_links l JOIN notes n ON n.id = l.note_id \
             WHERE l.target_key = ? AND l.kind IN ('link','embed') AND n.deleted_at IS NULL AND n.id != ? \
             ORDER BY n.updated_at DESC LIMIT ?",
        )
        .bind(&key)
        .bind(codec::id(id))
        .bind(MAX_MENTIONS as i64)
        .fetch_all(self.pool())
        .await?;
        // A title used twice links to the newest note only (notes come newest first).
        let owner = self.graph_notes().await?.into_iter().find(|n| n.key == key).map(|n| n.id);
        let mut linked = Vec::new();
        let mut seen = HashSet::from([id]);
        if owner == Some(id) {
            for row in &rows {
                let source = codec::parse_id(&row.try_get::<String, _>("id")?)?;
                let body: String = row.try_get("body_md")?;
                seen.insert(source);
                linked.push(Mention {
                    note: NoteRef { id: source, title: row.try_get("title")? },
                    excerpt: linking_line(&body, &key).map(|l| excerpt(&l)).unwrap_or_default(),
                });
            }
        }

        // Unlinked: full-text search narrows it down, then each candidate is checked properly.
        let words: Vec<&str> = key.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
        let mut unlinked = Vec::new();
        if !words.is_empty() && key.chars().count() >= 3 {
            let phrase = format!("{{body_md}} : \"{}\"", words.join(" "));
            let candidates = sqlx::query(
                "SELECT n.id, n.title, n.body_md FROM notes_fts JOIN notes n ON n.id = notes_fts.note_id \
                 WHERE notes_fts MATCH ? AND n.deleted_at IS NULL ORDER BY n.updated_at DESC LIMIT 200",
            )
            .bind(&phrase)
            .fetch_all(self.pool())
            .await?;
            for row in &candidates {
                let source = codec::parse_id(&row.try_get::<String, _>("id")?)?;
                if seen.contains(&source) {
                    continue;
                }
                let body: String = row.try_get("body_md")?;
                if let Some((_, _, line)) = find_unlinked(&body, &note.title) {
                    unlinked.push(Mention { note: NoteRef { id: source, title: row.try_get("title")? }, excerpt: excerpt(&line) });
                    if unlinked.len() >= MAX_MENTIONS {
                        break;
                    }
                }
            }
        }
        Ok(Backlinks { linked, unlinked })
    }

    /// Turn the first unlinked mention of `target`'s title in `source` into a link.
    pub async fn link_mention(&self, source: Id, target: Id) -> StoreResult<NoteSummary> {
        let title = self.get_note_summary(target).await?.title;
        let note = self.get_note(source).await?;
        let (start, end, _) = find_unlinked(&note.body_md, &title).ok_or_else(|| StoreError::Invalid("That mention is no longer in the note.".into()))?;
        let written = &note.body_md[start..end];
        let link = if written == title { format!("[[{title}]]") } else { format!("[[{title}|{written}]]") };
        let body = format!("{}{link}{}", &note.body_md[..start], &note.body_md[end..]);
        self.set_note_body(source, &body, Author::User, None).await
    }

    /// After a note is renamed, point links to its old title at the new one, as Obsidian does.
    /// Skipped when another note still has the old title. Returns the notes rewritten.
    pub async fn relink_after_rename(&self, renamed: Id, old_title: &str, new_title: &str) -> StoreResult<Vec<Id>> {
        let old_key = link_key(old_title);
        if old_key.is_empty() || old_key == link_key(new_title) || new_title.trim().is_empty() {
            return Ok(Vec::new());
        }
        if self.graph_notes().await?.iter().any(|n| n.id != renamed && n.key == old_key) {
            return Ok(Vec::new());
        }
        self.refresh_link_index().await?;
        let rows = sqlx::query(
            "SELECT DISTINCT n.id, n.body_md FROM note_links l JOIN notes n ON n.id = l.note_id \
             WHERE l.target_key = ? AND l.kind IN ('link','embed') AND n.deleted_at IS NULL",
        )
        .bind(&old_key)
        .fetch_all(self.pool())
        .await?;
        let mut changed = Vec::new();
        for row in &rows {
            let id = codec::parse_id(&row.try_get::<String, _>("id")?)?;
            let body: String = row.try_get("body_md")?;
            if let Some(next) = rewrite_wiki_links(&body, &old_key, new_title.trim()) {
                self.set_note_body(id, &next, Author::User, None).await?;
                changed.push(id);
            }
        }
        Ok(changed)
    }
}

fn plain_node(kind: GraphNodeKind, label: String) -> GraphNode {
    GraphNode { key: String::new(), kind, label, note_id: None, note_kind: None, category_id: None, person_id: None, color_index: None, degree: 0, created_at: None }
}

#[derive(Default)]
struct GraphBuilder {
    nodes: Vec<GraphNode>,
    index: HashMap<String, usize>,
    edges: HashMap<(usize, usize), GraphEdgeKind>,
}

impl GraphBuilder {
    fn node(&mut self, key: String, make: impl FnOnce() -> GraphNode) -> usize {
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        let mut node = make();
        node.key = key.clone();
        self.nodes.push(node);
        self.index.insert(key, self.nodes.len() - 1);
        self.nodes.len() - 1
    }

    /// One edge per pair; a written link wins over one read from a transcript.
    fn edge(&mut self, a: usize, b: usize, kind: GraphEdgeKind) {
        if a == b {
            return;
        }
        let pair = (a.min(b), a.max(b));
        match self.edges.get(&pair) {
            Some(GraphEdgeKind::Transcript) | None => {
                self.edges.insert(pair, kind);
            }
            Some(_) => {}
        }
    }

    fn finish(mut self) -> LinkGraph {
        let mut edges: Vec<GraphEdge> =
            self.edges.into_iter().map(|((a, b), kind)| GraphEdge { source: a as u32, target: b as u32, kind }).collect();
        edges.sort_by_key(|e| (e.source, e.target));
        for e in &edges {
            self.nodes[e.source as usize].degree += 1;
            self.nodes[e.target as usize].degree += 1;
        }
        LinkGraph { nodes: self.nodes, edges, focus: None }
    }
}

/// The part of `graph` within `depth` edges of `start`, walking through notes, people and links
/// to notes not created yet, but not through tags.
fn neighbourhood(graph: LinkGraph, start: usize, depth: u32) -> LinkGraph {
    let mut adjacent: Vec<Vec<usize>> = vec![Vec::new(); graph.nodes.len()];
    for e in &graph.edges {
        adjacent[e.source as usize].push(e.target as usize);
        adjacent[e.target as usize].push(e.source as usize);
    }
    let mut level: HashMap<usize, u32> = HashMap::from([(start, 0)]);
    let mut queue = VecDeque::from([start]);
    while let Some(at) = queue.pop_front() {
        let d = level[&at];
        if d >= depth || (at != start && graph.nodes[at].kind == GraphNodeKind::Tag) {
            continue;
        }
        for &next in &adjacent[at] {
            if let std::collections::hash_map::Entry::Vacant(slot) = level.entry(next) {
                slot.insert(d + 1);
                queue.push_back(next);
            }
        }
    }
    let mut keep: Vec<usize> = level.keys().copied().collect();
    keep.sort_unstable();
    let remap: HashMap<usize, u32> = keep.iter().enumerate().map(|(new, &old)| (old, new as u32)).collect();
    let edges = graph
        .edges
        .into_iter()
        .filter_map(|e| Some(GraphEdge { source: *remap.get(&(e.source as usize))?, target: *remap.get(&(e.target as usize))?, kind: e.kind }))
        .collect();
    let focus = remap.get(&start).copied();
    let mut nodes = graph.nodes;
    let nodes = keep.iter().map(|&i| std::mem::replace(&mut nodes[i], plain_node(GraphNodeKind::Tag, String::new()))).collect();
    LinkGraph { nodes, edges, focus }
}

#[cfg(test)]
mod tests {
    use cilica_domain::NoteKind;

    use super::*;
    use crate::NewNote;

    fn refs(md: &str) -> Vec<(RefKind, String)> {
        parse_refs(md).into_iter().map(|r| (r.kind, r.target)).collect()
    }

    fn link(t: &str) -> (RefKind, String) {
        (RefKind::Link, t.to_string())
    }

    #[test]
    fn reads_obsidian_links() {
        assert_eq!(
            refs("See [[Q3 planning]], [[Pricing#Tiers|the tiers]], [[folder/Deep note.md]] and [[Spec#^abc123]]."),
            vec![link("Q3 planning"), link("Pricing"), link("Deep note"), link("Spec")]
        );
        assert_eq!(refs("![[Onboarding review]] ![[diagram.png]] ![[deck.pdf]]"), vec![(RefKind::Embed, "Onboarding review".into())]);
        assert_eq!(refs("[the plan](Q3%20planning.md) [site](https://example.com/a.md) [x](<Other note.md#Top>)"), vec![link("Q3 planning"), link("Other note")]);
        assert_eq!(refs("[[#Heading in this note]] [[]]"), vec![]);
        assert_eq!(refs("| a | [[Table link\\|shown]] |"), vec![link("Table link")]);
    }

    #[test]
    fn reads_tags_like_obsidian() {
        assert_eq!(
            refs("#pricing at the start, mid #area/sub-topic, #2024 is not, C# is not, url https://a.com/#frag is not, #tag/ trims"),
            vec![(RefKind::Tag, "pricing".into()), (RefKind::Tag, "area/sub-topic".into()), (RefKind::Tag, "tag".into())]
        );
        assert_eq!(refs("# Heading\n## Another"), vec![]);
    }

    #[test]
    fn skips_code_and_comments() {
        let md = "```\n[[In code]] #code\n```\nInline `[[code]]` and ``[[two]]``.\n%% [[hidden]] %% shown [[Real]]\n%%\n[[Hidden too]]\n%%\n~~~~\n[[x]]\n~~~\nstill fenced [[y]]\n~~~~\n[[After]]";
        assert_eq!(refs(md), vec![link("Real"), link("After")]);
    }

    #[test]
    fn reads_frontmatter() {
        let md = "---\ntags: [client, \"#acme\"]\nattendees:\n  - \"[[Sam Lee]]\"\naliases:\n  - Kick\n---\nBody #inline\n";
        assert_eq!(
            refs(md),
            vec![(RefKind::Tag, "client".into()), (RefKind::Tag, "acme".into()), link("Sam Lee"), (RefKind::Tag, "inline".into())]
        );
        let list = "---\ntags:\n  - one\n  - two\ntitle: x\n---\n";
        assert_eq!(refs(list), vec![(RefKind::Tag, "one".into()), (RefKind::Tag, "two".into())]);
    }

    #[test]
    fn dedupes_case_insensitively() {
        assert_eq!(refs("[[Plan]] [[plan]] [[PLAN|x]] #Tag #tag"), vec![link("Plan"), (RefKind::Tag, "Tag".into())]);
    }

    #[test]
    fn finds_unlinked_mentions_outside_links_and_code() {
        let md = "The [[Pricing]] page.\n`pricing` in code, prepricing, https://pricing.com\nWe talked pricing today.";
        let (s, e, line) = find_unlinked(md, "Pricing").unwrap();
        assert_eq!(&md[s..e], "pricing");
        assert_eq!(line, "We talked pricing today.");
        assert!(find_unlinked("Only [[Pricing|pricing]] here", "Pricing").is_none());
        assert!(find_unlinked("Café menu", "café").is_some());
    }

    #[test]
    fn rewrites_links_on_rename() {
        let md = "[[Plan]] and [[plan#Goals|the goals]] and ![[Plan]]\n```\n[[Plan]]\n```\n[[Planning]]";
        let out = rewrite_wiki_links(md, "plan", "Q3 plan").unwrap();
        assert_eq!(out, "[[Q3 plan]] and [[Q3 plan#Goals|the goals]] and ![[Q3 plan]]\n```\n[[Plan]]\n```\n[[Planning]]");
        assert!(rewrite_wiki_links("[[Other]]", "plan", "x").is_none());
    }

    async fn db() -> Db {
        Db::open_in_memory().await.unwrap()
    }

    async fn note(db: &Db, title: &str, body: &str) -> Id {
        db.create_note(NewNote { title: title.into(), body_md: body.into(), kind: NoteKind::Manual, ..Default::default() }).await.unwrap().id
    }

    fn labels(g: &LinkGraph) -> Vec<String> {
        let mut l: Vec<String> = g.nodes.iter().map(|n| n.label.clone()).collect();
        l.sort();
        l
    }

    #[tokio::test]
    async fn graph_resolves_titles_people_tags_and_missing() {
        let db = db().await;
        let plan = note(&db, "Q3 planning", "Links to [[Pricing]] and [[Nowhere yet]] #roadmap").await;
        let pricing = note(&db, "Pricing", "Back to [[q3 planning]] and [[Sam Ortiz]]").await;
        db.create_person("Sam Ortiz", None).await.unwrap();
        let g = db.link_graph(None, 1).await.unwrap();
        assert_eq!(labels(&g), vec!["#roadmap", "Nowhere yet", "Pricing", "Q3 planning", "Sam Ortiz"]);
        // Both directions of Q3 planning ↔ Pricing make one edge.
        assert_eq!(g.edges.len(), 4);
        let node = |label: &str| g.nodes.iter().find(|n| n.label == label).unwrap();
        assert_eq!(node("Q3 planning").degree, 3);
        assert_eq!(node("Q3 planning").note_id, Some(plan));
        assert_eq!(node("Pricing").note_id, Some(pricing));
        assert_eq!(node("Sam Ortiz").kind, GraphNodeKind::Person);
        assert_eq!(node("Nowhere yet").kind, GraphNodeKind::Missing);
        assert_eq!(node("#roadmap").kind, GraphNodeKind::Tag);
    }

    #[tokio::test]
    async fn index_follows_edits_trash_and_deletes() {
        let db = db().await;
        let a = note(&db, "A", "[[B]]").await;
        let b = note(&db, "B", "").await;
        assert_eq!(db.link_graph(None, 1).await.unwrap().edges.len(), 1);
        assert_eq!(db.refresh_link_index().await.unwrap(), 0, "nothing stale after a read");
        db.set_note_body(a, "no links now", Author::User, None).await.unwrap();
        assert_eq!(db.refresh_link_index().await.unwrap(), 1, "an edit marks only that note stale");
        assert!(db.link_graph(None, 1).await.unwrap().edges.is_empty());
        db.set_note_body(a, "[[B]] again", Author::User, None).await.unwrap();
        db.trash_note(b).await.unwrap();
        let g = db.link_graph(None, 1).await.unwrap();
        assert_eq!(labels(&g), vec!["A", "B"], "a link to a note in Trash shows as not created yet");
        assert_eq!(g.nodes.iter().find(|n| n.label == "B").unwrap().kind, GraphNodeKind::Missing);
        db.delete_note_forever(a).await.unwrap();
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM note_links").fetch_one(db.pool()).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn local_graph_stops_at_depth_and_at_tags() {
        let db = db().await;
        let a = note(&db, "A", "[[B]] #shared").await;
        note(&db, "B", "[[C]]").await;
        note(&db, "C", "[[D]]").await;
        note(&db, "E", "#shared").await;
        assert_eq!(labels(&db.link_graph(Some(a), 1).await.unwrap()), vec!["#shared", "A", "B"]);
        let two = db.link_graph(Some(a), 2).await.unwrap();
        assert_eq!(labels(&two), vec!["#shared", "A", "B", "C"], "E is only reachable through the tag");
        assert_eq!(two.nodes[two.focus.unwrap() as usize].label, "A");
        for e in &two.edges {
            assert!((e.source as usize) < two.nodes.len() && (e.target as usize) < two.nodes.len());
        }
    }

    #[tokio::test]
    async fn backlinks_and_linking_a_mention() {
        let db = db().await;
        let pricing = note(&db, "Pricing", "").await;
        let linked = note(&db, "Sync", "Agreed on [[Pricing|the pricing]] today.").await;
        let mention = note(&db, "Interview", "Marco asked about pricing again.\n\nMore pricing later.").await;
        note(&db, "Unrelated", "Nothing here").await;
        let back = db.backlinks(pricing).await.unwrap();
        assert_eq!(back.linked.len(), 1);
        assert_eq!(back.linked[0].note.id, linked);
        assert_eq!(back.linked[0].excerpt, "Agreed on the pricing today.");
        assert_eq!(back.unlinked.len(), 1);
        assert_eq!(back.unlinked[0].note.id, mention);

        db.link_mention(mention, pricing).await.unwrap();
        let body = db.get_note(mention).await.unwrap().body_md;
        assert_eq!(body, "Marco asked about [[Pricing|pricing]] again.\n\nMore pricing later.");
        let back = db.backlinks(pricing).await.unwrap();
        assert_eq!(back.linked.len(), 2);
        assert!(back.unlinked.is_empty());
    }

    #[tokio::test]
    async fn rename_relinks_other_notes() {
        let db = db().await;
        let plan = note(&db, "Plan", "").await;
        let other = note(&db, "Other", "See [[Plan#Goals|goals]] and [[plan]].").await;
        db.rename_note(plan, "Q3 plan").await.unwrap();
        assert_eq!(db.relink_after_rename(plan, "Plan", "Q3 plan").await.unwrap(), vec![other]);
        assert_eq!(db.get_note(other).await.unwrap().body_md, "See [[Q3 plan#Goals|goals]] and [[Q3 plan]].");
        // Another note still called "Plan" keeps the links.
        note(&db, "Plan", "").await;
        let third = note(&db, "Third", "[[Plan]]").await;
        db.rename_note(third, "Third").await.unwrap();
        assert!(db.relink_after_rename(plan, "Plan", "Q3 plan").await.unwrap().is_empty());
    }
    /// `cargo test -p cilica-store --release links::tests::scale -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "timing, run by hand"]
    async fn scale() {
        let db = db().await;
        let n = 5_000;
        for i in 0..n {
            let body = format!("Meeting {i} about [[Note {}]], [[Note {}]] and [[Note {}]] #topic{} with more prose to read past.\n\n- point one\n- point two", (i * 7) % n, (i * 13) % n, i + n, i % 40);
            note(&db, &format!("Note {i}"), &body).await;
        }
        let t = std::time::Instant::now();
        let first = db.refresh_link_index().await.unwrap();
        println!("index {first} notes: {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let g = db.link_graph(None, 1).await.unwrap();
        println!("global graph {} nodes {} edges: {:?}", g.nodes.len(), g.edges.len(), t.elapsed());
        let t = std::time::Instant::now();
        let id = g.nodes[0].note_id.unwrap();
        let local = db.link_graph(Some(id), 2).await.unwrap();
        println!("local graph {} nodes: {:?}", local.nodes.len(), t.elapsed());
        let t = std::time::Instant::now();
        db.backlinks(id).await.unwrap();
        println!("backlinks: {:?}", t.elapsed());
        db.set_note_body(id, "[[Note 1]]", Author::User, None).await.unwrap();
        let t = std::time::Instant::now();
        assert_eq!(db.refresh_link_index().await.unwrap(), 1);
        println!("refresh after one edit: {:?}", t.elapsed());
    }
}
