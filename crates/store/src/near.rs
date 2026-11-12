//! Close matches for a search nobody matched: words said in meetings that are a near spelling of
//! the query, or the same words run together or split apart ("road map" for "roadmap").
//! Reads notes, segments and speakers; owns no tables. Counted here, no model.

use std::collections::{HashMap, HashSet};

use margin_domain::{AudioChannel, Id, NearWord, Speaker};
use sqlx::Row;

use crate::codec::{parse_id, parse_opt_id};
use crate::{Db, SELF_LABEL, StoreResult};

/// Meetings looked at, newest first.
const MEETING_LIMIT: i64 = 200;
/// Close matches handed to the palette.
const RESULT_LIMIT: usize = 3;
/// Shorter queries have too many neighbours to be worth suggesting.
const MIN_QUERY: usize = 4;

/// Lowercased letters and digits only, so spacing and punctuation do not count as a difference.
fn fold(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Lowercased words, split on anything that is not a letter, digit or apostrophe.
fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '’'))
        .map(|w| w.trim_matches(['\'', '’']).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Edits between two strings: insert, delete, replace, or swap two neighbours.
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[a.len()][b.len()]
}

/// How many edits a word of this length may be from the query and still be close.
fn allowed(len: usize) -> usize {
    if len >= 7 { 2 } else { 1 }
}

/// How close `phrase` (one or two words) is to the folded query, or None when it is not close.
/// A single word may be a near spelling; two words count only when they are the query split
/// apart, or a near spelling of a two-word query.
pub fn closeness(phrase: &str, query: &str) -> Option<usize> {
    let words = query.split_whitespace().count();
    if words == 0 || words > 2 {
        return None;
    }
    let q = fold(query);
    let p = fold(phrase);
    let two = phrase.contains(' ');
    // The query itself: full-text search already looked for it.
    if phrase == query.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join(" ") {
        return None;
    }
    if p == q {
        return Some(0);
    }
    if two && words == 1 {
        return None;
    }
    let max = allowed(q.chars().count());
    if p.chars().count().abs_diff(q.chars().count()) > max {
        return None;
    }
    let d = distance(&p, &q);
    (d <= max).then_some(d)
}

#[derive(Default)]
struct Tally {
    closeness: usize,
    count: u32,
    notes: HashSet<Id>,
    /// The newest meeting first: rows arrive newest first.
    first: Option<(Id, String, u32)>,
    /// Who said it, by meeting: (note, speaker, channel).
    voices: Vec<(Id, Option<Id>, AudioChannel)>,
}

impl Db {
    /// Up to three things said in recent meetings that are close to `query`, closest first.
    pub async fn near_words(&self, query: &str) -> StoreResult<Vec<NearWord>> {
        let words = query.split_whitespace().count();
        if fold(query).chars().count() < MIN_QUERY || words > 2 {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT s.note_id, s.speaker_id, s.channel, s.start_ms, s.text, n.title FROM segments s \
             JOIN (SELECT id, title, COALESCE(started_at, created_at) AS held FROM notes \
                   WHERE deleted_at IS NULL AND kind = 'meeting' ORDER BY held DESC LIMIT ?) n ON n.id = s.note_id \
             WHERE s.is_final = 1 ORDER BY n.held DESC, s.start_ms",
        )
        .bind(MEETING_LIMIT)
        .fetch_all(self.pool())
        .await?;

        let mut found: HashMap<String, Tally> = HashMap::new();
        for row in &rows {
            let text: String = row.try_get("text")?;
            let toks = tokens(&text);
            let mut seen = |phrase: String, close: usize| -> StoreResult<()> {
                let note_id = parse_id(&row.try_get::<String, _>("note_id")?)?;
                let speaker = parse_opt_id(row.try_get("speaker_id")?)?;
                let channel = if row.try_get::<String, _>("channel")? == "mic" { AudioChannel::Mic } else { AudioChannel::System };
                let at: i64 = row.try_get("start_ms")?;
                let t = found.entry(phrase).or_default();
                t.closeness = close;
                t.count += 1;
                t.notes.insert(note_id);
                if t.first.is_none() {
                    t.first = Some((note_id, row.try_get("title")?, u32::try_from(at.max(0)).unwrap_or(u32::MAX)));
                }
                t.voices.push((note_id, speaker, channel));
                Ok(())
            };
            for (i, w) in toks.iter().enumerate() {
                if let Some(c) = closeness(w, query) {
                    seen(w.clone(), c)?;
                }
                if let Some(next) = toks.get(i + 1) {
                    let pair = format!("{w} {next}");
                    if let Some(c) = closeness(&pair, query) {
                        seen(pair, c)?;
                    }
                }
            }
        }

        let mut ranked: Vec<(String, Tally)> = found.into_iter().collect();
        ranked.sort_by(|(pa, a), (pb, b)| a.closeness.cmp(&b.closeness).then(b.count.cmp(&a.count)).then(pa.cmp(pb)));
        ranked.truncate(RESULT_LIMIT);

        let mut people: HashMap<Id, (Vec<Speaker>, HashMap<Id, String>)> = HashMap::new();
        let mut out = Vec::with_capacity(ranked.len());
        for (phrase, t) in ranked {
            let Some((note_id, note_title, at_ms)) = t.first else { continue };
            // Who said it most; the unnamed "Others" channel names nobody.
            let mut by: HashMap<(String, bool), u32> = HashMap::new();
            for (note, speaker, channel) in &t.voices {
                if !people.contains_key(note) {
                    let speakers = self.list_speakers(*note).await?;
                    let names = self.speaker_display_names(&speakers).await?;
                    people.insert(*note, (speakers, names));
                }
                let (speakers, names) = &people[note];
                let sp = speaker.and_then(|id| speakers.iter().find(|s| s.id == id));
                let mine = sp.map_or(*channel == AudioChannel::Mic, |s| s.is_self);
                let name = if mine { Some(SELF_LABEL.to_string()) } else { sp.and_then(|s| names.get(&s.id).cloned()) };
                if let Some(name) = name {
                    *by.entry((name, mine)).or_default() += 1;
                }
            }
            let top = by.into_iter().max_by(|(ka, a), (kb, b)| a.cmp(b).then(kb.0.cmp(&ka.0)));
            out.push(NearWord {
                phrase,
                count: t.count,
                meetings: t.notes.len() as u32,
                is_self: top.as_ref().is_some_and(|((_, mine), _)| *mine),
                speaker: top.map(|((name, _), _)| name),
                note_id,
                note_title,
                at_ms,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use margin_domain::{NoteKind, Segment, new_id, now};

    use crate::NewNote;

    #[test]
    fn close_means_spelling_or_spacing() {
        assert_eq!(closeness("road map", "roadmap"), Some(0));
        assert_eq!(closeness("roadmap", "road map"), Some(0));
        assert_eq!(closeness("roadmaps", "roadmap"), Some(1));
        assert_eq!(closeness("onboarding", "onbaording"), Some(1));
        assert_eq!(closeness("pricing page", "pricng page"), Some(1));
        // The query itself, unrelated words, and pairs around a one-word query are not close.
        assert_eq!(closeness("roadmap", "Roadmap"), None);
        assert_eq!(closeness("budget", "roadmap"), None);
        assert_eq!(closeness("the roadmap", "roadmap"), None);
        assert_eq!(closeness("plan", "plans for the week"), None);
        assert_eq!(distance("abcd", "abdc"), 1);
    }

    #[tokio::test]
    async fn finds_who_said_the_near_word() {
        let db = Db::open_in_memory().await.unwrap();
        let note = db
            .create_note(NewNote { title: "Q2 planning".into(), kind: NoteKind::Meeting, started_at: Some(now()), ..NewNote::default() })
            .await
            .unwrap();
        let (_, others) = db.ensure_channel_speakers(note.id).await.unwrap();
        db.rename_speaker(others.id, "Priya").await.unwrap();
        for (i, t) in ["The road map is late.", "Fix the road map first.", "Roadmaps are hard."].iter().enumerate() {
            let start = i as u32 * 3000;
            let seg = Segment {
                id: new_id(),
                note_id: note.id,
                speaker_id: Some(others.id),
                channel: AudioChannel::System,
                start_ms: start,
                end_ms: start + 2000,
                text: (*t).into(),
                is_final: true,
            };
            db.upsert_segment(&seg).await.unwrap();
        }

        let near = db.near_words("roadmap").await.unwrap();
        assert_eq!(near.len(), 2);
        assert_eq!((near[0].phrase.as_str(), near[0].count, near[0].meetings), ("road map", 2, 1));
        assert_eq!(near[0].speaker.as_deref(), Some("Priya"));
        assert!(!near[0].is_self);
        assert_eq!((near[0].note_title.as_str(), near[0].at_ms), ("Q2 planning", 0));
        assert_eq!(near[1].phrase, "roadmaps");

        assert!(db.near_words("map").await.unwrap().is_empty());
        assert!(db.near_words("zebra crossing").await.unwrap().is_empty());
    }
}
