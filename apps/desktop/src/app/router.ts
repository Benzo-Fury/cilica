/**
 * A tiny router: the current view lives in zustand. Every launch opens on home, with no note
 * selected. Phase 2 adds views by extending `View` and rendering them in `features/shell/Canvas`.
 */
import { create } from "zustand";

export type SettingsPageId =
  | "general"
  | "import"
  | "appearance"
  | "keybinds"
  | "recording"
  | "transcription"
  | "cilica"
  | "sounds"
  | "swear_jar";

export type View =
  | { name: "home" }
  | {
      name: "note";
      noteId: string;
      /** Open on this panel (search hits in a transcript, a new recording). */
      panel?: "notes" | "transcript";
      /** Transcript hits: where in the recording the match is (phase 2 seeks to it). */
      atMs?: number;
    }
  | { name: "trash" }
  | { name: "settings"; page: SettingsPageId }
  | { name: "people" }
  /** The link graph. `focus` is a node key to centre on (`n:<note id>`, `t:<tag>`). */
  | { name: "graph"; focus?: string }
  | { name: "tasks"; taskRunId?: string }
  | { name: "activity" };

export type ViewName = View["name"];

interface RouterState {
  view: View;
  /** The view before the current one, for "Done" in settings and Esc. */
  previous: View | null;
  navigate: (view: View) => void;
  back: () => void;
}

export const useRouter = create<RouterState>()((set, get) => ({
  view: { name: "home" },
  previous: null,
  navigate: (view) => {
    const current = get().view;
    if (JSON.stringify(current) === JSON.stringify(view)) return;
    set({ view, previous: current });
  },
  back: () => {
    const { previous } = get();
    set({ view: previous ?? { name: "home" }, previous: null });
  },
}));

export const navigate = (view: View) => useRouter.getState().navigate(view);
export const openNote = (
  noteId: string,
  options: { panel?: "notes" | "transcript"; atMs?: number } = {},
) => navigate({ name: "note", noteId, ...options });
export const openGraph = (focus?: string) =>
  navigate(focus ? { name: "graph", focus } : { name: "graph" });
export const openSettings = (page: SettingsPageId = "general") =>
  navigate({ name: "settings", page });

/** The open note's id, or null on other views. */
export function useOpenNoteId(): string | null {
  return useRouter((s) => (s.view.name === "note" ? s.view.noteId : null));
}
