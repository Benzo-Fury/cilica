"""The engines under test and how to run each one.

`cmd(wav, out, threads, accel, cache, here)` returns the command line. `threads=None`
means the engine's default. `profiles` limits which hardware profiles make sense.
"""

import sys
from pathlib import Path

MARGIN_MODELS = Path.home() / "Library/Application Support/app.margin.desktop/models"
VAD = str(MARGIN_MODELS / "silero_vad_v5.onnx")


def _margin(mode: str, model: str, language: str | None):
    def cmd(wav, out, threads, accel, cache, here):
        path = MARGIN_MODELS / model if (MARGIN_MODELS / model).exists() else cache / "models" / "whisper" / model
        args = [str(here / "margin-runner/target/release/margin-runner"), mode, str(path)]
        if mode == "stream":
            args.append(VAD)
        args += [wav, out]
        if threads:
            args += ["--threads", str(threads)]
        if not accel:
            args.append("--cpu")
        if language:
            args += ["--language", language]
        return args

    return cmd


def _py(script: str, *extra: str):
    def cmd(wav, out, threads, accel, cache, here):
        args = [sys.executable, "-I", str(here / script), *extra, "--wav", wav, "--out", out, "--cache", str(cache)]
        if threads:
            args += ["--threads", str(threads)]
        if accel:
            args.append("--accel")
        return args

    return cmd


ENGINES: dict[str, dict] = {
    # --- Margin today -------------------------------------------------------------------
    "margin-whisper-turbo-live": {
        "label": "Margin today: Whisper turbo q5, live, language auto",
        "cmd": _margin("stream", "ggml-large-v3-turbo-q5_0.bin", None),
    },
    "margin-whisper-turbo-live-en": {
        "label": "Margin today, language set to English",
        "cmd": _margin("stream", "ggml-large-v3-turbo-q5_0.bin", "en"),
    },
    "margin-whisper-small-live-en": {
        "label": "Margin pipeline, Whisper small, English",
        "cmd": _margin("stream", "ggml-small.bin", "en"),
    },
    "margin-whisper-base-live-en": {
        "label": "Margin pipeline, Whisper base, English",
        "cmd": _margin("stream", "ggml-base.bin", "en"),
    },
    "whisper-turbo-offline-en": {
        "label": "Whisper turbo q5, whole recording, beam 5 (after-meeting pass)",
        "cmd": _margin("offline", "ggml-large-v3-turbo-q5_0.bin", "en"),
    },
    # --- Apple SpeechAnalyzer (macOS 26+) ------------------------------------------------
    "apple-speech-live": {
        "label": "Apple SpeechAnalyzer, live (real time)",
        "cmd": lambda wav, out, threads, accel, cache, here: [str(here / "apple-runner/apple-runner"), "--wav", wav, "--out", out, "--realtime"],
        "profiles": ["accel"],
    },
    "apple-speech-offline": {
        "label": "Apple SpeechAnalyzer, whole recording",
        "cmd": lambda wav, out, threads, accel, cache, here: [str(here / "apple-runner/apple-runner"), "--wav", wav, "--out", out],
        "profiles": ["accel"],
    },
    # --- Parakeet on Apple's GPU (MLX) ----------------------------------------------------
    "parakeet-v3-mlx-stream": {
        "label": "Parakeet TDT v3 (MLX, GPU), native streaming",
        "cmd": _py("runner_mlx.py", "--model", "mlx-community/parakeet-tdt-0.6b-v3", "--strategy", "native"),
        "profiles": ["accel"],
    },
    "parakeet-v3-mlx-chunked": {
        "label": "Parakeet TDT v3 (MLX, GPU), Margin's chunking",
        "cmd": _py("runner_mlx.py", "--model", "mlx-community/parakeet-tdt-0.6b-v3", "--strategy", "chunked"),
        "profiles": ["accel"],
    },
    "parakeet-v3-mlx-offline": {
        "label": "Parakeet TDT v3 (MLX, GPU), whole recording",
        "cmd": _py("runner_mlx.py", "--model", "mlx-community/parakeet-tdt-0.6b-v3", "--strategy", "offline"),
        "profiles": ["accel"],
    },
    # --- Other models on Apple's GPU (mlx-audio) -------------------------------------------
    "voxtral-rt-4b-mlx": {
        "label": "Voxtral Mini 4B Realtime (MLX 4-bit, GPU), native streaming, 480 ms delay",
        "cmd": _py("runner_mlxaudio.py", "--family", "voxtral-rt", "--model", "mlx-community/Voxtral-Mini-4B-Realtime-2602-4bit", "--strategy", "native"),
        "profiles": ["accel"],
    },
    "qwen3-asr-1.7b-mlx-offline": {
        "label": "Qwen3-ASR 1.7B (MLX 8-bit, GPU), whole recording",
        "cmd": _py("runner_mlxaudio.py", "--family", "qwen3", "--model", "mlx-community/Qwen3-ASR-1.7B-8bit", "--strategy", "offline"),
        "profiles": ["accel"],
    },
    # --- Moonshine v2 streaming (CPU) -------------------------------------------------------
    "moonshine-medium-stream": {
        "label": "Moonshine v2 medium streaming (CPU)",
        "cmd": _py("runner_moonshine.py", "--arch", "medium"),
        "profiles": ["cpu4", "old2"],
    },
    "moonshine-small-stream": {
        "label": "Moonshine v2 small streaming (CPU)",
        "cmd": _py("runner_moonshine.py", "--arch", "small"),
        "profiles": ["cpu4", "old2"],
    },
    "moonshine-tiny-stream": {
        "label": "Moonshine v2 tiny streaming (CPU)",
        "cmd": _py("runner_moonshine.py", "--arch", "tiny"),
        "profiles": ["old2"],
    },
}

# --- sherpa-onnx (CPU) -------------------------------------------------------------------
# name: (label, model kind, model dir under .cache/models, strategies, extra args)
_PARAKEET = "sherpa/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8"
_SHERPA = {
    "nemotron-en-160-stream": ("Nemotron Streaming EN 0.6B int8, 160 ms chunks", "online-transducer", "sherpa/sherpa-onnx-nemotron-speech-streaming-en-0.6b-160ms-int8-2026-04-25", "native", []),
    "nemotron-en-560-stream": ("Nemotron Streaming EN 0.6B int8, 560 ms chunks", "online-transducer", "sherpa/sherpa-onnx-nemotron-speech-streaming-en-0.6b-560ms-int8-2026-04-25", "native", []),
    "nemotron-en-560-continuous": ("Nemotron Streaming EN 0.6B int8, 560 ms, never reset", "online-transducer", "sherpa/sherpa-onnx-nemotron-speech-streaming-en-0.6b-560ms-int8-2026-04-25", "continuous", []),
    "nemotron-en-160-continuous": ("Nemotron Streaming EN 0.6B int8, 160 ms, never reset", "online-transducer", "sherpa/sherpa-onnx-nemotron-speech-streaming-en-0.6b-160ms-int8-2026-04-25", "continuous", []),
    "nemotron-3.5-560-continuous": ("Nemotron 3.5 Streaming (multilingual) int8, 560 ms, never reset", "online-transducer", "sherpa/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11", "continuous", []),
    "nemotron-3.5-560-stream": ("Nemotron 3.5 Streaming (multilingual) int8, 560 ms chunks", "online-transducer", "sherpa/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11", "native", []),
    "parakeet-v3-chunked": ("Parakeet TDT v3 int8 (CPU), Margin's chunking", "parakeet", _PARAKEET, "chunked", []),
    "parakeet-v3-chunked-700": ("Parakeet TDT v3 int8 (CPU), chunking on 700 ms pauses", "parakeet", _PARAKEET, "chunked", ["--min-silence-ms", "700"]),
    "parakeet-v3-offline": ("Parakeet TDT v3 int8 (CPU), whole recording", "parakeet", _PARAKEET, "offline", []),
    "qwen3-asr-0.6b-offline": ("Qwen3-ASR 0.6B int8 (CPU), whole recording", "qwen3", "sherpa/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25", "offline", []),
}
for _name, (_label, _kind, _dir, _strategy, _extra) in _SHERPA.items():
    ENGINES[_name] = {
        "label": _label,
        "cmd": _py("runner_sherpa.py", "--model-kind", _kind, "--model", _dir, "--strategy", _strategy, *_extra),
        "profiles": ["cpu4", "old2"],
    }
