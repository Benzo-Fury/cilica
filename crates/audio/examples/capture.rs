//! Record the microphone and system audio for a few seconds, print levels, write files.
//!
//! ```sh
//! cargo run -p margin-audio --example capture -- --seconds 5 [--out target/capture] [--no-system] [--ask]
//! ```
//!
//! Writes `<out>/capture.wav` (stereo: mic left, system right) and `<out>/capture.ogg`.
//! On macOS the first run triggers the Microphone and System Audio Recording prompts for the
//! app that launched it (your terminal); `--ask` shows the prompts first and waits for answers.
//! Without a permission macOS delivers silence rather than an error; the summary flags that.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use margin_audio::{CaptureConfig, CaptureEvent, CaptureSession, MicSource, SourceKind, SystemSource, permissions};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let seconds: u64 = arg("--seconds").and_then(|s| s.parse().ok()).unwrap_or(5);
    let out = PathBuf::from(arg("--out").unwrap_or_else(|| "target/capture".into()));
    let with_system = !args.iter().any(|a| a == "--no-system");

    println!("permissions: {:?}", permissions::all());
    if args.iter().any(|a| a == "--ask") {
        // Shows the macOS prompts (for the app that launched this: your terminal) and waits.
        println!("asking for the microphone (answer the prompt)…");
        println!("microphone: {:?}", permissions::request(margin_domain::PermissionKind::Microphone));
        if with_system {
            println!("system audio: {:?}", permissions::request(margin_domain::PermissionKind::SystemAudio));
        }
    }
    match margin_audio::list_input_devices() {
        Ok(devices) => {
            for d in devices {
                println!("input: {}{} [{}]", d.name, if d.is_default { " (default)" } else { "" }, d.id);
            }
        }
        Err(e) => println!("could not list inputs: {e}"),
    }

    std::fs::create_dir_all(&out).expect("create output dir");
    let wav = out.join("capture.wav");
    let config = CaptureConfig {
        mic: Box::new(MicSource::new(None)),
        system: if with_system { Some(Box::new(SystemSource)) } else { None },
        wav_path: wav.clone(),
        echo_cancel: true,
        follow_default_input: true,
        mute_start_ms: 0,
    };
    let (tx, rx) = crossbeam_channel::unbounded();
    let session = match CaptureSession::start(config, tx) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not start: {e}");
            std::process::exit(1);
        }
    };
    println!("recording {seconds} s…");
    let until = Instant::now() + Duration::from_secs(seconds);
    let (mut mic_samples, mut sys_samples) = (0usize, 0usize);
    let (mut mic_peak, mut sys_peak) = (0.0f32, 0.0f32);
    let mut last_print = Instant::now();
    while Instant::now() < until {
        while let Ok(ev) = rx.recv_timeout(Duration::from_millis(50)) {
            match ev {
                CaptureEvent::Levels(l) => {
                    mic_peak = mic_peak.max(l.mic);
                    sys_peak = sys_peak.max(l.system);
                    if last_print.elapsed() >= Duration::from_millis(250) {
                        last_print = Instant::now();
                        println!("mic {:<20} system {:<20}", bar(l.mic), bar(l.system));
                    }
                }
                CaptureEvent::Audio { kind: SourceKind::Mic, samples, .. } => mic_samples += samples.len(),
                CaptureEvent::Audio { kind: SourceKind::System, samples, .. } => sys_samples += samples.len(),
                other => println!("event: {other:?}"),
            }
            if Instant::now() >= until {
                break;
            }
        }
    }
    let summary = session.stop().expect("stop");
    for ev in rx.try_iter() {
        if let CaptureEvent::Audio { kind, samples, .. } = ev {
            match kind {
                SourceKind::Mic => mic_samples += samples.len(),
                SourceKind::System => sys_samples += samples.len(),
            }
        }
    }
    println!(
        "done: {} ms, mic {} samples (peak level {:.2}), system {} samples (peak level {:.2})",
        summary.duration_ms, mic_samples, mic_peak, sys_samples, sys_peak
    );
    println!("wav: {}", wav.display());
    // macOS hands out silence, not an error, when a permission is missing: say so.
    if let Ok(ch) = margin_audio::read_wav_16k(&wav) {
        for (name, c) in ["mic", "system"].iter().zip(&ch) {
            let peak = c.iter().fold(0.0f32, |a, v| a.max(v.abs()));
            let rms = (c.iter().map(|v| v * v).sum::<f32>() / c.len().max(1) as f32).sqrt();
            let note = if peak == 0.0 { "  <- digital silence: permission missing, or nothing playing / said" } else { "" };
            println!("{name}: peak {peak:.4}, rms {rms:.5}{note}");
        }
    }
    let ogg = out.join("capture.ogg");
    match margin_audio::encode_ogg_opus(&wav, &ogg) {
        Ok(ms) => println!("ogg: {} ({ms} ms, {} bytes)", ogg.display(), std::fs::metadata(&ogg).map(|m| m.len()).unwrap_or(0)),
        Err(e) => println!("ogg encoding failed: {e}"),
    }
}

fn bar(level: f32) -> String {
    "#".repeat((level * 20.0).round() as usize)
}
