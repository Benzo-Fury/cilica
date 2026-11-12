//! One recording session end to end, independent of Tauri:
//! capture → echo cancellation → VAD → speech-to-text → store → events → diarize → summarise.
//!
//! [`RecordingService`] is the whole public surface the shell needs: start, stop, toggle,
//! keep recording, regenerate the summary, meeting detection ([`RecordingService::apply_settings`]
//! and [`RecordingService::handle_detect_event`]), and the launch work
//! ([`RecordingService::startup`]: finish interrupted recordings, apply audio retention). The
//! outside world comes in through [`AudioBackend`], [`SpeechBackend`] and a
//! [`SummarizerProvider`], so a whole session runs in tests from fixture audio.
//!
//! Phases (`AppEvent::RecordingStateChanged`): Idle → Starting → Recording → Stopping (the last
//! audio is transcribed) → Diarizing → Summarizing → Idle.

mod backend;
mod echo;
mod files;
mod finalize;
mod live;
mod service;

pub use backend::{AudioBackend, DeviceAudio, LocalModels, SpeechBackend};
pub use files::RECORDINGS_DIR;
pub use service::{RecordingService, ServiceConfig, SummarizerProvider, default_title};
pub use margin_detect::DetectEvent;

#[cfg(test)]
mod tests;
