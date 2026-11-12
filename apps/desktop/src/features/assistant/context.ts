/**
 * The assistant's view context (spec › "Context, without being told"): what is open, which
 * panel, the selected text, the category, the search query and whether Margin is recording.
 * Built from app state on every turn and shown as the context line ("Viewing: Acme kickoff ·
 * Transcript").
 */
import type { View } from "@/app/router";
import type { NotePanel, NoteSummary, RecordingState, ViewContext } from "@/ipc/types";
import { displayTitle } from "@/lib/format";

export interface ContextInputs {
  view: View;
  /** The open note, when the view is a note and it is loaded. */
  note: Pick<NoteSummary, "id" | "title" | "category_id"> | null;
  panel: NotePanel;
  selectedText: string;
  /** The sidebar search, when it is open. */
  searchQuery: string | null;
  recording: Pick<RecordingState, "phase">;
}

export function buildViewContext(inputs: ContextInputs): ViewContext {
  const { view, note } = inputs;
  const onNote = view.name === "note" && note !== null && note.id === view.noteId;
  const selected = inputs.selectedText.trim();
  const search = inputs.searchQuery?.trim() ?? "";
  return {
    note_id: onNote ? note.id : null,
    note_title: onNote ? displayTitle(note.title) : null,
    panel: onNote ? inputs.panel : null,
    selected_text: onNote && inputs.panel === "notes" && selected ? selected : null,
    category_id: onNote ? note.category_id : null,
    search_query: search || null,
    is_recording: inputs.recording.phase === "starting" || inputs.recording.phase === "recording",
  };
}

const VIEW_LABELS: Partial<Record<View["name"], string>> = {
  home: "All notes",
  trash: "Trash",
  people: "Saved people",
  tasks: "Scheduled tasks",
  activity: "Margin activity",
  settings: "Settings",
};

/** The context line: "Viewing: Acme kickoff · Transcript · 12 words selected". */
export function contextLabel(context: ViewContext, view: View): string {
  const parts: string[] = [];
  if (context.note_id) {
    parts.push(context.note_title ?? "Untitled");
    parts.push(context.panel === "transcript" ? "Transcript" : "Notes");
    if (context.selected_text) {
      const words = context.selected_text.split(/\s+/).filter(Boolean).length;
      parts.push(words === 1 ? "1 word selected" : `${words} words selected`);
    }
  } else if (context.search_query) {
    parts.push(`Search for “${context.search_query}”`);
  } else {
    parts.push(VIEW_LABELS[view.name] ?? "All notes");
  }
  if (context.is_recording) parts.push("Recording");
  return `Viewing: ${parts.join(" · ")}`;
}

/** The composer's scope chip: what "this" means in a question. */
export function scopeLabel(context: ViewContext, categoryName: string | null): string {
  if (context.selected_text) return "Selection";
  if (context.note_id) return "This note";
  if (categoryName) return categoryName;
  return "All notes";
}
