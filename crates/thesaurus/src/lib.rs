//! The bundled thesaurus: synonyms for a selected word, grouped by meaning, with no network.
//!
//! The data is the ABC Thesaurus (CC BY-SA 4.0), cleaned of slurs and with rude words kept only for
//! rude headwords (<https://github.com/Benzo-Fury/abcthesaurus-data>), plus Open English WordNet's
//! irregular forms (CC BY 4.0). `tools/build-thesaurus.py` builds it and it is compiled into the app
//! (about 1.2 MB). It is read the first time it is needed and kept for the rest of the session.
//!
//! A word is looked up as written, then as an irregular form ("ran" → run), then with a regular
//! ending taken off ("sprints", "walked", "running", "larger"). Synonyms are put back into the
//! selection's form where English does that regularly ("walked" → strolled); otherwise they are
//! given in their plain form.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::OnceLock;

use cilica_domain::{SynonymGroup, Synonyms};

const DATA: &[u8] = include_bytes!("../data/thesaurus.txt.gz");
/// The most meanings returned for one word.
pub const MAX_GROUPS: usize = 12;
/// The most words in a phrase that is looked up ("look into", "in spite of").
pub const MAX_PHRASE_WORDS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pos {
    Noun,
    Verb,
    Adjective,
    Adverb,
    /// Prepositions, pronouns, conjunctions and the rest: looked up only as written.
    Other(&'static str),
}

impl Pos {
    fn parse(s: &str) -> Option<Pos> {
        Some(match s {
            "noun" => Pos::Noun,
            "verb" => Pos::Verb,
            "adjective" => Pos::Adjective,
            "adverb" => Pos::Adverb,
            "preposition" => Pos::Other("preposition"),
            "pronoun" => Pos::Other("pronoun"),
            "conjunction" => Pos::Other("conjunction"),
            "other" => Pos::Other("other"),
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Pos::Noun => "noun",
            Pos::Verb => "verb",
            Pos::Adjective => "adjective",
            Pos::Adverb => "adverb",
            Pos::Other(name) => name,
        }
    }
}

/// The ending the selection had, which the synonyms are given too.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ending {
    /// As written, or an irregular form ("ran"): synonyms in their plain form.
    Plain,
    /// "cats", "runs".
    S,
    /// "walked".
    Ed,
    /// "walking".
    Ing,
    /// "larger", "largest": synonyms in their plain form (English mixes -er and "more").
    Degree,
}

impl Ending {
    /// Parts of speech the ending can belong to.
    fn allows(self, pos: Pos) -> bool {
        match self {
            Ending::Plain => true,
            Ending::S => matches!(pos, Pos::Noun | Pos::Verb),
            Ending::Ed | Ending::Ing => pos == Pos::Verb,
            Ending::Degree => matches!(pos, Pos::Adjective | Pos::Adverb),
        }
    }
}

struct Group {
    pos: Pos,
    gloss: Box<str>,
    words: Box<[Box<str>]>,
}

/// Synonyms by meaning, keyed by lower-case word.
pub struct Thesaurus {
    groups: Vec<Group>,
    words: HashMap<Box<str>, Box<[u32]>>,
    /// Irregular form → its word ("ran" → "run").
    forms: HashMap<Box<str>, Box<str>>,
    /// Word → its irregular forms, as WordNet lists them ("run" → ran, running).
    irregular: HashMap<Box<str>, Vec<Box<str>>>,
}

impl Thesaurus {
    /// The bundled thesaurus, read on first use.
    pub fn shared() -> &'static Thesaurus {
        static SHARED: OnceLock<Thesaurus> = OnceLock::new();
        SHARED.get_or_init(|| {
            let mut text = String::new();
            flate2::read::GzDecoder::new(DATA).read_to_string(&mut text).expect("the bundled thesaurus is valid gzip");
            Thesaurus::parse(&text)
        })
    }

    /// Read the format `tools/build-thesaurus.py` writes. Lines it does not know are skipped.
    pub fn parse(text: &str) -> Thesaurus {
        let mut t = Thesaurus { groups: Vec::new(), words: HashMap::new(), forms: HashMap::new(), irregular: HashMap::new() };
        for line in text.lines() {
            let mut f = line.split('\t');
            match (f.next(), f.next(), f.next(), f.next()) {
                (Some("G"), Some(pos), Some(gloss), Some(words)) => {
                    let Some(pos) = Pos::parse(pos) else { continue };
                    t.groups.push(Group { pos, gloss: gloss.into(), words: words.split('|').map(Into::into).collect() });
                }
                (Some("W"), Some(word), Some(ids), None) => {
                    t.words.insert(word.into(), ids.split(',').filter_map(|i| i.parse().ok()).collect());
                }
                (Some("F"), Some(form), Some(word), None) => {
                    t.forms.insert(form.into(), word.into());
                    t.irregular.entry(word.into()).or_default().push(form.into());
                }
                _ => {}
            }
        }
        t
    }

    /// Synonyms for a word or short phrase, grouped by meaning. Empty `groups` when there are none.
    pub fn lookup(&self, text: &str) -> Synonyms {
        let key = normalize(text);
        let none = Synonyms { word: String::new(), groups: Vec::new() };
        if key.is_empty() || key.split(' ').count() > MAX_PHRASE_WORDS {
            return none;
        }
        // As written ("running" the adjective), or an irregular form of a word ("ran").
        let mut word = key.clone();
        let mut plain = self.groups_for(&key, &key, Ending::Plain);
        if plain.is_empty()
            && let Some(base) = self.forms.get(key.as_str())
        {
            word = base.to_string();
            plain = self.groups_for(base, &key, Ending::Plain);
        }
        // With a regular ending taken off ("running" the verb): the first plain form that has any.
        let inflected = if key.contains(' ') {
            None
        } else {
            base_forms(&key).into_iter().filter(|(w, _)| self.words.contains_key(w.as_str())).find_map(|(w, e)| {
                let groups = self.groups_for(&w, &key, e);
                (!groups.is_empty()).then_some((w, e, groups))
            })
        };
        let (word, groups) = match inflected {
            None => (word, plain),
            Some((base, _, groups)) if plain.is_empty() => (base, groups),
            // "-ing" and "-ed" words are verbs more often than not in a sentence.
            Some((base, Ending::Ing | Ending::Ed, groups)) => (base, merge(groups, plain)),
            Some((_, _, groups)) => (word, merge(plain, groups)),
        };
        if groups.is_empty() { none } else { Synonyms { word, groups } }
    }

    fn groups_for(&self, word: &str, typed: &str, ending: Ending) -> Vec<SynonymGroup> {
        let Some(ids) = self.words.get(word) else { return Vec::new() };
        // Each synonym once, in its first (most used) meaning; never the word itself.
        let mut seen: HashSet<String> = HashSet::from([word.to_string(), typed.to_string()]);
        let mut out = Vec::new();
        for g in ids.iter().filter_map(|&i| self.groups.get(i as usize)) {
            if !ending.allows(g.pos) {
                continue;
            }
            let words: Vec<String> = g
                .words
                .iter()
                .map(|w| self.inflect(w, g.pos, ending))
                .filter(|w| seen.insert(w.to_lowercase()))
                .collect();
            if !words.is_empty() {
                out.push(SynonymGroup { part_of_speech: g.pos.name().into(), gloss: g.gloss.to_string(), words });
            }
            if out.len() == MAX_GROUPS {
                break;
            }
        }
        out
    }

    /// `word` with the selection's ending. Verbs take it on their first word ("look into" →
    /// "looked into"), nouns on their last ("melodic theme" → "melodic themes").
    fn inflect(&self, word: &str, pos: Pos, ending: Ending) -> String {
        let (head, last) = match word.rsplit_once(' ') {
            Some((head, last)) => (Some(head), last),
            None => (None, word),
        };
        let (first, rest) = match word.split_once(' ') {
            Some((first, rest)) => (first, Some(rest)),
            None => (word, None),
        };
        let join_first = |w: String| rest.map_or(w.clone(), |r| format!("{w} {r}"));
        match (ending, pos) {
            (Ending::S, Pos::Noun) => head.map_or_else(|| plural(last), |h| format!("{h} {}", plural(last))),
            (Ending::S, _) => match first {
                "be" => join_first("is".into()),
                "have" => join_first("has".into()),
                _ => join_first(plural(first)),
            },
            (Ending::Ed, _) => join_first(self.past(first)),
            (Ending::Ing, _) => join_first(self.gerund(first)),
            _ => word.to_string(),
        }
    }

    fn past(&self, verb: &str) -> String {
        if let Some(form) = self.irregular.get(verb).and_then(|f| f.iter().find(|f| !f.ends_with("ing") && !f.ends_with('s'))) {
            return form.to_string();
        }
        if verb.ends_with('e') {
            format!("{verb}d")
        } else if let Some(stem) = consonant_y(verb) {
            format!("{stem}ied")
        } else if doubles(verb) {
            format!("{verb}{}ed", &verb[verb.len() - 1..])
        } else {
            format!("{verb}ed")
        }
    }

    fn gerund(&self, verb: &str) -> String {
        if let Some(form) = self.irregular.get(verb).and_then(|f| f.iter().find(|f| f.ends_with("ing"))) {
            return form.to_string();
        }
        if let Some(stem) = verb.strip_suffix("ie") {
            format!("{stem}ying")
        } else if verb.ends_with('e') && !verb.ends_with("ee") && !verb.ends_with("ye") && !verb.ends_with("oe") && verb.len() > 2 {
            format!("{}ing", &verb[..verb.len() - 1])
        } else if doubles(verb) {
            format!("{verb}{}ing", &verb[verb.len() - 1..])
        } else {
            format!("{verb}ing")
        }
    }
}

/// `first` then `then`, each synonym once, at most `MAX_GROUPS`, keeping room for a few of `then`
/// (a verb with many meanings must not crowd out the adjective).
fn merge(first: Vec<SynonymGroup>, then: Vec<SynonymGroup>) -> Vec<SynonymGroup> {
    let mut seen = HashSet::new();
    let room = MAX_GROUPS - then.len().min(3);
    first
        .into_iter()
        .take(room)
        .chain(then)
        .filter_map(|mut g| {
            g.words.retain(|w| seen.insert(w.to_lowercase()));
            (!g.words.is_empty()).then_some(g)
        })
        .take(MAX_GROUPS)
        .collect()
}

/// Lower case, single spaces, without the punctuation around a selection or a possessive ending
/// ("“Happy,” " → "happy", "Mum's" → "mum").
fn normalize(text: &str) -> String {
    let trimmed = text.trim_matches(|c: char| !c.is_alphanumeric());
    let trimmed = trimmed.strip_suffix("'s").or_else(|| trimmed.strip_suffix("’s")).unwrap_or(trimmed);
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Plain forms a regularly inflected word could come from, with the ending taken off.
fn base_forms(w: &str) -> Vec<(String, Ending)> {
    let mut out: Vec<(String, Ending)> = Vec::new();
    let mut add = |stem: &str, tail: &str, e: Ending| {
        if stem.chars().count() >= 2 {
            out.push((format!("{stem}{tail}"), e));
        }
    };
    let undoubled = |stem: &str| -> Option<String> {
        let b = stem.as_bytes();
        (b.len() >= 3 && b[b.len() - 1] == b[b.len() - 2]).then(|| stem[..stem.len() - 1].to_string())
    };
    if let Some(s) = w.strip_suffix("ies") {
        add(s, "y", Ending::S);
    }
    // "-s" before "-es": "ideas" is "idea", not "ide".
    if let Some(s) = w.strip_suffix('s').filter(|_| !w.ends_with("ss")) {
        add(s, "", Ending::S);
    }
    if let Some(s) = w.strip_suffix("es") {
        add(s, "", Ending::S);
    }
    if let Some(s) = w.strip_suffix("ied") {
        add(s, "y", Ending::Ed);
    }
    if let Some(s) = w.strip_suffix("ed") {
        add(s, "", Ending::Ed);
        add(s, "e", Ending::Ed);
        if let Some(u) = undoubled(s) {
            add(&u, "", Ending::Ed);
        }
    }
    if let Some(s) = w.strip_suffix("ying") {
        add(s, "ie", Ending::Ing);
    }
    if let Some(s) = w.strip_suffix("ing") {
        add(s, "", Ending::Ing);
        add(s, "e", Ending::Ing);
        if let Some(u) = undoubled(s) {
            add(&u, "", Ending::Ing);
        }
    }
    for suffix in ["est", "er"] {
        if let Some(s) = w.strip_suffix(suffix) {
            if let Some(s) = s.strip_suffix('i') {
                add(s, "y", Ending::Degree);
            }
            add(s, "", Ending::Degree);
            add(s, "e", Ending::Degree);
            if let Some(u) = undoubled(s) {
                add(&u, "", Ending::Degree);
            }
        }
    }
    out
}

fn plural(w: &str) -> String {
    if let Some(stem) = consonant_y(w) {
        format!("{stem}ies")
    } else if ["s", "x", "z", "ch", "sh"].iter().any(|e| w.ends_with(e)) {
        format!("{w}es")
    } else {
        format!("{w}s")
    }
}

/// The stem of a word ending in a consonant and "y" ("carry" → "carr").
fn consonant_y(w: &str) -> Option<&str> {
    let stem = w.strip_suffix('y')?;
    let before = stem.chars().last()?;
    (!"aeiou".contains(before)).then_some(stem)
}

fn is_vowel(c: u8) -> bool {
    b"aeiou".contains(&c)
}

/// One-syllable words ending consonant-vowel-consonant double their last letter: stop → stopped.
fn doubles(w: &str) -> bool {
    let b = w.as_bytes();
    if b.len() < 3 || !b.iter().all(u8::is_ascii_lowercase) {
        return false;
    }
    let (c1, v, c2) = (b[b.len() - 3], b[b.len() - 2], b[b.len() - 1]);
    let syllables = b.windows(2).filter(|p| !is_vowel(p[0]) && is_vowel(p[1])).count() + usize::from(is_vowel(b[0]));
    syllables == 1 && !is_vowel(c1) && is_vowel(v) && b"bdgklmnprt".contains(&c2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &Synonyms) -> Vec<&str> {
        s.groups.iter().flat_map(|g| g.words.iter().map(String::as_str)).collect()
    }

    #[test]
    fn finds_synonyms_grouped_by_meaning() {
        let t = Thesaurus::shared();
        let happy = t.lookup("happy");
        assert_eq!(happy.word, "happy");
        assert!(words(&happy).contains(&"glad"), "{happy:?}");
        assert!(happy.groups.iter().all(|g| g.part_of_speech == "adjective" && !g.gloss.is_empty()));
        assert!(!words(&happy).contains(&"happy"), "never the word itself");
        let big = t.lookup("“Big,”");
        assert!(words(&big).contains(&"large"));
    }

    #[test]
    fn each_synonym_once_and_capped() {
        let run = Thesaurus::shared().lookup("run");
        let all = words(&run);
        let unique: HashSet<&str> = all.iter().copied().collect();
        assert_eq!(all.len(), unique.len());
        assert!(run.groups.len() <= MAX_GROUPS);
    }

    #[test]
    fn keeps_the_selection_form() {
        let t = Thesaurus::shared();
        let walked = t.lookup("walked");
        assert_eq!(walked.word, "walk");
        assert!(walked.groups.iter().all(|g| g.part_of_speech == "verb"));
        let past = |w: &&str| {
            let first = w.split(' ').next().unwrap();
            first.ends_with("ed") || t.forms.contains_key(first)
        };
        assert!(words(&walked).iter().all(past), "{walked:?}");
        let ideas = t.lookup("ideas");
        assert_eq!(ideas.word, "idea");
        assert!(words(&ideas).contains(&"thoughts"), "{ideas:?}");
        // "running" is also an adjective ("running water"); the verb comes first.
        let running = t.lookup("Running");
        assert_eq!(running.word, "run");
        assert_eq!(running.groups[0].part_of_speech, "verb");
        assert!(running.groups[0].words.iter().all(|w| w.split(' ').next().unwrap().ends_with("ing")), "{running:?}");
        assert!(running.groups.iter().any(|g| g.part_of_speech == "adjective"));
    }

    #[test]
    fn writers_synonyms_closest_first() {
        let t = Thesaurus::shared();
        let customer = words(&t.lookup("customer")).into_iter().map(str::to_string).collect::<Vec<_>>();
        for w in ["client", "buyer", "consumer", "shopper"] {
            assert!(customer.iter().take(8).any(|c| c == w), "{w} in {customer:?}");
        }
        // Every meaning's close synonyms come before any near synonym.
        let big = t.lookup("big");
        assert!(["large", "great", "sizable"].contains(&big.groups[0].words[0].as_str()), "{big:?}");
    }

    #[test]
    fn possessives_and_british_words() {
        let t = Thesaurus::shared();
        let mums = t.lookup("Mum's");
        assert_eq!(mums.word, "mum");
        assert_eq!(words(&mums).first(), Some(&"mother"), "{mums:?}");
        assert!(words(&t.lookup("mum’s")).contains(&"mother"));
    }

    #[test]
    fn no_slurs_and_rude_words_only_for_rude_ones() {
        let t = Thesaurus::shared();
        let offers = |w: &str, bad: &str| words(&t.lookup(w)).iter().any(|x| x.eq_ignore_ascii_case(bad));
        for (plain, bad) in [
            ("breast", "tit"), ("detective", "dick"), ("complain", "bitch"), ("drink", "hooker"),
            ("prostitute", "whore"), ("tire", "fag"), ("crack", "chink"), ("black", "colored"),
            ("aggression", "rape"), ("assault", "rape"), ("trilogy", "threesome"), ("rub", "fingering"),
        ] {
            assert!(!offers(plain, bad), "{plain} offers {bad}");
        }
        assert!(offers("arse", "ass"), "a rude word still gets its rude synonyms");
        assert!(offers("accompany", "escort") && offers("grab", "snatch"));
    }

    #[test]
    fn irregular_forms_give_plain_synonyms() {
        let ran = Thesaurus::shared().lookup("ran");
        assert_eq!(ran.word, "run");
        assert!(!ran.groups.is_empty());
    }

    #[test]
    fn phrases_and_unknown_words() {
        let t = Thesaurus::shared();
        assert!(!t.lookup("look into").groups.is_empty());
        let none = t.lookup("cilicaxyz");
        assert!(none.groups.is_empty() && none.word.is_empty());
        assert!(t.lookup("").groups.is_empty());
        assert!(t.lookup("one two three four five").groups.is_empty());
    }

    #[test]
    fn regular_endings() {
        assert_eq!(plural("box"), "boxes");
        assert_eq!(plural("story"), "stories");
        assert_eq!(plural("day"), "days");
        assert!(doubles("stop") && doubles("plan") && !doubles("open") && !doubles("walk") && !doubles("fix"));
        let t = Thesaurus::parse("");
        assert_eq!(t.past("stroll"), "strolled");
        assert_eq!(t.past("hurry"), "hurried");
        assert_eq!(t.past("move"), "moved");
        assert_eq!(t.gerund("make"), "making");
        assert_eq!(t.gerund("lie"), "lying");
        assert_eq!(t.gerund("stop"), "stopping");
        assert_eq!(t.gerund("see"), "seeing");
    }

    #[test]
    fn loads_fast_enough() {
        let start = std::time::Instant::now();
        let t = Thesaurus::shared();
        let loaded = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            t.lookup("running");
        }
        let per_lookup = start.elapsed() / 1000;
        eprintln!("loaded in {loaded:?}, {per_lookup:?} per lookup");
        assert!(per_lookup.as_millis() < 5);
    }
}

