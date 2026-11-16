#!/usr/bin/env python3
"""Build the bundled thesaurus (crates/thesaurus/data/thesaurus.txt.gz).

    python3 tools/build-thesaurus.py path/to/abcthesaurus-en.clean.jsonl.gz path/to/english-wordnet-2025.xml.gz

Synonyms come from the cleaned ABC Thesaurus (CC BY-SA 4.0): https://github.com/Benzo-Fury/abcthesaurus-data,
`dist/abcthesaurus-en.clean.jsonl.gz`, a fork of https://abcthesaurus.com's data with slurs removed and rude words
marked. Open English WordNet (CC BY 4.0, https://github.com/globalwordnet/english-wordnet/releases) gives only
the irregular forms ("ran" is a form of "run"). The output keeps only what the synonyms list needs:

    G <pos> <gloss> <word|word|...>    a group of words with one meaning, numbered from 0
    W <word> <group,group,...>         a word's groups, in order
    F <form> <word>                    an irregular form: "ran" is a form of "run"

Lines are tab-separated. Each meaning gives one group of its close synonyms. Near synonyms and antonyms are
left out: the fork's audit found 28% of near synonyms wrong in context, against 1.3% of synonyms, and a
one-click replacement has to be right. Words the cleaned data marks `rude` are kept only for a headword that
is itself rude ("arse" → ass), never for a plain one ("breast" never gives "tit").
"""

import gzip
import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "crates/thesaurus/data/thesaurus.txt.gz"
POS = {"noun": "noun", "verb": "verb", "adj": "adjective", "adv": "adverb", "prep": "preposition",
       "pron": "pronoun", "conj": "conjunction", "other": "other"}


def words(relations: list[dict], head: str, rude_ok: bool) -> list[str]:
    out = []
    for r in relations:
        w = r["word"]
        if r.get("rude") and not rude_ok:
            continue
        if w.lower() != head.lower() and "\t" not in w and "|" not in w:
            out.append(w)
    return out


def irregular_forms(source: str, known: set[str]) -> dict[str, str]:
    """Irregular form → its plain word, from WordNet's lexical entries."""
    forms: dict[str, str] = {}
    with gzip.open(source, "rb") if source.endswith(".gz") else open(source, "rb") as f:
        for _, el in ET.iterparse(f, events=("end",)):
            if el.tag == "LexicalEntry":
                word = el.find("Lemma").get("writtenForm").lower()
                for form in el.findall("Form"):
                    written = (form.get("writtenForm") or "").lower()
                    if written and written != word and word in known and written not in known:
                        forms.setdefault(written, word)
                el.clear()
    return forms


def main(thesaurus: str, wordnet: str) -> None:
    lines_g: list[str] = []
    lines_w: list[str] = []
    known: set[str] = set()
    for o in map(json.loads, gzip.open(thesaurus, "rt", encoding="utf-8")):
        head = o["word"]
        rude_ok = bool(o.get("rude"))
        ids = []
        for s in o["senses"]:
            pos, gloss = POS.get(s["pos"], "other"), " ".join(s["gloss"].split())
            ws = words(s.get("synonyms", []), head, rude_ok)
            if ws:
                ids.append(len(lines_g))
                lines_g.append(f"G\t{pos}\t{gloss}\t{'|'.join(ws)}")
        if ids:
            key = head.lower()
            known.add(key)
            lines_w.append(f"W\t{key}\t{','.join(map(str, ids))}")
    forms = irregular_forms(wordnet, known)
    lines_f = [f"F\t{form}\t{word}" for form, word in sorted(forms.items())]

    header = ("# Synonyms from ABC Thesaurus (https://abcthesaurus.com, CC BY-SA 4.0), cleaned in "
              "https://github.com/Benzo-Fury/abcthesaurus-data. Irregular forms from Open English WordNet 2025 "
              "(CC BY 4.0). Built by tools/build-thesaurus.py.\n")
    text = header + "\n".join(lines_g + lines_w + lines_f) + "\n"
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with gzip.GzipFile(OUT, "wb", compresslevel=9, mtime=0) as out:
        out.write(text.encode("utf-8"))
    print(f"{len(lines_g)} groups, {len(lines_w)} words, {len(lines_f)} forms")
    print(f"{len(text.encode()) / 1e6:.1f} MB raw, {OUT.stat().st_size / 1e6:.2f} MB compressed -> {OUT}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
