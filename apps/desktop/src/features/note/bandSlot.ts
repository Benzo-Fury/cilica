import { create } from "zustand";

/**
 * A place in the note's title band, beside the Notes / Transcript switch, that the transcript
 * fills with its player (through a portal) when the meeting has audio.
 */
export const useBandSlot = create<{
  el: HTMLElement | null;
  setEl: (el: HTMLElement | null) => void;
}>()((set) => ({ el: null, setEl: (el) => set({ el }) }));
