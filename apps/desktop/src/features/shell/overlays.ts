import { create } from "zustand";

export type Overlay = "palette" | "cheatsheet";

interface OverlayState {
  open: Overlay | null;
  show: (overlay: Overlay) => void;
  toggle: (overlay: Overlay) => void;
  close: () => void;
}

/** Which full-window overlay is showing. Only one at a time. */
export const useOverlays = create<OverlayState>()((set, get) => ({
  open: null,
  show: (open) => set({ open }),
  toggle: (overlay) => set({ open: get().open === overlay ? null : overlay }),
  close: () => set({ open: null }),
}));
