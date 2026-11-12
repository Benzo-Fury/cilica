"""Run engines over the clips under hardware profiles, score everything, write a summary.

Each run is a separate process wrapped in `/usr/bin/time -l` (peak memory). Runs are
sequential so they never compete for the machine.

Profiles (stand-ins on one Apple silicon Mac; rerun on real hardware for real numbers):
  accel  GPU / Neural Engine where the engine supports it, default threads (high-end Mac)
  cpu4   CPU only, 4 threads                                      (typical laptop / PC)
  old2   CPU only, 2 threads, efficiency cores only via taskpolicy (old PC)

Usage:
  python -I run.py run  <engine>[,<engine>...] <profile>[,...] [clip,...]
  python -I run.py summary
  python -I run.py list
"""

import json
import os
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from engines import ENGINES  # noqa: E402
from score import score  # noqa: E402

CLIPS = ["en2002a-ihm", "en2002a-sdm", "en2002b-ihm", "en2002b-sdm"]
PROFILES = {
    "accel": {"threads": None, "accel": True, "prefix": []},
    "cpu4": {"threads": 4, "accel": False, "prefix": []},
    "old2": {"threads": 2, "accel": False, "prefix": ["taskpolicy", "-b"]},
}
CACHE = HERE / ".cache"
RESULTS = HERE / "results"


def run_one(engine: str, profile: str, clip: str) -> dict | None:
    spec = ENGINES[engine]
    prof = PROFILES[profile]
    if profile not in spec.get("profiles", PROFILES):
        return None
    out_dir = RESULTS / f"{engine}__{profile}"
    out_dir.mkdir(parents=True, exist_ok=True)
    out = out_dir / f"{clip}.json"
    wav = CACHE / "clips" / f"{clip}.wav"
    cmd = spec["cmd"](wav=str(wav), out=str(out), threads=prof["threads"], accel=prof["accel"], cache=CACHE, here=HERE)
    full = ["/usr/bin/time", "-l", *prof["prefix"], *cmd]
    print(f"== {engine} / {profile} / {clip}", flush=True)
    proc = subprocess.run(full, capture_output=True, text=True, env={**os.environ, "PYTHONUNBUFFERED": "1"})
    if proc.returncode != 0 or not out.exists():
        print(proc.stdout[-2000:], proc.stderr[-4000:], file=sys.stderr)
        return None
    rss = re.search(r"(\d+)\s+maximum resident set size", proc.stderr)
    result = json.loads(out.read_text())
    result["peak_rss_mb"] = int(rss.group(1)) / 1e6 if rss else None
    out.write_text(json.dumps(result, indent=1))
    ref = json.loads((CACHE / "clips" / f"{clip}.ref.json").read_text())
    s = score(result, ref)
    (out_dir / f"{clip}.score.json").write_text(json.dumps(s, indent=1))
    lag = s.get("lag_shown", {}).get("median")
    print(f"   WER {s['wer']:.1%}  RTF {s['rtf']:.3f}  lag {lag if lag is None else round(lag, 2)}s  RSS {result['peak_rss_mb']:.0f} MB", flush=True)
    return s


def fmt(v, kind):
    if v is None:
        return "–"
    return {"pct": f"{v:.1%}", "s": f"{v:.2f}", "x": f"{v:.3f}", "mb": f"{v:,.0f}"}[kind]


def summary() -> None:
    rows = []
    for d in sorted(RESULTS.glob("*__*")):
        engine, profile = d.name.split("__")
        scores = {p.name.removesuffix(".score.json"): json.loads(p.read_text()) for p in d.glob("*.score.json")}
        if not scores:
            continue

        def mean(key, sub=None, clips=None):
            vals = []
            for c, s in scores.items():
                if clips and not any(c.endswith(x) for x in clips):
                    continue
                v = s.get(key)
                v = v.get(sub) if (sub and isinstance(v, dict)) else v
                if v is not None:
                    vals.append(v)
            return sum(vals) / len(vals) if vals else None

        # WER pooled over words, not averaged over clips.
        ref = sum(s["ref_words"] for s in scores.values())
        wer = sum(s["wer"] * s["ref_words"] for s in scores.values()) / ref
        rows.append(
            {
                "engine": engine,
                "label": ENGINES.get(engine, {}).get("label", engine),
                "profile": profile,
                "clips": len(scores),
                "wer": wer,
                "wer_ihm": mean("wer", clips=["-ihm"]),
                "wer_sdm": mean("wer", clips=["-sdm"]),
                "del": mean("del"),
                "ins": mean("ins"),
                "lag_shown": mean("lag_shown", "median"),
                "lag_shown_p90": mean("lag_shown", "p90"),
                "lag_final": mean("lag_final", "median"),
                "tail": mean("tail_secs"),
                "rtf": mean("rtf"),
                "rss": max((s["peak_rss_mb"] or 0) for s in scores.values()),
            }
        )
    (RESULTS / "summary.json").write_text(json.dumps(rows, indent=1))
    head = "| Engine | Profile | Clips | WER | WER close-talk | WER far-field | Del | Ins | Shown lag (med / p90) | Final lag | Backlog at end | RTF | Peak RAM MB |"
    lines = [head, "|" + "|".join(["---"] * 13) + "|"]
    for r in sorted(rows, key=lambda r: (r["profile"], r["wer"])):
        lines.append(
            f"| {r['label']} | {r['profile']} | {r['clips']} | {fmt(r['wer'],'pct')} | {fmt(r['wer_ihm'],'pct')} | {fmt(r['wer_sdm'],'pct')} | "
            f"{fmt(r['del'],'pct')} | {fmt(r['ins'],'pct')} | {fmt(r['lag_shown'],'s')} / {fmt(r['lag_shown_p90'],'s')} | {fmt(r['lag_final'],'s')} | "
            f"{fmt(r['tail'],'s')} | {fmt(r['rtf'],'x')} | {fmt(r['rss'],'mb')} |"
        )
    (RESULTS / "summary.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


def main() -> None:
    if sys.argv[1] == "list":
        for k, v in ENGINES.items():
            print(f"{k:32} {v['label']}  profiles={','.join(v.get('profiles', PROFILES))}")
    elif sys.argv[1] == "run":
        engines = sys.argv[2].split(",")
        profiles = sys.argv[3].split(",")
        clips = sys.argv[4].split(",") if len(sys.argv) > 4 else CLIPS
        for e in engines:
            for p in profiles:
                for c in clips:
                    run_one(e, p, c)
        summary()
    elif sys.argv[1] == "summary":
        summary()


if __name__ == "__main__":
    main()
