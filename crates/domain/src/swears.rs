//! The swear jar: which words put a coin in it, and what the jar holds this week.
//!
//! Matching is a fixed, on-device word list over whole words. Each entry belongs to a root ("fuck",
//! "shit") so the jar can say which word was your favourite whatever form it took. Slurs are left
//! out on purpose: the jar is for swearing, not for words that attack people.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

// ---------- Word list ---------- //

/// Strong swears, grouped by root. The first word of each group is the root.
const STRONG: &[&[&str]] = &[
    &[
        "fuck", "fucks", "fucked", "fucker", "fuckers", "fucking", "fuckin", "fuckup", "fuckups", "fuckwit", "fuckwits", "fuckhead",
        "fuckheads", "fuckface", "fuckery", "motherfucker", "motherfuckers", "motherfucking", "motherfuckin", "mofo", "clusterfuck",
        "wtf", "ffs", "stfu", "gtfo",
    ],
    &[
        "shit", "shits", "shitty", "shitting", "shitted", "shite", "shitshow", "shithead", "shitheads", "shithole", "shitload",
        "shitstorm", "bullshit", "bullshitting", "horseshit", "dipshit", "apeshit", "batshit", "chickenshit",
    ],
    &["ass", "asshole", "assholes", "dumbass", "jackass", "smartass", "hardass"],
    &["arsehole", "arseholes"],
    &["bitch", "bitches", "bitching", "bitchy", "sonofabitch"],
    &["bastard", "bastards"],
    &["dick", "dicks", "dickhead", "dickheads"],
    &["prick", "pricks"],
    &["cunt", "cunts"],
    &["twat", "twats"],
    &["wanker", "wankers", "wank", "wanking"],
    &["bollocks", "bollock"],
    &["tosser", "tossers"],
    &["knobhead", "knobheads"],
    &["piss", "pissed", "pissing", "pisses", "pisstake", "pissy"],
    &["douche", "douchebag", "douchebags"],
];

/// Mild ones, which Settings can leave out.
const MILD: &[&[&str]] = &[
    &["damn", "damned", "dammit", "damnit"],
    &["goddamn", "goddamned", "goddammit", "goddamnit"],
    &["bloody"],
    &["crap", "crappy", "crapped"],
    &["hell"],
    &["arse", "arses", "badass"],
    &["bugger", "buggered", "buggering"],
    &["sod", "sodding"],
    &["feck", "fecking"],
    &["frigging", "friggin"],
];

// ---------- Types ---------- //

/// One word the jar listens for. The UI matches typed words with this list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SwearWord {
    pub word: String,
    /// The group it counts under ("fucking" counts as "fuck").
    pub root: String,
    pub mild: bool,
}

/// One swear found in a piece of text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwearHit {
    pub root: &'static str,
    pub mild: bool,
    /// Byte range of the word in the text.
    pub start: usize,
    pub end: usize,
}

/// Someone who put coins in the jar this week.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SwearPerson {
    /// None for you.
    pub name: Option<String>,
    /// 1..=8, the `speaker-N` colour. None for you.
    pub color_index: Option<u8>,
    pub count: u32,
}

/// A word and how often it went in the jar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SwearCount {
    pub word: String,
    pub count: u32,
}

/// One week of the jar, starting Monday at midnight local time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SwearWeek {
    pub start: Timestamp,
    pub count: u32,
}

/// The jar: this week's coins and the weeks before. Shown on Home and in Settings › Swear jar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SwearJar {
    /// Coins since this week started, or since the jar was last emptied by hand.
    pub count: u32,
    pub last_week: u32,
    /// Everyone who spoke in this week's meetings (none yet is zero), you included once you have
    /// a coin or a meeting. Most coins first.
    pub people: Vec<SwearPerson>,
    /// This week's words, most used first.
    pub words: Vec<SwearCount>,
    /// The last nine weeks, oldest first, ending with this one.
    pub weeks: Vec<SwearWeek>,
    /// The fullest of those weeks, when any had a coin.
    pub record: Option<SwearWeek>,
    pub week_start: Timestamp,
    /// True when this week had a meeting, so the jar can talk about who's paying.
    pub has_meetings: bool,
}

// ---------- Matching ---------- //

/// The jar's word list and how text is matched against it.
pub struct Swears;

impl Swears {
    /// Every word the jar listens for, strong ones first.
    pub fn words() -> Vec<SwearWord> {
        let mut out = Vec::new();

        // Flatten both lists, keeping each word's root and strength
        for (groups, mild) in [(STRONG, false), (MILD, true)] {
            for group in groups {
                for word in *group {
                    out.push(SwearWord { word: (*word).to_string(), root: group[0].to_string(), mild });
                }
            }
        }
        out
    }

    /// The root and strength of one word, if it is a swear. Accepts any case, a trailing "'s",
    /// stretched letters ("fuuuck") and censored forms ("f***").
    pub fn canonical(token: &str) -> Option<(&'static str, bool)> {
        let word = normalize(token);
        if word.is_empty() {
            return None;
        }

        // Starred words match a listed word of the same length, letter for letter
        if word.contains('*') {
            return starred(&word);
        }

        // As written, then with stretched letters squeezed to one and to two
        lookup(&word).or_else(|| lookup(&squeeze(&word, 1))).or_else(|| lookup(&squeeze(&word, 2)))
    }

    /// Whether a root is one of the mild ones.
    pub fn is_mild(root: &str) -> bool {
        MILD.iter().any(|g| g[0] == root)
    }

    /// Every swear in `text`, in order. Mild ones only when `mild` is true.
    pub fn find(text: &str, mild: bool) -> Vec<SwearHit> {
        let mut out = Vec::new();
        let mut start: Option<usize> = None;

        // Walk the text, closing a word at each character that cannot be part of one
        for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
            if is_word_char(c) {
                start.get_or_insert(i);
                continue;
            }
            let Some(from) = start.take() else { continue };
            if let Some((root, is_mild)) = Self::canonical(&text[from..i])
                && (mild || !is_mild)
            {
                out.push(SwearHit { root, mild: is_mild, start: from, end: i });
            }
        }
        out
    }
}

/// Letters, apostrophes (straight and curly) and the asterisks of censored words.
fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || c == '\'' || c == '’' || c == '*'
}

/// Lower case, straight apostrophes, no possessive "'s", no apostrophes at either end or inside.
fn normalize(token: &str) -> String {
    let lower = token.to_lowercase().replace('’', "'");
    let trimmed = lower.trim_matches('\'');
    let base = trimmed.strip_suffix("'s").unwrap_or(trimmed);
    base.replace('\'', "")
}

/// Runs of three or more of the same letter cut down to `keep`.
fn squeeze(word: &str, keep: usize) -> String {
    let chars: Vec<char> = word.chars().collect();
    let mut out = String::with_capacity(word.len());
    let mut i = 0;

    // Copy each run, shortening the long ones
    while i < chars.len() {
        let c = chars[i];
        let run = chars[i..].iter().take_while(|&&x| x == c).count();
        let take = if run >= 3 { keep } else { run };
        out.extend(std::iter::repeat_n(c, take));
        i += run;
    }
    out
}

/// The listed word itself, strong first.
fn lookup(word: &str) -> Option<(&'static str, bool)> {
    for (groups, mild) in [(STRONG, false), (MILD, true)] {
        for group in groups {
            if group.contains(&word) {
                return Some((group[0], mild));
            }
        }
    }
    None
}

/// A censored word: a letter first, at least one star, and every letter in place.
fn starred(word: &str) -> Option<(&'static str, bool)> {
    let chars: Vec<char> = word.chars().collect();
    if chars.first().is_none_or(|c| *c == '*') || chars.len() < 3 {
        return None;
    }
    for (groups, mild) in [(STRONG, false), (MILD, true)] {
        for group in groups {
            let hit = group.iter().any(|w| {
                w.chars().count() == chars.len() && w.chars().zip(&chars).all(|(a, b)| *b == '*' || a == *b)
            });
            if hit {
                return Some((group[0], mild));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(text: &str, mild: bool) -> Vec<&'static str> {
        Swears::find(text, mild).into_iter().map(|h| h.root).collect()
    }

    #[test]
    fn finds_whole_words_only() {
        assert_eq!(roots("Oh, for fuck’s sake. That's bullshit.", true), ["fuck", "shit"]);
        assert!(roots("We assess the class in Scunthorpe, hello shell", true).is_empty());
    }

    #[test]
    fn mild_ones_can_be_left_out() {
        assert_eq!(roots("Damn, that was a bloody good shit idea", true), ["damn", "bloody", "shit"]);
        assert_eq!(roots("Damn, that was a bloody good shit idea", false), ["shit"]);
    }

    #[test]
    fn reads_stretched_censored_and_abbreviated_words() {
        assert_eq!(roots("fuuuuck, shiiiit, f***, s**t, WTF", true), ["fuck", "shit", "fuck", "shit", "fuck"]);
        assert_eq!(roots("f**king hell", true), ["fuck", "hell"]);
        assert!(roots("** and *s and x", true).is_empty());
    }

    #[test]
    fn reports_byte_ranges() {
        let text = "Holy shit, we’re recording";
        let hit = &Swears::find(text, true)[0];
        assert_eq!(&text[hit.start..hit.end], "shit");
    }

    #[test]
    fn every_word_maps_to_its_root() {
        let words = Swears::words();
        assert!(words.iter().any(|w| w.word == "motherfucking" && w.root == "fuck" && !w.mild));
        assert!(words.iter().any(|w| w.word == "bloody" && w.mild));
        assert!(Swears::is_mild("damn") && !Swears::is_mild("fuck"));
        for w in &words {
            assert_eq!(Swears::canonical(&w.word).map(|c| c.0), Some(w.root.as_str()), "{}", w.word);
        }
    }
}
