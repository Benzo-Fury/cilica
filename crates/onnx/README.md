# margin-onnx

Owns the single ONNX Runtime environment for the process. `margin-stt` (voice-activity detection) and `margin-diarize` (speaker labelling) both get it from `margin_onnx::env()`; neither may call `ort::init()` itself, because ONNX Runtime allows one environment per process.

Depends only on `ort`. Test: `cargo test -p margin-onnx`.
