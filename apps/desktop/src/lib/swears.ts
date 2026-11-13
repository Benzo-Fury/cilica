/**
 * Matching words against the swear jar's list, the same way `cilica_domain::Swears` does in Rust:
 * whole words, any case, a trailing "'s" ignored, stretched letters ("fuuuck") squeezed and
 * censored forms ("f***") read letter for letter. The list itself comes from the backend.
 */
import type { SwearWord } from "@/ipc/types";

/** One swear found in a piece of text. `start`/`end` are string indices. */
export interface SwearHit {
  root: string;
  mild: boolean;
  start: number;
  end: number;
}

const WORD = /[\p{L}'’*]+/gu;

/** Lower case, straight apostrophes, no possessive "'s", no apostrophes left. */
function normalize(token: string): string {
  const lower = token.toLowerCase().replace(/’/g, "'");
  const trimmed = lower.replace(/^'+|'+$/g, "");
  const base = trimmed.endsWith("'s") ? trimmed.slice(0, -2) : trimmed;
  return base.replace(/'/g, "");
}

/** Runs of three or more of the same letter cut down to `keep`. */
function squeeze(word: string, keep: number): string {
  return word.replace(/(.)\1{2,}/gu, (_, c: string) => c.repeat(keep));
}

/** The jar's word list, ready to match against. */
export class SwearMatcher {
  private byWord = new Map<string, SwearWord>();
  private ordered: SwearWord[];

  constructor(words: readonly SwearWord[]) {
    // Strong words win a censored match over mild ones, as in Rust
    this.ordered = [...words.filter((w) => !w.mild), ...words.filter((w) => w.mild)];
    for (const w of this.ordered) if (!this.byWord.has(w.word)) this.byWord.set(w.word, w);
  }

  /** The root and strength of one word, if it is a swear. */
  canonical(token: string): { root: string; mild: boolean } | null {
    const word = normalize(token);
    if (!word) return null;
    if (word.includes("*")) return this.starred(word);
    const hit =
      this.byWord.get(word) ??
      this.byWord.get(squeeze(word, 1)) ??
      this.byWord.get(squeeze(word, 2));
    return hit ? { root: hit.root, mild: hit.mild } : null;
  }

  /** Every swear in `text`, in order. Mild ones only when `mild` is true. */
  find(text: string, mild: boolean): SwearHit[] {
    const out: SwearHit[] = [];
    for (const m of text.matchAll(WORD)) {
      const c = this.canonical(m[0]);
      if (c && (mild || !c.mild)) out.push({ ...c, start: m.index, end: m.index + m[0].length });
    }
    return out;
  }

  /** A censored word: a letter first, at least one star, every letter in place. */
  private starred(word: string): { root: string; mild: boolean } | null {
    const chars = [...word];
    if (chars.length < 3 || chars[0] === "*") return null;
    const hit = this.ordered.find(
      (w) =>
        [...w.word].length === chars.length &&
        [...w.word].every((c, i) => chars[i] === "*" || chars[i] === c),
    );
    return hit ? { root: hit.root, mild: hit.mild } : null;
  }
}
