/**
 * "AI edit accepted" (motion.md): after a diff card is accepted, the changed sentences in the
 * open note glow `accent-soft` and fade over 2s. The card names the text; the editor finds and
 * washes it when the new body arrives.
 */
import type { Node as PmNode } from "@tiptap/pm/model";
import { create } from "zustand";

interface AiGlowState {
  pending: { noteId: string; phrases: string[] } | null;
  request: (noteId: string, phrases: string[]) => void;
  take: (noteId: string) => string[] | null;
}

export const useAiGlow = create<AiGlowState>()((set, get) => ({
  pending: null,
  request: (noteId, phrases) =>
    set({ pending: { noteId, phrases: phrases.filter((p) => p.trim()) } }),
  take: (noteId) => {
    const pending = get().pending;
    if (!pending || pending.noteId !== noteId) return null;
    set({ pending: null });
    return pending.phrases;
  },
}));

/** Document ranges of each phrase, matched inside one text block at a time. */
export function rangesOf(doc: PmNode, phrases: readonly string[]): { from: number; to: number }[] {
  const out: { from: number; to: number }[] = [];
  const wanted = phrases.map((p) => p.trim()).filter((p) => p.length > 2);
  if (wanted.length === 0) return out;
  doc.descendants((node, pos) => {
    if (!node.isTextblock) return true;
    const text = node.textContent;
    for (const phrase of wanted) {
      const at = text.indexOf(phrase);
      if (at >= 0) {
        // Text offsets map 1:1 to positions inside a block of plain text and inline marks.
        out.push({ from: pos + 1 + at, to: pos + 1 + at + phrase.length });
      }
    }
    return false;
  });
  return out;
}
