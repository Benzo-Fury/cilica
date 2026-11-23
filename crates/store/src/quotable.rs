//! Turning a sentence into a line worth quoting: the strongest clause of it, cut where an aside,
//! a hedge or a second thought begins, never where the meaning turns. Home uses it for lines from
//! written notes (`pick`) and for the moments Cilica lifted from meetings (`tighten`).
//! Every result is the speaker's own words, a contiguous piece of the sentence: the only edits
//! are a dropped spoken filler at the start, a capital letter and a closing full stop.
//! Plain rules, no model.

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

/// More cues than this stop counting: a ramble full of "maybe" and "probably" is not a better line.
const CUE_CAP: u32 = 2;
/// A line shorter or longer than this is a fragment or a paragraph, not a line to quote.
pub(crate) const MIN_WORDS: usize = 5;
pub(crate) const MAX_WORDS: usize = 30;
/// Sentences up to this long are still looked at, in case one clause of them is a line.
pub(crate) const LONGEST_SENTENCE: usize = 80;
/// Spoken quotes longer than this get cut to their strongest clause; shorter ones are left alone.
const SPOKEN_WORDS: usize = 20;

/// What may follow a comma and be dropped with it: hedges, padding and reasons that soften or
/// explain a claim without changing it. Anything else after a comma ("but", "unless", "which",
/// "so", "maybe") can turn the meaning, so the sentence is never cut there.
const SOFT_TAILS: &[&str] = &[
    "if possible",
    "where possible",
    "wherever possible",
    "if we can",
    "if you can",
    "ideally",
    "i think",
    "i guess",
    "i suppose",
    "i feel like",
    "you know",
    "i mean",
    "or something",
    "or whatever",
    "and so on",
    "and stuff",
    "etc",
    "eg",
    "e.g.",
    "i.e.",
    "ie",
    "for example",
    "for instance",
    "because",
    "basically",
    "obviously",
    "honestly",
    "really",
    "right",
];

/// Dashes that start an aside when they stand between spaces.
const DASHES: &[&str] = &[" - ", " – ", " — ", " -- "];

/// Shorthand that marks a line as notes to self rather than something said.
const SHORTHAND: &[&str] = &["eg", "e.g.", "ie", "i.e.", "etc", "etc."];

/// Words that open a line with conviction.
const ABSOLUTES: &[&str] = &["absolutely", "everything", "nothing", "never", "always", "every", "everyone", "nobody"];

/// Words a clause cannot start with and still stand alone.
const DEPENDENT: &[&str] = &["because", "which", "and", "but", "so", "or", "not", "only", "then"];

/// Spoken fillers dropped from the start of a line: these on their own...
const FILLERS: &[&str] = &["so", "um", "uh", "erm", "er", "oh", "and"];
/// ...and these only when a comma follows, since "like" or "right" can carry meaning.
const COMMA_FILLERS: &[&str] = &["yeah", "yes", "well", "okay", "ok", "right", "look", "like", "anyway", "i mean", "you know", "basically", "actually"];

/// One possible line cut from a sentence.
struct Candidate {
    text: String,
    /// True when the line was lifted out of a longer sentence, so it needs its own full stop.
    cut: bool,
}

/// The best line in a written sentence and how strongly it reads as a thought, or None when no
/// part of it is a thought of the right length.
pub(crate) fn pick(sentence: &str) -> Option<(String, u32)> {
    let base = strip_fillers(sentence.trim());
    let whole = words(&base);
    if !(MIN_WORDS..=LONGEST_SENTENCE).contains(&whole) {
        return None;
    }
    // Every clause that says something, scored; the whole sentence wins ties
    candidates(&base)
        .into_iter()
        .filter(|c| (MIN_WORDS..=MAX_WORDS).contains(&words(&c.text)) && cues(&c.text) > 0)
        .map(|c| (score(&c.text), c))
        .reduce(|best, next| if next.0 > best.0 { next } else { best })
        .map(|(s, c)| (finish(c), s.max(1) as u32))
}

/// A line Cilica lifted from a meeting, cut to its strongest clause when it runs long. Short lines
/// only lose a filler at the start; a long line with nowhere safe to cut is kept whole.
pub(crate) fn tighten(spoken: &str) -> String {
    let base = strip_fillers(spoken.trim());
    if words(&base) <= SPOKEN_WORDS {
        return base;
    }
    candidates(&base)
        .into_iter()
        .filter(|c| c.cut && (MIN_WORDS..=SPOKEN_WORDS).contains(&words(&c.text)))
        .map(|c| (score(&c.text), c))
        .reduce(|best, next| if next.0 > best.0 { next } else { best })
        .map_or(base, |(_, c)| finish(c))
}

// ---------- Cutting ---------- //

/// The whole sentence, then each clause: every "; " or ": " starts a new one, and each clause is
/// also tried up to each place an aside starts.
fn candidates(base: &str) -> Vec<Candidate> {
    let mut out = vec![Candidate { text: base.to_string(), cut: false }];
    for (i, segment) in segments(base).into_iter().enumerate() {
        let segment = if i == 0 { segment.to_string() } else { capitalise(segment) };
        for at in cut_points(&segment) {
            let head = segment[..at].trim_end_matches([',', ';', ':', ' ']).to_string();
            if !head.is_empty() {
                out.push(Candidate { text: head, cut: true });
            }
        }
        // A later clause is lifted out of the sentence, so it needs its own full stop too
        if i > 0 {
            out.push(Candidate { text: segment, cut: true });
        }
    }
    out
}

/// A sentence split after each "; " and ": ".
fn segments(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (at, c) in text.char_indices() {
        if matches!(c, ';' | ':') && text[at + 1..].starts_with(' ') {
            out.push(text[start..at].trim());
            start = at + 1;
        }
    }
    out.push(text[start..].trim());
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Byte offsets where an aside starts: a spaced dash, an opening bracket, or a comma followed by
/// a soft tail.
fn cut_points(text: &str) -> Vec<usize> {
    text.char_indices()
        .filter(|&(at, c)| {
            let rest = &text[at..];
            DASHES.iter().any(|d| rest.starts_with(d))
                || c == '('
                || (c == ',' && SOFT_TAILS.iter().any(|tail| starts_with_phrase(text[at + 1..].trim_start(), tail)))
        })
        .map(|(at, _)| at)
        .collect()
}

/// A cut line with its capital and full stop.
fn finish(c: Candidate) -> String {
    let text = capitalise(&c.text);
    if c.cut && !text.ends_with(['.', '!', '?', '…']) { format!("{text}.") } else { text }
}

/// A line without the spoken filler at its start ("So, yeah, we should…" → "We should…").
fn strip_fillers(text: &str) -> String {
    let mut rest = text;
    loop {
        let lower = rest.to_lowercase();
        let next = FILLERS
            .iter()
            .filter(|f| starts_with_phrase(&lower, f))
            .map(|f| rest[f.len()..].trim_start_matches(',').trim_start())
            .chain(COMMA_FILLERS.iter().filter_map(|f| {
                let after = rest.get(f.len()..)?;
                (starts_with_phrase(&lower, f) && after.starts_with(',')).then(|| after[1..].trim_start())
            }))
            .next();
        match next {
            // Never strip a line down to almost nothing
            Some(shorter) if words(shorter) >= 3 => rest = shorter,
            _ => break,
        }
    }
    if rest.len() == text.len() { text.to_string() } else { capitalise(rest) }
}

// ---------- Scoring ---------- //

/// How well a line reads on its own: what it says, how tightly, and how little it trails off.
fn score(text: &str) -> i32 {
    let first = text.split_whitespace().next().unwrap_or("").to_lowercase();
    let first = first.trim_matches(|c: char| !c.is_alphanumeric());
    let length = match words(text) {
        0..=4 => 0,
        5..=14 => 2,
        15..=20 => 1,
        _ => 0,
    };
    let opens_strong = i32::from(ABSOLUTES.contains(&first));
    let dangles = if DEPENDENT.contains(&first) { 2 } else { 0 };
    let asides = cut_points(text).len().min(3) as i32;
    let shorthand = text.split_whitespace().filter(|w| SHORTHAND.contains(&w.to_lowercase().trim_matches(['(', ')', ',']))).count().min(2) as i32;
    cues(text).min(CUE_CAP) as i32 + length + opens_strong - dangles - asides - shorthand
}

/// How many cue phrases a line holds, whole words only: "should" is not in "shoulder".
fn cues(text: &str) -> u32 {
    let words: String = text.to_lowercase().replace('’', "'").chars().map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' }).collect();
    let padded = format!(" {} ", words.split_whitespace().collect::<Vec<_>>().join(" "));
    CUES.iter().filter(|cue| padded.contains(&format!(" {cue} "))).count() as u32
}

// ---------- Text helpers ---------- //

fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Whether `text` starts with `phrase` (ASCII, any case) as whole words.
fn starts_with_phrase(text: &str, phrase: &str) -> bool {
    text.get(..phrase.len()).is_some_and(|head| head.eq_ignore_ascii_case(phrase))
        && text[phrase.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric())
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// What the quote picker has to get right. Each test is one goal, with the lines that prove it.
#[cfg(test)]
mod goals {
    use super::*;

    /// The quote as Home would show it, or None when the sentence has nothing worth quoting.
    fn quote(sentence: &str) -> Option<String> {
        pick(sentence).map(|(text, _)| text)
    }

    /// Whether `out` is lifted from `input`: a piece of it, give or take the first letter's case
    /// and a closing full stop.
    fn lifted(out: &str, input: &str) -> bool {
        let core = out.strip_suffix('.').unwrap_or(out);
        let mut chars = core.chars();
        let rest: String = chars.by_ref().skip(1).collect();
        let first = core.chars().next().unwrap_or(' ');
        input.to_lowercase().contains(&format!("{}{}", first.to_lowercase(), rest.to_lowercase()))
    }

    #[test]
    fn a_run_on_thought_is_cut_to_its_point() {
        let s = "Absolutely everything that can be encrypted should be, if possible, only store decryption keys locally - not doable with everything (eg notes) because cloud requires access to notes.";
        assert_eq!(quote(s).as_deref(), Some("Absolutely everything that can be encrypted should be."));
    }

    #[test]
    fn a_line_that_is_already_tight_is_left_alone() {
        for s in ["I think the onboarding problem is really a pricing problem.", "Pricing is the real problem.", "We should ship before the pricing change"] {
            assert_eq!(quote(s).as_deref(), Some(s), "{s}");
        }
    }

    #[test]
    fn a_line_is_never_cut_where_its_meaning_turns() {
        for s in [
            "We should drop the free tier, but not before we talk to the twenty biggest users.",
            "We should ship the new pricing on Monday, unless sales pushes back.",
            "We picked Postgres for the sync layer, which turned out to be the best call we made.",
            "We should launch in March, maybe April.",
            "We missed the deadline, so we decided to cut the import screen.",
        ] {
            assert_eq!(quote(s).as_deref(), Some(s), "{s}");
        }
    }

    #[test]
    fn hedges_and_padding_after_a_comma_are_dropped() {
        assert_eq!(quote("We should never store raw audio on the server, if possible.").as_deref(), Some("We should never store raw audio on the server."));
        assert_eq!(quote("Onboarding should fit on one screen, ideally.").as_deref(), Some("Onboarding should fit on one screen."));
        assert_eq!(quote("The real problem is that nobody reads the changelog, you know.").as_deref(), Some("The real problem is that nobody reads the changelog."));
    }

    #[test]
    fn asides_in_brackets_and_after_dashes_are_dropped() {
        assert_eq!(quote("We should keep onboarding to three screens (five lost half the testers).").as_deref(), Some("We should keep onboarding to three screens."));
        assert_eq!(quote("The real problem is trust - and honestly the rest of the roadmap depends on it.").as_deref(), Some("The real problem is trust."));
    }

    #[test]
    fn the_clause_after_a_colon_can_stand_alone() {
        assert_eq!(quote("Lesson from the launch: never ship on a Friday.").as_deref(), Some("Never ship on a Friday."));
    }

    #[test]
    fn a_long_sentence_still_gives_up_its_best_clause() {
        let s = "We should never store raw audio on the server, if possible, and the rest of this note is about the twelve migration steps the team listed on Tuesday afternoon for the sync rollout.";
        assert!(words(s) > MAX_WORDS);
        assert_eq!(quote(s).as_deref(), Some("We should never store raw audio on the server."));
    }

    #[test]
    fn a_tight_line_outranks_a_ramble_full_of_cues() {
        let tight = pick("Absolutely everything that can be encrypted should be.").unwrap().1;
        let ramble = pick("I think we should probably maybe look at the pricing page again because honestly the problem is that nobody reads it and we never update it.").unwrap().1;
        assert!(tight > ramble, "tight {tight}, ramble {ramble}");
    }

    #[test]
    fn lists_fragments_and_plain_facts_are_not_quoted() {
        for s in ["Groceries, milk, eggs.", "My shoulder hurts after the long flight home.", "We should.", "Meeting moved to 3pm on Thursday in room four."] {
            assert_eq!(quote(s), None, "{s}");
        }
    }

    #[test]
    fn a_clause_that_leans_on_the_one_before_is_not_quoted() {
        let out = quote("Cloud sync is fine for settings; because it needs our notes it should never touch them.").unwrap();
        assert!(!out.starts_with("Because"), "{out}");
    }

    #[test]
    fn every_quote_is_the_writers_own_words() {
        for s in [
            "Absolutely everything that can be encrypted should be, if possible, only store decryption keys locally - not doable with everything (eg notes) because cloud requires access to notes.",
            "We should keep onboarding to three screens (five lost half the testers).",
            "Lesson from the launch: never ship on a Friday.",
            "So, yeah, I think we should just ship it on Friday.",
            "We should drop the free tier, but not before we talk to the twenty biggest users.",
        ] {
            let out = quote(s).unwrap();
            assert!(lifted(&out, s), "{out:?} is not lifted from {s:?}");
        }
    }

    #[test]
    fn a_cut_line_ends_like_a_sentence() {
        for s in ["Absolutely everything that can be encrypted should be, if possible, only store keys locally.", "Lesson from the launch: never ship on a Friday"] {
            let out = quote(s).unwrap();
            assert!(out.ends_with('.') && out.starts_with(char::is_uppercase), "{out}");
        }
    }

    #[test]
    fn spoken_lines_lose_their_filler() {
        assert_eq!(tighten("So, yeah, I think we should just ship it on Friday."), "I think we should just ship it on Friday.");
        assert_eq!(tighten("Um so we never actually tested the migration."), "We never actually tested the migration.");
        // Not a filler here, and not when it would leave almost nothing
        assert_eq!(tighten("Like the old importer, it drops every attachment."), "Like the old importer, it drops every attachment.");
        assert_eq!(tighten("So, yeah, fine."), "So, yeah, fine.");
    }

    #[test]
    fn short_spoken_lines_are_kept_whole() {
        for s in ["If we lose them there, nothing after it matters.", "Ship it.", "We should ship it, if we can."] {
            assert_eq!(tighten(s), s, "{s}");
        }
    }

    #[test]
    fn long_spoken_lines_are_cut_to_their_point() {
        assert_eq!(
            tighten("We have to fix onboarding before anything else, you know, because every single person we lost last quarter dropped off in the first ten minutes of setup."),
            "We have to fix onboarding before anything else."
        );
    }

    #[test]
    fn long_spoken_lines_with_no_safe_cut_are_kept_whole() {
        let s = "We could ship the redesign this week, but only if design signs off on the mobile layouts and the copy team finishes all of the empty states.";
        assert!(words(s) > SPOKEN_WORDS);
        assert_eq!(tighten(s), s);
    }
}
