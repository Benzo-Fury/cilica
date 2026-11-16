# cilica-thesaurus

Synonyms for the selection toolbar, grouped by meaning, with nothing sent over the network.

## Public API

- `Thesaurus::shared()`: the bundled thesaurus, read on first use (about 0.2 s in a debug build, then kept).
- `Thesaurus::lookup(text) -> Synonyms`: a word or phrase of up to four words. It tries the text as written, then as an irregular form ("ran" → run), then with a regular ending removed ("ideas", "walked", "running", "larger"). Synonyms come back in the selection's form where English does that regularly ("walked" → strolled). "-ing" and "-ed" words list their verb meanings first.

Depends only on `cilica-domain` (for `Synonyms`) and `flate2`. Never on `tauri` or the store.

## Data

`data/thesaurus.txt.gz` (about 1.2 MB) is built by `tools/build-thesaurus.py` from two sources:

- **Synonyms:** the cleaned ABC Thesaurus, `dist/abcthesaurus-en.clean.jsonl.gz` in <https://github.com/Benzo-Fury/abcthesaurus-data>, a fork of <https://abcthesaurus.com>'s data with slurs removed, rude words marked and out-of-context relations removed (its `CHANGES.md` says how, and its tests check it). Only close synonyms are kept: the fork's audit found 28% of its near synonyms wrong in context. Rude words are kept only for headwords that are themselves rude, so "arse" offers "ass" but "breast" never offers "tit".
- **Irregular forms** ("ran" → run): Open English WordNet, `english-wordnet-<year>.xml.gz` from <https://github.com/globalwordnet/english-wordnet/releases>.

```bash
python3 tools/build-thesaurus.py path/to/abcthesaurus-en.clean.jsonl.gz path/to/english-wordnet-2025.xml.gz
```

Licences and credit (the file's first line carries the same):

- "Thesaurus data © ABC Thesaurus (https://abcthesaurus.com), CC BY-SA 4.0 (<https://creativecommons.org/licenses/by-sa/4.0/>), cleaned in github.com/Benzo-Fury/abcthesaurus-data." The bundled file is an adaptation and is shared under the same licence; Cilica's code is not affected.
- "Open English WordNet 2025, by the Global WordNet Association and contributors", CC BY 4.0.
