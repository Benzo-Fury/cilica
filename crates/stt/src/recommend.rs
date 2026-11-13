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

/// A computer this weak is better served by a cloud service.
pub fn suggest_cloud(cores: u32, ram_gb: u32) -> bool {
    cores < 4 || ram_gb < 8
}

/// Build a profile from what the system reports.
pub fn profile(cores: u32, ram_gb: u32, apple_silicon: bool) -> HardwareProfile {
    HardwareProfile { cores, ram_gb, apple_silicon, suggest_cloud: suggest_cloud(cores, ram_gb) }
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
