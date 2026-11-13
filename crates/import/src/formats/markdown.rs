//! Markdown documents: YAML frontmatter, Obsidian syntax (wikilinks, embeds, callouts), and
//! images carried inline so they show in Margin's editor.

use std::sync::LazyLock;

use base64::Engine;
use margin_domain::Timestamp;
use regex::{Captures, Regex};
use yaml_rust2::{Yaml, YamlLoader};

use crate::text::{parse_date, tidy};

/// Largest image carried inline. Bigger ones are reported instead.
pub const MAX_INLINE_IMAGE: usize = 2 * 1024 * 1024;

/// What the frontmatter said, plus the body without it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct MarkdownDoc {
    pub title: Option<String>,
    pub body: String,
    pub created: Option<Timestamp>,
    pub updated: Option<Timestamp>,
    pub tags: Vec<String>,
    pub attendees: Vec<String>,
    pub properties: Vec<(String, String)>,
}

/// Split and read YAML frontmatter (`---` … `---` at the very top).
pub fn read(markdown: &str) -> MarkdownDoc {
    let text = markdown.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut doc = MarkdownDoc::default();
    let Some(rest) = text.strip_prefix("---\n") else {
        doc.body = text;
        return doc;
    };
    let Some(end) = rest.find("\n---").filter(|e| rest[*e + 4..].starts_with('\n') || rest.len() == *e + 4) else {
        doc.body = text;
        return doc;
    };
    let yaml = &rest[..end];
    doc.body = rest[end + 4..].trim_start_matches('\n').to_string();
    let Ok(docs) = YamlLoader::load_from_str(yaml) else {
        // Unreadable frontmatter stays in the note rather than being lost.
        doc.body = text;
        return doc;
    };
    let Some(Yaml::Hash(map)) = docs.into_iter().next() else { return doc };
    for (k, v) in map {
        let Some(key) = k.as_str().map(str::to_string) else { continue };
        let lower = key.to_ascii_lowercase();
        match lower.as_str() {
            "title" => doc.title = scalar(&v),
            "tags" | "tag" | "keywords" => doc.tags.extend(list(&v).into_iter().flat_map(|t| t.split([',', ' ']).map(str::to_string).collect::<Vec<_>>()).filter(|t| !t.is_empty())),
            "created" | "date" | "created_at" | "creation date" | "date created" | "createdat" | "created time" => {
                doc.created = doc.created.or_else(|| scalar(&v).and_then(|s| parse_date(&s)))
            }
            "updated" | "modified" | "updated_at" | "last modified" | "date modified" | "updatedat" | "last edited time" => {
                doc.updated = doc.updated.or_else(|| scalar(&v).and_then(|s| parse_date(&s)))
            }
            "attendees" | "participants" | "people" => doc.attendees.extend(list(&v).into_iter().map(|a| unlink(&a))),
            // Obsidian's own bookkeeping and display settings.
            "cssclasses" | "cssclass" | "publish" | "permalink" | "position" => {}
            _ => {
                let value = list(&v).into_iter().map(|a| unlink(&a)).collect::<Vec<_>>().join(", ");
                if !value.is_empty() {
                    doc.properties.push((key, value));
                }
            }
        }
    }
    doc.tags = doc.tags.iter().map(|t| t.trim().trim_start_matches('#').to_string()).filter(|t| !t.is_empty()).collect();
    doc.tags.dedup();
    doc
}

fn scalar(v: &Yaml) -> Option<String> {
    match v {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        Yaml::Array(a) => a.first().and_then(scalar),
        _ => None,
    }
}

fn list(v: &Yaml) -> Vec<String> {
    match v {
        Yaml::Array(a) => a.iter().filter_map(scalar).collect(),
        Yaml::Null | Yaml::BadValue => Vec::new(),
        other => scalar(other).into_iter().collect(),
    }
}

/// `[[Sam Lee]]` → `Sam Lee` (properties often hold links).
fn unlink(s: &str) -> String {
    let s = s.trim().trim_start_matches("[[").trim_end_matches("]]");
    s.rsplit_once('|').map_or(s, |(_, alias)| alias).to_string()
}

/// An attachment found while rewriting a note.
pub struct Embed {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

/// What rewriting found that Margin cannot carry.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Rewrite {
    pub images: u32,
    pub too_big: u32,
    pub other_files: u32,
    pub embedded_notes: u32,
    pub missing: u32,
}

impl Rewrite {
    pub fn add(&mut self, other: Rewrite) {
        self.images += other.images;
        self.too_big += other.too_big;
        self.other_files += other.other_files;
        self.embedded_notes += other.embedded_notes;
        self.missing += other.missing;
    }
}

static WIKI_EMBED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[\[([^\]|#^]+)(?:#[^\]|]*)?(?:\|([^\]]*))?\]\]").unwrap());
static MD_IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"!\[([^\]]*)\]\(<?([^)>\s]+)>?(?:\s+"[^"]*")?\)"#).unwrap());
static CALLOUT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(>\s*)\[!(\w+)\][+-]?\s*(.*)$").unwrap());
static HIGHLIGHT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"==([^=\n]+)==").unwrap());

/// Rewrite Obsidian and relative-path Markdown for Margin. Wiki links are kept. `resolve` finds an attachment by the
/// name or relative path written in the note; it returns `None` when the file is missing.
pub fn rewrite(body: &str, resolve: &mut dyn FnMut(&str) -> Option<Embed>) -> (String, Rewrite) {
    let mut found = Rewrite::default();
    // ![[image.png|300]] and ![[Other note]]
    let body = WIKI_EMBED.replace_all(body, |c: &Captures| {
        let target = c[1].trim();
        if is_image_name(target) {
            inline(target, c.get(2).map(|m| m.as_str()).filter(|a| !a.chars().all(|ch| ch.is_ascii_digit() || ch == 'x')), resolve, &mut found)
        } else if has_file_extension(target) {
            found.other_files += 1;
            format!("*{target} (attachment not imported)*")
        } else {
            found.embedded_notes += 1;
            format!("*Embedded note: {}*", c.get(2).map_or(target, |m| m.as_str()))
        }
    });
    // ![alt](relative/path.png)
    let body = MD_IMAGE.replace_all(&body, |c: &Captures| {
        let src = &c[2];
        if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("data:") {
            c[0].to_string()
        } else {
            let decoded = percent_decode(src);
            inline(&decoded, Some(&c[1]).filter(|a| !a.is_empty()), resolve, &mut found)
        }
    });
    // [[Note]] links stay as written: Margin reads the same syntax for links and the graph.
    // > [!warning] Title → > **Warning: Title**
    let body = CALLOUT.replace_all(&body, |c: &Captures| {
        let kind = capitalise(&c[2]);
        let title = c[3].trim();
        if title.is_empty() { format!("{}**{kind}**", &c[1]) } else { format!("{}**{kind}: {title}**", &c[1]) }
    });
    let body = HIGHLIGHT.replace_all(&body, "**$1**");
    (tidy(&body), found)
}

fn inline(target: &str, alt: Option<&str>, resolve: &mut dyn FnMut(&str) -> Option<Embed>, found: &mut Rewrite) -> String {
    let name = target.rsplit(['/', '\\']).next().unwrap_or(target);
    let alt = alt.unwrap_or(name);
    match resolve(target) {
        Some(embed) if embed.bytes.len() <= MAX_INLINE_IMAGE => {
            found.images += 1;
            format!("![{alt}](data:{};base64,{})", embed.mime, base64::engine::general_purpose::STANDARD.encode(&embed.bytes))
        }
        Some(_) => {
            found.too_big += 1;
            format!("*{name} (image too large to import)*")
        }
        None => {
            found.missing += 1;
            format!("*{name} (image not found)*")
        }
    }
}

pub fn is_image_name(name: &str) -> bool {
    image_mime(name).is_some()
}

pub fn image_mime(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => return None,
    })
}

fn has_file_extension(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, e)| (1..=5).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphanumeric()) && !e.eq_ignore_ascii_case("md"))
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()).unwrap_or_default()
}

/// `My%20File.png` → `My File.png`.
pub fn percent_decode(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_is_read_and_removed() {
        let doc = read("---\ntitle: Kickoff\ntags: [client, \"#acme\"]\ncreated: 2025-03-04 09:00\nattendees:\n  - \"[[Sam Lee]]\"\n  - Ana\nstatus: draft\ncssclasses: wide\n---\n# Heading\n\nBody");
        assert_eq!(doc.title.as_deref(), Some("Kickoff"));
        assert_eq!(doc.tags, vec!["client", "acme"]);
        assert!(doc.created.is_some());
        assert_eq!(doc.attendees, vec!["Sam Lee", "Ana"]);
        assert_eq!(doc.properties, vec![("status".to_string(), "draft".to_string())]);
        assert!(doc.body.starts_with("# Heading"));
    }

    #[test]
    fn no_frontmatter_leaves_the_body_alone() {
        assert_eq!(read("Just text\n---\nmore").body, "Just text\n---\nmore");
        assert_eq!(read("---\nnot: closed").body, "---\nnot: closed");
    }

    #[test]
    fn obsidian_syntax_is_rewritten() {
        let mut resolve = |name: &str| (name == "pic.png").then(|| Embed { bytes: vec![1, 2, 3], mime: "image/png" });
        let (body, found) = rewrite(
            "See [[Project X|the project]] and [[Notes]].\n\n![[pic.png]]\n![[gone.png]]\n![[deck.pdf]]\n![[Other note]]\n\n> [!warning] Careful\n> text\n\n==key==",
            &mut resolve,
        );
        assert!(body.contains("See [[Project X|the project]] and [[Notes]]."));
        assert!(body.contains("![pic.png](data:image/png;base64,AQID)"));
        assert!(body.contains("gone.png (image not found)"));
        assert!(body.contains("deck.pdf (attachment not imported)"));
        assert!(body.contains("*Embedded note: Other note*"));
        assert!(body.contains("> **Warning: Careful**"));
        assert!(body.contains("**key**"));
        assert_eq!(found, Rewrite { images: 1, too_big: 0, other_files: 1, embedded_notes: 1, missing: 1 });
    }

    #[test]
    fn relative_images_are_resolved_and_remote_ones_kept() {
        let mut resolve = |name: &str| (name == "assets/My Pic.jpg").then(|| Embed { bytes: vec![0], mime: "image/jpeg" });
        let (body, _) = rewrite("![x](assets/My%20Pic.jpg) ![y](https://example.com/a.png)", &mut resolve);
        assert!(body.contains("data:image/jpeg;base64,AA=="));
        assert!(body.contains("https://example.com/a.png"));
    }
}
