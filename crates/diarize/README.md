# cilica-diarize

Telling speakers apart live, on this computer, and voice embeddings for recognising Saved
people.

## Public API

- `online::OnlineDiarizer::load(segmentation, embedding, OnlineConfig)`, `.push(16 kHz mono)
  -> Vec<Span>`, `.skip(samples)` (a gap in capture), `.finish()`, `.take_merges()`. Every
  second the last 10 s go through pyannote segmentation 3.0 (up to three local speakers per
  window, powerset output); each local speaker with at least 1 s of clean (non-overlapped)
  speech is embedded and matched one-to-one against the speakers so far (cosine ≥ `match_sim`
  is them; below `new_sim` to everyone with 2 s of clean speech is someone new; anything else is
  `Pending`). Each window votes on its frames; a frame is committed 3 s behind the audio as
  silence, pending or a speaker. Two speakers whose centroids reach `merge_sim` are folded
  together (reported by `take_merges`, so lines given to the later one can move).
- `embedding::Embedder::load(path)`, `.embed(clip)`: an L2-normalised voice embedding (80-bin
  Kaldi fbank, `fbank`). Voiceprints of Saved people are always made with the default CAM++
  model, whichever model tells speakers apart.
- `SUGGEST_THRESHOLD` (cosine 0.6; measured on the fixture: same voice 0.77–0.83, different
  voices 0.13–0.16) and `RECOGNISE_THRESHOLD` (0.72). Suggestions are only ever shown; linking
  is the person's choice.

Measured on three AMI meetings (four people each, headset mix), live detection with CAM++
gave the right person for 94–99% of the speech it labelled, against 76–92% for Soniox's live
labels; about 3–6% of speech is still pending when it is committed. Run the benchmark with
`examples/live_bench.rs`.

## Models

Downloaded by `cilica-stt`'s `ModelManager` on first use (about 36 MB; other voice models from
its `SPEAKER_MODELS` catalogue when the person picks one); this crate never touches the
network.

| Model | File | Source (pinned) | Licence |
| --- | --- | --- | --- |
| pyannote segmentation 3.0, ONNX export by sherpa-onnx | `pyannote-segmentation-3.0.onnx` | huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0 @ 9403a69 (`model.onnx`) | MIT |
| 3D-Speaker CAM++ (VoxCeleb, 16 kHz), ONNX by sherpa-onnx | `campplus-voxceleb-16k.onnx` | huggingface.co/csukuangfj/speaker-embedding-models @ 0743f30 (`3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx`) | Apache-2.0 |

## Must not depend on

`tauri`, `cilica-store`, the network, or any app crate except `cilica-domain`. Heavy: call from a
blocking thread.

## Tests

`cargo test -p cilica-diarize` (similarity, fbank). With the models in `target/test-models/`
(commands at the top of `tests/models.rs`): `cargo test --release -p cilica-diarize -- --ignored
--nocapture` tells the two voices of the stt fixture apart live and checks voiceprint
similarity.
