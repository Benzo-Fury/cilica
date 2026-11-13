//! Home's facts from written notes, for people who write more than they record: sentences that
//! read like a thought, the hours you write in, and questions you left yourself.
//! Reads notes and note_versions; owns no tables. Plain rules, no model.

use cilica_domain::chrono::{TimeZone, Timelike};
use std::collections::HashMap;

use cilica_domain::{Author, BareNote, Ghostwriter, Id, NoteLine, SpellFix, Timestamp, WritingHours, chrono, now};
use sqlx::Row;

use crate::codec::{parse_id, parse_ts};
use crate::{Db, StoreResult};

/// Written notes looked at, most recently changed first.
const NOTE_LIMIT: i64 = 200;
/// Lines handed to Home; it shows one, or a few questions, at a time.
const LINE_LIMIT: usize = 12;
/// Fewer saves than this say nothing about when you write.
const MIN_SAVES: u32 = 6;
/// A sentence shorter or longer than this is a fragment or a paragraph, not a line to quote.
const MIN_WORDS: usize = 5;
const MAX_WORDS: usize = 30;

/// Phrases that mark a sentence as someone thinking aloud or deciding. Each one found adds a point.
const CUES: &[&str] = &[
    "i think",
    "i feel",
    "i like",
    "i love",
    "i hate",
    "i want",
    "i wonder",
    "i believe",
    "i'd",
    "i'm sure",
    "we should",
    "we need",
    "we decided",
    "decided",
    "the problem",
    "the real",
    "the point",
    "the trick",
    "what if",
    "maybe",
    "probably",
    "should",
    "must",
    "never",
    "always",
    "because",
    "turns out",
    "honestly",
    "the best",
    "the worst",
    "matters",
];

/// A Markdown line as plain text: list, heading and quote markers, emphasis and links removed.
/// None for lines that are not prose (checkboxes, code, tables, links on their own).
fn plain(line: &str) -> Option<String> {
    let mut t = line.trim();
    if t.is_empty() || t.starts_with("```") || t.starts_with('|') || t.starts_with("- [") || t.starts_with("* [") || t.contains("://") {
        return None;
    }
    t = t.trim_start_matches(['#', '>', ' ']);
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(marker) {
            t = rest;
        }
    }
    if let Some((num, rest)) = t.split_once(". ")
        && !num.is_empty()
        && num.chars().all(|c| c.is_ascii_digit())
    {
        t = rest;
    }
    // [text](target) keeps its text.
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find("](").and_then(|close| after[close..].find(')').map(|end| (close, close + end))) {
            Some((close, end)) => {
                out.push_str(&after[..close]);
                rest = &after[end + 1..];
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    // Inline code is not prose: drop it whole. Underscores go only as emphasis, not inside words.
    let out: String = out.split('`').step_by(2).collect();
    let chars: Vec<char> = out.chars().collect();
    let text: String = chars
        .iter()
        .enumerate()
        .filter(|&(i, &c)| match c {
            '*' => false,
            '_' => {
                let inside = |j: Option<usize>| j.and_then(|j| chars.get(j)).is_some_and(|n| n.is_alphanumeric());
                inside(i.checked_sub(1)) && inside(Some(i + 1))
            }
            _ => true,
        })
        .map(|(_, &c)| c)
        .collect();
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The prose lines of a note body, as plain text, skipping fenced code blocks.
fn prose(body: &str) -> Vec<String> {
    let mut fenced = false;
    let mut out = Vec::new();
    for line in body.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if !fenced && let Some(text) = plain(line) {
            out.push(text);
        }
    }
    out
}

/// Sentences of a line, each with the punctuation that ended it.
fn sentences(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    for (i, &(at, c)) in chars.iter().enumerate() {
        let ends = matches!(c, '.' | '!' | '?') && chars.get(i + 1).is_none_or(|(_, n)| n.is_whitespace());
        if ends {
            let end = at + c.len_utf8();
            out.push(line[start..end].trim().to_string());
            start = end;
        }
    }
    let tail = line[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Sentences worth quoting from a note body, with how strongly they read as a thought and who
/// said them: None for your own words, a name when the note quotes someone else.
pub fn own_lines(body: &str) -> Vec<(String, u32, Option<String>)> {
    let lines = prose(body);
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        // A byline only names the speaker of the line above it
        if Attribution::byline(line).is_some() {
            continue;
        }
        // Who said it: quoted in the line, named on the line below, or you
        let (text, said_by) = match Attribution::parse(line) {
            Some(quoted) => (quoted.quote, Some(quoted.who)),
            None => match lines.get(i + 1).and_then(|next| Attribution::byline(next)) {
                Some(who) => (unquote(line).to_string(), Some(who)),
                None => (line.clone(), None),
            },
        };
        // Score each sentence by the cues it contains
        for s in sentences(&text) {
            let words = s.split_whitespace().count();
            if s.ends_with('?') || !(MIN_WORDS..=MAX_WORDS).contains(&words) {
                continue;
            }
            // Whole words only: "should" is not in "shoulder".
            let words: String = s.to_lowercase().replace('’', "'").chars().map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' }).collect();
            let padded = format!(" {} ", words.split_whitespace().collect::<Vec<_>>().join(" "));
            let score = CUES.iter().filter(|cue| padded.contains(&format!(" {cue} "))).count() as u32;
            if score > 0 {
                out.push((s, score, said_by.clone()));
            }
        }
    }
    out
}

/// Questions in a note body: sentences that end with a question mark and say enough to stand alone.
pub fn questions(body: &str) -> Vec<String> {
    prose(body).iter().flat_map(|line| sentences(line)).filter(|s| s.ends_with('?') && s.split_whitespace().count() >= 3).collect()
}

/// Saves per local hour of the day, or None with too few saves to say anything.
pub fn writing_hours<Tz: TimeZone>(times: &[Timestamp], tz: &Tz) -> Option<WritingHours> {
    let saves = times.len() as u32;
    if saves < MIN_SAVES {
        return None;
    }
    let mut hours = vec![0u32; 24];
    for t in times {
        hours[t.with_timezone(tz).hour() as usize] += 1;
    }
    // Ties go to the later hour: evenings are the more telling answer.
    let peak_hour = (0..24).rev().max_by_key(|&h| hours[h]).unwrap_or(0) as u8;
    Some(WritingHours { hours, saves, peak_hour })
}

// ---------- Attribution ---------- //

/// Opening and closing quote marks, straight and curly.
const OPEN_QUOTES: &[char] = &['"', '“'];
const CLOSE_QUOTES: &[char] = &['"', '”'];
/// Dashes that introduce a byline. A plain hyphen only counts doubled (`-- Sam`), since a single
/// one is a list marker or part of the sentence.
const BYLINE_DASHES: &[&str] = &["—", "–", "--"];
/// Words that end a speaker label but are not names (`Note: "…"`), and pronouns that mean you.
const NOT_NAMES: &[&str] = &["i", "me", "you", "we", "us", "note", "notes", "quote", "idea", "decision", "update", "todo", "q", "a", "re"];

/// A line in a note that quotes someone else: what they said and who said it.
///
/// Recognised shapes, all on one plain line: `"…" — Sam`, `"…," said Sam`, `"…," Sam said`,
/// `Sam said "…"`, `Sam: "…"`, and an unquoted sentence ending ` — Sam`. A quote followed by a
/// line that is only a byline (`— Sam`) is handled by `own_lines` through `byline`.
#[derive(Debug, PartialEq)]
struct Attribution {
    /// The quoted words, without quote marks or the comma before "said".
    quote: String,
    who: String,
}

impl Attribution {
    /// The quote and its speaker in one line, or None when the line quotes nobody (or quotes you).
    fn parse(line: &str) -> Option<Self> {
        // A quoted span: the first opening mark to the last closing mark after it
        if let Some(open) = line.find(OPEN_QUOTES) {
            let inner_start = open + line[open..].chars().next()?.len_utf8();
            let close = inner_start + line[inner_start..].rfind(CLOSE_QUOTES)?;
            let quote = line[inner_start..close].trim().trim_end_matches(',').trim();
            let before = line[..open].trim();
            let after = line[close + line[close..].chars().next()?.len_utf8()..].trim();
            let who = Self::speaker_after(after).or_else(|| Self::speaker_before(before))?;
            return (!quote.is_empty()).then(|| Self { quote: quote.to_string(), who });
        }
        // No quote marks: a sentence signed off with a dash
        BYLINE_DASHES.iter().filter(|d| **d != "--").find_map(|dash| {
            let (quote, who) = line.rsplit_once(&format!(" {dash} "))?;
            let who = Self::name(who)?;
            Some(Self { quote: quote.trim().to_string(), who })
        })
    }

    /// A line that is only a byline, `— Sam` or `-- Sam Lee`: the name it gives.
    fn byline(line: &str) -> Option<String> {
        BYLINE_DASHES.iter().find_map(|dash| line.trim().strip_prefix(dash)).and_then(Self::name)
    }

    /// The speaker named after a closing quote: `— Sam`, `, said Sam.`, `Sam said`.
    fn speaker_after(after: &str) -> Option<String> {
        // A dash byline right after the quote
        if let Some(who) = BYLINE_DASHES.iter().find_map(|dash| after.strip_prefix(dash)) {
            return Self::name(who);
        }
        // "said Sam" or "Sam said", either verb tense
        let rest = after.trim_start_matches([',', ' ']).trim_end_matches(['.', '!']).trim();
        Self::said(rest)
    }

    /// The speaker named before an opening quote: `Sam:`, `Sam said`, `Sam said:`.
    fn speaker_before(before: &str) -> Option<String> {
        let labelled = before.trim_end().ends_with(':');
        let rest = before.trim_end().trim_end_matches([':', ',']).trim_end();
        // "Sam said" before the quote, then a bare speaker label "Sam:"
        Self::said(rest).or_else(|| if labelled { Self::name(rest) } else { None })
    }

    /// `said Sam`, `Sam said`, `says Sam` or `Sam says`: the name either side of the verb.
    fn said(text: &str) -> Option<String> {
        ["said", "says"].iter().find_map(|verb| {
            text.strip_prefix(&format!("{verb} ")).or_else(|| text.strip_suffix(&format!(" {verb}"))).and_then(Self::name)
        })
    }

    /// One to three capitalised words that read as a person's name, not you and not a label.
    fn name(text: &str) -> Option<String> {
        let name = text.trim().trim_end_matches(['.', ',', ':', '!']).trim();
        let words: Vec<&str> = name.split_whitespace().collect();
        // Shape: a few words, each starting with a capital, letters and joiners only
        let shaped = (1..=3).contains(&words.len())
            && words.iter().all(|w| {
                w.chars().next().is_some_and(char::is_uppercase) && w.chars().all(|c| c.is_alphabetic() || matches!(c, '\'' | '’' | '-' | '.'))
            });
        // Not a pronoun for you, nor a label like "Note"
        let named = !(words.len() == 1 && NOT_NAMES.contains(&name.to_lowercase().as_str()));
        (shaped && named).then(|| name.to_string())
    }
}

/// A line with its surrounding quote marks removed.
fn unquote(line: &str) -> &str {
    line.trim().trim_start_matches(OPEN_QUOTES).trim_end_matches(CLOSE_QUOTES).trim()
}

// ---------- Bare notes, cut-off notes, spelling, ghostwriting ---------- //

/// A note this new may still be about to get its body.
const BARE_MIN_AGE: chrono::TimeDelta = chrono::TimeDelta::days(1);
/// Fewer cut-off notes than this is a coincidence.
const MIN_MID_SENTENCE: u32 = 2;
/// The last line needs this many words before a missing full stop means anything.
const MID_SENTENCE_MIN_WORDS: usize = 4;
/// Fewer fixes than this is a typo, not a habit.
const MIN_SPELL_FIXES: u32 = 2;
/// Words written before a share of them means anything.
const MIN_GHOST_WORDS: u32 = 200;
/// Cilica's share below this is not worth a face.
const MIN_GHOST_PCT: u8 = 5;

/// Famous misspellings, as (right, wrong forms). Only these count, so names and jargon never do.
const SPELLINGS: &[(&str, &[&str])] = &[
    ("definitely", &["definately", "definatly", "definetly", "definitly"]),
    ("separate", &["seperate"]),
    ("receive", &["recieve"]),
    ("necessary", &["neccessary", "necessery", "neccesary"]),
    ("accommodate", &["accomodate", "acommodate"]),
    ("calendar", &["calender"]),
    ("tomorrow", &["tommorow", "tomorow", "tommorrow"]),
    ("until", &["untill"]),
    ("weird", &["wierd"]),
    ("beginning", &["begining"]),
    ("believe", &["beleive"]),
    ("occurred", &["occured"]),
    ("embarrass", &["embarass"]),
    ("environment", &["enviroment"]),
    ("business", &["buisness"]),
    ("address", &["adress"]),
    ("schedule", &["schedual"]),
    ("restaurant", &["restaraunt", "resturant"]),
    ("committee", &["commitee"]),
    ("publicly", &["publically"]),
];

/// Whether a body's last line is prose that stops mid-sentence: no closing punctuation after a
/// line long enough to have needed one. Lists, headings, checkboxes and code never count.
pub fn ends_mid_sentence(body: &str) -> bool {
    let Some(last) = body.lines().map(str::trim).rfind(|l| !l.is_empty()) else { return false };
    let structural = ["#", "-", "*", "+", ">", "|", "```", "~~~"].iter().any(|m| last.starts_with(m))
        || last.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    if structural || last.split_whitespace().count() < MID_SENTENCE_MIN_WORDS {
        return false;
    }
    last.chars().last().is_some_and(|c| c.is_alphanumeric())
}

/// Whole-word counts of each misspelling list entry in one body: (right, wrong) → (wrong count, right count).
fn spelling_counts(body: &str) -> HashMap<(&'static str, &'static str), (u32, u32)> {
    let words: Vec<String> = body.to_lowercase().split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).map(str::to_string).collect();
    let count = |w: &str| words.iter().filter(|x| x.as_str() == w).count() as u32;
    let mut out = HashMap::new();
    for (right, wrongs) in SPELLINGS {
        let r = count(right);
        for wrong in *wrongs {
            let w = count(wrong);
            if w > 0 || r > 0 {
                out.insert((*right, *wrong), (w, r));
            }
        }
    }
    out
}

/// Fixes across consecutive saves of one note, oldest first: a misspelling that went down while
/// the right spelling went up. Adds into `into`.
pub fn spell_fixes(saves: &[&str], into: &mut HashMap<(&'static str, &'static str), u32>) {
    let counted: Vec<_> = saves.iter().map(|b| spelling_counts(b)).collect();
    for pair in counted.windows(2) {
        for (key, (wrong_before, right_before)) in &pair[0] {
            let (wrong_after, right_after) = pair[1].get(key).copied().unwrap_or((0, 0));
            let fixed = wrong_before.saturating_sub(wrong_after).min(right_after.saturating_sub(*right_before));
            if fixed > 0 {
                *into.entry(*key).or_default() += fixed;
            }
        }
    }
}

/// Words added by each side across one note's saves, oldest first: (author of the change, body
/// after it). The first body is yours: a written note starts with you.
pub fn added_words(saves: &[(Author, &str)]) -> (u32, u32) {
    let count = |b: &str| b.split_whitespace().count() as u32;
    let (mut you, mut margin) = (saves.first().map(|(_, b)| count(b)).unwrap_or(0), 0);
    for pair in saves.windows(2) {
        let grew = count(pair[1].1).saturating_sub(count(pair[0].1));
        match pair[1].0 {
            Author::Ai => margin += grew,
            Author::User => you += grew,
        }
    }
    (you, margin)
}

// ---------- Glance ---------- //

pub(crate) struct Written {
    pub own_lines: Vec<NoteLine>,
    pub writing: Option<WritingHours>,
    pub questions: Vec<NoteLine>,
    pub bare_note: Option<BareNote>,
    pub mid_sentence: Option<u32>,
    pub spelling: Option<SpellFix>,
    pub ghostwriter: Option<Ghostwriter>,
}

impl Db {
    /// What Home can say from written notes alone.
    pub(crate) async fn written_glance(&self) -> StoreResult<Written> {
        let rows = sqlx::query(
            "SELECT id, title, body_md, created_at, updated_at FROM notes \
             WHERE deleted_at IS NULL AND kind = 'manual' ORDER BY updated_at DESC LIMIT ?",
        )
        .bind(NOTE_LIMIT)
        .fetch_all(self.pool())
        .await?;

        let mut ranked: Vec<(u32, usize, NoteLine)> = Vec::new();
        let mut asked: Vec<NoteLine> = Vec::new();
        let mut times: Vec<Timestamp> = Vec::new();
        let mut bare_note: Option<BareNote> = None;
        let mut cut_off = 0u32;
        let mut current: HashMap<Id, String> = HashMap::new();
        for (order, row) in rows.iter().enumerate() {
            let note_id = parse_id(&row.try_get::<String, _>("id")?)?;
            let note_title: String = row.try_get("title")?;
            let body: String = row.try_get("body_md")?;
            let written_at = parse_ts(&row.try_get::<String, _>("updated_at")?)?;
            let created = parse_ts(&row.try_get::<String, _>("created_at")?)?;
            times.push(created);
            if written_at > created {
                times.push(written_at);
            }

            // A title with nothing under it, oldest first; and notes that stop mid-sentence
            if body.trim().is_empty() && !note_title.trim().is_empty() && now() - created >= BARE_MIN_AGE && bare_note.as_ref().is_none_or(|b| created < b.created_at) {
                bare_note = Some(BareNote { note_id, title: note_title.trim().to_string(), created_at: created });
            }
            if ends_mid_sentence(&body) {
                cut_off += 1;
            }
            current.insert(note_id, body.clone());
            let line = |text: String, said_by: Option<String>| NoteLine { note_id, note_title: note_title.clone(), written_at, text, said_by };
            for (text, score, said_by) in own_lines(&body) {
                ranked.push((score, order, line(text, said_by)));
            }
            for text in questions(&body) {
                asked.push(line(text, None));
            }
        }
        // Every saved version, oldest first per note. A version holds the body as it was before a
        // write, tagged with who made that write; the note's current body comes after the last.
        let mut saves: HashMap<Id, Vec<(Author, String)>> = HashMap::new();
        for row in sqlx::query(
            "SELECT v.note_id, v.body_md, v.author, v.created_at FROM note_versions v JOIN notes n ON n.id = v.note_id \
             WHERE n.kind = 'manual' AND n.deleted_at IS NULL ORDER BY v.note_id, v.created_at ASC, v.id ASC",
        )
        .fetch_all(self.pool())
        .await?
        {
            let author = if row.try_get::<String, _>("author")? == "ai" { Author::Ai } else { Author::User };
            // Your own saves in between count for the writing clock (Cilica's edits are not your writing).
            if author == Author::User {
                times.push(parse_ts(&row.try_get::<String, _>("created_at")?)?);
            }
            saves.entry(parse_id(&row.try_get::<String, _>("note_id")?)?).or_default().push((author, row.try_get("body_md")?));
        }

        // Spelling fixes and who wrote what, walking each note's saves in order
        let mut fixes: HashMap<(&'static str, &'static str), u32> = HashMap::new();
        let (mut by_you, mut by_cilica) = (0u32, 0u32);
        for (note_id, body) in &current {
            let versions = saves.remove(note_id).unwrap_or_default();
            // Each change belongs to the author on the version it replaced; the first body is yours.
            let mut chain: Vec<(Author, &str)> = Vec::with_capacity(versions.len() + 1);
            for (i, (_, b)) in versions.iter().enumerate() {
                chain.push((if i == 0 { Author::User } else { versions[i - 1].0 }, b.as_str()));
            }
            chain.push((versions.last().map(|v| v.0).unwrap_or(Author::User), body.as_str()));
            let bodies: Vec<&str> = chain.iter().map(|(_, b)| *b).collect();
            spell_fixes(&bodies, &mut fixes);
            let (you, margin) = added_words(&chain);
            by_you += you;
            by_cilica += margin;
        }
        let spelling = fixes
            .into_iter()
            .filter(|(_, n)| *n >= MIN_SPELL_FIXES)
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
            .map(|((right, wrong), count)| SpellFix { word: right.to_string(), wrong: wrong.to_string(), count });
        let total = by_you + by_cilica;
        let ghostwriter = (total >= MIN_GHOST_WORDS)
            .then(|| (by_cilica as u64 * 100 / total as u64) as u8)
            .filter(|pct| *pct >= MIN_GHOST_PCT)
            .map(|cilica_pct| Ghostwriter { cilica_pct, words: total });

        // Strongest first, newer notes breaking ties.
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let own_lines = ranked.into_iter().map(|(_, _, l)| l).take(LINE_LIMIT).collect();
        asked.truncate(LINE_LIMIT);
        Ok(Written {
            own_lines,
            writing: writing_hours(&times, &chrono::Local),
            questions: asked,
            bare_note,
            mid_sentence: (cut_off >= MIN_MID_SENTENCE).then_some(cut_off),
            spelling,
            ghostwriter,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cilica_domain::chrono::{FixedOffset, TimeDelta};
    use cilica_domain::{NoteKind, now};

    use crate::NewNote;

    #[test]
    fn markdown_becomes_plain_prose() {
        assert_eq!(plain("## I think **this** matters").as_deref(), Some("I think this matters"));
        assert_eq!(plain("- see [the doc](notes/a.md) first").as_deref(), Some("see the doc first"));
        assert_eq!(plain("1. Pricing is the real problem").as_deref(), Some("Pricing is the real problem"));
        assert_eq!(plain("- [ ] Send Sam the plan"), None);
        assert_eq!(plain("Run `WHERE notes_fts MATCH ?` first").as_deref(), Some("Run  first"));
        assert_eq!(plain("_really_ keep snake_case").as_deref(), Some("really keep snake_case"));
        assert_eq!(plain("https://example.com"), None);
    }

    #[test]
    fn picks_thoughts_and_questions() {
        let body = "# Pricing\n\nI think the onboarding problem is really a pricing problem. Ship it.\n\
                    - Do we need a free tier at all?\n- Why?\n- [ ] What about annual?\n\nGroceries, milk, eggs.";
        let lines = own_lines(body);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, "I think the onboarding problem is really a pricing problem.");
        assert_eq!(lines[0].1, 1);
        assert_eq!(lines[0].2, None);
        assert!(own_lines("My shoulder hurts after the long flight home.").is_empty());
        assert_eq!(questions(body), vec!["Do we need a free tier at all?".to_string()]);
        assert!(questions("```sql\nSELECT * FROM notes WHERE id = ?\n```").is_empty());
    }

    #[test]
    fn quotes_keep_their_speaker() {
        let said_by = |body: &str| own_lines(body).into_iter().map(|(text, _, who)| (text, who)).collect::<Vec<_>>();
        let sam = |text: &str| vec![(text.to_string(), Some("Sam".to_string()))];
        // Quoted in the line, every common shape
        assert_eq!(said_by("\"We should ship before the pricing change.\" — Sam"), sam("We should ship before the pricing change."));
        assert_eq!(said_by("“We should ship before the pricing change,” said Sam."), sam("We should ship before the pricing change"));
        assert_eq!(said_by("\"We should ship before the pricing change,\" Sam said."), sam("We should ship before the pricing change"));
        assert_eq!(said_by("Sam said \"we should ship before the pricing change.\""), sam("we should ship before the pricing change."));
        assert_eq!(said_by("- Sam: \"We should ship before the pricing change.\""), sam("We should ship before the pricing change."));
        assert_eq!(said_by("We should ship before the pricing change. — Sam"), sam("We should ship before the pricing change."));
        // A blockquote signed on the line below
        assert_eq!(said_by("> We should ship before the pricing change.\n> — Sam Lee"), vec![(
            "We should ship before the pricing change.".to_string(),
            Some("Sam Lee".to_string())
        )]);
        // Still yours: no speaker, a label, or a quote of yourself
        let mine = |body: &str| said_by(body).into_iter().all(|(_, who)| who.is_none());
        assert!(mine("I think \"good enough\" should be the bar for v1."));
        assert!(mine("Note: \"we should ship before the pricing change.\""));
        assert!(mine("\"We should ship before the pricing change,\" I said."));
        assert!(mine("We should ship before the pricing change - the team agrees."));
    }

    #[test]
    fn spots_notes_that_stop_mid_sentence() {
        assert!(ends_mid_sentence("Intro.\n\nWe should really think about the"));
        assert!(!ends_mid_sentence("We should really think about it."));
        assert!(!ends_mid_sentence("- milk\n- eggs and some bread"));
        assert!(!ends_mid_sentence("## Plans for the next quarter"));
        assert!(!ends_mid_sentence("ok so"));
        assert!(!ends_mid_sentence(""));
    }

    #[test]
    fn counts_spelling_fixes_between_saves() {
        let mut fixes = HashMap::new();
        spell_fixes(&["I definately will.", "I definitely will.", "Definately, definately.", "Definitely, definitely."], &mut fixes);
        assert_eq!(fixes.get(&("definitely", "definately")), Some(&3));
        // Deleting a misspelt word is not fixing it.
        let mut none = HashMap::new();
        spell_fixes(&["Seperate it.", "Done."], &mut none);
        assert!(none.is_empty());
    }

    #[test]
    fn splits_written_words_by_author() {
        // You write 4 words, Cilica adds 6, you add 2.
        let chain = [(Author::User, "one two three four"), (Author::Ai, "one two three four a b c d e f"), (Author::User, "one two three four a b c d e f g h")];
        assert_eq!(added_words(&chain), (6, 6));
    }

    #[test]
    fn hours_follow_the_local_clock() {
        let base = "2026-11-10T20:30:00Z".parse::<Timestamp>().unwrap();
        let times: Vec<Timestamp> = (0..6).map(|i| base + TimeDelta::minutes(i * 5)).collect();
        // 20:30 UTC is 21:30 one hour east.
        let w = writing_hours(&times, &FixedOffset::east_opt(3600).unwrap()).unwrap();
        assert_eq!((w.peak_hour, w.saves, w.hours.len(), w.hours[21]), (21, 6, 24, 6));
        assert!(writing_hours(&times[..3], &FixedOffset::east_opt(0).unwrap()).is_none());
    }

    #[tokio::test]
    async fn glance_reads_written_notes_only() {
        let db = Db::open_in_memory().await.unwrap();
        let note = db.create_note(NewNote { title: "Product thoughts".into(), ..NewNote::default() }).await.unwrap();
        db.set_note_body(note.id, "We should stop adding settings. Is anyone using the export?", cilica_domain::Author::User, None).await.unwrap();
        let meeting =
            db.create_note(NewNote { title: "Sync".into(), kind: NoteKind::Meeting, started_at: Some(now()), ..NewNote::default() }).await.unwrap();
        db.set_note_body(meeting.id, "I think Cilica wrote this summary.", cilica_domain::Author::Ai, None).await.unwrap();

        let w = db.written_glance().await.unwrap();
        assert_eq!(w.own_lines.len(), 1);
        assert_eq!(w.own_lines[0].text, "We should stop adding settings.");
        assert_eq!(w.own_lines[0].note_title, "Product thoughts");
        assert_eq!(w.own_lines[0].said_by, None);
        assert_eq!(w.questions[0].text, "Is anyone using the export?");
        // Two notes created and one changed: too few saves for a clock.
        assert!(w.writing.is_none());
    }

    #[tokio::test]
    async fn glance_finds_bare_cut_off_and_ghostwritten_notes() {
        let db = Db::open_in_memory().await.unwrap();
        // A bare note only counts once it is a day old.
        let bare = db.create_note(NewNote { title: "Big idea".into(), ..NewNote::default() }).await.unwrap();
        sqlx::query("UPDATE notes SET created_at = ? WHERE id = ?")
            .bind((now() - TimeDelta::days(41)).to_rfc3339())
            .bind(bare.id.to_string())
            .execute(db.pool())
            .await
            .unwrap();
        for t in ["One", "Two"] {
            let n = db.create_note(NewNote { title: t.into(), ..NewNote::default() }).await.unwrap();
            db.set_note_body(n.id, "We should really think about the", cilica_domain::Author::User, None).await.unwrap();
        }
        // You write 300 words, then Cilica adds 100.
        let ghost = db.create_note(NewNote { title: "Plan".into(), ..NewNote::default() }).await.unwrap();
        let yours = format!("{}.", vec!["word"; 300].join(" "));
        db.set_note_body(ghost.id, &yours, cilica_domain::Author::User, None).await.unwrap();
        db.set_note_body(ghost.id, &format!("{yours} {}.", vec!["more"; 100].join(" ")), cilica_domain::Author::Ai, None).await.unwrap();

        let w = db.written_glance().await.unwrap();
        assert_eq!(w.bare_note.map(|b| b.title), Some("Big idea".to_string()));
        assert_eq!(w.mid_sentence, Some(2));
        // Cilica's 100 words against your 300 here and 12 in the cut-off notes.
        let g = w.ghostwriter.unwrap();
        assert_eq!((g.cilica_pct, g.words), (24, 412));
        assert!(w.spelling.is_none());
    }
}
