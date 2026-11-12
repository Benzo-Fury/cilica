//! Transcripts in plain text, WebVTT and SRT, as exported by Otter, Teams, Zoom, Webex, Avoma,
//! Tactiq, Read, Fireflies, Grain, MacWhisper, Jamie and many more.
//!
//! Each parser returns turns with a speaker when the text names one. Consecutive lines from the
//! same speaker are merged so the transcript reads like Margin's own.

use std::sync::LazyLock;

use regex::Regex;

use crate::model::{ImportedTranscript, SpeakerRef, Turn};
use crate::text::parse_clock;

const CLOCK: &str = r"\d{1,2}:\d{2}(?::\d{2})?(?:[.,]\d{1,3})?";

/// `Jane Doe  0:03` on its own line (Otter, Granola copy, Plaud), or `Jane Doe (00:01:02)`.
static NAME_THEN_TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"^(?P<name>[^\[\]():]{{1,60}}?)\s*(?:\(|\s{{1,}}|\s*-\s*|\s*\|\s*)\[?(?P<time>{CLOCK})\]?\)?:?$")).unwrap());
/// `[00:01:02] Jane: text`, `00:01:02 Jane: text`, `(0:03) Jane - text`.
static TIME_NAME_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"^[\[(]?(?P<time>{CLOCK})[\])]?\s*[-–]?\s*(?P<name>[^:\n]{{1,60}}?)\s*[:\-–]\s+(?P<text>.+)$")).unwrap()
});
/// `Jane (00:01:02): text`, `Jane [0:03]: text`, `Jane 00:01:02: text`.
static NAME_TIME_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"^(?P<name>[^\[\]():\n]{{1,60}}?)\s*[\[(]?(?P<time>{CLOCK})[\])]?\s*:\s+(?P<text>.+)$")).unwrap()
});
/// `Jane: text`, `**Jane:** text`, `**Jane**: text`.
static NAME_TEXT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\*{0,2}(?P<name>[^:*\n]{1,40}?)\*{0,2}\s*:\*{0,2}\s+(?P<text>.+)$").unwrap());
/// `**Jane Doe**` alone on a line (Jamie), optionally with a time.
static BOLD_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"^\*\*(?P<name>[^*\n]{{1,60}})\*\*(?:\s*[\[(]?(?P<time>{CLOCK})[\])]?)?:?$")).unwrap()
});

/// Read a transcript from plain text. `None` when the text does not look like one (most lines
/// must belong to a speaker turn).
pub fn parse_text(text: &str) -> Option<ImportedTranscript> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let content_lines = lines.iter().filter(|l| !l.is_empty()).count();
    if content_lines == 0 {
        return None;
    }
    let mut turns: Vec<Turn> = Vec::new();
    let mut matched = 0usize;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.is_empty() {
            i += 1;
            continue;
        }
        // A header line: speaker (and maybe time), with the words on the following lines.
        let header = NAME_THEN_TIME
            .captures(line)
            .filter(|c| plausible_name(&c["name"]))
            .map(|c| (c["name"].trim().to_string(), parse_clock(&c["time"])))
            .or_else(|| BOLD_NAME.captures(line).map(|c| (c["name"].trim().to_string(), c.name("time").and_then(|t| parse_clock(t.as_str())))));
        if let Some((name, start)) = header {
            let mut body = Vec::new();
            let mut j = i + 1;
            while j < lines.len() && !lines[j].is_empty() && !is_header(lines[j]) {
                body.push(lines[j]);
                j += 1;
            }
            if !body.is_empty() {
                matched += 1 + body.len();
                push(&mut turns, &name, start, body.join(" "));
                i = j;
                continue;
            }
        }
        let inline = TIME_NAME_TEXT
            .captures(line)
            .filter(|c| plausible_name(&c["name"]))
            .map(|c| (c["name"].to_string(), parse_clock(&c["time"]), c["text"].to_string()))
            .or_else(|| {
                NAME_TIME_TEXT.captures(line).filter(|c| plausible_name(&c["name"])).map(|c| (c["name"].to_string(), parse_clock(&c["time"]), c["text"].to_string()))
            })
            .or_else(|| NAME_TEXT.captures(line).filter(|c| plausible_name(&c["name"])).map(|c| (c["name"].to_string(), None, c["text"].to_string())));
        if let Some((name, start, words)) = inline {
            matched += 1;
            push(&mut turns, name.trim(), start, words);
        } else if let Some(last) = turns.last_mut() {
            // A wrapped line continues the previous turn.
            last.text.push(' ');
            last.text.push_str(line);
            matched += 1;
        }
        i += 1;
    }
    let speakers = turns.iter().filter(|t| t.speaker != SpeakerRef::Unknown).count();
    (turns.len() >= 2 && speakers >= 2 && matched * 10 >= content_lines * 7).then(|| finish(turns))
}

fn is_header(line: &str) -> bool {
    NAME_THEN_TIME.captures(line).is_some_and(|c| plausible_name(&c["name"])) || BOLD_NAME.is_match(line)
}

/// Names are short and are not sentences, URLs or list items.
fn plausible_name(name: &str) -> bool {
    let name = name.trim().trim_matches('*');
    let words = name.split_whitespace().count();
    !name.is_empty()
        && words <= 5
        && name.chars().count() <= 40
        && !name.starts_with(['-', '#', '>', '`', '|'])
        && !name.contains("http")
        && !name.ends_with(['.', '?', '!', ','])
        && name.chars().next().is_some_and(|c| c.is_alphanumeric())
        && !matches!(name.to_ascii_lowercase().as_str(), "note" | "notes" | "summary" | "action items" | "date" | "time" | "duration" | "attendees" | "title" | "agenda" | "tags" | "created" | "updated" | "source" | "url" | "link" | "next steps" | "decisions")
}

fn push(turns: &mut Vec<Turn>, name: &str, start: Option<u32>, text: String) {
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    let speaker = speaker_ref(name);
    if let Some(last) = turns.last_mut()
        && last.speaker == speaker
        && start.is_none()
    {
        last.text.push(' ');
        last.text.push_str(&text);
        return;
    }
    turns.push(Turn { speaker, start_ms: start, end_ms: None, text });
}

/// Map how an app names a speaker to Margin's speakers.
pub fn speaker_ref(name: &str) -> SpeakerRef {
    let n = name.trim().trim_matches('*').trim();
    match n.to_ascii_lowercase().as_str() {
        "" | "unknown" | "unknown speaker" => SpeakerRef::Unknown,
        "me" | "you" | "microphone" | "mic" => SpeakerRef::Me,
        "them" | "others" | "system" | "speaker" => SpeakerRef::Them,
        _ => SpeakerRef::Named(n.to_string()),
    }
}

/// Fill in end times from the next turn's start.
pub fn finish(mut turns: Vec<Turn>) -> ImportedTranscript {
    for i in 0..turns.len() {
        if turns[i].end_ms.is_none()
            && let Some(start) = turns[i].start_ms
        {
            let next = turns[i + 1..].iter().find_map(|t| t.start_ms).filter(|n| *n >= start);
            let estimate = start + (turns[i].text.split_whitespace().count() as u32 * 400).max(1_000);
            turns[i].end_ms = Some(next.unwrap_or(estimate));
        }
    }
    ImportedTranscript::new(turns)
}

// ── WebVTT and SRT ───────────────────────────────────────────────────────

static VOICE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<v(?:\.[^ >]+)?\s+([^>]+)>").unwrap());
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"</?[a-z][^>]*>").unwrap());

/// WebVTT (Teams uses `<v Name>` voice tags; Zoom writes `Name: text`) and SRT. Cues from the same
/// speaker that follow each other closely become one turn.
pub fn parse_cues(text: &str) -> Option<ImportedTranscript> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut turns: Vec<Turn> = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let Some(arrow) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let (from, to) = lines[arrow].split_once("-->")?;
        let start = parse_clock(from.trim());
        let end = parse_clock(to.split_whitespace().next().unwrap_or(""));
        let raw = lines[arrow + 1..].join(" ");
        if raw.trim().is_empty() || start.is_none() {
            continue;
        }
        let (name, words) = if let Some(c) = VOICE.captures(&raw) {
            (Some(c[1].trim().to_string()), TAGS.replace_all(&raw, "").to_string())
        } else {
            let plain = TAGS.replace_all(&raw, "").to_string();
            match NAME_TEXT.captures(&plain).filter(|c| plausible_name(&c["name"])) {
                Some(c) => (Some(c["name"].trim().to_string()), c["text"].to_string()),
                None => (None, plain),
            }
        };
        let speaker = name.as_deref().map(speaker_ref).unwrap_or(SpeakerRef::Unknown);
        let words = words.trim().to_string();
        if let Some(last) = turns.last_mut()
            && last.speaker == speaker
            && last.end_ms.zip(start).is_some_and(|(e, s)| s.saturating_sub(e) < 2_000)
            && last.text.len() < 1_500
        {
            last.text.push(' ');
            last.text.push_str(&words);
            last.end_ms = end.or(last.end_ms);
            continue;
        }
        turns.push(Turn { speaker, start_ms: start, end_ms: end, text: words });
    }
    (!turns.is_empty()).then(|| finish(turns))
}

/// Pull a transcript section out of a Markdown or text document: everything under a heading or
/// line called "Transcript". Returns the transcript and the document without it.
pub fn split_transcript_section(markdown: &str) -> Option<(ImportedTranscript, String)> {
    static HEADING: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?im)^(?:#{1,6}\s*|\*\*)?(?:full\s+|meeting\s+)?transcript(?:ion)?\s*(?:\*\*)?:?\s*$").unwrap());
    let m = HEADING.find(markdown)?;
    let level = markdown[m.start()..].chars().take_while(|c| *c == '#').count();
    let rest = &markdown[m.end()..];
    // The section ends at the next heading of the same or a higher level.
    let end = if level > 0 {
        let pattern = format!(r"(?m)^#{{1,{level}}}\s");
        Regex::new(&pattern).ok().and_then(|r| r.find(rest)).map_or(rest.len(), |n| n.start())
    } else {
        rest.len()
    };
    let transcript = parse_text(&rest[..end]).or_else(|| parse_cues(&rest[..end]))?;
    let body = format!("{}{}", &markdown[..m.start()], &rest[end..]);
    Some((transcript, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(turn: &Turn) -> &str {
        match &turn.speaker {
            SpeakerRef::Named(n) => n,
            _ => "",
        }
    }

    #[test]
    fn otter_style_headers() {
        let t = parse_text("Jane Doe  0:00\nThanks everyone for coming.\n\nAlex Taylor  0:07\nHappy to be here.\nLet's start.\n\nJane Doe  1:02:03\nGreat.").unwrap();
        assert_eq!(t.turns.len(), 3);
        assert_eq!(named(&t.turns[1]), "Alex Taylor");
        assert_eq!(t.turns[1].start_ms, Some(7_000));
        assert_eq!(t.turns[1].text, "Happy to be here. Let's start.");
        assert_eq!(t.turns[0].end_ms, Some(7_000));
        assert_eq!(t.turns[2].start_ms, Some(3_723_000));
        assert!(t.timed);
    }

    #[test]
    fn inline_time_and_name() {
        let t = parse_text("[00:00:01] Sam: Hello\n[00:00:04] Ana: Hi Sam\n[00:00:09] Sam: Shall we?").unwrap();
        assert_eq!(t.turns.len(), 3);
        assert_eq!(named(&t.turns[1]), "Ana");
        assert_eq!(t.turns[2].start_ms, Some(9_000));
        let t = parse_text("Sam (00:01): Hello\nAna (00:04): Hi Sam").unwrap();
        assert_eq!(t.turns[1].start_ms, Some(4_000));
    }

    #[test]
    fn untimed_names_and_jamie_bold() {
        let t = parse_text("Sam: Hello\nAna: Hi\nSam: Bye").unwrap();
        assert!(!t.timed);
        let t = parse_text("**Sarah Johnson**\nHi everyone!\n\n**Alex Chen**\nMorning.").unwrap();
        assert_eq!(named(&t.turns[1]), "Alex Chen");
        assert!(!t.timed);
    }

    #[test]
    fn prose_is_not_a_transcript() {
        assert!(parse_text("We met on Tuesday. Note: budget is tight.\nNext steps are below.\nThanks, all.").is_none());
        assert!(parse_text("# Title\n\nSome paragraph of text.\n\n- a list\n- of things").is_none());
    }

    #[test]
    fn webvtt_with_voices_merges_neighbours() {
        let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\n<v Jane Doe>Hello there.</v>\n\n00:00:02.100 --> 00:00:04.000\n<v Jane Doe>How are you?</v>\n\n00:00:04.500 --> 00:00:06.000\n<v Sam>Fine.</v>\n";
        let t = parse_cues(vtt).unwrap();
        assert_eq!(t.turns.len(), 2);
        assert_eq!(t.turns[0].text, "Hello there. How are you?");
        assert_eq!(t.turns[0].end_ms, Some(4_000));
        assert_eq!(named(&t.turns[1]), "Sam");
    }

    #[test]
    fn srt_and_zoom_names() {
        let srt = "1\n00:00:01,000 --> 00:00:03,000\nSam: Hello\n\n2\n00:00:04,000 --> 00:00:05,500\nAna: Hi\n";
        let t = parse_cues(srt).unwrap();
        assert_eq!(t.turns.len(), 2);
        assert_eq!(t.turns[1].start_ms, Some(4_000));
        assert_eq!(named(&t.turns[1]), "Ana");
    }

    #[test]
    fn transcript_section_is_split_out() {
        let md = "# Sync\n\n## Summary\n\nWe agreed.\n\n## Transcript\n\nSam: Hello\nAna: Hi\nSam: Bye\n\n## Links\n\nnone";
        let (t, body) = split_transcript_section(md).unwrap();
        assert_eq!(t.turns.len(), 3);
        assert!(body.contains("We agreed.") && body.contains("## Links") && !body.contains("Sam: Hello"));
    }
}
