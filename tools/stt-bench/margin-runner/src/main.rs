//! Benchmark runner for Margin's own Whisper engine.
//!
//! `stream` feeds a clip through `margin_stt::WhisperLocal` exactly as the live pipeline does
//! (100 ms chunks, Silero VAD, partials and finals) and records when every event would have
//! reached the screen. Time is simulated: a chunk can be processed once it has been "spoken"
//! and the previous chunk is done, so a slow engine falls behind just as it would live.
//!
//! `offline` transcribes the whole clip in one whisper.cpp call with the settings a careful
//! after-the-meeting pass would use (full 30 s windows, beam search, running context).
//!
//! Usage:
//!   margin-runner stream  <model.bin> <vad.onnx> <clip.wav> <out.json> [--threads N] [--cpu] [--language xx]
//!   margin-runner offline <model.bin> <clip.wav> <out.json> [--threads N] [--cpu] [--language xx]

use std::path::PathBuf;
use std::time::Instant;

use margin_stt::{StreamOptions, SttEvent, Transcriber, WhisperLocal, WhisperOptions};
use serde_json::{Value, json};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const SR: f64 = 16_000.0;
/// Capture hands audio over in 100 ms chunks.
const CHUNK: usize = 1_600;

// ----------------------------------------------------------------------------------------
// Arguments
// ----------------------------------------------------------------------------------------

/// Flags shared by both modes.
struct Flags {
    threads: Option<usize>,
    cpu: bool,
    language: Option<String>,
}

impl Flags {
    /// Split `--flag value` pairs out of the argument list, leaving the positionals.
    fn parse(args: &mut Vec<String>) -> Self {
        let mut flags = Flags { threads: None, cpu: false, language: None };
        let mut rest = Vec::new();
        let mut it = std::mem::take(args).into_iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--threads" => flags.threads = it.next().and_then(|v| v.parse().ok()),
                "--cpu" => flags.cpu = true,
                "--language" => flags.language = it.next().filter(|l| l != "auto"),
                _ => rest.push(a),
            }
        }
        *args = rest;
        flags
    }
}

/// Read a 16 kHz mono 16-bit WAV as f32.
fn read_wav(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).expect("open clip");
    assert_eq!(reader.spec().sample_rate, 16_000, "clips must be 16 kHz");
    reader.samples::<i16>().map(|s| s.expect("sample") as f32 / 32768.0).collect()
}

fn write_json(path: &str, value: &Value) {
    std::fs::write(path, serde_json::to_string_pretty(value).expect("json")).expect("write result");
}

// ----------------------------------------------------------------------------------------
// Live stream, as the app runs it
// ----------------------------------------------------------------------------------------

fn run_stream(model: &str, vad: &str, clip: &str, out: &str, flags: &Flags) {
    // Load the engine the way the pipeline does.
    let load_started = Instant::now();
    let mut options = WhisperOptions::new(PathBuf::from(model), Some(PathBuf::from(vad)));
    if let Some(t) = flags.threads {
        options.threads = t;
    }
    options.use_gpu = !flags.cpu;
    let threads = options.threads;
    let engine = WhisperLocal::load(options).expect("load whisper");
    let mut stream = engine
        .open_stream(StreamOptions { channel: None, language: flags.language.clone() })
        .expect("open stream");
    let load_secs = load_started.elapsed().as_secs_f64();

    // Feed 100 ms chunks on a simulated clock.
    let audio = read_wav(clip);
    let mut clock = 0.0f64;
    let mut compute = 0.0f64;
    let mut events = Vec::new();
    let mut record = |evs: Vec<SttEvent>, at: f64, events: &mut Vec<Value>| {
        for e in evs {
            let (kind, u) = match e {
                SttEvent::Partial(u) => ("partial", u),
                SttEvent::Final(u) => ("final", u),
            };
            events.push(json!({
                "id": u.id.to_string(), "kind": kind, "text": u.text, "at": at,
                "start": u.start_ms as f64 / 1000.0, "end": u.end_ms as f64 / 1000.0,
            }));
        }
    };
    for (i, chunk) in audio.chunks(CHUNK).enumerate() {
        let spoken = (i * CHUNK + chunk.len()) as f64 / SR;
        let started = Instant::now();
        let evs = stream.push((i * CHUNK) as u64, chunk).expect("push");
        let took = started.elapsed().as_secs_f64();
        compute += took;
        clock = clock.max(spoken) + took;
        record(evs, clock, &mut events);
    }
    let started = Instant::now();
    let evs = stream.finish().expect("finish");
    let took = started.elapsed().as_secs_f64();
    compute += took;
    clock = clock.max(audio.len() as f64 / SR) + took;
    record(evs, clock, &mut events);

    write_json(
        out,
        &json!({
            "mode": "stream",
            "audio_secs": audio.len() as f64 / SR,
            "load_secs": load_secs,
            "compute_secs": compute,
            "threads": threads,
            "gpu": !flags.cpu,
            "events": events,
        }),
    );
}

// ----------------------------------------------------------------------------------------
// Whole clip at once, for an after-the-meeting pass
// ----------------------------------------------------------------------------------------

fn run_offline(model: &str, clip: &str, out: &str, flags: &Flags) {
    // Load the model with the GPU unless asked not to.
    let load_started = Instant::now();
    let mut cparams = WhisperContextParameters::default();
    cparams.use_gpu(!flags.cpu);
    let ctx = WhisperContext::new_with_params(model, cparams).expect("load whisper");
    let mut state = ctx.create_state().expect("state");
    let load_secs = load_started.elapsed().as_secs_f64();

    // Careful settings: beam search, context carried across windows, temperature fallback.
    let threads = flags.threads.unwrap_or_else(|| {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8)
    });
    let mut params = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 });
    params.set_n_threads(threads as i32);
    params.set_language(Some(flags.language.as_deref().unwrap_or("auto")));
    params.set_no_context(false);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    params.set_temperature(0.0);
    params.set_temperature_inc(0.2);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);

    // Transcribe and collect segments.
    let audio = read_wav(clip);
    let started = Instant::now();
    state.full(params, &audio).expect("transcribe");
    let compute = started.elapsed().as_secs_f64();
    let audio_secs = audio.len() as f64 / SR;
    let events: Vec<Value> = (0..state.full_n_segments())
        .filter_map(|i| state.get_segment(i))
        .enumerate()
        .map(|(i, seg)| {
            json!({
                "id": i.to_string(), "kind": "final",
                "text": seg.to_str_lossy().map(|t| t.into_owned()).unwrap_or_default(),
                "at": audio_secs + compute,
                "start": seg.start_timestamp() as f64 / 100.0, "end": seg.end_timestamp() as f64 / 100.0,
            })
        })
        .collect();

    write_json(
        out,
        &json!({
            "mode": "offline",
            "audio_secs": audio_secs,
            "load_secs": load_secs,
            "compute_secs": compute,
            "threads": threads,
            "gpu": !flags.cpu,
            "events": events,
        }),
    );
}

fn main() {
    // Quiet whisper.cpp's own logging.
    whisper_rs::install_logging_hooks();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let flags = Flags::parse(&mut args);
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["stream", model, vad, clip, out] => run_stream(model, vad, clip, out, &flags),
        ["offline", model, clip, out] => run_offline(model, clip, out, &flags),
        _ => {
            eprintln!("usage: margin-runner stream <model> <vad> <clip> <out> | offline <model> <clip> <out> [--threads N] [--cpu] [--language xx]");
            std::process::exit(2);
        }
    }
}
