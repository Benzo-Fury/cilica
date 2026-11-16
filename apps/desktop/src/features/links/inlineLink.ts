/**
 * Finding a note title you just typed, so the editor can offer to link it (`inlineLinkHint.ts`).
 *
 * This runs on the key that ends a word, so it never reaches the backend: it reads the titles the
 * link resolver already keeps in memory, looks at the last few words before the caret, and tries
 * them longest first. That is a handful of map lookups per word.
 *
 * A note's aliases are spotted like its title ("SSO" offers "Single sign-on"); a title wins when
 * one note's alias is another note's title.
 *
 * Besides the title as written it finds the forms a sentence puts it in: plurals ("cytoplasms"),
 * possessives ("Lisbon's", "students'") and hyphens written as spaces ("sign on" for "Sign-on").
 * Only the plain endings of English nouns are undone, never a general stemmer: "cytoplasmic" is a
 * different word, and a match you did not expect is worse than none.
 */
import { linkKey } from "@/lib/links";
import { COMMON_WORDS } from "./commonWords";

/** A title or alias shorter than this is too likely to match by accident (same as the store). */
export const MIN_TITLE_CHARS = 3;
/** The most words a title can have and still be spotted as you type. */
export const MAX_TITLE_WORDS = 6;

export interface TitleEntry {
  noteId: string;
  title: string;
  /** `linkKey` of the title: what links, the note's lists and skips compare by. */
  key: string;
  /** A one-word title that is an everyday word ("Ideas"): offered only when it looks meant. */
  common: boolean;
}

export interface TitleIndex {
  /** By `foldKey` of the title. */
  byKey: ReadonlyMap<string, TitleEntry>;
  /** Words in the longest usable title, so we never look further back than that. */
  maxWords: number;
}

/** Edge punctuation that is not part of a word you typed: `(cytoplasm`, `“Cytoplasm`. */
const LEADING = /^[^\p{L}\p{N}]+/u;
/** A possessive ending: `Lisbon's`, `Lisbon’s`, `students'`. */
const POSSESSIVE = /(?:['’]s|(?<=s)['’])$/iu;
/** Where a sentence ends: a capital after one of these is just the start of a sentence. */
const SENTENCE_END = /(?:^|[.!?…:]["”’)]*)$/u;

/** `linkKey` with hyphens and underscores read as spaces, so "sign on" finds "Sign-on". */
export function foldKey(text: string): string {
  return linkKey(text.replace(/[-‐‑–—_]+/g, " "));
}

/**
 * The singular forms a plural word could have come from, most likely first. Empty for a word that
 * does not look plural. Only stems of three letters or more, so "bus" or "gas" stay as they are.
 */
export function singulars(word: string): string[] {
  const w = word.toLowerCase();
  const out: string[] = [];
  const add = (stem: string, tail = "") => {
    if (stem.length >= MIN_TITLE_CHARS) out.push(stem + tail);
  };
  if (!w.endsWith("s") || w.endsWith("ss") || w.endsWith("us") || w.endsWith("is")) return out;
  if (w.endsWith("ies")) add(w.slice(0, -3), "y"); // stories → story
  if (w.endsWith("ves")) {
    add(w.slice(0, -3), "f"); // leaves → leaf
    add(w.slice(0, -3), "fe"); // knives → knife
  }
  if (/(?:s|x|z|ch|sh|o)es$/.test(w)) add(w.slice(0, -2)); // boxes → box, heroes → hero
  add(w.slice(0, -1)); // notes → note, ideas → idea
  return out;
}

/** Whether a title is worth spotting: long enough, has a letter, not too many words. */
function usable(key: string): boolean {
  if (key.length < MIN_TITLE_CHARS || !/\p{L}/u.test(key)) return false;
  return key.split(" ").length <= MAX_TITLE_WORDS;
}

/** Whether a one-word title is an everyday word, in any of its forms. */
function everyday(folded: string): boolean {
  if (folded.includes(" ")) return false;
  return COMMON_WORDS.has(folded) || singulars(folded).some((s) => COMMON_WORDS.has(s));
}

/**
 * Titles and aliases keyed by `foldKey`, from the resolver's map (already newest-wins per title).
 * Every title goes in before any alias, so a title is never shadowed by another note's alias.
 */
export function buildTitleIndex(
  notes: ReadonlyMap<string, { id: string; title: string; aliases?: readonly string[] }>,
): TitleIndex {
  const byKey = new Map<string, TitleEntry>();
  let maxWords = 0;
  const add = (name: string, key: string, note: { id: string; title: string }) => {
    const folded = foldKey(name);
    if (!usable(folded) || byKey.has(folded)) return;
    byKey.set(folded, { noteId: note.id, title: note.title.trim(), key, common: everyday(folded) });
    maxWords = Math.max(maxWords, folded.split(" ").length);
  };
  for (const [key, note] of notes) add(note.title, key, note);
  for (const [key, note] of notes) for (const alias of note.aliases ?? []) add(alias, key, note);
  return { byKey, maxWords };
}

export interface TitleMatch extends TitleEntry {
  /** The words to link, as typed ("cytoplasms", "Lisbon" out of "Lisbon's"). */
  typed: string;
  /** Length of `typed` (in UTF-16 units). */
  length: number;
  /** Characters typed after the linked words that stay outside the link: the `'s` of "Lisbon's". */
  trail: number;
  /** Typed as the title is written (bar case): the link can simply be `[[typed]]`. */
  exact: boolean;
}

export interface MatchOptions {
  /** Turn a match down (this note, already linked, turned down); a shorter title is then tried. */
  skip?: (key: string, entry: TitleEntry) => boolean;
  /** Titles you have linked from a hint before: an everyday word among them is offered as usual. */
  trusted?: (key: string) => boolean;
}

/**
 * The longest title that `before` ends with, starting at a word boundary. `before` is the text of
 * the paragraph up to the end of the word just finished.
 *
 * A one-word title that is an everyday word is only offered when you have linked it from a hint
 * before, or wrote it with a capital in the middle of a sentence ("in my Ideas note"): a capital
 * at the start of a sentence says nothing.
 */
export function matchTitleBefore(
  before: string,
  index: TitleIndex,
  { skip = () => false, trusted = () => false }: MatchOptions = {},
): TitleMatch | null {
  if (!index.maxWords || !before || /\s$/.test(before)) return null;
  // Where each of the last few whitespace-separated words starts, nearest first. A hyphenated
  // title typed with spaces has more words than the title, so look one further back.
  const starts: number[] = [];
  let i = before.length;
  while (starts.length < index.maxWords + 1 && i > 0) {
    while (i > 0 && !/\s/.test(before[i - 1] ?? "")) i--;
    starts.push(i);
    while (i > 0 && /\s/.test(before[i - 1] ?? "")) i--;
  }
  for (let w = starts.length - 1; w >= 0; w--) {
    const start = starts[w] ?? 0;
    const raw = before.slice(start);
    const lead = LEADING.exec(raw)?.[0].length ?? 0;
    const span = raw.slice(lead);
    // A title never crosses a hard break.
    if (!span || span.includes("\n")) continue;
    const meant = () =>
      /^\p{Lu}/u.test(span) && !SENTENCE_END.test(before.slice(0, start + lead).trimEnd());
    // As typed first (a title can end in 's: "McDonald's"), then without the possessive.
    const possessive = POSSESSIVE.exec(span)?.[0].length ?? 0;
    for (const trail of possessive ? [0, possessive] : [0]) {
      const typed = span.slice(0, span.length - trail);
      for (const form of forms(typed)) {
        const entry = index.byKey.get(form);
        if (!entry || skip(entry.key, entry)) continue;
        if (entry.common && !trusted(entry.key) && !meant()) continue;
        return {
          ...entry,
          typed,
          length: typed.length,
          trail,
          exact: linkKey(typed) === entry.key,
        };
      }
    }
  }
  return null;
}

/** The note a whole word or phrase names, by title or alias, in any of its forms ("vacations"). */
export function findTitle(text: string, index: TitleIndex): TitleEntry | null {
  const span = text.trim();
  if (!span) return null;
  for (const form of forms(span)) {
    const entry = index.byKey.get(form);
    if (entry) return entry;
  }
  return null;
}

/** The keys `typed` could stand for, as written first, then with its last word made singular. */
function forms(typed: string): string[] {
  const folded = foldKey(typed);
  const cut = folded.lastIndexOf(" ") + 1;
  const head = folded.slice(0, cut);
  return [folded, ...singulars(folded.slice(cut)).map((s) => head + s)];
}
