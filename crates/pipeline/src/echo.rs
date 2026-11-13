//! The second line of defence against remote voices being transcribed twice.
//!
//! Echo cancellation in the capture session removes most of what the mic hears from the
//! speakers, but a loud room or a laptop at full volume can leave enough for Whisper to
//! transcribe. Here a final mic utterance that overlaps a system utterance in time and repeats
//! most of its words is treated as echo and dropped, whichever of the two finished first.

use std::collections::VecDeque;

use cilica_domain::{AudioChannel, Id};

/// Utterances this far apart in time never count as echo of each other.
const SLACK_MS: u32 = 1500;
/// Share of the mic utterance's words found in the system utterance.
const MIN_SHARED: f32 = 0.6;
/// Short mic utterances ("yes", "okay, thanks") are kept: people do say what others just said.
const MIN_WORDS: usize = 3;
/// History kept per channel.
const KEEP_MS: u32 = 30_000;

struct Heard {
    id: Id,
    start_ms: u32,
    end_ms: u32,
    words: Vec<String>,
}

#[derive(Default)]
pub(crate) struct EchoFilter {
    mic: VecDeque<Heard>,
    system: VecDeque<Heard>,
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect::<String>().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

fn is_echo(mic: &Heard, system: &Heard) -> bool {
    if mic.words.len() < MIN_WORDS || mic.start_ms > system.end_ms + SLACK_MS || system.start_ms > mic.end_ms + SLACK_MS {
        return false;
    }
    let mut pool: Vec<&str> = system.words.iter().map(String::as_str).collect();
    let mut shared = 0;
    for w in &mic.words {
        if let Some(i) = pool.iter().position(|p| p == w) {
            pool.swap_remove(i);
            shared += 1;
        }
    }
    shared as f32 / mic.words.len() as f32 >= MIN_SHARED
}

impl EchoFilter {
    /// Record a final utterance. Returns the id of a mic utterance that is echo and must not be
    /// kept: the new one itself, or an earlier mic utterance the new system one explains.
    pub fn on_final(&mut self, channel: AudioChannel, id: Id, start_ms: u32, end_ms: u32, text: &str) -> Option<Id> {
        let heard = Heard { id, start_ms, end_ms, words: words(text) };
        for list in [&mut self.mic, &mut self.system] {
            while list.front().is_some_and(|h| h.end_ms + KEEP_MS < start_ms) {
                list.pop_front();
            }
        }
        match channel {
            AudioChannel::Mic => {
                if self.system.iter().any(|s| is_echo(&heard, s)) {
                    return Some(id);
                }
                self.mic.push_back(heard);
                None
            }
            AudioChannel::System => {
                let echoed = self.mic.iter().position(|m| is_echo(m, &heard));
                self.system.push_back(heard);
                echoed.and_then(|i| self.mic.remove(i)).map(|m| m.id)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_mic_echo_either_way_round() {
        let mut f = EchoFilter::default();
        let (s, m) = (cilica_domain::new_id(), cilica_domain::new_id());
        assert_eq!(f.on_final(AudioChannel::System, s, 5_000, 9_000, "The launch is set for the third of March."), None);
        assert_eq!(f.on_final(AudioChannel::Mic, m, 5_100, 9_100, "launch is set for the 3rd of March"), Some(m));

        // The mic finished first; the system utterance arrives later and takes it back.
        let mut f = EchoFilter::default();
        let m = cilica_domain::new_id();
        assert_eq!(f.on_final(AudioChannel::Mic, m, 1_000, 4_000, "Marketing needs the final copy by Friday."), None);
        assert_eq!(f.on_final(AudioChannel::System, s, 900, 3_900, "Marketing needs the final copy by Friday."), Some(m));
    }

    #[test]
    fn keeps_real_replies() {
        let mut f = EchoFilter::default();
        let s = cilica_domain::new_id();
        f.on_final(AudioChannel::System, s, 5_000, 9_000, "Can you send the copy on Friday?");
        // Short agreement, and a reply that shares a few words.
        assert_eq!(f.on_final(AudioChannel::Mic, cilica_domain::new_id(), 9_200, 9_800, "Yes, Friday."), None);
        assert_eq!(f.on_final(AudioChannel::Mic, cilica_domain::new_id(), 9_900, 12_000, "Sure, I will send it first thing Friday morning."), None);
        // Same words, but a minute later: not echo.
        assert_eq!(f.on_final(AudioChannel::Mic, cilica_domain::new_id(), 70_000, 74_000, "Can you send the copy on Friday?"), None);
    }
}
