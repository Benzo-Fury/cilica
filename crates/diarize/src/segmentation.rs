//! pyannote/segmentation-3.0 (MIT): who is speaking, frame by frame, inside a 10 s window, for up
//! to three local speakers (a "powerset" over speaker combinations).

use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

use crate::{DiarizeError, DiarizeResult, ort_env};

pub const WINDOW: usize = 160_000;
/// Samples per output frame and the receptive-field offset of frame 0.
pub const FRAME_STEP: usize = 270;
pub const FRAME_OFFSET: usize = 721;
pub const LOCAL_SPEAKERS: usize = 3;

/// Silence, the three singletons, then the three pairs.
const POWERSET: [[bool; LOCAL_SPEAKERS]; 7] = [
    [false, false, false],
    [true, false, false],
    [false, true, false],
    [false, false, true],
    [true, true, false],
    [true, false, true],
    [false, true, true],
];

/// Centre sample of output frame `k` within its window.
pub fn frame_center(k: usize) -> usize {
    FRAME_OFFSET / 2 + k * FRAME_STEP
}

pub struct Segmenter {
    session: Session,
}

impl Segmenter {
    pub fn load(path: &Path) -> DiarizeResult<Self> {
        let session = Session::builder(ort_env()?)
            .and_then(|b| b.with_intra_threads(2).map_err(Into::into))
            .and_then(|mut b| b.commit_from_file(path))
            .map_err(|e| DiarizeError::Model(format!("segmentation model: {e}")))?;
        Ok(Self { session })
    }

    /// Per-frame activity of the three local speakers in one window of exactly [`WINDOW`] samples.
    pub fn run(&mut self, window: &[f32]) -> DiarizeResult<Vec<[bool; LOCAL_SPEAKERS]>> {
        debug_assert_eq!(window.len(), WINDOW);
        let input = Tensor::from_array(([1usize, 1, WINDOW], window.to_vec())).map_err(DiarizeError::from)?;
        let outputs = self.session.run(ort::inputs![input]).map_err(DiarizeError::from)?;
        let (shape, data) = outputs[0].try_extract_tensor::<f32>().map_err(DiarizeError::from)?;
        let classes = *shape.last().unwrap_or(&7) as usize;
        if classes != POWERSET.len() {
            return Err(DiarizeError::Model(format!("unexpected segmentation output with {classes} classes")));
        }
        Ok(data
            .chunks_exact(classes)
            .map(|row| {
                let best = row.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| i).unwrap_or(0);
                POWERSET[best]
            })
            .collect())
    }
}
