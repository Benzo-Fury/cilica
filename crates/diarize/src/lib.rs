//! Telling speakers apart for Cilica, live, on this computer ([`online`]), and voice embeddings
//! for recognising Saved people ([`embedding`]).
//!
//! Models are loaded from paths (downloaded by `cilica-stt`'s model manager); this crate never
//! touches the network. See README.md for sources and licences.

pub mod embedding;
pub mod fbank;
pub mod online;
pub mod segmentation;

use cilica_domain::{AppError, ErrorCode};

pub const SAMPLE_RATE: u32 = 16_000;

/// Cosine similarity at or above which a cluster is suggested as a Saved person. Tuned on CAM++:
/// the same synthetic voice across turns scores 0.7–0.9, different voices below 0.4.
pub const SUGGEST_THRESHOLD: f32 = 0.6;

/// Cosine similarity at or above which Cilica names a speaker as a Saved person by itself (still
/// a guess the person can correct). Stricter than [`SUGGEST_THRESHOLD`]: between the two Cilica
/// only asks.
pub const RECOGNISE_THRESHOLD: f32 = 0.72;

#[derive(Debug, thiserror::Error)]
pub enum DiarizeError {
    #[error("The speaker models could not be loaded: {0}. They download with the speech model; try downloading it again in Settings › Transcription.")]
    Model(String),
    #[error("Speaker detection failed: {0}")]
    Inference(String),
    #[error("Not enough speech to tell speakers apart.")]
    TooShort,
}

impl From<ort::Error> for DiarizeError {
    fn from(e: ort::Error) -> Self {
        DiarizeError::Inference(e.to_string())
    }
}

pub type DiarizeResult<T> = Result<T, DiarizeError>;

impl From<DiarizeError> for AppError {
    fn from(e: DiarizeError) -> Self {
        let code = match e {
            DiarizeError::Model(_) => ErrorCode::Unavailable,
            _ => ErrorCode::Internal,
        };
        AppError::new(code, e.to_string())
    }
}

pub(crate) fn ort_env() -> DiarizeResult<&'static ort::environment::Environment> {
    // Shared with voice-activity detection: ONNX Runtime allows one environment per process.
    cilica_onnx::env().map_err(|e| DiarizeError::Model(format!("ONNX Runtime: {e}")))
}

pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n <= f32::EPSILON { v.to_vec() } else { v.iter().map(|x| x / n).collect() }
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut d, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        d += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= f32::EPSILON || nb <= f32::EPSILON { 0.0 } else { d / (na.sqrt() * nb.sqrt()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_and_normalising() {
        let v = l2_normalize(&[3.0, 4.0]);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[2.0, 0.0]) - 1.0).abs() < 1e-6);
        assert_eq!(cosine(&[1.0, 0.0], &[0.0, 1.0]), 0.0);
        assert_eq!(cosine(&[1.0], &[1.0, 0.0]), 0.0, "different models never match");
    }
}
