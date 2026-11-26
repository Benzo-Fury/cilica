//! Markdown helpers: which attachments a text points at, and turning inline `data:` images
//! (Markdown `![alt](data:…)` and HTML `<img src="data:…">`, both written by the importer) into
//! `![alt](attachments/<name>)` links.

use std::sync::LazyLock;

use regex::Regex;

use crate::store::{ImageStore, SaveOptions, decode_base64};

/// `attachments/<32 hex>.<ext>` anywhere in a text, Markdown or JSON alike.
static REFERENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"attachments/([0-9a-f]{32}\.(?:png|jpg|gif|webp|avif|bmp))\b").expect("reference pattern"));

/// A Markdown image whose target is a base64 data URI, with an optional `"title"`.
static MARKDOWN_INLINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"!\[(?P<alt>[^\]\n]*)\]\(\s*<?(?P<uri>data:image/[A-Za-z0-9.+-]+(?:;[A-Za-z0-9.+=_-]+)*;base64,[A-Za-z0-9+/=\s]*?)>?(?P<title>\s+"[^"\n]*")?\s*\)"#)
        .expect("markdown image pattern")
});

/// An HTML `<img …>` tag.
static HTML_IMG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<img\b[^>]*>").expect("img pattern"));
/// The `src` attribute of a tag, when it is a base64 data URI.
static HTML_SRC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?is)\bsrc\s*=\s*(?:"(data:image/[^"]*)"|'(data:image/[^']*)')"#).expect("src pattern"));
/// The `alt` attribute of a tag.
static HTML_ALT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?is)\balt\s*=\s*(?:"([^"]*)"|'([^']*)')"#).expect("alt pattern"));

/// A Markdown image that points at an attachment: `![alt|align|width](attachments/<name> "caption")`.
static NOTE_IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"!\[(?P<alt>[^\]\n]*)\]\(\s*<?(?P<path>attachments/[0-9a-f]{32}\.(?:png|jpg|gif|webp|avif|bmp))>?(?:\s+"(?P<caption>[^"\n]*)")?\s*\)"#)
        .expect("note image pattern")
});

/// One image a note shows from the attachments folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteImageRef {
    /// `attachments/<name>`, as written in the note.
    pub path: String,
    /// The alt text, without the `|align|width` layout parts.
    pub alt: String,
    pub caption: String,
}

/// The attachment images a note shows, in order, each path once.
pub fn note_images(markdown: &str) -> Vec<NoteImageRef> {
    let mut seen = std::collections::HashSet::new();
    NOTE_IMAGE
        .captures_iter(markdown)
        .filter(|c| seen.insert(c["path"].to_string()))
        .map(|c| NoteImageRef {
            path: c["path"].to_string(),
            alt: c["alt"].split('|').next().unwrap_or("").trim().to_string(),
            caption: c.name("caption").map(|m| m.as_str().trim().to_string()).unwrap_or_default(),
        })
        .collect()
}

/// The attachment file names (`3f2a…c1.png`) a text points at, in order, repeats included.
pub fn referenced_names(markdown: &str) -> impl Iterator<Item = String> + '_ {
    REFERENCE.captures_iter(markdown).map(|c| c[1].to_string())
}

/// What [`externalize_inline_images`] did to one text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Externalized {
    /// The text with every image that could be saved pointing at its file.
    pub markdown: String,
    /// Inline images saved and rewritten.
    pub converted: u32,
    /// Inline images that couldn't be read or kept; they stay as they were.
    pub failed: u32,
}

/// One inline image found in a text, before it is saved.
struct InlineImage {
    /// Byte range of the whole image (Markdown image or `<img>` tag).
    start: usize,
    end: usize,
    alt: String,
    /// The Markdown title part (` "Caption"`), kept as written.
    title: String,
    uri: String,
}

/// Save every inline `data:` image in `markdown` into `store` and point at the file instead.
/// Images keep their alt text (for `<img>`, its `alt` attribute). Ones that fail stay unchanged.
pub async fn externalize_inline_images(markdown: &str, store: &ImageStore) -> Externalized {
    // Find both kinds, in order
    let images = find_inline_images(markdown);
    if images.is_empty() {
        return Externalized { markdown: markdown.to_string(), converted: 0, failed: 0 };
    }

    // Save each and stitch the text back together around them
    let mut out = String::with_capacity(markdown.len() / 4);
    let (mut converted, mut failed, mut at) = (0, 0, 0);
    for image in images {
        out.push_str(&markdown[at..image.start]);
        let saved = match decode_base64(&image.uri) {
            Some(bytes) => store.save_bytes(bytes, None, SaveOptions::default()).await.ok(),
            None => None,
        };
        match saved {
            Some(saved) => {
                out.push_str(&format!("![{}]({}{})", image.alt, saved.path, image.title));
                converted += 1;
            }
            None => {
                out.push_str(&markdown[image.start..image.end]);
                failed += 1;
            }
        }
        at = image.end;
    }
    out.push_str(&markdown[at..]);
    Externalized { markdown: out, converted, failed }
}

/// Every inline image in `markdown`, Markdown and HTML, sorted and never overlapping.
fn find_inline_images(markdown: &str) -> Vec<InlineImage> {
    // Markdown images
    let mut found: Vec<InlineImage> = MARKDOWN_INLINE
        .captures_iter(markdown)
        .map(|c| {
            let whole = c.get(0).expect("match");
            InlineImage {
                start: whole.start(),
                end: whole.end(),
                alt: c["alt"].to_string(),
                title: c.name("title").map(|t| t.as_str().to_string()).unwrap_or_default(),
                uri: c["uri"].to_string(),
            }
        })
        .collect();

    // HTML tags with a data URI source
    for tag in HTML_IMG.find_iter(markdown) {
        let Some(src) = HTML_SRC.captures(tag.as_str()) else { continue };
        let uri = src.get(1).or_else(|| src.get(2)).map(|m| m.as_str().to_string()).unwrap_or_default();
        let alt = HTML_ALT.captures(tag.as_str()).and_then(|a| a.get(1).or_else(|| a.get(2))).map(|m| html_unescape(m.as_str())).unwrap_or_default();
        found.push(InlineImage { start: tag.start(), end: tag.end(), alt: markdown_alt(&alt), title: String::new(), uri });
    }

    // Keep them in order and drop any that sit inside another
    found.sort_by_key(|i| i.start);
    let mut kept: Vec<InlineImage> = Vec::with_capacity(found.len());
    for image in found {
        if kept.last().is_none_or(|last| image.start >= last.end) {
            kept.push(image);
        }
    }
    kept
}

/// Alt text that can't break out of `![…]`: one line, no square brackets.
fn markdown_alt(alt: &str) -> String {
    alt.replace(['[', ']'], "").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The few entities that turn up in `alt` attributes.
fn html_unescape(text: &str) -> String {
    text.replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use image::ImageFormat;

    use super::*;
    use crate::format::tests::encoded;

    /// A base64 PNG of the given size.
    fn png64(width: u32, height: u32) -> String {
        base64::engine::general_purpose::STANDARD.encode(encoded(width, height, ImageFormat::Png))
    }

    #[test]
    fn finds_references_anywhere() {
        let a = "0123456789abcdef0123456789abcdef";
        let text = format!(
            "![Shot|center|400](attachments/{a}.png \"Cap\") and {{\"body\":\"![x](attachments/{a}.jpg)\"}} but not attachments/{a}.svg or attachments/short.png"
        );
        let names: Vec<String> = referenced_names(&text).collect();
        assert_eq!(names, vec![format!("{a}.png"), format!("{a}.jpg")]);
    }

    #[test]
    fn lists_note_images_with_alt_and_caption() {
        let a = "0123456789abcdef0123456789abcdef";
        let b = "fedcba9876543210fedcba9876543210";
        let text = format!("![Q3 chart|center|400](attachments/{a}.png \"Revenue by month\")\n![](attachments/{b}.jpg)\n![again](attachments/{a}.png)\n![web](https://x.test/a.png)");
        let found = note_images(&text);
        assert_eq!(
            found,
            vec![
                NoteImageRef { path: format!("attachments/{a}.png"), alt: "Q3 chart".into(), caption: "Revenue by month".into() },
                NoteImageRef { path: format!("attachments/{b}.jpg"), alt: String::new(), caption: String::new() },
            ]
        );
    }

    #[tokio::test]
    async fn rewrites_markdown_and_html_inline_images() {
        let temp = tempfile::tempdir().unwrap();
        let store = ImageStore::new(temp.path().join("attachments"));
        let (one, two) = (png64(3, 3), png64(4, 4));
        let markdown = format!(
            "Intro\n\n![Chart|center|400](data:image/png;base64,{one} \"Q3\")\n\n<img alt=\"A &amp; B\" src=\"data:image/png;base64,{two}\">\n\n![bad](data:image/png;base64,AAAA)\n"
        );
        let done = externalize_inline_images(&markdown, &store).await;
        assert_eq!((done.converted, done.failed), (2, 1));
        assert!(!done.markdown.contains(&one) && !done.markdown.contains(&two));
        assert!(done.markdown.contains("![bad](data:image/png;base64,AAAA)"));

        // Alt and title are kept; the <img> becomes Markdown with its alt
        let names: Vec<String> = referenced_names(&done.markdown).collect();
        assert_eq!(names.len(), 2);
        assert!(done.markdown.contains(&format!("![Chart|center|400](attachments/{} \"Q3\")", names[0])));
        assert!(done.markdown.contains(&format!("![A & B](attachments/{})", names[1])));
        assert!(done.markdown.starts_with("Intro\n\n") && done.markdown.ends_with(")\n"));
    }

    #[tokio::test]
    async fn leaves_plain_text_alone() {
        let temp = tempfile::tempdir().unwrap();
        let store = ImageStore::new(temp.path());
        let text = "No images here, just data:image/png mentioned.";
        let done = externalize_inline_images(text, &store).await;
        assert_eq!(done, Externalized { markdown: text.to_string(), converted: 0, failed: 0 });
    }
}
