"""Shared pieces for the Python engine runners.

- `Clock`: the simulated live clock. A 100 ms chunk can be processed once it has been
  spoken and the previous chunk is done, so a slow engine falls behind as it would live.
- `Segmenter`: a port of Margin's Silero segmenter (crates/stt/src/vad.rs), so engines that
  only transcribe finished chunks can be cut exactly as Margin cuts Whisper's input.
- `ChunkedStream`: Margin's live strategy around any "transcribe this clip" function: a
  partial about every second of an open utterance, a final when it closes.
- `write_result`: the result format `score.py` reads.
"""

import json
import time
from collections import deque
from dataclasses import dataclass

import numpy as np
import onnxruntime as ort
import soundfile as sf

SR = 16_000
CHUNK = 1_600  # capture hands audio over in 100 ms chunks
FRAME = 512  # Silero's frame at 16 kHz
CONTEXT = 64


def read_clip(path: str) -> np.ndarray:
    audio, sr = sf.read(path, dtype="float32")
    assert sr == SR, "clips must be 16 kHz"
    return audio


class Clock:
    """Simulated wall clock for live runs."""

    def __init__(self) -> None:
        self.now = 0.0
        self.compute = 0.0

    def run(self, spoken_until: float, fn):
        """Run `fn` once audio up to `spoken_until` exists; returns (result, time it finished)."""
        started = time.perf_counter()
        result = fn()
        took = time.perf_counter() - started
        self.compute += took
        self.now = max(self.now, spoken_until) + took
        return result, self.now


class Events:
    def __init__(self) -> None:
        self.items: list[dict] = []

    def add(self, uid, kind: str, text: str, at: float, start: float | None = None, end: float | None = None) -> None:
        self.items.append({"id": str(uid), "kind": kind, "text": text, "at": at, "start": start, "end": end})


def write_result(path: str, mode: str, audio_secs: float, load_secs: float, compute_secs: float, events: Events, **extra) -> None:
    with open(path, "w") as f:
        json.dump(
            {
                "mode": mode,
                "audio_secs": audio_secs,
                "load_secs": load_secs,
                "compute_secs": compute_secs,
                "events": events.items,
                **extra,
            },
            f,
            indent=1,
        )


# ---------------------------------------------------------------------------------------
# Margin's segmenter
# ---------------------------------------------------------------------------------------


class SileroVad:
    """Silero VAD v5, one 512-sample frame at a time with its recurrent state."""

    def __init__(self, path: str) -> None:
        opts = ort.SessionOptions()
        opts.intra_op_num_threads = 1
        opts.inter_op_num_threads = 1
        self.session = ort.InferenceSession(path, opts, providers=["CPUExecutionProvider"])
        self.reset()

    def reset(self) -> None:
        self.state = np.zeros((2, 1, 128), dtype=np.float32)
        self.context = np.zeros(CONTEXT, dtype=np.float32)

    def prob(self, frame: np.ndarray) -> float:
        x = np.concatenate([self.context, frame]).astype(np.float32)[None, :]
        out, self.state = self.session.run(None, {"input": x, "state": self.state, "sr": np.array(SR, dtype=np.int64)})
        self.context = frame[-CONTEXT:].copy()
        return float(out.reshape(-1)[0])


@dataclass
class SegmenterConfig:
    start_threshold: float = 0.5
    end_threshold: float = 0.35
    min_silence_ms: int = 400
    pre_roll_ms: int = 300
    post_roll_ms: int = 200
    max_utterance_ms: int = 20_000


def _ms(v: int) -> int:
    return v * SR // 1000


@dataclass
class Chunk:
    start_sample: int
    samples: np.ndarray
    speech_samples: int


class Segmenter:
    """Port of margin_stt::vad::Segmenter (same thresholds, rolls and caps)."""

    def __init__(self, vad: SileroVad, cfg: SegmenterConfig) -> None:
        self.vad, self.cfg = vad, cfg
        self.frame: list[float] = []
        self.pos = 0
        self.preroll: deque = deque()
        self.active: dict | None = None

    def current(self):
        a = self.active
        return None if a is None else (a["start"], np.concatenate(a["audio"]) if a["audio"] else np.zeros(0, np.float32), a["speech"])

    def push(self, samples: np.ndarray) -> list[Chunk]:
        done: list[Chunk] = []
        i = 0
        while i < len(samples):
            take = min(FRAME - len(self.frame), len(samples) - i)
            self.frame.extend(samples[i : i + take])
            i += take
            self.pos += take
            if len(self.frame) == FRAME:
                frame = np.asarray(self.frame, dtype=np.float32)
                self.frame = []
                self._on_frame(self.pos - FRAME, frame, done)
        return done

    def flush(self) -> Chunk | None:
        if self.active is not None and self.frame:
            self.active["audio"].append(np.asarray(self.frame, dtype=np.float32))
        self.frame = []
        return self._close()

    def _on_frame(self, frame_start: int, frame: np.ndarray, done: list[Chunk]) -> None:
        p = self.vad.prob(frame)
        a = self.active
        if a is None:
            if p >= self.cfg.start_threshold:
                pre = np.asarray(self.preroll, dtype=np.float32)
                self.preroll.clear()
                self.active = {"start": frame_start - len(pre), "audio": [pre, frame], "len": len(pre) + FRAME, "speech": FRAME, "silence": 0}
            else:
                self.preroll.extend(frame)
                while len(self.preroll) > _ms(self.cfg.pre_roll_ms):
                    self.preroll.popleft()
            return
        a["audio"].append(frame)
        a["len"] += FRAME
        if p >= self.cfg.end_threshold:
            a["speech"] += FRAME + min(a["silence"], _ms(300))
            a["silence"] = 0
        else:
            a["silence"] += FRAME
        if a["silence"] >= _ms(self.cfg.min_silence_ms) or a["len"] >= _ms(self.cfg.max_utterance_ms):
            c = self._close()
            if c is not None:
                done.append(c)

    def _close(self) -> Chunk | None:
        a, self.active = self.active, None
        if a is None:
            return None
        audio = np.concatenate(a["audio"])
        keep_tail = _ms(self.cfg.post_roll_ms)
        if a["silence"] > keep_tail:
            audio = audio[: len(audio) - (a["silence"] - keep_tail)]
        return Chunk(a["start"], audio, a["speech"])


def has_enough_speech(samples: np.ndarray, speech_samples: int) -> bool:
    if speech_samples < SR // 4 or len(samples) == 0:
        return False
    return float(np.sqrt(np.mean(samples.astype(np.float64) ** 2))) > 0.002


class ChunkedStream:
    """Margin's live strategy around a clip transcriber: partials while an utterance is open
    (at most one per second of new audio, from 0.75 s), a final when it closes.
    `partials=False` sends finals only."""

    def __init__(self, transcribe, segmenter: Segmenter, partials: bool = True) -> None:
        self.transcribe, self.seg, self.partials = transcribe, segmenter, partials
        self.next_id = 1
        self.current_id: int | None = None
        self.last_partial_len = 0

    def _take_id(self) -> int:
        self.next_id += 1
        return self.next_id - 1

    def _final(self, c: Chunk, out: list) -> None:
        uid = self.current_id if self.current_id is not None else self._take_id()
        self.current_id = None
        self.last_partial_len = 0
        text = self.transcribe(c.samples) if has_enough_speech(c.samples, c.speech_samples) else ""
        out.append((uid, "final", text.strip(), c.start_sample / SR, (c.start_sample + len(c.samples)) / SR))

    def push(self, samples: np.ndarray) -> list:
        out: list = []
        for c in self.seg.push(samples):
            self._final(c, out)
        cur = self.seg.current()
        if cur is not None and self.current_id is None:
            self.current_id = self._take_id()
        if self.partials and cur is not None:
            start, audio, speech = cur
            if len(audio) >= SR * 3 // 4 and len(audio) >= self.last_partial_len + SR and has_enough_speech(audio, speech):
                self.last_partial_len = len(audio)
                text = self.transcribe(audio).strip()
                if text:
                    out.append((self.current_id, "partial", text, start / SR, (start + len(audio)) / SR))
        return out

    def finish(self) -> list:
        out: list = []
        c = self.seg.flush()
        if c is not None:
            self._final(c, out)
        return out


def run_chunked(clip: np.ndarray, stream: ChunkedStream) -> tuple[Events, float]:
    """Feed a clip through a ChunkedStream on the simulated clock."""
    clock, events = Clock(), Events()
    for i in range(0, len(clip), CHUNK):
        piece = clip[i : i + CHUNK]
        evs, at = clock.run((i + len(piece)) / SR, lambda: stream.push(piece))
        for uid, kind, text, s, e in evs:
            events.add(uid, kind, text, at, s, e)
    evs, at = clock.run(len(clip) / SR, stream.finish)
    for uid, kind, text, s, e in evs:
        events.add(uid, kind, text, at, s, e)
    return events, clock.compute
