/**
 * What happened to the inline link hints, per title, remembered between launches: how often you
 * linked a title from its hint, and how often you went past it. A title you keep ignoring stops
 * being offered inline (the note header's suggestions still have it); linking it once from a hint
 * brings it back and marks it as one you mean, even when it is an everyday word.
 */
import { create } from "zustand";
import { persist } from "zustand/middleware";

/** How a hint ended. `passed`: you typed on or moved away; `refused`: Esc. */
export type HintOutcome = "taken" | "passed" | "refused";

export interface TitleMemory {
  /** Times linked from a hint. */
  taken: number;
  /** Ignores since the last time you linked it: typing past counts 1, Esc counts 2. */
  ignored: number;
}

/** Ignores (with no link since) after which a title is no longer offered inline. */
export const QUIET_AFTER = 3;

export function remember(memory: TitleMemory | undefined, outcome: HintOutcome): TitleMemory {
  const m = memory ?? { taken: 0, ignored: 0 };
  if (outcome === "taken") return { taken: m.taken + 1, ignored: 0 };
  return { taken: m.taken, ignored: m.ignored + (outcome === "refused" ? 2 : 1) };
}

/** Ignored often enough that it should stop showing up inline. */
export function isQuiet(memory: TitleMemory | undefined): boolean {
  return (memory?.ignored ?? 0) >= QUIET_AFTER;
}

/** Linked from a hint before: you treat it as a thing to link. */
export function isTrusted(memory: TitleMemory | undefined): boolean {
  return (memory?.taken ?? 0) > 0;
}

interface HintMemoryState {
  /** By the title's `linkKey`. */
  titles: Record<string, TitleMemory>;
  record: (key: string, outcome: HintOutcome) => void;
  /** Offer a title inline again. */
  forget: (key: string) => void;
}

export const useHintMemory = create<HintMemoryState>()(
  persist(
    (set) => ({
      titles: {},
      record: (key, outcome) =>
        set(({ titles }) => ({ titles: { ...titles, [key]: remember(titles[key], outcome) } })),
      forget: (key) =>
        set(({ titles }) => {
          const { [key]: _, ...rest } = titles;
          return { titles: rest };
        }),
    }),
    { name: "cilica.linkHints", partialize: ({ titles }) => ({ titles }) },
  ),
);
