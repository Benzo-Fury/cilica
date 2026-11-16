//! Note properties: the YAML frontmatter (`---` … `---`) at the very top of a note body.
//!
//! The editor never shows it; it is written when the user does something that needs it (approving
//! a suggested link) and travels with the Markdown, so Obsidian reads it as Properties. Only
//! top-level keys are read and written, line by line: every other key, comment and its layout are
//! kept exactly as they were.

/// `[[links]]` to related notes the user approved. Indexed like links in the text.
pub const RELATED: &str = "related";
/// Titles the user said are not related, so they are never suggested again. Plain text, not links.
pub const UNRELATED: &str = "unrelated";
/// Other names for the note. Matched like its title when suggesting links.
pub const ALIASES: &str = "aliases";
/// A note's `aliases`, trimmed, without blanks or repeats (ignoring case), in the order written.
pub fn aliases(markdown: &str) -> Vec<String> {
    let Some(yaml) = split(markdown).0 else { return Vec::new() };
    let mut seen = std::collections::HashSet::new();
    list(yaml, ALIASES)
        .into_iter()
        .map(|a| a.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|a| !a.is_empty() && seen.insert(a.to_lowercase()))
        .collect()
}

/// A one-line description of the note.
pub const DESCRIPTION: &str = "description";

/// `(yaml, body)`: the property block (without its `---` lines) and the rest. `None` when the note
/// has no block, or what sits between the rules does not read as YAML (a horizontal rule the
/// user wrote, say).
pub fn split(markdown: &str) -> (Option<&str>, &str) {
    let Some(rest) = markdown.strip_prefix("---\n") else { return (None, markdown) };
    let (yaml, after) = match rest.strip_prefix("---") {
        Some(after) => ("", after),
        None => match rest.find("\n---") {
            Some(end) => (&rest[..end], &rest[end + 4..]),
            None => return (None, markdown),
        },
    };
    if !(after.is_empty() || after.starts_with('\n')) || !reads_as_yaml(yaml) {
        return (None, markdown);
    }
    (Some(yaml), after.trim_start_matches('\n'))
}

/// The body without its property block: what the editor shows.
pub fn body(markdown: &str) -> &str {
    split(markdown).1
}

/// A body with a property block on top. An empty block is written as `---\n---`, which a save
/// reads as "clear the properties" (a body with no block keeps the note's current ones).
pub fn join(yaml: &str, body: &str) -> String {
    let yaml = yaml.trim_matches('\n');
    let body = body.trim_start_matches('\n');
    if yaml.trim().is_empty() { format!("---\n---\n{body}") } else { format!("---\n{yaml}\n---\n\n{body}") }
}

/// The values of a top-level key, in any of YAML's list forms (`key: a`, `key: [a, b]`, or one
/// `- a` per line), unquoted. Keys compare case-insensitively.
pub fn list(yaml: &str, key: &str) -> Vec<String> {
    let lines: Vec<&str> = yaml.lines().collect();
    let Some((start, end)) = key_block(&lines, key) else { return Vec::new() };
    let value = lines[start].split_once(':').map_or("", |(_, v)| v).trim();
    if !value.is_empty() {
        return match value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            Some(inner) => flow_items(inner),
            None => vec![unquote(value)].into_iter().filter(|v| !v.is_empty()).collect(),
        };
    }
    lines[start + 1..end]
        .iter()
        .filter_map(|l| l.trim().strip_prefix('-'))
        .map(|v| unquote(v.trim()))
        .filter(|v| !v.is_empty())
        .collect()
}

/// The first value of a top-level key, if it has one.
pub fn get(yaml: &str, key: &str) -> Option<String> {
    list(yaml, key).into_iter().next()
}

/// Set a top-level key to a list (one `- "value"` per line), in place if it exists, else at the
/// end. An empty list removes the key.
pub fn set_list(yaml: &str, key: &str, values: &[String]) -> String {
    let lines: Vec<&str> = yaml.lines().collect();
    let mut block = Vec::new();
    if !values.is_empty() {
        block.push(format!("{key}:"));
        block.extend(values.iter().map(|v| format!("  - {}", quote(v))));
    }
    let mut out: Vec<String> = match key_block(&lines, key) {
        Some((start, end)) => {
            let mut out: Vec<String> = lines[..start].iter().map(|l| l.to_string()).collect();
            out.extend(block);
            out.extend(lines[end..].iter().map(|l| l.to_string()));
            out
        }
        None => {
            let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            while out.last().is_some_and(|l| l.trim().is_empty()) {
                out.pop();
            }
            out.extend(block);
            out
        }
    };
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// The note a `[[Title]]`, `[[Title|text]]` or `[[Title#Heading]]` value points at; plain text as is.
pub fn link_target(value: &str) -> &str {
    let v = value.trim();
    let inner = v.strip_prefix("[[").and_then(|v| v.strip_suffix("]]")).unwrap_or(v);
    let inner = inner.split('|').next().unwrap_or(inner);
    inner.split('#').next().unwrap_or(inner).trim()
}

// ── Reading ─────────────────────────────────────────────────────────────────

/// Every non-blank line is a `key:` line, an indented or `- ` continuation, or a comment, and the
/// first is a key. Keeps a note that opens with `---`, a paragraph and `---` a note.
fn reads_as_yaml(yaml: &str) -> bool {
    let mut lines = yaml.lines().filter(|l| !l.trim().is_empty());
    let Some(first) = lines.next() else { return true };
    is_key_line(first) && lines.all(|l| is_key_line(l) || l.starts_with([' ', '\t', '-', '#']))
}

fn is_key_line(line: &str) -> bool {
    let Some((key, _)) = line.split_once(':') else { return false };
    !key.is_empty() && !key.starts_with([' ', '\t', '-', '#']) && key.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' ' | '.'))
}

/// Lines `[start, end)` of a top-level key: its own line and the indented or `- ` lines under it.
fn key_block(lines: &[&str], key: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| is_key_line(l) && l.split_once(':').is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key)))?;
    let mut end = start + 1;
    while end < lines.len() && (lines[end].starts_with([' ', '\t', '-']) || lines[end].trim().is_empty()) {
        end += 1;
    }
    // Blank lines after the block belong to what follows.
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    Some((start, end))
}

/// `a, "b, c", 'd'` → `[a, b, c, d]`, splitting only on commas outside quotes and `[[…]]`.
fn flow_items(inner: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut depth = 0usize;
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => {
                current.push(c);
                quote = None;
            }
            (Some('"'), '\\') => {
                current.push(c);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (Some(_), _) => current.push(c),
            (None, '"' | '\'') => {
                current.push(c);
                quote = Some(c);
            }
            (None, '[') => {
                depth += 1;
                current.push(c);
            }
            (None, ']') => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            (None, ',') if depth == 0 => items.push(std::mem::take(&mut current)),
            (None, _) => current.push(c),
        }
    }
    items.push(current);
    items.iter().map(|v| unquote(v.trim())).filter(|v| !v.is_empty()).collect()
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        return v[1..v.len() - 1].replace("\\\"", "\"").replace("\\\\", "\\");
    }
    if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        return v[1..v.len() - 1].replace("''", "'");
    }
    v.to_string()
}

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_joins() {
        let md = join("related:\n  - \"[[Pricing]]\"", "Hello");
        assert_eq!(md, "---\nrelated:\n  - \"[[Pricing]]\"\n---\n\nHello");
        assert_eq!(split(&md), (Some("related:\n  - \"[[Pricing]]\""), "Hello"));
        assert_eq!(body("Just text"), "Just text");
        assert_eq!(split("---\n---\nBody"), (Some(""), "Body"));
        assert_eq!(join("", "Body"), "---\n---\nBody");
    }

    #[test]
    fn a_rule_around_a_paragraph_is_not_properties() {
        let md = "---\n\nHello there\n\n---\n\nMore";
        assert_eq!(split(md), (None, md));
        assert_eq!(split("---\nnot: closed"), (None, "---\nnot: closed"));
    }

    #[test]
    fn reads_every_list_form() {
        let yaml = "aliases: SSO\ntags: [a, \"b, c\"]\nrelated:\n  - \"[[One, two]]\"\n  - '[[Three]]'\ntitle: x";
        assert_eq!(list(yaml, "aliases"), ["SSO"]);
        assert_eq!(list(yaml, "Tags"), ["a", "b, c"]);
        assert_eq!(list(yaml, "related"), ["[[One, two]]", "[[Three]]"]);
        assert_eq!(list("related: [\"[[A, B]]\", [[C]]]", "related"), ["[[A, B]]", "[[C]]"]);
        assert!(list(yaml, "missing").is_empty());
        assert_eq!(get(yaml, "title").as_deref(), Some("x"));
    }

    #[test]
    fn sets_a_list_and_keeps_everything_else() {
        let yaml = "# mine\ntags: [x]\nrelated:\n  - \"[[A]]\"\nsource: notion";
        let next = set_list(yaml, "related", &["[[A]]".into(), "[[B \"q\"]]".into()]);
        assert_eq!(next, "# mine\ntags: [x]\nrelated:\n  - \"[[A]]\"\n  - \"[[B \\\"q\\\"]]\"\nsource: notion");
        assert_eq!(list(&next, "related"), ["[[A]]", "[[B \"q\"]]"]);
        assert_eq!(set_list(&next, "related", &[]), "# mine\ntags: [x]\nsource: notion");
        assert_eq!(set_list("", "unrelated", &["Q3".into()]), "unrelated:\n  - \"Q3\"");
    }

    #[test]
    fn link_targets() {
        assert_eq!(link_target("[[Pricing|price]]"), "Pricing");
        assert_eq!(link_target("[[Pricing#Tiers]]"), "Pricing");
        assert_eq!(link_target("Plain title"), "Plain title");
    }
}
