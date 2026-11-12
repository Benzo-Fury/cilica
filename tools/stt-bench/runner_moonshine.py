"""Benchmark runner for Moonshine v2 streaming models (moonshine-voice, MIT).

Moonshine keeps its own line segmentation: a line's text is refined in place (partials)
and closed when the speaker pauses (final). The stream updates at most every
--update-interval seconds of audio, backing off by itself if a pass costs more than that.

Usage: python -I runner_moonshine.py --arch medium|small|tiny --wav W --out O --cache C
         [--update-interval 0.5]
"""

import argparse
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import moonshine_voice as mv  # noqa: E402
from moonshine_voice.transcriber import LineCompleted, LineTextChanged  # noqa: E402

from common import CHUNK, SR, Clock, Events, read_clip, write_result  # noqa: E402

ARCH = {
    "tiny": mv.ModelArch.TINY_STREAMING,
    "small": mv.ModelArch.SMALL_STREAMING,
    "medium": mv.ModelArch.MEDIUM_STREAMING,
}


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--arch", required=True, choices=list(ARCH))
    p.add_argument("--wav", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--cache", required=True)
    p.add_argument("--threads", type=int)
    p.add_argument("--accel", action="store_true")
    p.add_argument("--update-interval", type=float, default=0.5)
    a = p.parse_args()

    # Load (downloads from Moonshine's CDN into the cache on first use).
    started = time.perf_counter()
    path, arch = mv.get_model_for_language("en", ARCH[a.arch], cache_root=Path(a.cache) / "models" / "moonshine")
    transcriber = mv.Transcriber(model_path=path, model_arch=arch, update_interval=a.update_interval)
    stream = transcriber.create_stream(update_interval=a.update_interval)
    pending: list[tuple[str, str, str, float, float]] = []

    def listen(event) -> None:
        line = event.line
        if isinstance(event, LineCompleted):
            pending.append((str(line.line_id if hasattr(line, "line_id") else line.id), "final", line.text, line.start_time, line.start_time + line.duration))
        elif isinstance(event, LineTextChanged):
            pending.append((str(line.line_id if hasattr(line, "line_id") else line.id), "partial", line.text, line.start_time, line.start_time + line.duration))

    stream.add_listener(listen)
    stream.start()
    load_secs = time.perf_counter() - started

    # Feed 100 ms chunks on the simulated clock.
    clip = read_clip(a.wav)
    clock, events = Clock(), Events()

    def flush(at: float) -> None:
        for uid, kind, text, s, e in pending:
            events.add(uid, kind, text, at, s, e)
        pending.clear()

    for i in range(0, len(clip), CHUNK):
        piece = clip[i : i + CHUNK]
        _, at = clock.run((i + len(piece)) / SR, lambda: stream.add_audio(piece.tolist(), SR))
        flush(at)
    _, at = clock.run(len(clip) / SR, stream.stop)
    flush(at)
    write_result(a.out, "stream", len(clip) / SR, load_secs, clock.compute, events)


if __name__ == "__main__":
    main()
