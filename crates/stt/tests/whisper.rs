//! Tests against real models. They are ignored by default because the models are downloaded,
//! not checked in. To run them:
//!
//! ```sh
//! mkdir -p target/test-models && cd target/test-models
//! curl -LO https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin
//! curl -L -o silero_vad.onnx https://raw.githubusercontent.com/snakers4/silero-vad/v5.1.2/src/silero_vad/data/silero_vad.onnx
//! # optional, for the benchmark:
//! curl -LO https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin
//! cd ../.. && cargo test -p cilica-stt --release -- --ignored --nocapture
//! ```
//!
//! `CILICA_TEST_MODELS` overrides the folder.

use std::path::PathBuf;
use std::time::Instant;

use cilica_stt::vad::{Segmenter, SegmenterConfig, SileroVad};
use cilica_stt::{StreamOptions, SttEvent, Transcriber, WhisperLocal, WhisperOptions};

fn models_dir() -> PathBuf {
    std::env::var_os("CILICA_TEST_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-models"))
}

fn fixture() -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two_voices.wav");
    let r = hound::WavReader::open(path).unwrap();
    assert_eq!(r.spec().sample_rate, 16_000);
    r.into_samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
}

const TRUTH: &str = "Good morning everyone. Let's review the quarterly budget. Thanks. Revenue grew by twelve percent this quarter. That is great news. What about marketing costs? Marketing spending stayed flat compared to last year.";

fn words(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_lowercase())
        .filter(|w| !w.is_empty())
        .map(|w| if w == "12" { "twelve".into() } else { w })
        .flat_map(|w| if w == "12%" { vec!["twelve".to_string(), "percent".to_string()] } else { vec![w] })
        .collect()
}

/// Word error rate by edit distance.
fn wer(reference: &str, hypothesis: &str) -> f32 {
    let r = words(reference);
    let h = words(hypothesis);
    let mut d = vec![vec![0usize; h.len() + 1]; r.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=r.len() {
        for j in 1..=h.len() {
            let sub = if r[i - 1] == h[j - 1] { 0 } else { 1 };
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + sub);
        }
    }
    d[r.len()][h.len()] as f32 / r.len().max(1) as f32
}

fn engine(model: &str) -> Option<WhisperLocal> {
    let dir = models_dir();
    let path = dir.join(model);
    if !path.exists() {
        eprintln!("skipping: {} not found", path.display());
        return None;
    }
    let vad = dir.join("silero_vad.onnx");
    Some(WhisperLocal::load(WhisperOptions::new(path, vad.exists().then_some(vad))).expect("model loads"))
}

#[test]
#[ignore = "needs target/test-models/silero_vad.onnx"]
fn silero_vad_finds_the_four_turns() {
    let path = models_dir().join("silero_vad.onnx");
    let vad = SileroVad::load(&path).expect("vad loads");
    let mut seg = Segmenter::new(Box::new(vad), SegmenterConfig { min_silence_ms: 300, ..Default::default() });
    let audio = fixture();
    let mut chunks = Vec::new();
    for (i, c) in audio.chunks(1600).enumerate() {
        chunks.extend(seg.push(i as u64 * 1600, c));
    }
    chunks.extend(seg.flush());
    let spans: Vec<(u64, u64)> = chunks.iter().map(|c| (c.start_sample / 16, (c.start_sample + c.samples.len() as u64) / 16)).collect();
    println!("speech spans (ms): {spans:?}");
    // Ground truth starts: 300, 4119, 8044, 11556 ms. Each should begin within 400 ms of a turn.
    for truth in [300u64, 4119, 8044, 11556] {
        assert!(spans.iter().any(|(s, _)| s.abs_diff(truth) < 400), "no utterance near {truth} ms");
    }
    // Silence is not speech.
    let mut seg = Segmenter::new(Box::new(SileroVad::load(&path).unwrap()), SegmenterConfig::default());
    let noise: Vec<f32> = (0..160_000).map(|i| ((i * 7919 % 1000) as f32 / 1000.0 - 0.5) * 0.002).collect();
    let mut found = seg.push(0, &noise);
    found.extend(seg.flush());
    assert!(found.is_empty(), "noise produced {} utterances", found.len());
}

#[test]
#[ignore = "needs target/test-models/ggml-tiny.bin"]
fn tiny_transcribes_fixture() {
    let Some(engine) = engine("ggml-tiny.bin") else { return };
    let audio = fixture();
    let started = Instant::now();
    let utterances = engine.transcribe(&audio, Some("en")).unwrap();
    let took = started.elapsed().as_secs_f32();
    let text: Vec<&str> = utterances.iter().map(|u| u.text.as_str()).collect();
    let text = text.join(" ");
    let w = wer(TRUTH, &text);
    println!("tiny: {text:?}\nWER {:.1}%  real-time factor {:.3}", w * 100.0, took / (audio.len() as f32 / 16_000.0));
    assert!(w < 0.25, "WER {w}");
    assert!(utterances.len() >= 3, "utterances split on pauses: {}", utterances.len());
    assert!(utterances.windows(2).all(|p| p[0].end_ms <= p[1].start_ms + 50));
}

#[test]
#[ignore = "needs target/test-models/ggml-tiny.bin"]
fn streaming_emits_partials_then_finals_quickly() {
    let Some(engine) = engine("ggml-tiny.bin") else { return };
    let mut stream = engine.open_stream(StreamOptions { channel: None, language: None }).unwrap();
    let audio = fixture();
    let mut partials = 0;
    let mut finals = Vec::new();
    let mut max_lag_ms = 0u64;
    let wall = Instant::now();
    for (i, chunk) in audio.chunks(1600).enumerate() {
        // Feed in real time so lag is meaningful.
        let due = std::time::Duration::from_millis(i as u64 * 100);
        if let Some(wait) = due.checked_sub(wall.elapsed()) {
            std::thread::sleep(wait);
        }
        for e in stream.push(i as u64 * 1600, chunk).unwrap() {
            match e {
                SttEvent::Partial(_) => partials += 1,
                SttEvent::Final(u) => {
                    let lag = (wall.elapsed().as_millis() as u64).saturating_sub(u.end_ms as u64);
                    max_lag_ms = max_lag_ms.max(lag);
                    finals.push(u);
                }
            }
        }
    }
    finals.extend(stream.finish().unwrap().into_iter().filter_map(|e| if let SttEvent::Final(u) = e { Some(u) } else { None }));
    let text: Vec<String> = finals.iter().map(|u| u.text.clone()).collect();
    println!("partials {partials}, finals {:?}, max final lag {max_lag_ms} ms", text);
    assert!(partials >= 2);
    assert!(finals.len() >= 3);
    assert!(max_lag_ms < 3000, "final lag {max_lag_ms} ms");
    assert!(wer(TRUTH, &text.join(" ")) < 0.25);
}

#[test]
#[ignore = "needs target/test-models/ggml-tiny.bin"]
fn silence_and_noise_produce_no_text() {
    let Some(engine) = engine("ggml-tiny.bin") else { return };
    let mut audio = vec![0.0f32; 16_000 * 5];
    audio.extend((0..16_000 * 5).map(|i| ((i * 7919 % 1000) as f32 / 1000.0 - 0.5) * 0.004));
    let out = engine.transcribe(&audio, None).unwrap();
    assert!(out.is_empty(), "hallucinated: {out:?}");
}

#[test]
#[ignore = "needs target/test-models/ggml-large-v3-turbo-q5_0.bin; run with --release"]
fn benchmark_default_model() {
    let Some(engine) = engine("ggml-large-v3-turbo-q5_0.bin") else { return };
    let audio = fixture();
    // Warm up (Metal shader compilation).
    engine.transcribe(&audio[..32_000], Some("en")).unwrap();
    let started = Instant::now();
    let utterances = engine.transcribe(&audio, Some("en")).unwrap();
    let took = started.elapsed().as_secs_f32();
    let text = utterances.iter().map(|u| u.text.as_str()).collect::<Vec<_>>().join(" ");
    let secs = audio.len() as f32 / 16_000.0;
    println!("large-v3-turbo-q5_0: {text:?}\nWER {:.1}%, {took:.2} s for {secs:.1} s of audio, real-time factor {:.3}", wer(TRUTH, &text) * 100.0, took / secs);
    assert!(wer(TRUTH, &text) < 0.1);

    // Live: fed in real time, how far behind speech does each final land?
    let (finals, partials, max_lag_ms) = stream_in_real_time(&engine, &audio);
    let text = finals.join(" ");
    println!("live: {partials} partials, finals {finals:?}\nWER {:.1}%, max final lag {max_lag_ms} ms", wer(TRUTH, &text) * 100.0);
    assert!(max_lag_ms < 3000, "final lag {max_lag_ms} ms");
    assert!(wer(TRUTH, &text) < 0.1);
}

/// Push 100 ms chunks at real-time pace; returns (final texts, partial count, worst lag of a final
/// behind the end of its speech).
fn stream_in_real_time(engine: &WhisperLocal, audio: &[f32]) -> (Vec<String>, usize, u64) {
    let mut stream = engine.open_stream(StreamOptions { channel: None, language: None }).unwrap();
    let (mut finals, mut partials, mut max_lag_ms) = (Vec::new(), 0, 0u64);
    let wall = Instant::now();
    for (i, chunk) in audio.chunks(1600).enumerate() {
        let due = std::time::Duration::from_millis(i as u64 * 100);
        if let Some(wait) = due.checked_sub(wall.elapsed()) {
            std::thread::sleep(wait);
        }
        for e in stream.push(i as u64 * 1600, chunk).unwrap() {
            match e {
                SttEvent::Partial(_) => partials += 1,
                SttEvent::Final(u) => {
                    max_lag_ms = max_lag_ms.max((wall.elapsed().as_millis() as u64).saturating_sub(u.end_ms as u64));
                    finals.push(u.text);
                }
            }
        }
    }
    for e in stream.finish().unwrap() {
        if let SttEvent::Final(u) = e {
            finals.push(u.text);
        }
    }
    (finals, partials, max_lag_ms)
}
