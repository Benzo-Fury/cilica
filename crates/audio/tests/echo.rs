//! Echo cancellation on real (synthesised) speech: a remote voice leaking from the speakers into
//! the mic is removed, and the local voice talking over it survives.

use std::path::PathBuf;

use margin_audio::{EchoCanceller, FRAME_SAMPLES, read_wav_16k};

fn fixture(name: &str) -> Vec<f32> {
    read_wav_16k(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap().remove(0)
}

fn energy(x: &[f32]) -> f32 {
    x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32
}

/// Far end twice in a row (so the filter converges on the first pass), heard by the mic 30 ms
/// later at about -16 dB; the near end talks over the second pass.
fn scene() -> (Vec<f32>, Vec<f32>, Vec<f32>, usize) {
    let far_once = fixture("far_speech.wav");
    let near = fixture("near_speech.wav");
    let mut far = far_once.clone();
    far.extend(&far_once);
    let n = far.len().div_ceil(FRAME_SAMPLES) * FRAME_SAMPLES;
    far.resize(n, 0.0);
    let delay = 480;
    let mut echo = vec![0.0f32; n];
    for i in delay + 2..n {
        echo[i] = 0.16 * far[i - delay] + 0.05 * far[i - delay - 2];
    }
    let mut near_track = vec![0.0f32; n];
    let at = far_once.len() + 8000;
    for (i, s) in near.iter().enumerate() {
        if at + i < n {
            near_track[at + i] = *s;
        }
    }
    (far, echo, near_track, far_once.len())
}

fn run(far: &[f32], mic: &[f32]) -> Vec<f32> {
    let mut aec = EchoCanceller::new();
    assert!(aec.is_active());
    let mut out = mic.to_vec();
    for (r, m) in far.chunks(FRAME_SAMPLES).zip(out.chunks_mut(FRAME_SAMPLES)) {
        aec.process(r, m);
    }
    out
}

#[test]
fn removes_remote_speech_from_the_mic() {
    let (far, echo, _, half) = scene();
    let out = run(&far, &echo);
    // Second pass, after convergence.
    let erle = 10.0 * (energy(&echo[half..]) / energy(&out[half..]).max(1e-12)).log10();
    println!("echo return loss enhancement on speech: {erle:.1} dB");
    assert!(erle > 15.0, "only {erle:.1} dB");
}

#[test]
fn keeps_the_local_voice_during_double_talk() {
    let (far, echo, near, _) = scene();
    let mic: Vec<f32> = echo.iter().zip(&near).map(|(e, n)| e + n).collect();
    let out = run(&far, &mic);
    let span: Vec<usize> = (0..near.len()).filter(|&i| near[i].abs() > 0.0).collect();
    let (lo, hi) = (span[0], *span.last().unwrap());
    let kept = energy(&out[lo..hi]) / energy(&near[lo..hi]);
    println!("near-end energy kept during double talk: {kept:.2}");
    assert!(kept > 0.4, "the local voice was suppressed: {kept:.2}");
}
