//! Time raw whisper.cpp decodes of single utterances with the shortened encoder window Cilica
//! uses live, in order, to see warm-up and per-window-size costs.
//!
//! ```sh
//! cargo run --release -p cilica-stt --example bench -- <model.bin> <16 kHz mono wav> <start_ms-end_ms>...
//! ```

use std::time::Instant;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let audio: Vec<f32> = hound::WavReader::open(&args[2]).unwrap().into_samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
    let spans: Vec<(usize, usize)> = args[3..]
        .iter()
        .map(|a| {
            let (s, e) = a.split_once('-').unwrap();
            (s.parse::<usize>().unwrap() * 16, e.parse::<usize>().unwrap() * 16)
        })
        .collect();
    let mut cp = WhisperContextParameters::default();
    cp.use_gpu(true);
    let t = Instant::now();
    let ctx = WhisperContext::new_with_params(&args[1], cp).unwrap();
    let mut state = ctx.create_state().unwrap();
    println!("load {:.2}s", t.elapsed().as_secs_f32());
    for round in 0..2 {
        for &(s, e) in &spans {
            let clip = &audio[s.saturating_sub(4800)..(e + 3200).min(audio.len())];
            let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            p.set_language(Some("en"));
            p.set_no_context(true);
            p.set_print_progress(false);
            p.set_print_realtime(false);
            p.set_print_timestamps(false);
            p.set_temperature_inc(0.0);
            let frames = (clip.len() as f32 / 16_000.0 * 50.0).ceil() as i32 + 64;
            let audio_ctx = ((frames + 63) / 64 * 64).max(256);
            p.set_audio_ctx(audio_ctx);
            let t = Instant::now();
            state.full(p, clip).unwrap();
            let text: String = (0..state.full_n_segments()).filter_map(|i| state.get_segment(i)).map(|s| s.to_str_lossy().unwrap().into_owned()).collect();
            println!("round {round} ctx {audio_ctx} {:.2}s audio: {:.2}s {text:?}", clip.len() as f32 / 16_000.0, t.elapsed().as_secs_f32());
        }
    }
}
