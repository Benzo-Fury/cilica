"""Score one benchmark result against its reference.

Metrics
- WER: word error rate of the final transcript (Whisper's English normaliser, as the Open ASR
  Leaderboard uses), split into substitutions / deletions / insertions.
- Lag (stream results only), measured on correctly recognised words: the final transcript is
  aligned to the reference, and every hit is timed against when it was spoken (reference
  utterance times, words spread evenly across the utterance).
    final  when the word's utterance was finalised
    shown  when the word first appeared on screen: the first event of its utterance (partial
           or final) that already showed at least as many words as the word's position
  Median and 90th percentile, in seconds.
- RTF: compute time / audio time (lower is faster; above 1 cannot keep up live).
- Tail: how long after the clip ended the last text arrived (a backlog shows up here).

Usage: python -I score.py <result.json> <ref.json>   (prints JSON)
"""

import json
import sys
from collections import defaultdict

import jiwer
from whisper_normalizer.english import EnglishTextNormalizer

_norm = EnglishTextNormalizer()


def words(text: str) -> list[str]:
    return _norm(text).split() if text.strip() else []


def reference_words(ref: dict) -> list[tuple[str, float]]:
    """(word, time it was finished being spoken) in reference order."""
    out = []
    for u in ref["utterances"]:
        ws = words(u["text"])
        step = (u["end"] - u["start"]) / max(len(ws), 1)
        out += [(w, u["start"] + step * (i + 1)) for i, w in enumerate(ws)]
    return out


def hypothesis_words(events: list[dict]) -> list[tuple[str, float, float]]:
    """(word, shown at, final at) for the final transcript, in the order finals arrived."""
    by_id: dict[str, list[dict]] = defaultdict(list)
    order: list[str] = []
    for e in sorted(events, key=lambda e: e["at"]):
        if e["id"] not in by_id:
            order.append(e["id"])
        by_id[e["id"]].append(e)
    finals = [(i, [e for e in by_id[i] if e["kind"] == "final"]) for i in order]
    finals = [(i, f[-1]) for i, f in finals if f]
    finals.sort(key=lambda x: x[1]["at"])
    out = []
    for uid, final in finals:
        ws = words(final["text"])
        counts = [(e["at"], len(words(e["text"]))) for e in by_id[uid]]
        for j, w in enumerate(ws):
            shown = next((at for at, n in counts if n >= j + 1), final["at"])
            out.append((w, min(shown, final["at"]), final["at"]))
    return out


def percentile(values: list[float], p: float) -> float | None:
    if not values:
        return None
    s = sorted(values)
    return s[min(len(s) - 1, int(round(p / 100 * (len(s) - 1))))]


def score(result: dict, ref: dict) -> dict:
    ref_words = reference_words(ref)
    hyp_words = hypothesis_words(result["events"])
    ref_text = " ".join(w for w, _ in ref_words)
    hyp_text = " ".join(w for w, *_ in hyp_words) or "<empty>"
    out = jiwer.process_words(ref_text, hyp_text)
    n = len(ref_words)
    scored = {
        "wer": out.wer,
        "sub": out.substitutions / n,
        "del": out.deletions / n,
        "ins": out.insertions / n,
        "ref_words": n,
        "hyp_words": len(hyp_words),
        "rtf": result["compute_secs"] / result["audio_secs"],
        "load_secs": result.get("load_secs"),
        "peak_rss_mb": result.get("peak_rss_mb"),
    }
    if result["mode"] == "stream":
        shown, final = [], []
        for chunk in out.alignments[0]:
            if chunk.type != "equal":
                continue
            for k in range(chunk.ref_end_idx - chunk.ref_start_idx):
                spoken = ref_words[chunk.ref_start_idx + k][1]
                _, s, f = hyp_words[chunk.hyp_start_idx + k]
                shown.append(s - spoken)
                final.append(f - spoken)
        last = max((e["at"] for e in result["events"]), default=result["audio_secs"])
        scored["lag_shown"] = {"median": percentile(shown, 50), "p90": percentile(shown, 90)}
        scored["lag_final"] = {"median": percentile(final, 50), "p90": percentile(final, 90)}
        scored["tail_secs"] = max(0.0, last - result["audio_secs"])
    return scored


def main() -> None:
    result = json.loads(open(sys.argv[1]).read())
    ref = json.loads(open(sys.argv[2]).read())
    print(json.dumps(score(result, ref), indent=1))


if __name__ == "__main__":
    main()
