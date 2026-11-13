//! Live speaker detection and voiceprints against the real ONNX models on the two-voice
//! fixture. Ignored by default:
//!
//! ```sh
//! mkdir -p target/test-models && cd target/test-models
//! curl -L -o pyannote-segmentation-3.0.onnx https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/main/model.onnx
//! curl -L -o campplus-voxceleb.onnx https://huggingface.co/csukuangfj/speaker-embedding-models/resolve/main/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx
//! cd ../.. && cargo test -p cilica-diarize -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::time::Instant;

use cilica_diarize::embedding::Embedder;
use cilica_diarize::online::{Label, OnlineConfig, OnlineDiarizer, Span};
use cilica_diarize::{SUGGEST_THRESHOLD, cosine};

fn model_paths() -> Option<(PathBuf, PathBuf)> {
    let dir = std::env::var_os("CILICA_TEST_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-models"));
    let (seg, emb) = (dir.join("pyannote-segmentation-3.0.onnx"), dir.join("campplus-voxceleb.onnx"));
    if !seg.exists() || !emb.exists() {
        eprintln!("skipping: models not in {}", dir.display());
        return None;
    }
    Some((seg, emb))
}

fn fixture() -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../stt/tests/fixtures/two_voices.wav");
    hound::WavReader::open(path).unwrap().into_samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
}

/// Ground truth from two_voices.json: (start, end, speaker).
const TURNS: [(u32, u32, usize); 4] = [(300, 3519, 0), (4119, 7444, 1), (8044, 10956, 0), (11556, 14671, 1)];

/// The speaker who talks most between two times.
fn speaker_during(spans: &[Span], start: u32, end: u32) -> Option<usize> {
    let mut talk = std::collections::HashMap::<usize, u32>::new();
    for s in spans {
        if let Label::Speaker(c) = s.label {
            *talk.entry(c).or_default() += end.min(s.end_ms).saturating_sub(start.max(s.start_ms));
        }
    }
    talk.into_iter().filter(|(_, ms)| *ms > 0).max_by_key(|(_, ms)| *ms).map(|(c, _)| c)
}

#[test]
#[ignore = "needs the speaker models in target/test-models"]
fn tells_two_voices_apart_live() {
    let Some((seg, emb)) = model_paths() else { return };
    let mut d = OnlineDiarizer::load(&seg, &emb, OnlineConfig::default()).unwrap();
    let audio = fixture();
    let started = Instant::now();
    let mut spans = Vec::new();
    // 100 ms at a time, as capture delivers it.
    for chunk in audio.chunks(1_600) {
        spans.extend(d.push(chunk).unwrap());
    }
    spans.extend(d.finish());
    println!("{:.1} s in {:?}: {} speakers, {spans:?}", audio.len() as f32 / 16000.0, started.elapsed(), d.speakers());
    assert_eq!(d.speakers(), 2, "two speakers");
    let who: Vec<Option<usize>> = TURNS.iter().map(|(s, e, _)| speaker_during(&spans, *s, *e)).collect();
    println!("speaker per turn: {who:?}");
    assert!(who.iter().all(Option::is_some));
    assert_eq!(who[0], who[2]);
    assert_eq!(who[1], who[3]);
    assert_ne!(who[0], who[1]);
    // Spans tile the timeline in order.
    assert!(spans.windows(2).all(|w| w[0].end_ms == w[1].start_ms));
}

#[test]
#[ignore = "needs the speaker models in target/test-models"]
fn voiceprints_match_the_same_voice() {
    let Some((_, emb)) = model_paths() else { return };
    let mut e = Embedder::load(&emb).unwrap();
    let audio = fixture();
    let clip = |i: usize| {
        let (s, end, _) = TURNS[i];
        audio[(s as usize * 16)..(end as usize * 16)].to_vec()
    };
    let v: Vec<Vec<f32>> = (0..4).map(|i| e.embed(&clip(i)).unwrap()).collect();
    for x in &v {
        let n: f32 = x.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 1e-3, "normalised");
    }
    let (same_a, same_b) = (cosine(&v[0], &v[2]), cosine(&v[1], &v[3]));
    let diff = [cosine(&v[0], &v[1]), cosine(&v[0], &v[3]), cosine(&v[2], &v[1]), cosine(&v[2], &v[3])];
    println!("same voice: {same_a:.3}, {same_b:.3}; different voices: {diff:?}");
    assert!(same_a >= SUGGEST_THRESHOLD && same_b >= SUGGEST_THRESHOLD);
    assert!(diff.iter().all(|d| *d < SUGGEST_THRESHOLD));
}
