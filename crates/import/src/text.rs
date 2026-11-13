//! Small text helpers shared by every format: dates, clock times, titles and file names.

use std::path::Path;
use std::sync::LazyLock;

use cilica_domain::Timestamp;
use cilica_domain::chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;

/// Parse the many ways apps write a date: RFC 3339, `2024-01-31`, `2024-01-31 15:30`,
/// `20240131T153000Z` (Evernote), `2024_01_31` (Logseq journals), Unix seconds or milliseconds.
/// Dates without a zone are taken as local time.
pub fn parse_date(text: &str) -> Option<Timestamp> {
    let s = text.trim().trim_matches('"').trim_matches('\'');
    if s.is_empty() {
        return None;
    }
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Some(t.with_timezone(&Utc));
    }
    if let Ok(t) = DateTime::parse_from_rfc2822(s) {
        return Some(t.with_timezone(&Utc));
    }
    if let Ok(n) = s.parse::<f64>()
        && n.is_finite()
        && n > 0.0
    {
        // Seconds until the year 5138; anything bigger is milliseconds, then microseconds.
        let secs = if n > 1e14 { n / 1e6 } else if n > 1e11 { n / 1e3 } else { n };
        return Utc.timestamp_opt(secs as i64, ((secs.fract()) * 1e9) as u32).single();
    }
    let naive_formats = [
        "%Y%m%dT%H%M%SZ",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y %H:%M",
        "%m/%d/%Y %I:%M %p",
        "%B %d, %Y %I:%M %p",
        "%B %d, %Y %H:%M",
        "%b %d, %Y %I:%M %p",
    ];
    for f in naive_formats {
        if let Ok(t) = NaiveDateTime::parse_from_str(s, f) {
            // Evernote's compact form is UTC; the rest have no zone and are local.
            return if f.ends_with('Z') { Some(Utc.from_utc_datetime(&t)) } else { local(t) };
        }
    }
    let date_formats = ["%Y-%m-%d", "%Y_%m_%d", "%Y/%m/%d", "%Y.%m.%d", "%m/%d/%Y", "%d %B %Y", "%B %d, %Y", "%b %d, %Y", "%d %b %Y", "%A, %B %d, %Y"];
    for f in date_formats {
        if let Ok(d) = NaiveDate::parse_from_str(s, f) {
            return local(d.and_hms_opt(0, 0, 0)?);
        }
    }
    None
}

fn local(t: NaiveDateTime) -> Option<Timestamp> {
    Local.from_local_datetime(&t).earliest().map(|t| t.with_timezone(&Utc))
}

/// A clock offset into a recording: `1:02:03`, `02:03`, `00:01:02.500`, `00:01:02,500`, `1:02`.
pub fn parse_clock(text: &str) -> Option<u32> {
    let s = text.trim().trim_start_matches('[').trim_end_matches(']').trim_start_matches('(').trim_end_matches(')');
    let (main, frac) = match s.split_once(['.', ',']) {
        Some((m, f)) => (m, Some(f)),
        None => (s, None),
    };
    let parts: Vec<&str> = main.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 || parts.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let nums: Vec<u64> = parts.iter().map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let secs = match nums.as_slice() {
        [m, s] if *s < 60 => m * 60 + s,
        [h, m, s] if *m < 60 && *s < 60 => h * 3600 + m * 60 + s,
        _ => return None,
    };
    let ms = match frac {
        Some(f) if !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()) => {
            let f = &f[..f.len().min(3)];
            f.parse::<u64>().ok()? * 10u64.pow(3 - f.len() as u32)
        }
        Some(_) => return None,
        None => 0,
    };
    u32::try_from(secs * 1000 + ms).ok()
}

/// Seconds (possibly fractional) to whole milliseconds.
pub fn secs_to_ms(secs: f64) -> Option<u32> {
    (secs.is_finite() && secs >= 0.0).then(|| (secs * 1000.0).round().min(u32::MAX as f64) as u32)
}

static NOTION_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+[0-9a-f]{32}$").unwrap());

/// Notion appends a 32-character page id to every exported name: `Meeting notes 1a2b…ef.md`.
pub fn strip_notion_id(name: &str) -> String {
    NOTION_ID.replace(name, "").to_string()
}

pub fn has_notion_id(name: &str) -> bool {
    NOTION_ID.is_match(name)
}

/// A file's name without its extension(s): `Weekly.excalidraw.md` → `Weekly.excalidraw`.
pub fn file_stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Lowercase extension without the dot.
pub fn ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// The first `# Heading` of a Markdown document, and the document without it.
pub fn take_h1(markdown: &str) -> (Option<String>, String) {
    let trimmed = markdown.trim_start();
    if let Some(rest) = trimmed.strip_prefix("# ") {
        let (line, after) = rest.split_once('\n').unwrap_or((rest, ""));
        let title = line.trim().to_string();
        if !title.is_empty() {
            return (Some(title), after.trim_start_matches(['\n', '\r']).to_string());
        }
    }
    (None, markdown.to_string())
}

/// Collapse runs of blank lines and trim, so imported bodies look tidy.
pub fn tidy(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut blank = 0;
    for line in markdown.replace("\r\n", "\n").lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.trim().to_string()
}

/// "1 note", "3 notes".
pub fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_in_many_shapes() {
        assert_eq!(parse_date("2024-01-31T15:30:00Z").unwrap().to_rfc3339(), "2024-01-31T15:30:00+00:00");
        assert_eq!(parse_date("20240131T153000Z").unwrap().to_rfc3339(), "2024-01-31T15:30:00+00:00");
        assert_eq!(parse_date("1706715000").unwrap().to_rfc3339(), "2024-01-31T15:30:00+00:00");
        assert_eq!(parse_date("1706715000000").unwrap().to_rfc3339(), "2024-01-31T15:30:00+00:00");
        assert_eq!(parse_date("1706715000000000").unwrap().to_rfc3339(), "2024-01-31T15:30:00+00:00");
        assert!(parse_date("2024-01-31").is_some());
        assert!(parse_date("2024_01_31").is_some());
        assert!(parse_date("March 4, 2025 9:00 AM").is_some());
        assert!(parse_date("not a date").is_none());
        assert!(parse_date("").is_none());
    }

    #[test]
    fn clocks() {
        assert_eq!(parse_clock("0:03"), Some(3_000));
        assert_eq!(parse_clock("1:02:03"), Some(3_723_000));
        assert_eq!(parse_clock("00:01:02.5"), Some(62_500));
        assert_eq!(parse_clock("00:01:02,500"), Some(62_500));
        assert_eq!(parse_clock("[00:00:07]"), Some(7_000));
        assert_eq!(parse_clock("1:75"), None);
        assert_eq!(parse_clock("Sam"), None);
        assert_eq!(parse_clock("12"), None);
    }

    #[test]
    fn notion_names() {
        assert_eq!(strip_notion_id("Kickoff 1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d"), "Kickoff");
        assert_eq!(strip_notion_id("Kickoff"), "Kickoff");
    }

    #[test]
    fn headings_and_tidying() {
        assert_eq!(take_h1("# Title\n\nBody"), (Some("Title".into()), "Body".into()));
        assert_eq!(take_h1("Body").0, None);
        assert_eq!(tidy("a\n\n\n\nb  \n"), "a\n\nb");
    }
}
