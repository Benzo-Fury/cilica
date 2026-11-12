//! Cloud engines, behind [`Transcriber`]. The person picks a provider and brings their own key;
//! audio goes to that provider only while they use it.
//!
//! - Soniox, Deepgram and AssemblyAI stream 16 kHz PCM over the provider's WebSocket and send
//!   interim results ([`SttEvent::Partial`]) and finals.
//! - OpenAI and any OpenAI-compatible service take utterances cut at pauses ([`openai_compat`]).
//!
//! Keys come from the OS keychain through a [`KeyLoader`] the shell passes in; this crate never
//! stores, logs or returns them, and every provider error is scrubbed of the key.

mod assemblyai;
mod deepgram;
mod link;
mod openai_compat;
mod soniox;
mod ws;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use margin_domain::SttEngine;

use crate::vad::{EnergyVad, SileroVad, Vad};
use crate::{StreamOptions, SttError, SttResult, Transcriber, TranscriptStream};


/// Reads the provider's key from the keychain each time a stream opens.
pub type KeyLoader = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// What a cloud engine needs from Settings and the keychain.
#[derive(Clone)]
pub struct CloudConfig {
    pub engine: SttEngine,
    pub key: KeyLoader,
    /// OpenAI-compatible services: the base URL (e.g. "https://api.groq.com/openai/v1").
    pub base_url: Option<String>,
    /// Provider model; `None` uses the provider's default.
    pub model: Option<String>,
    /// Ask the provider to label speakers on each channel's stream (where it can).
    pub speaker_labels: bool,
}

/// The provider's name, for messages.
pub fn display_name(engine: SttEngine) -> &'static str {
    match engine {
        SttEngine::Deepgram => "Deepgram",
        SttEngine::Soniox => "Soniox",
        SttEngine::AssemblyAi => "AssemblyAI",
        SttEngine::OpenAi => "OpenAI",
        SttEngine::OpenAiCompatible => "The transcription service",
        SttEngine::WhisperLocal => "Local Whisper",
    }
}

pub struct CloudTranscriber {
    config: CloudConfig,
    /// Silero VAD, for providers that take utterances; `None` uses the energy detector.
    vad_model_path: Option<PathBuf>,
}

impl CloudTranscriber {
    pub fn new(config: CloudConfig, vad_model_path: Option<PathBuf>) -> Self {
        Self { config, vad_model_path }
    }

    fn make_vad(&self) -> Box<dyn Vad> {
        if let Some(path) = self.vad_model_path.as_ref()
            && let Ok(v) = SileroVad::load(path)
        {
            return Box::new(v);
        }
        Box::new(EnergyVad::default())
    }
}

impl Transcriber for CloudTranscriber {
    fn engine(&self) -> SttEngine {
        self.config.engine
    }

    fn open_stream(&self, options: StreamOptions) -> SttResult<Box<dyn TranscriptStream>> {
        let language = options.language.filter(|l| !l.is_empty() && l != "auto");
        // Both channels: the call can have several people on it, and so can the microphone
        // (people in the room, or audio from another device).
        let speakers = self.config.speaker_labels && options.channel.is_some();
        match self.config.engine {
            SttEngine::OpenAi | SttEngine::OpenAiCompatible => {
                Ok(Box::new(openai_compat::open(&self.config, language, self.make_vad())?))
            }
            SttEngine::Soniox => streaming(&self.config, "soniox", move |key| soniox::Soniox::new(key, language, speakers)),
            SttEngine::Deepgram => streaming(&self.config, "deepgram", move |key| deepgram::Deepgram::new(key, language, speakers)),
            SttEngine::AssemblyAi => streaming(&self.config, "assemblyai", move |key| assemblyai::AssemblyAi::new(key, language, speakers)),
            SttEngine::WhisperLocal => Err(SttError::EngineUnavailable("Local Whisper through the cloud engine".into())),
        }
    }
}

/// A WebSocket provider on its own connection thread. The key is read when the stream opens.
fn streaming<P, F>(config: &CloudConfig, name: &str, make: F) -> SttResult<Box<dyn TranscriptStream>>
where
    P: ws::Protocol + 'static,
    F: FnOnce(String) -> P + Send + 'static,
{
    let key = key_or_missing(&config.key, display_name(config.engine))?;
    let stream = link::LinkStream::spawn(name, move |audio, emitter| async move {
        let scrub_key = key.clone();
        ws::run(make(key), audio, emitter).await.map_err(|e| scrub(e, &scrub_key))
    })?;
    Ok(Box::new(stream))
}

/// Fail fast before recording: the key is there and the provider has what it needs.
pub fn check(config: &CloudConfig) -> SttResult<()> {
    if matches!(config.engine, SttEngine::OpenAi | SttEngine::OpenAiCompatible) {
        let target = openai_compat::Target::from_config(config)?;
        key_or_missing(&config.key, &target.name)?;
        return Ok(());
    }
    key_or_missing(&config.key, display_name(config.engine))?;
    Ok(())
}

/// Check a key with a cheap authenticated call before it is saved.
pub async fn verify_key(engine: SttEngine, key: &str, base_url: Option<&str>) -> SttResult<()> {
    let result = match engine {
        SttEngine::OpenAi | SttEngine::OpenAiCompatible => {
            let config = CloudConfig {
                engine,
                key: Arc::new(|| None),
                base_url: base_url.map(str::to_owned),
                model: Some("check".into()),
                speaker_labels: false,
            };
            openai_compat::verify(&openai_compat::Target::from_config(&config)?, key).await
        }
        SttEngine::Soniox => match verify_get(soniox::VERIFY_URL, ("Authorization", format!("Bearer {key}")), "Soniox").await {
            // Listing models needs a different permission from live transcription: check that too.
            Ok(()) => soniox::verify_realtime(key).await,
            other => other,
        },
        SttEngine::Deepgram => verify_get(deepgram::VERIFY_URL, ("Authorization", format!("Token {key}")), "Deepgram").await,
        SttEngine::AssemblyAi => verify_get(assemblyai::VERIFY_URL, ("Authorization", key.to_string()), "AssemblyAI").await,
        SttEngine::WhisperLocal => Ok(()),
    };
    result.map_err(|e| scrub(e, key))
}

/// A cheap authenticated GET that costs nothing (list models or projects).
async fn verify_get(url: &str, (header, value): (&str, String), name: &str) -> SttResult<()> {
    let r = http_client(Duration::from_secs(15))
        .get(url)
        .header(header, value)
        .send()
        .await
        .map_err(|_| SttError::Network(name.to_string()))?;
    match r.status().as_u16() {
        200..=299 => Ok(()),
        401 => Err(SttError::Auth(name.to_string())),
        403 => Err(not_allowed(name)),
        402 => Err(SttError::Provider(format!("{name} says the account is out of credit. Top it up, then try again."))),
        status => Err(SttError::Provider(format!("{name} answered with an error ({status}). Try again in a moment."))),
    }
}

/// A 403: the key is real but may not do this.
pub(crate) fn not_allowed(name: &str) -> SttError {
    SttError::NotAllowed(format!("{name} accepted the key, but the key is not allowed to transcribe. Check its permissions in your {name} account, then try again."))
}

/// The key, or a `MissingKey` error naming the provider.
pub(crate) fn key_or_missing(key: &KeyLoader, name: &str) -> SttResult<String> {
    key().filter(|k| !k.trim().is_empty()).map(|k| k.trim().to_string()).ok_or_else(|| SttError::MissingKey(name.to_string()))
}

/// Remove the key from an error message, should a provider ever echo it.
pub(crate) fn scrub(error: SttError, key: &str) -> SttError {
    match error {
        SttError::Provider(m) if !key.is_empty() && m.contains(key) => SttError::Provider(m.replace(key, "[key]")),
        other => other,
    }
}

pub(crate) fn http_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("Margin/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(timeout)
        .build()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_the_key() {
        let e = scrub(SttError::Provider("bad key abc123 given".into()), "abc123");
        assert_eq!(e.to_string(), "bad key [key] given");
    }

    #[test]
    fn missing_key_names_the_provider() {
        let none: KeyLoader = Arc::new(|| Some("  ".into()));
        let e = key_or_missing(&none, "Soniox").unwrap_err();
        assert_eq!(e.to_string(), "Add your Soniox API key in Settings › Transcription, then start recording again.");
    }
}

#[cfg(test)]
mod stream_tests;
