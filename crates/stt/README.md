# cilica-stt

Speech-to-text for Cilica. Local Whisper is the default and keeps audio on the machine.

## Public API

- `Transcriber` (trait): `open_stream(StreamOptions) -> TranscriptStream`, `transcribe(samples)`,
  `transcribe_recording(samples)` (the careful after-meeting pass; defaults to `transcribe`).
  `TranscriptStream::push(start_sample, samples) -> Vec<SttEvent>` and `finish()`; events are
  `Partial(Utterance)` (replaced in place by `id`) and `Final(Utterance)` (an empty final
  retracts its partial). Calls block while the model runs: drive streams from their own threads.
- `WhisperLocal::load(WhisperOptions)`: whisper.cpp through `whisper-rs`, Metal on macOS. Both
  channels share the model; each stream has its own decoder state, and each decode is primed
  with the stream's last ~200 characters of final text. Live strategy: Silero VAD cuts each
  channel into utterances (300 ms pre-roll; ends after 700 ms of silence, or 250 ms once it is
  6 s long; capped at 12 s); while someone speaks a partial is decoded about once a second; the
  final is decoded when they pause. `transcribe_recording` is the after-meeting pass: beam
  search 5, full 30 s windows, context carried, temperature fallback, and segments where VAD
  heard under 20% speech dropped. The
  encoder runs over the utterance length plus 1.3 s instead of the full 30 s window (on an M4,
  large-v3-turbo: 0.3–0.4 s instead of 2 s per utterance); segments that start at the end of the
  audio or run past it (Whisper repeating the utterance into the padding) are dropped; finals
  retry at most at temperatures 0.4 and 0.8. Flash attention stays off: with a shortened window
  it garbles output.
- Hallucination guards (`guards`): non-speech annotations, video-outro phantoms on short or
  doubtful audio, real short replies ("okay.", "thank you") only when the model doubts there was
  speech, loops ("I think so. I think so. I think so.").
- `vad::{SileroVad, EnergyVad, Segmenter}`: Silero VAD v5 on ONNX Runtime, frame by frame with
  state; an energy fallback when the model is missing.
- `ModelManager` (`<data_dir>/models/`): the catalogue (`list`), resumable, SHA-256-verified
  downloads with progress as `AppEvent::ModelDownload` (`start_download`, `cancel`, `delete`),
  `speech_paths(id)` (or `ModelMissing`, whose message names the fix), and the speaker models
  for `cilica-diarize` (`ensure_speaker_models`, fetched on first use, progress id
  `speaker-models`). `SPEAKER_MODELS` lists the voice models for live speakers with thresholds
  tuned for each (`list_speaker_models`, `speaker_model_path`); they download and delete
  through the same calls as speech models. The default (CAM++) is never deleted: voiceprints
  are made with it.
- `recommend`: which model suits a computer (`recommended_model(cores, ram_gb, apple_silicon)`,
  `suggest_cloud`), whether it tells speakers apart comfortably while it records
  (`local_speakers_comfortable`: Apple silicon or 8+ cores, with 16 GB) and which voice model
  to suggest (`recommended_speaker_model`); `ModelManager::set_hardware` makes `list()` mark it recommended, and
  `download_and_wait` reports when a download ends (download-then-use).
- `cloud::CloudTranscriber` with a `CloudConfig` (engine, a `KeyLoader` the shell reads from the
  keychain, base URL and model for OpenAI-compatible services). Each stream runs its connection
  on its own thread (`cloud::link`), so `push` never waits for the network. Soniox (`stt-rt-v6`),
  Deepgram (Nova-3) and AssemblyAI (Universal-Streaming English, Universal-3.6 Pro otherwise)
  stream 100 ms PCM frames over WebSocket (`cloud::ws`: keepalives, the provider's end message,
  one reconnect); OpenAI (`gpt-transcribe`) and OpenAI-compatible services get Silero-cut
  utterances as WAV (finals only). `verify_key` makes a free authenticated call (for Soniox also
  an empty live stream, to catch keys without real-time access); `check` fails fast before
  recording. Errors never contain the key.
- Speaker labels: with `CloudConfig::speaker_labels`, each channel's stream asks Soniox
  (`enable_speaker_diarization`), Deepgram (`diarize=true`) or AssemblyAI (`speaker_labels=true`)
  to label speakers. Each `Utterance` carries the provider's label in `speaker`, and a change of
  speaker starts a new utterance. Labels are per stream (mic "1" is not call "1").

## Models

All pinned to a commit and checked by size and SHA-256.

| Id | File | Source | Licence |
| --- | --- | --- | --- |
| `base`, `small` | `ggml-*.bin` | huggingface.co/ggerganov/whisper.cpp | MIT (weights: OpenAI Whisper, MIT) |
| `large-v3-turbo-q5_0` (default, 574 MB) | `ggml-large-v3-turbo-q5_0.bin` | same | MIT |
| VAD | `silero_vad_v5.onnx` | github.com/snakers4/silero-vad v5.1.2 | MIT |

The default in `TranscriptionSettings::default()` is `large-v3-turbo-q5_0`; a unit test keeps the
catalogue and the default in agreement.

## Must not depend on

`tauri`, `cilica-store`, `keyring`, or any app crate except `cilica-domain`. Audio never leaves
the machine with the local engine; with a cloud engine it goes only to the provider the person
chose.

## Tests

- `cargo test -p cilica-stt`: guards, segmenter, catalogue and recommendation, a resumable
  download against a local HTTP server (range requests, checksum failure), encoder window and
  phantom-segment rules, the cloud stream timeline, and each streaming provider against a local
  WebSocket server replaying its messages (auth header, settings, partials, finals, rejected key),
  one stream over TLS with both rustls crypto backends compiled in (as in the app), and a
  connection thread that crashes being reported rather than going quiet.
- With models (ignored by default): download into `target/test-models/` (commands at the top
  of `tests/whisper.rs`), then `cargo test --release -p cilica-stt -- --ignored --nocapture`.
  Reports word error rate, real-time factor and live final lag on `tests/fixtures/two_voices.wav`
  (two `say` voices, ground truth in `two_voices.json`).
- `cargo run --release -p cilica-stt --example bench -- <model> <wav> <start_ms-end_ms>…` times
  single-utterance decodes under different encoder windows.
