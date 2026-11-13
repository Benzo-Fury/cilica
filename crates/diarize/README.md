# cilica-diarize

Speaker diarization, run once per channel after a meeting, and voiceprint matching.

## Public API

- `Diarizer::load(segmentation, embedding)`, `.diarize(16 kHz mono) -> Diarization { speakers,
  turns }`, `.embed(clip)`. Pipeline: a 10 s window every 2.5 s through pyannote segmentation
  3.0 (up to three local speakers per window, powerset output); each local speaker's
  non-overlapped speech is embedded with CAM++ (80-bin Kaldi fbank, `fbank`); embeddings are
  clustered (average linkage, cosine distance 0.6, at most 8 speakers, clusters under 2 s of
  speech folded into their neighbour); who-spoke-when is rebuilt on the 17 ms frame grid by
  voting across overlapping windows. Speakers are numbered by first appearance and carry an
  L2-normalised centroid embedding.
- `assign_segments(turns, segments)`: each transcript segment goes to the speaker who talks most
  during it (or the nearest within 2 s).
- `suggest_people(speakers, known, threshold)` and `SUGGEST_THRESHOLD` (cosine 0.6; measured on
  the fixture: same voice 0.77–0.83, different voices 0.13–0.16). Suggestions are only ever
  shown; linking is the person's choice.

## Models

Downloaded by `cilica-stt`'s `ModelManager` on first use (about 36 MB); this crate never touches
the network.

| Model | File | Source (pinned) | Licence |
| --- | --- | --- | --- |
| pyannote segmentation 3.0, ONNX export by sherpa-onnx | `pyannote-segmentation-3.0.onnx` | huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0 @ 9403a69 (`model.onnx`) | MIT |
| 3D-Speaker CAM++ (VoxCeleb, 16 kHz), ONNX by sherpa-onnx | `campplus-voxceleb-16k.onnx` | huggingface.co/csukuangfj/speaker-embedding-models @ 0743f30 (`3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx`) | Apache-2.0 |

## Must not depend on

`tauri`, `cilica-store`, the network, or any app crate except `cilica-domain`. Heavy: call from a
blocking thread.

## Tests

`cargo test -p cilica-diarize` (clustering, assignment, suggestions, fbank). With the models in
`target/test-models/` (commands at the top of `tests/models.rs`): `cargo test --release -p
cilica-diarize -- --ignored --nocapture` separates the two voices of the stt fixture and checks
voiceprint similarity and suggestions.
