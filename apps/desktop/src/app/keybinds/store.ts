import { create } from "zustand";
import type { Overrides } from "./registry";

interface KeybindState {
  /** `settings.keybinds`, mirrored here so the key handler reads it synchronously. */
  overrides: Overrides;
  /** True while Settings › Keybinds is recording a new shortcut: every handler stands down. */
  capturing: boolean;
  setOverrides: (overrides: Overrides) => void;
  setCapturing: (capturing: boolean) => void;
}

export const useKeybindStore = create<KeybindState>()((set) => ({
  overrides: {},
  capturing: false,
  setOverrides: (overrides) => set({ overrides }),
  setCapturing: (capturing) => set({ capturing }),
}));
