/**
 * What Margin says while it works. A running tool step names the thing it is touching
 * ("Reading “Q3 planning sync”") instead of a generic label; between the last step and the
 * first words of the answer the thread says it is writing.
 */
import type { AiAction } from "@/ipc/types";
import type { ThreadState } from "./thread";

const MAX_TITLE = 42;

function quote(text: string): string {
  const t = text.trim();
  return `“${t.length > MAX_TITLE ? `${t.slice(0, MAX_TITLE - 1).trimEnd()}…` : t}”`;
}

/** A specific line for a running tool, or null when the generic label is the best there is. */
export function workingLine(
  action: Pick<AiAction, "tool" | "args_json">,
  titleOf: (noteId: string) => string | undefined,
): string | null {
  let args: Record<string, unknown> = {};
  try {
    args = JSON.parse(action.args_json) as Record<string, unknown>;
  } catch {
    return null;
  }
  const str = (key: string) => (typeof args[key] === "string" ? (args[key] as string).trim() : "");
  const title = str("note_id") ? titleOf(str("note_id"))?.trim() : undefined;
  switch (action.tool) {
    case "search_notes":
      return str("query")
        ? `Searching your notes for ${quote(str("query"))}`
        : "Looking through your recent notes";
    case "get_note":
      return title ? `Reading ${quote(title)}` : null;
    case "get_transcript":
      return title ? `Rereading the transcript of ${quote(title)}` : "Rereading the transcript";
    case "get_note_history":
      return title ? `Checking earlier versions of ${quote(title)}` : null;
    case "edit_note":
      return title ? `Drafting an edit to ${quote(title)}` : null;
    case "create_note":
      return str("title") ? `Writing ${quote(str("title"))}` : null;
    case "list_people":
      return "Checking who you have saved";
    case "list_categories":
      return "Checking your categories";
    case "list_tasks":
      return "Checking your scheduled tasks";
    default:
      return null;
  }
}

/** True once the steps are done and the answer has not started: Margin is writing. */
export function isComposing(state: ThreadState): boolean {
  if (!state.activeTurn) return false;
  const message = [...state.messages]
    .reverse()
    .find((m) => m.turn_id === state.activeTurn && m.role === "assistant");
  const last = message?.parts[message.parts.length - 1];
  return last?.type === "tool" && last.action.status !== "running";
}
