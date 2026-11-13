//! Which local speech model suits a computer.
//!
//! Measured with Cilica's live pipeline on meeting audio: Whisper small and base transcribe about
//! as well as Large v3 Turbo live, at a fraction of the cost, while Turbo is clearly better for the
//! careful pass over the whole recording after the meeting. So strong machines get Turbo (it does
//! both jobs with one download), middling ones Small, and weak ones Base plus a nudge towards a
//! cloud service.

use cilica_domain::HardwareProfile;

/// Id of the model to recommend.
pub fn recommended_model(cores: u32, ram_gb: u32, apple_silicon: bool) -> &'static str {
    if apple_silicon && ram_gb >= 8 || cores >= 8 && ram_gb >= 16 {
        "large-v3-turbo-q5_0"
    } else if cores >= 4 && ram_gb >= 8 {
        "small"
    } else {
        "base"
    }
}

/// A computer that tells speakers apart on itself comfortably while it records (and, with a
/// local engine, transcribes): Apple silicon or a strong PC, with memory to spare.
pub fn local_speakers_comfortable(cores: u32, ram_gb: u32, apple_silicon: bool) -> bool {
    ram_gb >= 16 && (apple_silicon || cores >= 8)
}

/// Id of the voice model to recommend for live speakers: a heavier, more accurate one only where
/// it keeps up with room to spare.
pub fn recommended_speaker_model(cores: u32, ram_gb: u32, apple_silicon: bool) -> &'static str {
    let strong = local_speakers_comfortable(cores, ram_gb, apple_silicon) && cores >= 10;
    crate::models::SPEAKER_MODELS
        .iter()
        .rev()
        .find(|m| !m.heavy || strong)
        .map(|m| m.id)
        .unwrap_or(cilica_domain::DEFAULT_SPEAKER_MODEL)
}

/// A computer this weak is better served by a cloud service.
pub fn suggest_cloud(cores: u32, ram_gb: u32) -> bool {
    cores < 4 || ram_gb < 8
}

/// Build a profile from what the system reports.
pub fn profile(cores: u32, ram_gb: u32, apple_silicon: bool) -> HardwareProfile {
    HardwareProfile {
        cores,
        ram_gb,
        apple_silicon,
        suggest_cloud: suggest_cloud(cores, ram_gb),
        local_speakers: local_speakers_comfortable(cores, ram_gb, apple_silicon),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_by_hardware() {
        // Apple silicon, even a base 8 GB machine, runs Turbo on the GPU.
        assert_eq!(recommended_model(8, 8, true), "large-v3-turbo-q5_0");
        assert_eq!(recommended_model(10, 24, true), "large-v3-turbo-q5_0");
        // A strong PC.
        assert_eq!(recommended_model(12, 32, false), "large-v3-turbo-q5_0");
        // A typical laptop.
        assert_eq!(recommended_model(4, 8, false), "small");
        assert_eq!(recommended_model(8, 8, false), "small");
        // An old PC.
        assert_eq!(recommended_model(2, 8, false), "base");
        assert_eq!(recommended_model(4, 4, false), "base");
    }

    #[test]
    fn local_speakers_only_where_they_keep_up() {
        assert!(local_speakers_comfortable(10, 16, true), "an Apple silicon Mac with 16 GB");
        assert!(local_speakers_comfortable(8, 32, false), "a strong PC");
        assert!(!local_speakers_comfortable(8, 8, true), "a base 8 GB Mac");
        assert!(!local_speakers_comfortable(4, 16, false), "a typical laptop");
        assert!(profile(10, 24, true).local_speakers);
        for (c, r, a) in [(10, 24, true), (8, 8, true), (4, 8, false)] {
            assert!(crate::models::speaker_model(recommended_speaker_model(c, r, a)).is_some());
        }
        assert_eq!(recommended_speaker_model(4, 8, false), cilica_domain::DEFAULT_SPEAKER_MODEL, "weak machines get the light model");
    }

    #[test]
    fn cloud_only_for_weak_machines() {
        assert!(suggest_cloud(2, 16));
        assert!(suggest_cloud(8, 4));
        assert!(!suggest_cloud(4, 8));
        assert!(profile(2, 4, false).suggest_cloud);
    }

    #[test]
    fn every_recommendation_is_in_the_catalogue() {
        for (c, r, a) in [(10, 24, true), (4, 8, false), (2, 4, false)] {
            assert!(crate::models::speech_model(recommended_model(c, r, a)).is_some());
        }
    }
}
