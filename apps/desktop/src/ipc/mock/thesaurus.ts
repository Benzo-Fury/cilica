/**
 * Synonyms in the browser preview: a handful of words, shaped like the bundled thesaurus's answers
 * (`crates/thesaurus`). Enough to try the selection toolbar's list.
 */
import type { SynonymGroup, Synonyms } from "@/ipc/types";

const g = (part_of_speech: string, gloss: string, ...words: string[]): SynonymGroup => ({
  part_of_speech,
  gloss,
  words,
});

const WORDS: Record<string, SynonymGroup[]> = {
  happy: [
    g(
      "adjective",
      "enjoying or showing joy or pleasure",
      "glad",
      "blissful",
      "cheerful",
      "content",
    ),
    g("adjective", "marked by good fortune", "fortunate", "lucky", "felicitous"),
  ],
  quick: [
    g("adjective", "accomplished rapidly and without delay", "speedy", "fast", "rapid"),
    g("adjective", "moving quickly and lightly", "agile", "nimble", "spry"),
  ],
  idea: [
    g("noun", "the content of cognition", "thought", "notion", "concept"),
    g("noun", "your intention", "plan", "aim", "intention"),
  ],
  plan: [
    g("noun", "a series of steps to be carried out", "program", "strategy", "scheme"),
    g("verb", "make plans for something", "arrange", "organize", "map out"),
  ],
  holiday: [
    g("noun", "leisure time away from work", "vacation", "break", "getaway"),
    g("noun", "a day on which work is suspended", "day off", "bank holiday"),
  ],
  getaway: [g("noun", "a short vacation", "holiday", "vacation", "break", "Lisbon trip")],
  ranking: [g("noun", "a position on a scale", "rating", "order", "search relevance")],
};

export function mockSynonyms(text: string): Synonyms {
  const key = text
    .trim()
    .replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "")
    .toLowerCase();
  for (const [word, plural] of [
    [key, false],
    [key.replace(/s$/, ""), true],
  ] as const) {
    const groups = WORDS[word];
    if (groups) {
      const words = (w: string) => (plural ? `${w}s` : w);
      return {
        word,
        groups: groups.map((grp) => ({ ...grp, words: grp.words.map(words) })),
      };
    }
  }
  return { word: "", groups: [] };
}
