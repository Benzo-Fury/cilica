# margin-stt

Speech-to-text for Margin. Local Whisper is the default and keeps audio on the machine.

## Public API

- `Transcriber` (trait): `open_stream(StreamOptions) -> TranscriptStream`, `transcribe(samples)`.
  `TranscriptStream::push(start_sample, samples) -> Vec<SttEvent>` and `finish()`; events are
  `Partial(Utterance)` (replaced in place by `id`) and `Final(Utterance)` (an empty final
  retracts its partial). Calls block while the model runs: drive streams from their own threads.
- `WhisperLocal::load(WhisperOptions)`: whisper.cpp through `whisper-rs`, Metal on macOS. Both
  channels share one model and decoder state. Live strategy: Silero VAD cuts each channel into
  utterances (300 ms pre-roll, ends after 400 ms of silence, capped at 20 s); while someone
  speaks a partial is decoded about once a second; the final is decoded when they pause. The
  encoder runs over the utterance length plus 1.3 s instead of the full 30 s window (on an M4,
  large-v3-turbo: 0.3–0.4 s instead of 2 s per utterance); segments that start at the end of the
  audio or run past it (Whisper repeating the utterance into the padding) are dropped; finals
  retry at most at temperatures 0.4 and 0.8. Flash attention stays off: with a shortened window
  it garbles output.
- Hallucination guards (`guards`): non-speech annotations, the usual phantom phrases on short or
  doubtful audio, loops ("I think so. I think so. I think so.").
- `vad::{SileroVad, EnergyVad, Segmenter}`: Silero VAD v5 on ONNX Runtime, frame by frame with
  state; an energy fallback when the model is missing.
- `ModelManager` (`<data_dir>/models/`): the catalogue (`list`), resumable, SHA-256-verified
  downloads with progress as `AppEvent::ModelDownload` (`start_download`, `cancel`, `delete`),
  `speech_paths(id)` (or `ModelMissing`, whose message names the fix), and the speaker models
  for `margin-diarize` (`ensure_speaker_models`, fetched on first use, progress id
  `speaker-models`).
- `cloud::CloudTranscriber`: Deepgram, Soniox, AssemblyAI report `Unavailable` for now.

## Models

All pinned to a commit and checked by size and SHA-256.

| Id | File | Source | Licence |
| --- | --- | --- | --- |
| `tiny`, `base`, `small` | `ggml-*.bin` | huggingface.co/ggerganov/whisper.cpp | MIT (weights: OpenAI Whisper, MIT) |
| `large-v3-turbo-q5_0` (default, 574 MB) | `ggml-large-v3-turbo-q5_0.bin` | same | MIT |
| VAD | `silero_vad_v5.onnx` | github.com/snakers4/silero-vad v5.1.2 | MIT |

The default in `TranscriptionSettings::default()` is `large-v3-turbo-q5_0`; a unit test keeps the
catalogue and the default in agreement.

## Must not depend on

`tauri`, `margin-store`, or any app crate except `margin-domain`. Audio never leaves the machine
with the local engine; the only network use is downloading models.

## Tests

- `cargo test -p margin-stt`: guards, segmenter, catalogue, a resumable download against a local
  HTTP server (range requests, checksum failure), encoder window and phantom-segment rules.
- With models (ignored by default): download into `target/test-models/` (commands at the top
  of `tests/whisper.rs`), then `cargo test --release -p margin-stt -- --ignored --nocapture`.
  Reports word error rate, real-time factor and live final lag on `tests/fixtures/two_voices.wav`
  (two `say` voices, ground truth in `two_voices.json`).
- `cargo run --release -p margin-stt --example bench -- <model> <wav> <start_ms-end_ms>…` times
  single-utterance decodes under different encoder windows.
