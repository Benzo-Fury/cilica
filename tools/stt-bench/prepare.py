"""Build long-form test clips from the AMI meeting corpus (CC BY 4.0).

The Hugging Face copy of AMI (edinburghcstr/ami) stores one row per utterance. Live
transcription needs continuous audio, so each utterance is put back at its place on the
meeting's timeline: far-field (sdm) rows come from one microphone and are written in place;
close-talk (ihm) rows come from each speaker's headset and are mixed. Gaps between
utterances get a quiet noise floor instead of digital silence.

Writes, per clip, `<clip>.wav` (16 kHz mono) and `<clip>.ref.json` (utterances with times
and an estimated time for every word, used for the latency metrics).

Usage: python -I prepare.py <cache dir>
"""

import io
import json
import sys
from pathlib import Path

import numpy as np
import pyarrow.parquet as pq
import soundfile as sf

SR = 16_000
NOISE_DBFS = -65.0

# (clip name, mic setup, meeting, start s, length s)
CLIPS = [
    ("en2002a-ihm", "ihm", "EN2002a", 0.0, 600.0),
    ("en2002a-sdm", "sdm", "EN2002a", 0.0, 600.0),
    ("en2002b-ihm", "ihm", "EN2002b", 0.0, 600.0),
    ("en2002b-sdm", "sdm", "EN2002b", 0.0, 600.0),
]


def build(cache: Path, name: str, setup: str, meeting: str, start: float, length: float) -> None:
    out_dir = cache / "clips"
    out_dir.mkdir(parents=True, exist_ok=True)
    table = pq.read_table(
        cache / "data" / f"ami-{setup}" / "test-00000.parquet",
        filters=[("meeting_id", "=", meeting)],
        columns=["audio", "begin_time", "end_time", "speaker_id", "text"],
    )
    rows = table.to_pylist()
    end = start + length
    rows = [r for r in rows if r["begin_time"] >= start and r["end_time"] <= end]
    rows.sort(key=lambda r: (r["begin_time"], r["end_time"]))

    # Timeline: write far-field audio in place, mix close-talk audio.
    rng = np.random.default_rng(7)
    audio = np.zeros(int(length * SR), dtype=np.float32)
    covered = np.zeros(len(audio), dtype=bool)
    for r in rows:
        samples, sr = sf.read(io.BytesIO(r["audio"]["bytes"]), dtype="float32")
        if samples.ndim > 1:
            samples = samples.mean(axis=1)
        assert sr == SR, f"{name}: unexpected rate {sr}"
        at = int(round((r["begin_time"] - start) * SR))
        n = min(len(samples), len(audio) - at)
        if n <= 0:
            continue
        if setup == "sdm":
            audio[at : at + n] = samples[:n]
        else:
            audio[at : at + n] += samples[:n]
        covered[at : at + n] = True
    floor = 10 ** (NOISE_DBFS / 20)
    audio[~covered] = rng.normal(0, floor, int((~covered).sum())).astype(np.float32)
    peak = float(np.abs(audio).max())
    if peak > 0.99:
        audio *= 0.99 / peak
    sf.write(out_dir / f"{name}.wav", audio, SR, subtype="PCM_16")

    # Reference: utterances in time order; words spread evenly across their utterance.
    utterances = []
    for r in rows:
        words = r["text"].split()
        b, e = r["begin_time"] - start, r["end_time"] - start
        step = (e - b) / max(len(words), 1)
        utterances.append(
            {
                "speaker": r["speaker_id"],
                "start": round(b, 3),
                "end": round(e, 3),
                "text": r["text"],
                "word_ends": [round(b + step * (i + 1), 3) for i in range(len(words))],
            }
        )
    ref = {
        "clip": name,
        "source": f"AMI {meeting} ({setup}), {start:.0f}-{end:.0f} s, CC BY 4.0",
        "seconds": length,
        "utterances": utterances,
    }
    (out_dir / f"{name}.ref.json").write_text(json.dumps(ref, indent=1))
    words = sum(len(u["word_ends"]) for u in utterances)
    print(f"{name}: {len(utterances)} utterances, {words} words, speech covers {covered.mean():.0%}")


def main() -> None:
    cache = Path(sys.argv[1]).resolve()
    for clip in CLIPS:
        build(cache, *clip)


if __name__ == "__main__":
    main()
