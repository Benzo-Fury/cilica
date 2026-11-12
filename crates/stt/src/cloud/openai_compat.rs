//! OpenAI, and any service with an OpenAI-compatible `/audio/transcriptions` endpoint (Groq,
//! Fireworks, Together, Mistral, self-hosted servers).
//!
//! The endpoint takes whole files, not a live stream, so speech is cut into utterances with the
//! same Silero segmenter as local Whisper and each utterance is sent as a small WAV when the
//! speaker pauses. Results are finals only: sending the same audio again for partials would
//! cost the person money for nothing. The stream's recent text goes with each request as the
//! `prompt`, so names and context carry over.

use std::time::Duration;

use serde::Deserialize;
use tokio::sync::mpsc::UnboundedReceiver;

use super::link::{Audio, Emitter, LinkStream};
use super::{CloudConfig, http_client, key_or_missing};
use crate::guards;
use crate::vad::{Segmenter, SegmenterConfig, Vad};
use crate::{SAMPLE_RATE, SttError, SttResult};

pub(crate) const OPENAI_BASE: &str = "https://api.openai.com/v1";
pub(crate) const OPENAI_MODEL: &str = "gpt-transcribe";
/// Recent text sent as the prompt (characters).
const PROMPT_CHARS: usize = 200;

/// Where requests go and what to call the service in messages.
#[derive(Clone)]
pub(crate) struct Target {
    pub base_url: String,
    pub model: String,
    pub name: String,
}

impl Target {
    pub fn from_config(config: &CloudConfig) -> SttResult<Self> {
        let openai = config.engine == margin_domain::SttEngine::OpenAi;
        let base_url = match (&config.base_url, openai) {
            (Some(url), _) if !url.trim().is_empty() => url.trim().trim_end_matches('/').to_string(),
            (_, true) => OPENAI_BASE.to_string(),
            (_, false) => return Err(SttError::Provider("Add the service's base URL in Settings › Transcription.".into())),
        };
        let model = match (&config.model, openai) {
            (Some(m), _) if !m.trim().is_empty() => m.trim().to_string(),
            (_, true) => OPENAI_MODEL.to_string(),
            (_, false) => return Err(SttError::Provider("Add the model name in Settings › Transcription.".into())),
        };
        let name = if openai { "OpenAI".to_string() } else { host_of(&base_url) };
        Ok(Self { base_url, model, name })
    }
}

/// "https://api.groq.com/openai/v1" -> "api.groq.com".
fn host_of(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url).to_string()
}

pub(crate) fn open(config: &CloudConfig, language: Option<String>, vad: Box<dyn Vad>) -> SttResult<LinkStream> {
    let target = Target::from_config(config)?;
    let key = config.key.clone();
    LinkStream::spawn("openai", move |audio, emitter| async move {
        let key = key_or_missing(&key, &target.name)?;
        run(audio, emitter, target, key, language, vad).await
    })
}

async fn run(mut audio: UnboundedReceiver<Audio>, emitter: Emitter, target: Target, key: String, language: Option<String>, vad: Box<dyn Vad>) -> SttResult<()> {
    let client = http_client(Duration::from_secs(60));
    let mut seg = Segmenter::new(vad, SegmenterConfig::default());
    let mut pos = 0u64;
    let mut id = 0u64;
    let mut context = String::new();
    while let Some(item) = audio.recv().await {
        let (chunks, end) = match item {
            Audio::Pcm(pcm) => {
                let samples: Vec<f32> = pcm.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                let chunks = seg.push(pos, &samples);
                pos += samples.len() as u64;
                (chunks, false)
            }
            Audio::End => (seg.flush().into_iter().collect(), true),
        };
        for chunk in chunks {
            if !guards::has_enough_speech(&chunk.samples, chunk.speech_samples) {
                continue;
            }
            let start_ms = (chunk.start_sample * 1000 / SAMPLE_RATE as u64) as u32;
            let end_ms = ((chunk.start_sample + chunk.samples.len() as u64) * 1000 / SAMPLE_RATE as u64) as u32;
            let text = match transcribe(&client, &target, &key, &chunk.samples, language.as_deref(), &context).await {
                Ok(t) => t,
                Err(e @ (SttError::Auth(_) | SttError::NotAllowed(_) | SttError::MissingKey(_))) => return Err(e),
                Err(error) => {
                    tracing::warn!(%error, "an utterance could not be transcribed; skipping it");
                    continue;
                }
            };
            if let Some(text) = guards::clean(&text, 0.0, (end_ms - start_ms) as f32 / 1000.0) {
                id += 1;
                emitter.final_(id, &text, start_ms, end_ms, None);
                remember(&mut context, &text);
            }
        }
        if end {
            break;
        }
    }
    Ok(())
}

fn remember(context: &mut String, text: &str) {
    context.push(' ');
    context.push_str(text);
    let n = context.chars().count();
    if n > PROMPT_CHARS {
        *context = context.chars().skip(n - PROMPT_CHARS).collect();
    }
}

#[derive(Deserialize)]
struct Transcription {
    text: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: Option<ErrorDetail>,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: Option<String>,
}

/// One utterance, retried once on a network error, rate limit or server error.
async fn transcribe(client: &reqwest::Client, target: &Target, key: &str, samples: &[f32], language: Option<&str>, prompt: &str) -> SttResult<String> {
    let wav = wav_bytes(samples);
    let mut last = SttError::Network(target.name.clone());
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
        let mut form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(wav.clone())
                    .file_name("audio.wav")
                    .mime_str("audio/wav")
                    .map_err(|e| SttError::Provider(e.to_string()))?,
            )
            .text("model", target.model.clone())
            .text("response_format", "json");
        // `gpt-transcribe` takes `languages` instead of `language` and detects well without it;
        // everything else (Whisper-style services) takes `language`.
        if let Some(l) = language.filter(|l| !l.is_empty() && *l != "auto")
            && !target.model.starts_with("gpt-transcribe")
        {
            form = form.text("language", l.to_string());
        }
        if !prompt.trim().is_empty() {
            form = form.text("prompt", prompt.trim().to_string());
        }
        let response = client.post(format!("{}/audio/transcriptions", target.base_url)).bearer_auth(key).multipart(form).send().await;
        match response {
            Err(_) => last = SttError::Network(target.name.clone()),
            Ok(r) if r.status().is_success() => {
                let body: Transcription = r.json().await.map_err(|_| SttError::Provider(format!("{} sent a reply Margin could not read.", target.name)))?;
                return Ok(body.text);
            }
            Ok(r) => {
                let status = r.status();
                let error = status_error(&target.name, status, r.text().await.unwrap_or_default());
                if !(status.as_u16() == 429 || status.is_server_error()) {
                    return Err(error);
                }
                last = error;
            }
        }
    }
    Err(last)
}

/// Map an HTTP error to a message for people (never containing the key).
pub(crate) fn status_error(name: &str, status: reqwest::StatusCode, body: String) -> SttError {
    match status.as_u16() {
        401 => SttError::Auth(name.to_string()),
        403 => super::not_allowed(name),
        429 => SttError::Provider(format!("{name} is limiting requests right now. Some of the transcript may be missing.")),
        404 => SttError::Provider(format!("{name} does not know that model or address. Check them in Settings › Transcription.")),
        _ => {
            let detail = serde_json::from_str::<ErrorBody>(&body).ok().and_then(|b| b.error).and_then(|e| e.message);
            match detail {
                Some(m) => SttError::Provider(format!("{name}: {m}")),
                None => SttError::Provider(format!("{name} answered with an error ({status}).")),
            }
        }
    }
}

/// A cheap authenticated call: list the models.
pub(crate) async fn verify(target: &Target, key: &str) -> SttResult<()> {
    let client = http_client(Duration::from_secs(15));
    let r = client.get(format!("{}/models", target.base_url)).bearer_auth(key).send().await.map_err(|_| SttError::Network(target.name.clone()))?;
    if r.status().is_success() {
        Ok(())
    } else {
        let status = r.status();
        Err(status_error(&target.name, status, r.text().await.unwrap_or_default()))
    }
}

/// 16 kHz mono 16-bit PCM WAV in memory.
pub(crate) fn wav_bytes(samples: &[f32]) -> Vec<u8> {
    let data = super::link::pcm_bytes(&super::link::to_pcm16(samples));
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_valid() {
        let wav = wav_bytes(&[0.0; 16_000]);
        let r = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        assert_eq!(r.spec().sample_rate, 16_000);
        assert_eq!(r.spec().channels, 1);
        assert_eq!(r.len(), 16_000);
    }

    #[test]
    fn errors_name_the_service_and_never_the_key() {
        let e = status_error("api.groq.com", reqwest::StatusCode::UNAUTHORIZED, "{\"error\":{\"message\":\"Invalid API Key sk-123\"}}".into());
        assert!(matches!(e, SttError::Auth(_)));
        assert!(!e.to_string().contains("sk-123"));
        let e = status_error("OpenAI", reqwest::StatusCode::BAD_REQUEST, "{\"error\":{\"message\":\"Audio file is too short\"}}".into());
        assert_eq!(e.to_string(), "OpenAI: Audio file is too short");
        assert_eq!(host_of("https://api.groq.com/openai/v1"), "api.groq.com");
    }
}
