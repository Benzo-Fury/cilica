//! Synonyms for a selected word, from the bundled thesaurus (`cilica-thesaurus`).

use serde::{Deserialize, Serialize};
use specta::Type;

/// Synonyms for a word or short phrase, one group per meaning, the most used part of speech first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Synonyms {
    /// The form looked up: "run" for "running". Empty when nothing was found.
    pub word: String,
    pub groups: Vec<SynonymGroup>,
}

/// Words that share one meaning of the looked-up word.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SynonymGroup {
    /// "noun", "verb", "adjective", "adverb", "preposition", "pronoun", "conjunction" or "other".
    pub part_of_speech: String,
    /// A short definition of the meaning.
    pub gloss: String,
    /// Put in the form the selection was in where that is regular ("runs" gives "sprints").
    pub words: Vec<String>,
}
