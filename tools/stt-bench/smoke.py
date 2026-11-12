"""Quick check that each engine loads and transcribes: 20 s of meeting audio, low priority.

Usage: python -I smoke.py <engine> [<engine>...]   (prints RTF and the text)
"""

import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from engines import ENGINES  # noqa: E402

CACHE = HERE / ".cache"


def main() -> None:
    clip = CACHE / "clips" / "smoke.wav"
    if not clip.exists():
        import soundfile as sf

        audio, sr = sf.read(CACHE / "clips" / "en2002a-ihm.wav")
        sf.write(clip, audio[sr * 50 : sr * 70], sr)
    (CACHE / "smoke").mkdir(exist_ok=True)
    for name in sys.argv[1:]:
        out = CACHE / "smoke" / f"{name}.json"
        out.unlink(missing_ok=True)
        cmd = ENGINES[name]["cmd"](wav=str(clip), out=str(out), threads=2, accel=True, cache=CACHE, here=HERE)
        proc = subprocess.run(["taskpolicy", "-b", *cmd], capture_output=True, text=True)
        if not out.exists():
            print(f"{name}: FAILED\n{proc.stderr[-1500:]}")
            continue
        r = json.loads(out.read_text())
        finals: dict[str, str] = {}
        for e in r["events"]:
            if e["kind"] == "final":
                finals[e["id"]] = e["text"]
        text = " / ".join(t for t in finals.values() if t)
        print(f"{name}: rtf {r['compute_secs'] / r['audio_secs']:.3f}, {len(r['events'])} events | {text[:300]}")


if __name__ == "__main__":
    main()
