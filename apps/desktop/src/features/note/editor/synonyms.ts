/**
 * The selection toolbar's synonyms: which selections get them, and putting a synonym in the
 * selection's place in the same case. Kept free of React so it can be tested on its own.
 */
import type { EditorState } from "@tiptap/pm/state";
import { TextSelection } from "@tiptap/pm/state";

/** The most words a selection can have and still get synonyms ("look into", "in spite of"). */
export const MAX_SYNONYM_WORDS = 4;
const MAX_SYNONYM_CHARS = 40;
/** The most synonyms listed: the closest ones, across every meaning. */
export const MAX_SYNONYM_PILLS = 12;
/** A possessive ending, kept when a synonym replaces the word ("Mum's" → "Mother's"). */
const POSSESSIVE = /['’]s$/u;

export interface SynonymTarget {
  from: number;
  to: number;
  /** The selected words, without the spaces or punctuation around them. */
  text: string;
}

/**
 * The selected word or short phrase, when it is plain text in one paragraph (no links, mentions
 * or line breaks), outside code. The range is narrowed to the words, so "happy, " gives "happy",
 * and leaves out a possessive, so "Mum's" gives "Mum" and a synonym keeps the "'s".
 */
export function synonymTarget(state: EditorState): SynonymTarget | null {
  const { selection } = state;
  if (selection.empty || !(selection instanceof TextSelection)) return null;
  const { $from, $to } = selection;
  if ($from.parent !== $to.parent || !$from.parent.isTextblock || $from.parent.type.spec.code) {
    return null;
  }
  let plain = true;
  state.doc.nodesBetween(selection.from, selection.to, (node) => {
    if (node.isInline && !node.isText) plain = false;
    if (node.marks.some((m) => m.type.name === "code")) plain = false;
    return plain;
  });
  if (!plain) return null;
  const raw = state.doc.textBetween(selection.from, selection.to);
  const lead = /^[^\p{L}\p{N}]*/u.exec(raw)?.[0].length ?? 0;
  const tail = /[^\p{L}\p{N}]*$/u.exec(raw)?.[0].length ?? 0;
  let text = raw.slice(lead, raw.length - tail);
  let possessive = 0;
  if (POSSESSIVE.test(text) && text.length > 2) {
    possessive = 2;
    text = text.slice(0, -2);
  }
  if (!text || text.length > MAX_SYNONYM_CHARS || !/\p{L}/u.test(text)) return null;
  if (text.split(/\s+/).length > MAX_SYNONYM_WORDS) return null;
  return { from: selection.from + lead, to: selection.to - tail - possessive, text };
}

/** `word` in the case `original` was written in: "Happy" → "Glad", "HAPPY" → "GLAD". */
export function matchCase(original: string, word: string): string {
  if (original.length > 1 && /\p{Lu}/u.test(original) && original === original.toUpperCase()) {
    return word.toUpperCase();
  }
  if (/^\p{Lu}/u.test(original) && /^\p{Ll}/u.test(word)) {
    return word.charAt(0).toUpperCase() + word.slice(1);
  }
  return word;
}
