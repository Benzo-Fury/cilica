/**
 * What the open note view is showing, for the assistant's context line and turns: the panel
 * (Notes or Transcript) and the text selected in the editor.
 */
import { create } from "zustand";
import type { NotePanel } from "@/ipc/types";

interface NoteViewState {
  noteId: string | null;
  panel: NotePanel;
  selectedText: string;
  setPanel: (noteId: string, panel: NotePanel) => void;
  setSelection: (noteId: string, text: string) => void;
  clear: (noteId: string) => void;
}

/** Long selections are cut so a turn's context stays small. */
const MAX_SELECTION = 2000;

export const useNoteViewState = create<NoteViewState>()((set, get) => ({
  noteId: null,
  panel: "notes",
  selectedText: "",
  setPanel: (noteId, panel) =>
    set(get().noteId === noteId ? { panel } : { noteId, panel, selectedText: "" }),
  setSelection: (noteId, text) =>
    set({
      noteId,
      selectedText: text.length > MAX_SELECTION ? text.slice(0, MAX_SELECTION) : text,
    }),
  clear: (noteId) => {
    if (get().noteId === noteId) set({ noteId: null, panel: "notes", selectedText: "" });
  },
}));
