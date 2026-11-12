"""Benchmark runner for Parakeet on Apple's GPU through MLX (parakeet-mlx).

Strategies:
  native    parakeet-mlx streaming: local attention with a key/value cache; draft words show
            at once (partials) and become final as more audio arrives. Audio is added every
            --step-ms (each step re-encodes the cached window, so 100 ms steps waste work).
  chunked   Margin's live strategy (Silero segmenter, partial every second, final on pause)
  offline   the whole recording after the meeting, in 2-minute windows with overlap

Usage: python -I runner_mlx.py --model HF_ID --strategy S --wav W --out O --cache C
         [--step-ms 500] [--context 256,256] [--min-silence-ms 400] [--no-partials]
"""

import argparse
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mlx.core as mx  # noqa: E402
import numpy as np  # noqa: E402
from parakeet_mlx import from_pretrained  # noqa: E402
from parakeet_mlx.audio import get_logmel  # noqa: E402

from common import (  # noqa: E402
    CHUNK,
    SR,
    ChunkedStream,
    Clock,
    Events,
    Segmenter,
    SegmenterConfig,
    SileroVad,
    read_clip,
    run_chunked,
    write_result,
)
from engines import VAD  # noqa: E402


def text_of(tokens) -> str:
    return "".join(t.text for t in tokens).strip()


def run_native(model, clip: np.ndarray, step_ms: int, context: tuple[int, int]) -> tuple[Events, float]:
    """Finalised tokens are one growing final; draft tokens one growing partial."""
    clock, events = Clock(), Events()
    step = SR * step_ms // 1000
    with model.transcribe_stream(context_size=context) as stream:
        pending = []
        last = ("", "")

        def feed(audio: np.ndarray):
            stream.add_audio(mx.array(audio))
            return text_of(stream.finalized_tokens), text_of(stream.draft_tokens)

        for i in range(0, len(clip), CHUNK):
            pending.append(clip[i : i + CHUNK])
            spoken = (i + len(pending[-1])) / SR
            if sum(len(p) for p in pending) < step and i + CHUNK < len(clip):
                continue
            audio = np.concatenate(pending)
            pending = []
            (fin, draft), at = clock.run(spoken, lambda: feed(audio))
            if fin != last[0]:
                events.add("fin", "final", fin, at)
            if draft != last[1]:
                events.add("draft", "partial", draft, at)
            last = (fin, draft)
        # End of meeting: everything still in draft becomes final.
        (full,), at = clock.run(len(clip) / SR, lambda: (stream.result.text.strip(),))
        events.add("fin", "final", full, at)
        events.add("draft", "final", "", at)
    return events, clock.compute


def clip_transcriber(model):
    def transcribe(samples: np.ndarray) -> str:
        if len(samples) < SR // 2:
            samples = np.concatenate([samples, np.zeros(SR // 2 - len(samples), np.float32)])
        mel = get_logmel(mx.array(samples), model.preprocessor_config)
        return model.generate(mel)[0].text

    return transcribe


def run_offline(model, wav: str, audio_secs: float) -> tuple[Events, float]:
    started = time.perf_counter()
    result = model.transcribe(wav, chunk_duration=120.0, overlap_duration=15.0)
    compute = time.perf_counter() - started
    events = Events()
    for i, s in enumerate(result.sentences):
        events.add(i, "final", s.text.strip(), audio_secs + compute, s.start, s.end)
    return events, compute


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--model", required=True)
    p.add_argument("--strategy", required=True, choices=["native", "chunked", "offline"])
    p.add_argument("--wav", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--cache", required=True)
    p.add_argument("--threads", type=int)
    p.add_argument("--accel", action="store_true")
    p.add_argument("--step-ms", type=int, default=500)
    p.add_argument("--context", default="256,256")
    p.add_argument("--min-silence-ms", type=int, default=400)
    p.add_argument("--no-partials", action="store_true")
    a = p.parse_args()

    started = time.perf_counter()
    model = from_pretrained(a.model, cache_dir=Path(a.cache) / "models" / "hf")
    # Warm up kernels so first-call compilation is not counted as transcription time.
    clip_transcriber(model)(np.zeros(SR, dtype=np.float32))
    load_secs = time.perf_counter() - started
    clip = read_clip(a.wav)
    audio_secs = len(clip) / SR

    if a.strategy == "native":
        ctx = tuple(int(x) for x in a.context.split(","))
        events, compute = run_native(model, clip, a.step_ms, ctx)
    elif a.strategy == "chunked":
        seg = Segmenter(SileroVad(VAD), SegmenterConfig(min_silence_ms=a.min_silence_ms))
        events, compute = run_chunked(clip, ChunkedStream(clip_transcriber(model), seg, partials=not a.no_partials))
    else:
        events, compute = run_offline(model, a.wav, audio_secs)
    mode = "offline" if a.strategy == "offline" else "stream"
    write_result(a.out, mode, audio_secs, load_secs, compute, events, gpu=True)


if __name__ == "__main__":
    main()
