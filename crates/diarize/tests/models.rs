//! Diarization against the real ONNX models on the two-voice fixture. Ignored by default:
//!
//! ```sh
//! mkdir -p target/test-models && cd target/test-models
//! curl -L -o pyannote-segmentation-3.0.onnx https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/main/model.onnx
//! curl -L -o campplus-voxceleb.onnx https://huggingface.co/csukuangfj/speaker-embedding-models/resolve/main/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx
//! cd ../.. && cargo test -p cilica-diarize -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::time::Instant;

use cilica_diarize::{Diarizer, KnownVoice, SUGGEST_THRESHOLD, assign_segments, cosine, suggest_people};

fn models() -> Option<Diarizer> {
    let dir = std::env::var_os("CILICA_TEST_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-models"));
    let (seg, emb) = (dir.join("pyannote-segmentation-3.0.onnx"), dir.join("campplus-voxceleb.onnx"));
    if !seg.exists() || !emb.exists() {
        eprintln!("skipping: models not in {}", dir.display());
        return None;
    }
    Some(Diarizer::load(&seg, &emb).expect("models load"))
}

fn fixture() -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../stt/tests/fixtures/two_voices.wav");
    hound::WavReader::open(path).unwrap().into_samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
}

/// Ground truth from two_voices.json: (start, end, speaker).
const TURNS: [(u32, u32, usize); 4] = [(300, 3519, 0), (4119, 7444, 1), (8044, 10956, 0), (11556, 14671, 1)];

#[test]
#[ignore = "needs the speaker models in target/test-models"]
fn separates_two_voices() {
    let Some(mut d) = models() else { return };
    let audio = fixture();
    let started = Instant::now();
    let result = d.diarize(&audio).unwrap();
    println!("diarized {:.1} s in {:?}: {} speakers, turns {:?}", audio.len() as f32 / 16000.0, started.elapsed(), result.speakers.len(), result.turns);
    assert_eq!(result.speakers.len(), 2, "two speakers");
    let segs: Vec<_> = TURNS.iter().map(|(s, e, _)| (cilica_domain::new_id(), *s, *e)).collect();
    let got = assign_segments(&result.turns, &segs);
    assert_eq!(got.len(), 4);
    let sp: Vec<usize> = got.iter().map(|g| g.1).collect();
    println!("assigned speakers per turn: {sp:?}");
    assert_eq!(sp[0], sp[2]);
    assert_eq!(sp[1], sp[3]);
    assert_ne!(sp[0], sp[1]);
    for s in &result.speakers {
        let n: f32 = s.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 1e-3);
    }
}

#[test]
#[ignore = "needs the speaker models in target/test-models"]
fn voiceprint_similarity_and_suggestions() {
    let Some(mut d) = models() else { return };
    let audio = fixture();
    let clip = |i: usize| {
        let (s, e, _) = TURNS[i];
        audio[(s as usize * 16)..(e as usize * 16)].to_vec()
    };
    let e: Vec<Vec<f32>> = (0..4).map(|i| d.embed(&clip(i)).unwrap()).collect();
    let same_a = cosine(&e[0], &e[2]);
    let same_b = cosine(&e[1], &e[3]);
    let diff = [cosine(&e[0], &e[1]), cosine(&e[0], &e[3]), cosine(&e[2], &e[1]), cosine(&e[2], &e[3])];
    println!("same voice: {same_a:.3}, {same_b:.3}; different voices: {diff:?}");
    assert!(same_a >= SUGGEST_THRESHOLD && same_b >= SUGGEST_THRESHOLD);
    assert!(diff.iter().all(|d| *d < SUGGEST_THRESHOLD));

    // A voiceprint from turn 1 (voice A) is suggested for the speaker holding voice A.
    let result = d.diarize(&audio).unwrap();
    let a = cilica_domain::new_id();
    let known = vec![KnownVoice { person_id: a, embedding: e[0].clone() }];
    let sugg = suggest_people(&result.speakers, &known, SUGGEST_THRESHOLD);
    println!("suggestions: {sugg:?}");
    assert_eq!(sugg.len(), 1);
    let segs = vec![(cilica_domain::new_id(), TURNS[2].0, TURNS[2].1)];
    let who = assign_segments(&result.turns, &segs)[0].1;
    assert_eq!(sugg[0].0, who, "suggested speaker is the one who speaks in voice A");
}
