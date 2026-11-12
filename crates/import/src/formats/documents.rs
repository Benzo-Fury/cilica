//! Rich documents: HTML, Word (.docx), Evernote (.enex) and ProseMirror JSON (Anarlog, TipTap).

use std::collections::HashMap;
use std::io::Read;
use std::sync::LazyLock;

use base64::Engine;
use md5::{Digest, Md5};
use regex::{Captures, Regex};
use serde_json::Value;

use super::markdown::{MAX_INLINE_IMAGE, image_mime};
use super::xml::{self, Node};
use crate::model::ImportedNote;
use crate::text::{parse_date, tidy};

// ── HTML ───────────────────────────────────────────────────────────────────

/// HTML to Markdown. Scripts and styles are dropped.
pub fn html_to_markdown(html: &str) -> String {
    static STRIP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<(script|style|head)\b.*?</(script|style|head)>").unwrap());
    let cleaned = STRIP.replace_all(html, "");
    tidy(&htmd::convert(&cleaned).unwrap_or_else(|_| cleaned.to_string()))
}

// ── Word ───────────────────────────────────────────────────────────────────

/// A .docx as Markdown: headings, list items and paragraphs. Formatting inside a line is dropped.
pub fn docx_to_markdown(bytes: &[u8]) -> Option<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut xml_text = String::new();
    zip.by_name("word/document.xml").ok()?.read_to_string(&mut xml_text).ok()?;
    let root = xml::parse(&xml_text);
    let body = root.child("document")?.child("body")?;
    let mut lines = Vec::new();
    walk_docx(body, &mut lines);
    Some(tidy(&lines.join("\n\n")))
}

fn walk_docx(node: &Node, lines: &mut Vec<String>) {
    for el in node.elements() {
        match el.name.as_str() {
            "p" => {
                let text = docx_paragraph_text(el);
                if text.trim().is_empty() {
                    continue;
                }
                let props = el.child("pPr");
                let style = props.and_then(|p| p.child("pStyle")).and_then(|s| s.attr("val")).unwrap_or("").to_ascii_lowercase();
                let prefix = if style == "title" {
                    "# ".to_string()
                } else if let Some(level) = style.strip_prefix("heading").and_then(|l| l.trim().parse::<usize>().ok()) {
                    format!("{} ", "#".repeat((level + 1).clamp(2, 6)))
                } else if props.and_then(|p| p.child("numPr")).is_some() || style.contains("list") {
                    "- ".to_string()
                } else {
                    String::new()
                };
                lines.push(format!("{prefix}{}", text.trim()));
            }
            "tbl" => {
                for row in el.children_named("tr") {
                    let cells: Vec<String> = row.children_named("tc").map(|c| c.elements().map(docx_paragraph_text).collect::<Vec<_>>().join(" ").trim().to_string()).collect();
                    if cells.iter().any(|c| !c.is_empty()) {
                        lines.push(cells.join(" | "));
                    }
                }
            }
            _ => walk_docx(el, lines),
        }
    }
}

fn docx_paragraph_text(p: &Node) -> String {
    let mut out = String::new();
    fn go(n: &Node, out: &mut String) {
        for el in n.elements() {
            match el.name.as_str() {
                "t" => out.push_str(&el.text()),
                "tab" => out.push(' '),
                "br" | "cr" => out.push('\n'),
                "pPr" | "rPr" | "instrText" | "delText" => {}
                _ => go(el, out),
            }
        }
    }
    go(p, &mut out);
    out
}

// ── Evernote ───────────────────────────────────────────────────────────────

/// What an ENEX export held that Margin could not carry.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EnexSkipped {
    pub files: u32,
    pub encrypted: u32,
    pub too_big: u32,
}

/// Every note in an ENEX export. `notebook` (the file's name) becomes the folder.
pub fn read_enex(xml_text: &str, notebook: &str) -> (Vec<ImportedNote>, EnexSkipped) {
    let root = xml::parse(xml_text);
    let mut skipped = EnexSkipped::default();
    let Some(export) = root.child("en-export") else { return (Vec::new(), skipped) };
    let mut notes = Vec::new();
    for (index, note) in export.children_named("note").enumerate() {
        let title = note.child_text("title").unwrap_or_else(|| "Untitled".into());
        let created = note.child_text("created").and_then(|d| parse_date(&d));
        let updated = note.child_text("updated").and_then(|d| parse_date(&d));
        // Attachments by the MD5 of their bytes, as <en-media hash="…"> refers to them.
        let mut media: HashMap<String, (Vec<u8>, String, Option<String>)> = HashMap::new();
        for res in note.children_named("resource") {
            let Some(data) = res.child("data").map(|d| d.text()) else { continue };
            let compact: String = data.chars().filter(|c| !c.is_whitespace()).collect();
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(compact) else { continue };
            let hash: String = Md5::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
            let mime = res.child_text("mime").unwrap_or_default();
            let name = res.child("resource-attributes").and_then(|a| a.child_text("file-name"));
            media.insert(hash, (bytes, mime, name));
        }
        let content = note.child("content").map(|c| c.text()).unwrap_or_default();
        let mut images = 0u32;
        let html = enml_to_html(&content, &media, &mut skipped, &mut images);
        let tags = note.children_named("tag").map(|t| t.text().trim().to_string()).filter(|t| !t.is_empty()).collect();
        let source_url = note.child("note-attributes").and_then(|a| a.child_text("source-url"));
        notes.push(ImportedNote {
            external_id: format!("{notebook}/{index}/{title}"),
            title,
            created_at: created,
            updated_at: updated,
            notes_md: Some(html_to_markdown(&html).replace("MARGINTODODONE ", "- [x] ").replace("MARGINTODOOPEN ", "- [ ] ")),
            folder: vec![notebook.to_string()],
            tags,
            properties: source_url.map(|u| vec![("Source".to_string(), u)]).unwrap_or_default(),
            attachments: images,
            ..Default::default()
        });
    }
    (notes, skipped)
}

fn enml_to_html(enml: &str, media: &HashMap<String, (Vec<u8>, String, Option<String>)>, skipped: &mut EnexSkipped, images: &mut u32) -> String {
    static MEDIA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?s)<en-media\b([^>]*?)/?>(?:\s*</en-media>)?"#).unwrap());
    static HASH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"hash="([0-9a-fA-F]+)""#).unwrap());
    static TODO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"<en-todo\b([^>]*?)/?>(?:\s*</en-todo>)?"#).unwrap());
    static CRYPT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<en-crypt\b.*?</en-crypt>").unwrap());
    let html = MEDIA.replace_all(enml, |c: &Captures| {
        let Some(hash) = HASH.captures(&c[1]).map(|h| h[1].to_ascii_lowercase()) else { return String::new() };
        match media.get(&hash) {
            Some((bytes, mime, name)) if mime.starts_with("image/") => {
                if bytes.len() > MAX_INLINE_IMAGE {
                    skipped.too_big += 1;
                    return format!("<p><em>{} (image too large to import)</em></p>", name.as_deref().unwrap_or("Image"));
                }
                *images += 1;
                let mime = name.as_deref().and_then(image_mime).unwrap_or(mime.as_str());
                format!(r#"<img alt="{}" src="data:{mime};base64,{}">"#, name.as_deref().unwrap_or(""), base64::engine::general_purpose::STANDARD.encode(bytes))
            }
            Some((_, _, name)) => {
                skipped.files += 1;
                format!("<p><em>{} (attachment not imported)</em></p>", name.as_deref().unwrap_or("Attachment"))
            }
            None => String::new(),
        }
    });
    // Placeholders survive the HTML conversion (which would escape "[x]"); see `read_enex`.
    let html = TODO.replace_all(&html, |c: &Captures| if c[1].contains("checked=\"true\"") { "MARGINTODODONE " } else { "MARGINTODOOPEN " }.to_string());
    let html = CRYPT.replace_all(&html, |_: &Captures| {
        skipped.encrypted += 1;
        "<p><em>Encrypted text not imported</em></p>".to_string()
    });
    html.replace("<en-note", "<div").replace("</en-note>", "</div>")
}

// ── ProseMirror / TipTap JSON ────────────────────────────────────────────

/// A ProseMirror document (TipTap's JSON) as Markdown.
pub fn prosemirror_to_markdown(doc: &Value) -> String {
    let mut out = String::new();
    block_children(doc, &mut out, 0);
    tidy(&out)
}

fn block_children(node: &Value, out: &mut String, depth: usize) {
    if let Some(children) = node.get("content").and_then(Value::as_array) {
        for child in children {
            block(child, out, depth);
        }
    }
}

fn block(node: &Value, out: &mut String, depth: usize) {
    let kind = node.get("type").and_then(Value::as_str).unwrap_or("");
    let indent = "  ".repeat(depth);
    match kind {
        "heading" => {
            let level = node.pointer("/attrs/level").and_then(Value::as_u64).unwrap_or(2).clamp(1, 6) as usize;
            out.push_str(&format!("{} {}\n\n", "#".repeat(level), inline(node)));
        }
        "paragraph" => out.push_str(&format!("{indent}{}\n\n", inline(node))),
        "bulletList" | "orderedList" | "taskList" => {
            let ordered = kind == "orderedList";
            for (i, item) in node.get("content").and_then(Value::as_array).into_iter().flatten().enumerate() {
                let marker = if kind == "taskList" {
                    if item.pointer("/attrs/checked").and_then(Value::as_bool).unwrap_or(false) { "- [x]".to_string() } else { "- [ ]".to_string() }
                } else if ordered {
                    format!("{}.", i + 1)
                } else {
                    "-".to_string()
                };
                let mut first = true;
                for part in item.get("content").and_then(Value::as_array).into_iter().flatten() {
                    let part_kind = part.get("type").and_then(Value::as_str).unwrap_or("");
                    if first && part_kind == "paragraph" {
                        out.push_str(&format!("{indent}{marker} {}\n", inline(part)));
                        first = false;
                    } else {
                        let mut nested = String::new();
                        block(part, &mut nested, depth + 1);
                        out.push_str(nested.trim_end_matches('\n'));
                        out.push('\n');
                    }
                }
            }
            out.push('\n');
        }
        "blockquote" => {
            let mut inner = String::new();
            block_children(node, &mut inner, 0);
            for line in inner.trim().lines() {
                out.push_str(&format!("> {line}\n"));
            }
            out.push('\n');
        }
        "codeBlock" => {
            let lang = node.pointer("/attrs/language").and_then(Value::as_str).unwrap_or("");
            out.push_str(&format!("```{lang}\n{}\n```\n\n", plain(node)));
        }
        "horizontalRule" => out.push_str("---\n\n"),
        "image" => {
            if let Some(src) = node.pointer("/attrs/src").and_then(Value::as_str) {
                out.push_str(&format!("![{}]({src})\n\n", node.pointer("/attrs/alt").and_then(Value::as_str).unwrap_or("")));
            }
        }
        _ => {
            if node.get("content").is_some() {
                block_children(node, out, depth);
            } else if let Some(text) = node.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
        }
    }
}

fn plain(node: &Value) -> String {
    node.get("content").and_then(Value::as_array).into_iter().flatten().map(|c| c.get("text").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| plain(c))).collect()
}

fn inline(node: &Value) -> String {
    let mut out = String::new();
    for child in node.get("content").and_then(Value::as_array).into_iter().flatten() {
        match child.get("type").and_then(Value::as_str) {
            Some("text") => {
                let mut text = child.get("text").and_then(Value::as_str).unwrap_or("").to_string();
                for mark in child.get("marks").and_then(Value::as_array).into_iter().flatten() {
                    text = match mark.get("type").and_then(Value::as_str) {
                        Some("bold") | Some("strong") => format!("**{text}**"),
                        Some("italic") | Some("em") => format!("*{text}*"),
                        Some("code") => format!("`{text}`"),
                        Some("strike") => format!("~~{text}~~"),
                        Some("link") => format!("[{text}]({})", mark.pointer("/attrs/href").and_then(Value::as_str).unwrap_or("")),
                        _ => text,
                    };
                }
                out.push_str(&text);
            }
            Some("hardBreak") => out.push_str("  \n"),
            Some("mention") => out.push_str(child.pointer("/attrs/label").or_else(|| child.pointer("/attrs/id")).and_then(Value::as_str).unwrap_or("")),
            _ => out.push_str(&inline(child)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enex_notes_tags_and_images() {
        let png = [137u8, 80, 78, 71];
        let b64 = base64::engine::general_purpose::STANDARD.encode(png);
        let hash: String = Md5::digest(png).iter().map(|b| format!("{b:02x}")).collect();
        let enex = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE en-export SYSTEM "http://xml.evernote.com/pub/evernote-export3.dtd">
<en-export><note><title>Plan &amp; ideas</title><content><![CDATA[<?xml version="1.0"?><en-note><div>Hello <b>world</b></div><div><en-todo checked="true"/>Done thing</div><en-media hash="{hash}" type="image/png"/><en-media hash="ffff" type="application/pdf"/><en-crypt>xx</en-crypt></en-note>]]></content>
<created>20240131T153000Z</created><updated>20240201T080000Z</updated><tag>work</tag><tag>q1</tag>
<resource><data encoding="base64">{b64}</data><mime>image/png</mime><resource-attributes><file-name>shot.png</file-name></resource-attributes></resource></note></en-export>"#
        );
        let (notes, skipped) = read_enex(&enex, "Work");
        assert_eq!(notes.len(), 1);
        let n = &notes[0];
        assert_eq!(n.title, "Plan & ideas");
        assert_eq!(n.tags, vec!["work", "q1"]);
        assert_eq!(n.folder, vec!["Work"]);
        assert_eq!(n.created_at.unwrap().to_rfc3339(), "2024-01-31T15:30:00+00:00");
        let body = n.notes_md.as_deref().unwrap();
        assert!(body.contains("Hello **world**"), "{body}");
        assert!(body.contains("- [x] Done thing"), "{body}");
        assert!(body.contains("data:image/png;base64,"), "{body}");
        assert!(body.contains("Encrypted text not imported"));
        assert_eq!(n.attachments, 1);
        assert_eq!(skipped.encrypted, 1);
    }

    #[test]
    fn prosemirror_documents() {
        let doc: Value = serde_json::from_str(
            r#"{"type":"doc","content":[
            {"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Decisions"}]},
            {"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","marks":[{"type":"bold"}],"text":"Ship"},{"type":"text","text":" Friday"}]}]}]},
            {"type":"taskList","content":[{"type":"taskItem","attrs":{"checked":true},"content":[{"type":"paragraph","content":[{"type":"text","text":"Email Sam"}]}]}]},
            {"type":"horizontalRule"},
            {"type":"orderedList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"One"}]}]},{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"Two"}]}]}]}
        ]}"#,
        )
        .unwrap();
        let md = prosemirror_to_markdown(&doc);
        assert!(md.contains("## Decisions"));
        assert!(md.contains("- **Ship** Friday"));
        assert!(md.contains("- [x] Email Sam"));
        assert!(md.contains("---"));
        assert!(md.contains("1. One\n2. Two"));
    }

    #[test]
    fn html_becomes_markdown() {
        let md = html_to_markdown("<html><head><title>x</title></head><body><h2>Hi</h2><p>A <strong>b</strong></p><ul><li>one</li></ul><script>bad()</script></body></html>");
        assert!(md.contains("## Hi"), "{md}");
        assert!(md.contains("A **b**"));
        assert!(md.contains("one"));
        assert!(!md.contains("bad()"));
    }
}
