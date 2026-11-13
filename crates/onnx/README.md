# cilica-onnx

Owns the single ONNX Runtime environment for the process. `cilica-stt` (voice-activity detection) and `cilica-diarize` (speaker labelling) both get it from `cilica_onnx::env()`; neither may call `ort::init()` itself, because ONNX Runtime allows one environment per process.

Depends only on `ort`. Test: `cargo test -p cilica-onnx`.
