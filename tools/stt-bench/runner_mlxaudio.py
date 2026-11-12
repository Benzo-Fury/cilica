"""Benchmark runner for models served by mlx-audio on Apple's GPU.

  --family voxtral-rt   Mistral Voxtral Mini 4B Realtime (Apache-2.0): native streaming; the
                        decoder trails the audio by --delay-ms and never revises what it wrote,
                        so every delta is final text.
  --family qwen3        Qwen3-ASR (Apache-2.0): offline only. Strategies: chunked (Margin's live
                        strategy) or offline (whole recording, cut on longer pauses).

Usage: python -I runner_mlxaudio.py --family F --model HF_ID --strategy S --wav W --out O --cache C
         [--delay-ms 480]
"""

import argparse
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import numpy as np  # noqa: E402

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


def run_voxtral(model, clip: np.ndarray, delay_ms: int) -> tuple[Events, float]:
    """Feed 100 ms at a time, run the decoder until it has nothing more to say."""
    clock, events = Clock(), Events()
    sess = model.create_streaming_session(max_tokens=40_000, temperature=0.0, transcription_delay_ms=delay_ms)
    text = [""]

    def step(piece: np.ndarray | None) -> bool:
        if piece is None:
            sess.close()
        else:
            sess.feed(piece)
        changed = False
        while True:
            deltas = sess.step(max_decode_tokens=16)
            if not deltas:
                break
            text[0] += "".join(deltas)
            changed = True
        return changed

    for i in range(0, len(clip), CHUNK):
        piece = clip[i : i + CHUNK]
        changed, at = clock.run((i + len(piece)) / SR, lambda: step(piece))
        if changed:
            events.add("all", "final", text[0], at)

    def drain() -> bool:
        step(None)
        while not sess.done:
            step(np.zeros(0, np.float32))
        return True

    _, at = clock.run(len(clip) / SR, drain)
    events.add("all", "final", text[0], at)
    return events, clock.compute


def qwen_transcriber(model, language: str):
    def transcribe(samples: np.ndarray) -> str:
        if len(samples) < SR // 2:
            samples = np.concatenate([samples, np.zeros(SR // 2 - len(samples), np.float32)])
        out = model.generate(samples, language=language, temperature=0.0)
        return out.text if hasattr(out, "text") else str(out)

    return transcribe


def run_offline(transcribe, clip: np.ndarray) -> tuple[Events, float]:
    seg = Segmenter(SileroVad(VAD), SegmenterConfig(min_silence_ms=600, max_utterance_ms=25_000))
    started = time.perf_counter()
    chunks = seg.push(clip)
    last = seg.flush()
    if last is not None:
        chunks.append(last)
    texts = [(c, transcribe(c.samples)) for c in chunks]
    compute = time.perf_counter() - started
    events = Events()
    for i, (c, t) in enumerate(texts):
        events.add(i, "final", t.strip(), len(clip) / SR + compute, c.start_sample / SR, (c.start_sample + len(c.samples)) / SR)
    return events, compute


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--family", required=True, choices=["voxtral-rt", "qwen3"])
    p.add_argument("--model", required=True)
    p.add_argument("--strategy", required=True, choices=["native", "chunked", "offline"])
    p.add_argument("--wav", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--cache", required=True)
    p.add_argument("--threads", type=int)
    p.add_argument("--accel", action="store_true")
    p.add_argument("--delay-ms", type=int, default=480)
    p.add_argument("--language", default="English")
    a = p.parse_args()

    # Models download into the bench cache, not the user's global Hugging Face cache.
    os.environ["HF_HUB_CACHE"] = str(Path(a.cache) / "models" / "hf")
    from mlx_audio.stt import load  # noqa: E402  (after HF_HUB_CACHE is set)

    started = time.perf_counter()
    model = load(a.model)
    clip = read_clip(a.wav)
    if a.family == "qwen3":
        transcribe = qwen_transcriber(model, a.language)
        transcribe(np.zeros(SR, np.float32))  # warm up
    load_secs = time.perf_counter() - started

    if a.family == "voxtral-rt":
        events, compute = run_voxtral(model, clip, a.delay_ms)
        mode = "stream"
    elif a.strategy == "chunked":
        seg = Segmenter(SileroVad(VAD), SegmenterConfig())
        events, compute = run_chunked(clip, ChunkedStream(transcribe, seg))
        mode = "stream"
    else:
        events, compute = run_offline(transcribe, clip)
        mode = "offline"
    write_result(a.out, mode, len(clip) / SR, load_secs, compute, events, gpu=True)


if __name__ == "__main__":
    main()
