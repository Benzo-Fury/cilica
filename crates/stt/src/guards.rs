//! Hallucination guards. Whisper invents text for silence, music and noise ("Thank you for
//! watching!", "[BLANK_AUDIO]") and can loop on a phrase. These checks run before and after the
//! model.

use crate::SAMPLE_RATE;

/// Phrases Whisper invents from silence, music and noise (video outros). Dropped when they are
/// the whole result of a short clip or the model doubts there was speech.
const PHANTOMS: &[&str] = &[
    "thanks for watching",
    "thanks for watching!",
    "thank you for watching",
    "thank you for watching.",
    "thank you for watching!",
    "thank you so much for watching",
    "please subscribe",
    "subtitles by the amara.org community",
    "the end",
];

/// Short replies people really say in meetings, which Whisper also likes to invent for noise.
/// Dropped only when the model doubts there was speech.
const COMMON_REPLIES: &[&str] = &["you", "so", "bye", "bye.", "okay", "okay.", "thank you", "thank you."];

/// Is the audio worth sending to the model at all?
pub fn has_enough_speech(samples: &[f32], speech_samples: usize) -> bool {
    if speech_samples < SAMPLE_RATE as usize / 4 {
        return false;
    }
    let rms = (samples.iter().map(|s| (s * s) as f64).sum::<f64>() / samples.len().max(1) as f64).sqrt();
    rms > 0.002
}

/// Clean one model output; `None` drops it.
pub fn clean(text: &str, no_speech_prob: f32, audio_secs: f32) -> Option<String> {
    let mut t = strip_non_speech(text);
    t = collapse_loops(&t);
    // A decode that got stuck on one syllable ("who-o-o-o-o-o…") is noise, not speech.
    let t = collapse_stutter(&t)?;
    let t = t.trim().to_string();
    if t.is_empty() || t.chars().all(|c| !c.is_alphanumeric()) {
        return None;
    }
    if no_speech_prob > 0.8 {
        return None;
    }
    let lower = t.to_lowercase();
    if PHANTOMS.contains(&lower.as_str()) && (audio_secs < 3.0 || no_speech_prob > 0.3) {
        return None;
    }
    if COMMON_REPLIES.contains(&lower.as_str()) && no_speech_prob > 0.3 {
        return None;
    }
    Some(t)
}

/// Remove bracketed annotations and music symbols: "[BLANK_AUDIO]", "(silence)", "♪".
fn strip_non_speech(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0u32;
    for c in text.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            '♪' | '♫' => {}
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// More words than anyone says in that time, even after loops are collapsed: the decode went
/// wrong and is worth redoing carefully. (Plain repeats are fixed by [`clean`] for free.)
pub fn implausible(text: &str, audio_secs: f32) -> bool {
    collapse_loops(&strip_non_speech(text)).split_whitespace().count() as f32 > audio_secs * 4.5 + 4.0
}

/// Repeats needed before a run of `n` words counts as a loop: three for short runs ("we we we",
/// "I think so. I think so." is fine twice), two for runs of five words or more (nobody says the
/// same long sentence twice in a row).
fn repeats_needed(n: usize) -> usize {
    if n >= 5 { 2 } else { 3 }
}

/// "I think so. I think so. I think so. I think so." → "I think so."; a sentence of five or more
/// words said twice in a row → once.
fn collapse_loops(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 3 {
        return text.to_string();
    }
    let mut out: Vec<&str> = Vec::with_capacity(words.len());
    let mut i = 0;
    'outer: while i < words.len() {
        // Shortest repeating unit first: "I think so." ×4 collapses to one, not to two.
        for n in 1..=(words.len() / 2).min(40) {
            let need = repeats_needed(n);
            if i + need * n > words.len() {
                continue;
            }
            let gram = &words[i..i + n];
            let norm = |w: &str| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
            let same = |k: usize| (0..n).all(|j| norm(words[i + k * n + j]) == norm(gram[j]));
            if (1..need).all(same) {
                out.extend_from_slice(gram);
                let mut k = need;
                while i + (k + 1) * n <= words.len() && same(k) {
                    k += 1;
                }
                i += k * n;
                continue 'outer;
            }
        }
        out.push(words[i]);
        i += 1;
    }
    out.join(" ")
}

/// Whisper sometimes gets stuck repeating one or two characters inside a word
/// ("who-o-o-o-o-o-o", "aaaaaaaaaa"). Words like that are removed; if they were most of the
/// text, the whole result is dropped (`None`).
fn collapse_stutter(text: &str) -> Option<String> {
    fn stuck(word: &str) -> bool {
        // Numbers repeat digits legitimately ("1000000").
        if word.chars().any(|c| c.is_ascii_digit()) {
            return false;
        }
        let chars: Vec<char> = word.chars().collect();
        (1..=3).any(|unit| {
            if chars.len() < unit * 6 {
                return false;
            }
            let mut best = 1;
            let mut run = 1;
            for i in unit..chars.len() {
                if chars[i] == chars[i - unit] {
                    run += 1;
                    best = best.max(run / unit + 1);
                } else {
                    run = 1;
                }
            }
            best >= 6
        })
    }
    let total = text.chars().count();
    let (kept, dropped): (Vec<&str>, Vec<&str>) = text.split_whitespace().partition(|w| !stuck(w));
    if dropped.is_empty() {
        return Some(text.to_string());
    }
    let dropped_len: usize = dropped.iter().map(|w| w.chars().count()).sum();
    if kept.is_empty() || dropped_len * 2 > total {
        return None;
    }
    Some(kept.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_stuck_syllables() {
        let stuck = format!("Thanks a who{}", "-o".repeat(80));
        assert_eq!(clean(&stuck, 0.1, 3.0), None);
        assert_eq!(clean("Well aaaaaaaaaaaaaaaa that went fine in the end, thanks.", 0.1, 4.0).as_deref(), Some("Well that went fine in the end, thanks."));
        assert_eq!(clean("Mississippi is 1000000 miles away, no?", 0.1, 3.0).as_deref(), Some("Mississippi is 1000000 miles away, no?"));
        assert_eq!(clean("Hmm, cooool, that works.", 0.1, 3.0).as_deref(), Some("Hmm, cooool, that works."));
    }

    #[test]
    fn drops_phantoms_and_annotations() {
        assert_eq!(clean("[BLANK_AUDIO]", 0.1, 5.0), None);
        assert_eq!(clean(" Thanks for watching!", 0.5, 2.0), None);
        assert_eq!(clean("♪ ♪", 0.0, 2.0), None);
        assert_eq!(clean("(upbeat music) Hello there.", 0.1, 2.0).as_deref(), Some("Hello there."));
        assert_eq!(clean("Thank you.", 0.05, 6.0).as_deref(), Some("Thank you."));
        // Real short replies survive when the model heard speech, even in a short clip.
        assert_eq!(clean("Okay.", 0.05, 1.0).as_deref(), Some("Okay."));
        assert_eq!(clean("Thank you.", 0.1, 1.5).as_deref(), Some("Thank you."));
        // The same words from noise are dropped.
        assert_eq!(clean("you", 0.5, 1.0), None);
        assert_eq!(clean("Okay.", 0.4, 1.0), None);
        assert_eq!(clean("Real words", 0.95, 4.0), None);
    }

    #[test]
    fn collapses_loops() {
        assert_eq!(collapse_loops("I think so. I think so. I think so. I think so."), "I think so.");
        assert_eq!(collapse_loops("we we we we we went home today"), "we went home today");
        assert_eq!(collapse_loops("this is a normal sentence with no loops"), "this is a normal sentence with no loops");
        // Measured with large-v3-turbo and a shortened window.
        let looped = "Perfect. Let's also book a review meeting for Monday morning with the whole team. Perfect. Let's also book a review meeting for Monday morning with the whole team.";
        assert_eq!(collapse_loops(looped), "Perfect. Let's also book a review meeting for Monday morning with the whole team.");
        assert_eq!(clean(looped, 0.1, 5.0).as_deref(), Some("Perfect. Let's also book a review meeting for Monday morning with the whole team."));
        assert!(!implausible(looped, 5.0), "a plain repeat is collapsed, not redone");
        // Short phrases said twice are speech, not a loop.
        assert_eq!(collapse_loops("Thank you. Thank you."), "Thank you. Thank you.");
        assert!(!implausible("Marketing needs the final copy by the end of next week.", 4.0));
        assert!(implausible("one two three four five six seven eight nine ten eleven twelve thirteen", 1.5));
    }

    #[test]
    fn speech_gate() {
        assert!(!has_enough_speech(&[0.0; 16_000], 16_000));
        assert!(!has_enough_speech(&[0.1; 16_000], 1_000));
        assert!(has_enough_speech(&[0.1; 16_000], 8_000));
    }
}
