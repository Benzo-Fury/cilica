"""Benchmark runner for engines served by sherpa-onnx (ONNX Runtime, CPU).

Models (the `--model` directory holds the unpacked sherpa-onnx package):
  online-transducer   cache-aware streaming transducers (Nemotron Speech Streaming)
  parakeet            NeMo TDT transducer, offline (Parakeet TDT 0.6B v3)
  moonshine-v2        Moonshine v2, offline
  qwen3               Qwen3-ASR, offline
  canary              NeMo Canary, offline
  cohere              Cohere Transcribe, offline

Strategies:
  native      the model's own streaming (online models only): partial text after every chunk,
              final at the recogniser's endpoint
  chunked     Margin's live strategy (Silero segmenter + partial every second + final on pause)
              around an offline model; --min-silence-ms / --no-partials tune it
  offline     the whole recording after the meeting: segmented on longer pauses (up to 25 s)
              and decoded segment by segment

Usage: python -I runner_sherpa.py --model-kind K --model DIR --strategy S --wav W --out O
         --cache C [--threads N] [--accel] [--min-silence-ms 400] [--no-partials] [--language en]
"""

import argparse
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import numpy as np  # noqa: E402
import sherpa_onnx  # noqa: E402

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

# ---------------------------------------------------------------------------------------
# Loading
# ---------------------------------------------------------------------------------------


def pick(d: Path, *patterns: str) -> str:
    """The first file in `d` matching a pattern, preferring int8 builds."""
    for pattern in patterns:
        found = sorted(d.glob(pattern), key=lambda p: ("int8" not in p.name, p.name))
        if found:
            return str(found[0])
    raise FileNotFoundError(f"{d}: none of {patterns}")


def load(kind: str, d: Path, threads: int, language: str | None):
    if kind == "online-transducer":
        return sherpa_onnx.OnlineRecognizer.from_transducer(
            tokens=pick(d, "tokens.txt"),
            encoder=pick(d, "encoder*.onnx"),
            decoder=pick(d, "decoder*.onnx"),
            joiner=pick(d, "joiner*.onnx"),
            num_threads=threads,
            enable_endpoint_detection=True,
            # Close an utterance after 0.8 s of trailing silence (1.2 s before any text).
            rule1_min_trailing_silence=1.2,
            rule2_min_trailing_silence=0.8,
            rule3_min_utterance_length=20.0,
        )
    if kind == "parakeet":
        return sherpa_onnx.OfflineRecognizer.from_transducer(
            encoder=pick(d, "encoder*.onnx"),
            decoder=pick(d, "decoder*.onnx"),
            joiner=pick(d, "joiner*.onnx"),
            tokens=pick(d, "tokens.txt"),
            num_threads=threads,
            model_type="nemo_transducer",
        )
    if kind == "moonshine-v2":
        return sherpa_onnx.OfflineRecognizer.from_moonshine_v2(
            encoder=pick(d, "encoder*.onnx", "*encoder*.ort"),
            decoder=pick(d, "*decoder*.onnx", "*decoder*.ort"),
            tokens=pick(d, "tokens.txt"),
            num_threads=threads,
        )
    if kind == "qwen3":
        return sherpa_onnx.OfflineRecognizer.from_qwen3_asr(
            conv_frontend=pick(d, "conv*.onnx"),
            encoder=pick(d, "encoder*.onnx"),
            decoder=pick(d, "decoder*.onnx"),
            tokenizer=str(next(p for p in [d / "tokenizer", d] if p.exists())),
            num_threads=threads,
            max_new_tokens=256,
        )
    if kind == "canary":
        return sherpa_onnx.OfflineRecognizer.from_nemo_canary(
            encoder=pick(d, "encoder*.onnx"),
            decoder=pick(d, "decoder*.onnx"),
            tokens=pick(d, "tokens.txt"),
            src_lang=language or "en",
            tgt_lang=language or "en",
            num_threads=threads,
        )
    if kind == "cohere":
        return sherpa_onnx.OfflineRecognizer.from_cohere_transcribe(
            encoder=pick(d, "encoder*.onnx"),
            decoder=pick(d, "decoder*.onnx"),
            tokens=pick(d, "tokens.txt"),
            num_threads=threads,
            language=language or "en",
        )
    raise ValueError(kind)


def clip_transcriber(rec):
    """`samples -> text` for an offline recogniser."""

    def transcribe(samples: np.ndarray) -> str:
        if len(samples) < SR // 2:
            samples = np.concatenate([samples, np.zeros(SR // 2 - len(samples), np.float32)])
        s = rec.create_stream()
        s.accept_waveform(SR, samples)
        rec.decode_stream(s)
        return s.result.text

    return transcribe


# ---------------------------------------------------------------------------------------
# Strategies
# ---------------------------------------------------------------------------------------


def run_native(rec, clip: np.ndarray) -> tuple[Events, float]:
    """Online recogniser: decode as audio arrives; partial when the text changes."""
    clock, events = Clock(), Events()
    stream = rec.create_stream()
    state = {"id": 1, "last": "", "seg_start": 0.0}

    def step(piece: np.ndarray | None, now_audio: float):
        out = []
        if piece is None:
            stream.input_finished()
        else:
            stream.accept_waveform(SR, piece)
        while rec.is_ready(stream):
            rec.decode_stream(stream)
        text = rec.get_result(stream).strip()
        if piece is None or rec.is_endpoint(stream):
            if text or state["last"]:
                out.append((state["id"], "final", text, state["seg_start"], now_audio))
            state["id"] += 1
            state["last"] = ""
            state["seg_start"] = now_audio
            if piece is not None:
                rec.reset(stream)
        elif text and text != state["last"]:
            state["last"] = text
            out.append((state["id"], "partial", text, state["seg_start"], now_audio))
        return out

    for i in range(0, len(clip), CHUNK):
        piece = clip[i : i + CHUNK]
        t = (i + len(piece)) / SR
        evs, at = clock.run(t, lambda: step(piece, t))
        for e in evs:
            events.add(*e[:3], at, *e[3:])
    # Flush with a little silence so the last words come out, then finish.
    tail = np.zeros(SR, dtype=np.float32)
    t = len(clip) / SR
    evs, at = clock.run(t, lambda: step(tail, t) + step(None, t))
    for e in evs:
        events.add(*e[:3], at, *e[3:])
    return events, clock.compute


def split_sentences(text: str) -> list[str]:
    """Split after . ? ! followed by a space (keeps the punctuation)."""
    out, cur = [], []
    words = text.split()
    for i, w in enumerate(words):
        cur.append(w)
        if w[-1:] in ".?!" and i + 1 < len(words):
            out.append(" ".join(cur))
            cur = []
    if cur:
        out.append(" ".join(cur))
    return out


def run_native_continuous(rec, clip: np.ndarray) -> tuple[Events, float]:
    """Online recogniser that is never reset, so it keeps its context for the whole meeting.
    Text is split into sentences for display; every update is final (append-only)."""
    clock, events = Clock(), Events()
    stream = rec.create_stream()
    state = {"done": 0, "last": ""}

    def step(piece: np.ndarray | None):
        if piece is None:
            stream.input_finished()
        else:
            stream.accept_waveform(SR, piece)
        while rec.is_ready(stream):
            rec.decode_stream(stream)
        sentences = split_sentences(rec.get_result(stream).strip())
        out = []
        closed = len(sentences) if piece is None else max(len(sentences) - 1, 0)
        for k in range(state["done"], closed):
            out.append((k, "final", sentences[k]))
        state["done"] = max(state["done"], closed)
        if piece is not None and len(sentences) > state["done"]:
            tail = sentences[state["done"]]
            # Cache-aware transducers only ever append tokens, so text on screen is already
            # final (only the last word's spelling can still grow).
            if tail != state["last"]:
                state["last"] = tail
                out.append((state["done"], "final", tail))
        return out

    for i in range(0, len(clip), CHUNK):
        piece = clip[i : i + CHUNK]
        evs, at = clock.run((i + len(piece)) / SR, lambda: step(piece))
        for e in evs:
            events.add(*e, at)
    tail = np.zeros(SR, dtype=np.float32)
    evs, at = clock.run(len(clip) / SR, lambda: step(tail) + step(None))
    for e in evs:
        events.add(*e, at)
    return events, clock.compute


def run_offline(transcribe, clip: np.ndarray) -> tuple[Events, float]:
    """The whole recording after the meeting: cut on longer pauses, decode each piece."""
    seg = Segmenter(SileroVad(VAD), SegmenterConfig(min_silence_ms=600, max_utterance_ms=25_000))
    started = time.perf_counter()
    chunks = seg.push(clip)
    last = seg.flush()
    if last is not None:
        chunks.append(last)
    events = Events()
    texts = [(c, transcribe(c.samples)) for c in chunks]
    compute = time.perf_counter() - started
    at = len(clip) / SR + compute
    for i, (c, text) in enumerate(texts):
        events.add(i, "final", text.strip(), at, c.start_sample / SR, (c.start_sample + len(c.samples)) / SR)
    return events, compute


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--model-kind", required=True)
    p.add_argument("--model", required=True)
    p.add_argument("--strategy", required=True, choices=["native", "continuous", "chunked", "offline"])
    p.add_argument("--wav", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--cache")
    p.add_argument("--threads", type=int, default=4)
    p.add_argument("--accel", action="store_true")  # sherpa-onnx runs on the CPU here
    p.add_argument("--min-silence-ms", type=int, default=400)
    p.add_argument("--no-partials", action="store_true")
    p.add_argument("--language")
    a = p.parse_args()

    model_dir = Path(a.model)
    if not model_dir.is_absolute() and a.cache:
        model_dir = Path(a.cache) / "models" / model_dir
    started = time.perf_counter()
    rec = load(a.model_kind, model_dir, a.threads, a.language)
    load_secs = time.perf_counter() - started
    clip = read_clip(a.wav)

    if a.strategy == "native":
        events, compute = run_native(rec, clip)
    elif a.strategy == "continuous":
        events, compute = run_native_continuous(rec, clip)
    elif a.strategy == "chunked":
        seg = Segmenter(SileroVad(VAD), SegmenterConfig(min_silence_ms=a.min_silence_ms))
        events, compute = run_chunked(clip, ChunkedStream(clip_transcriber(rec), seg, partials=not a.no_partials))
    else:
        events, compute = run_offline(clip_transcriber(rec), clip)
    mode = "offline" if a.strategy == "offline" else "stream"
    write_result(a.out, mode, len(clip) / SR, load_secs, compute, events, threads=a.threads)


if __name__ == "__main__":
    main()
