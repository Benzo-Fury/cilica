import { create } from "zustand";

export type Renaming = { kind: "note" | "category"; id: string } | null;

interface SidebarUiState {
  /** An inline "New category" row is open for its name. */
  newCategory: boolean;
  renaming: Renaming;
  startNewCategory: () => void;
  stopNewCategory: () => void;
  setRenaming: (renaming: Renaming) => void;
}

export const useSidebarUi = create<SidebarUiState>()((set) => ({
  newCategory: false,
  renaming: null,
  startNewCategory: () => set({ newCategory: true, renaming: null }),
  stopNewCategory: () => set({ newCategory: false }),
  setRenaming: (renaming) => set({ renaming, newCategory: false }),
}));
