use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ProviderKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NoteFont {
    #[default]
    Sans,
    Serif,
    Mono,
}

/// Where the assistant lives in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MarginPlacement {
    /// A panel on the right that pushes the note aside.
    #[default]
    SidePanel,
    /// A bar at the bottom of the page that grows into the conversation.
    FloatingBar,
}

/// What Home shows when no note is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HomeFace {
    /// A different one each visit.
    #[default]
    Random,
    /// Your most repeated filler phrase. Took over from the word of the week, so a kept "word"
    /// still reads as this.
    #[serde(alias = "word")]
    Tics,
    Overheard,
    Talk,
    Pace,
    Silence,
    OwnWords,
    Writing,
    Questions,
    Overtime,
    SlowDecision,
    Monologue,
    BareNote,
    MidSentence,
    Spelling,
    Ghostwriter,
    Undos,
    /// This week's swear jar filling up.
    SwearJar,
    /// A stack of coins for each person in this week's meetings.
    WhosPaying,
    /// The empty jar, after a week with coins in it.
    CleanWeek,
    Doodle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct AppearanceSettings {
    pub theme: ThemeMode,
    /// When false every glass layer paints its opaque ground.
    pub transparency: bool,
    pub note_font: NoteFont,
    pub margin_placement: MarginPlacement,
    pub home_face: HomeFace,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            transparency: true,
            note_font: NoteFont::Sans,
            margin_placement: MarginPlacement::SidePanel,
            home_face: HomeFace::Random,
        }
    }
}

/// What to do when a meeting app is seen holding the mic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DetectMode {
    /// Show an action toast with Record.
    #[default]
    AskFirst,
    AutoStart,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct RecordingSettings {
    /// "Listening for meetings".
    pub auto_detect: bool,
    pub detect_mode: DetectMode,
    /// Display names of apps that count as meetings.
    pub meeting_apps: Vec<String>,
    /// `None` follows the system default input.
    pub input_device_id: Option<String>,
    /// Audio files older than this are removed. 0 keeps them for ever.
    pub audio_retention_days: u32,
    /// The one-time consent notice has been shown.
    pub consent_notice_seen: bool,
}

impl Default for RecordingSettings {
    fn default() -> Self {
        Self {
            auto_detect: true,
            detect_mode: DetectMode::AskFirst,
            meeting_apps: ["Zoom", "Microsoft Teams", "Google Meet", "Slack", "WhatsApp", "FaceTime", "Webex", "Discord", "Around"]
                .map(String::from)
                .to_vec(),
            input_device_id: None,
            audio_retention_days: 30,
            consent_notice_seen: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SttEngine {
    #[default]
    WhisperLocal,
    Deepgram,
    Soniox,
    AssemblyAi,
    OpenAi,
    /// Any service with an OpenAI-compatible `/audio/transcriptions` endpoint.
    OpenAiCompatible,
}

impl SttEngine {
    /// Audio goes to a third party.
    pub fn is_cloud(self) -> bool {
        self != SttEngine::WhisperLocal
    }

    /// The service can say who is speaking while the meeting is live.
    pub fn has_live_speakers(self) -> bool {
        matches!(self, SttEngine::Soniox | SttEngine::Deepgram | SttEngine::AssemblyAi)
    }
}

/// Settings for cloud engines that need more than a key.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct CloudSttSettings {
    /// Base URL of an OpenAI-compatible service, e.g. "https://api.groq.com/openai/v1".
    pub base_url: Option<String>,
    /// Model name for the OpenAI-compatible service (other providers use their own default).
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct TranscriptionSettings {
    pub engine: SttEngine,
    /// Id of the local Whisper model to use (see `SttModel`).
    pub model_id: String,
    /// ISO 639-1 code, or `None` to detect.
    pub language: Option<String>,
    /// When the meeting ends, transcribe the whole recording again, carefully, with Whisper
    /// Large v3 Turbo on this computer (if it is downloaded), replacing the live transcript.
    /// Only with the local engine: a cloud service's transcript is kept as it is.
    pub refine_after_meeting: bool,
    /// Ask the cloud service to label who is speaking, on the call and on the microphone, while
    /// the meeting is live (only services where [`SttEngine::has_live_speakers`]).
    pub live_speaker_labels: bool,
    /// When the meeting ends, tell speakers apart on this computer from their voices (more
    /// accurate than live labels) and suggest names for voices Margin knows.
    pub identify_speakers_after_meeting: bool,
    /// Name speakers whose voice sounds like a Saved person, live (with live labels) and after
    /// the meeting. Voiceprints stay on this computer.
    pub recognise_voices: bool,
    pub cloud: CloudSttSettings,
}

impl TranscriptionSettings {
    /// The engine will be asked to label speakers on the call as the meeting goes.
    pub fn live_speakers(&self) -> bool {
        self.live_speaker_labels && self.engine.has_live_speakers()
    }
}

impl Default for TranscriptionSettings {
    fn default() -> Self {
        Self {
            engine: SttEngine::WhisperLocal,
            model_id: "large-v3-turbo-q5_0".into(),
            language: None,
            refine_after_meeting: true,
            live_speaker_labels: true,
            identify_speakers_after_meeting: true,
            recognise_voices: true,
            cloud: CloudSttSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct AiSettings {
    pub provider: ProviderKind,
    /// Model for the assistant and summaries.
    pub model: String,
    /// Model for titles, snippets and prompt chips.
    pub fast_model: String,
    /// For OpenAI-compatible and Ollama providers.
    pub base_url: Option<String>,
    /// Promote the Edit tier to auto-apply (still versioned). Destructive can never be promoted.
    pub auto_apply_edits: bool,
    /// Conversations with no new message for this many days are deleted. 0 keeps them for ever.
    pub conversation_retention_days: u32,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: ProviderKind::Anthropic,
            model: "claude-sonnet-5-5".into(),
            fast_model: "claude-haiku-5-5".into(),
            base_url: None,
            auto_apply_edits: false,
            conversation_retention_days: 7,
        }
    }
}

impl AiSettings {
    /// Swap in the default Claude models when the provider serves only Claude models but the stored
    /// ones came from another provider (for example "qwen3:8b" left over after switching from Ollama).
    /// Returns whether anything changed.
    pub fn fit_models_to_provider(&mut self) -> bool {
        if !matches!(self.provider, ProviderKind::Anthropic | ProviderKind::ClaudeCode) {
            return false;
        }
        let defaults = Self::default();
        let mut changed = false;
        if !is_claude_model(&self.model) {
            self.model = defaults.model;
            changed = true;
        }
        if !is_claude_model(&self.fast_model) {
            self.fast_model = defaults.fast_model;
            changed = true;
        }
        changed
    }
}

fn is_claude_model(model: &str) -> bool {
    let m = model.trim().to_ascii_lowercase();
    m.starts_with("claude") || matches!(m.as_str(), "opus" | "sonnet" | "haiku" | "fable")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReadySound {
    #[default]
    Chime,
    Bloom,
    Ready,
    Glass,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct SoundSettings {
    pub enabled: bool,
    pub recording_start_stop: bool,
    pub notes_ready: bool,
    pub notes_ready_sound: ReadySound,
    pub interface_sounds: bool,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self { enabled: true, recording_start_stop: true, notes_ready: true, notes_ready_sound: ReadySound::Chime, interface_sounds: false }
    }
}

/// Settings › Swear jar: what counts, and whether the jar shows itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct SwearJarSettings {
    /// Off stops counting and hides the jar and its faces.
    pub enabled: bool,
    /// Everyone who speaks in a meeting.
    pub transcripts: bool,
    /// Words you type into notes.
    pub notes: bool,
    /// Damn, bloody, crap and friends.
    pub mild: bool,
    /// Show the jar when a coin goes in. Off still counts.
    pub popup: bool,
    /// What each swear would cost, in hundredths of the local currency. 0 is off.
    pub price_cents: u32,
    /// A soft clink when a coin lands, never while recording.
    pub clink: bool,
}

impl Default for SwearJarSettings {
    fn default() -> Self {
        Self { enabled: true, transcripts: true, notes: true, mild: true, popup: true, price_cents: 100, clink: false }
    }
}

/// App updates from GitHub Releases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct UpdateSettings {
    /// Download and install new versions in the background; they apply on the next restart.
    pub auto_install: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self { auto_install: true }
    }
}

/// Everything in Settings. Stored as one JSON document; secrets live in the OS keychain, never here.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct Settings {
    pub appearance: AppearanceSettings,
    pub recording: RecordingSettings,
    pub transcription: TranscriptionSettings,
    pub ai: AiSettings,
    pub sounds: SoundSettings,
    pub swear_jar: SwearJarSettings,
    /// Overrides of the default keybinds: action id → accelerator ("Mod+Shift+R").
    pub keybinds: BTreeMap<String, String>,
    pub launch_at_login: bool,
    pub onboarding_done: bool,
    pub updates: UpdateSettings,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_word_face_reads_as_verbal_tics() {
        let face: HomeFace = serde_json::from_str("\"word\"").unwrap();
        assert_eq!(face, HomeFace::Tics);
        assert_eq!(serde_json::to_string(&HomeFace::Tics).unwrap(), "\"tics\"");
    }
}

#[cfg(test)]
mod ai_model_tests {
    use super::*;

    #[test]
    fn ollama_models_are_replaced_for_anthropic() {
        let mut ai = AiSettings { model: "qwen3:8b".into(), fast_model: "qwen3:8b".into(), ..AiSettings::default() };
        assert!(ai.fit_models_to_provider());
        assert_eq!(ai.model, AiSettings::default().model);
        assert_eq!(ai.fast_model, AiSettings::default().fast_model);
    }

    #[test]
    fn claude_models_and_other_providers_are_left_alone() {
        let mut ai = AiSettings { model: "claude-opus-5-5".into(), ..AiSettings::default() };
        assert!(!ai.fit_models_to_provider());
        let mut ai = AiSettings { provider: ProviderKind::Ollama, model: "qwen3:8b".into(), ..AiSettings::default() };
        assert!(!ai.fit_models_to_provider());
        assert_eq!(ai.model, "qwen3:8b");
    }
}
