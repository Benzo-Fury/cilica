//! Run live speaker detection over a 16 kHz mono WAV and write who-spoke-when as
//! `start_ms end_ms label` lines (`S<n>` or `pending`) to `<out dir>/live.txt`, for the speaker
//! benchmark.
//!
//! ```sh
//! cargo run --release -p cilica-diarize --example live_bench -- <models dir> <audio.wav> <out dir> [key=value ...]
//! ```
//! Keys: step, latency (seconds), match, new, min_new, min_embed, emb (embedding model file in the
//! models dir), merge (similarity at which two speakers are folded together; 2 never folds).

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use cilica_diarize::online::{Label, OnlineConfig, OnlineDiarizer};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (models, wav, out) = (PathBuf::from(&args[1]), PathBuf::from(&args[2]), PathBuf::from(&args[3]));
    let seg = models.join("pyannote-segmentation-3.0.onnx");
    let mut emb = models.join("campplus-voxceleb-16k.onnx");
    let audio: Vec<f32> =
        hound::WavReader::open(&wav).unwrap().into_samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
    let secs = |v: &str| (v.parse::<f32>().unwrap() * 16_000.0) as usize;
    let mut cfg = OnlineConfig::default();
    for kv in &args[4..] {
        let (k, v) = kv.split_once('=').unwrap();
        match k {
            "step" => cfg.step = secs(v),
            "latency" => cfg.latency = secs(v),
            "match" => cfg.match_sim = v.parse().unwrap(),
            "new" => cfg.new_sim = v.parse().unwrap(),
            "min_new" => cfg.min_new_secs = v.parse().unwrap(),
            "min_embed" => cfg.min_embed_secs = v.parse().unwrap(),
            "merge" => cfg.merge_sim = v.parse().unwrap(),
            "emb" => emb = models.join(v),
            _ => panic!("unknown key {k}"),
        }
    }
    std::fs::create_dir_all(&out).unwrap();

    // Live: feed 100 ms chunks, as capture does.
    let mut d = OnlineDiarizer::load(&seg, &emb, cfg).unwrap();
    let started = Instant::now();
    let mut spans = Vec::new();
    // Folds rename earlier spans, as the app moves lines to the speaker they joined.
    let mut alias: Vec<usize> = Vec::new();
    for chunk in audio.chunks(1600) {
        spans.extend(d.push(chunk).unwrap());
        for (from, into) in d.take_merges() {
            if alias.len() <= from.max(into) {
                alias.extend(alias.len()..=from.max(into));
            }
            alias[from] = into;
        }
    }
    spans.extend(d.finish());
    let resolve = |mut c: usize| {
        while c < alias.len() && alias[c] != c {
            c = alias[c];
        }
        c
    };
    for s in &mut spans {
        if let Label::Speaker(c) = s.label {
            s.label = Label::Speaker(resolve(c));
        }
    }
    let took = started.elapsed();
    let mut f = std::fs::File::create(out.join("live.txt")).unwrap();
    for s in &spans {
        let l = match s.label {
            Label::Silence => continue,
            Label::Pending => "pending".to_string(),
            Label::Speaker(c) => format!("S{c}"),
        };
        writeln!(f, "{} {} {l}", s.start_ms, s.end_ms).unwrap();
    }
    eprintln!("live: {} speakers, {:.1} s audio in {took:?} ({:.0}x realtime)", d.speakers(), audio.len() as f32 / 16000.0, audio.len() as f32 / 16000.0 / took.as_secs_f32());

}
