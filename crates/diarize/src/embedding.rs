//! Speaker embeddings: 3D-Speaker CAM++ trained on VoxCeleb (Apache-2.0), 512 dimensions, from
//! Kaldi fbank features.

use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

use crate::fbank::{Fbank, NUM_BINS};
use crate::{DiarizeError, DiarizeResult, l2_normalize, ort_env};

pub struct Embedder {
    session: Session,
    fbank: Fbank,
}

impl Embedder {
    pub fn load(path: &Path) -> DiarizeResult<Self> {
        let session = Session::builder(ort_env()?)
            .and_then(|b| b.with_intra_threads(2).map_err(Into::into))
            .and_then(|mut b| b.commit_from_file(path))
            .map_err(|e| DiarizeError::Model(format!("speaker embedding model: {e}")))?;
        Ok(Self { session, fbank: Fbank::new() })
    }

    /// An L2-normalised embedding of 16 kHz speech (at least ~0.5 s for a useful result).
    pub fn embed(&mut self, samples: &[f32]) -> DiarizeResult<Vec<f32>> {
        let feats = self.fbank.compute(samples);
        if feats.len() < 10 {
            return Err(DiarizeError::TooShort);
        }
        let frames = feats.len();
        let flat: Vec<f32> = feats.iter().flat_map(|r| r.iter().copied()).collect();
        let input = Tensor::from_array(([1usize, frames, NUM_BINS], flat)).map_err(DiarizeError::from)?;
        let outputs = self.session.run(ort::inputs![input]).map_err(DiarizeError::from)?;
        let (_, data) = outputs[0].try_extract_tensor::<f32>().map_err(DiarizeError::from)?;
        Ok(l2_normalize(data))
    }
}
