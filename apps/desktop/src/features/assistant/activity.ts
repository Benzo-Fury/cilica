/**
 * What Cilica is doing right now, across every conversation, whether or not its panel is open:
 * the turns still running (for the conversation list and the "Working" header), the notes each
 * turn is touching, and where in a note it is working (the moving marker in the editor).
 *
 * `useCilicaActivity` (mounted once in the shell) folds the `assistant` event stream into this
 * store, keeps each remembered thread current so a conversation resumed later shows every step,
 * and raises a toast when a turn you are not watching finishes or needs your OK.
 */
import { create } from "zustand";
import type { AiAction, AssistantEvent, Proposal } from "@/ipc/types";

/** A note (or other place) a turn has touched, with what it did there. */
export interface Touch {
  key: string;
  noteId: string | null;
  /** Shown instead of the note's title when the place is not a note ("Scheduled tasks"). */
  label: string | null;
  /** Present tense while the step runs ("reading"), past tense after ("read"). */
  verb: string;
  live: boolean;
}

export interface Run {
  threadId: string;
  turnId: string;
  startedAt: number;
  /** The newest step, as the thread shows it ("Reading “Acme kickoff”"). */
  step: AiAction | null;
  touched: Touch[];
}

/** Where in a note Cilica is working: the editor draws the marker there. */
export interface Focus {
  noteId: string;
  turnId: string;
  mode: "reading" | "editing" | "suggesting";
  /** Text to find in the note; none means the whole note (a reading sweep). */
  phrases: string[];
  /** A suggested edit stays marked after the turn, until you accept or reject it. */
  proposalId?: string;
}

interface ActivityState {
  runs: Record<string, Run>;
  /** Which conversation each turn this session belongs to. */
  turnThreads: Record<string, string>;
  focus: Focus | null;
  apply: (event: AssistantEvent, now: number) => void;
}

const VERBS: Record<string, [live: string, done: string]> = {
  get_note: ["reading", "read"],
  get_note_history: ["reading history", "read history"],
  get_transcript: ["reading the transcript", "read the transcript"],
  edit_note: ["editing", "edited"],
  rename_note: ["renaming", "renamed"],
  move_note: ["moving", "moved"],
  create_note: ["writing", "wrote"],
  label_speaker: ["labelling speakers", "labelled speakers"],
  merge_notes: ["merging", "merged"],
  trash_note: ["moving to Trash", "moved to Trash"],
};

function argsOf(action: Pick<AiAction, "args_json">): Record<string, unknown> {
  try {
    const args = JSON.parse(action.args_json) as unknown;
    return args && typeof args === "object" ? (args as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

function str(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

/** What a tool call touched, or null for steps with no single place (searching, listing). */
export function touchOf(action: AiAction): Touch | null {
  const args = argsOf(action);
  const live = action.status === "running";
  const waiting = action.status === "proposed";
  if (action.tool === "create_task") {
    return {
      key: "tasks",
      noteId: null,
      label: "Scheduled tasks",
      verb: live ? "adding" : "added",
      live,
    };
  }
  const verbs = VERBS[action.tool];
  if (!verbs) return null;
  const verb = waiting ? "waiting on you" : live ? verbs[0] : verbs[1];
  if (action.tool === "create_note") {
    const title = str(args.title) || "a new note";
    return { key: `new:${title}`, noteId: null, label: title, verb, live };
  }
  const noteId = str(args.note_id);
  if (!noteId) return null;
  return { key: noteId, noteId, label: null, verb, live };
}

/** Merge a touch into the list: the same place keeps its slot and takes the newest verb. */
export function addTouch(list: Touch[], touch: Touch | null): Touch[] {
  if (!touch) return list;
  const index = list.findIndex((t) => t.key === touch.key);
  if (index < 0) return [...list, touch];
  const next = list.slice();
  next[index] = touch;
  return next;
}

const MARKDOWN_PREFIX = /^(#{1,6}\s+|[-*+]\s+(\[[ xX]\]\s+)?|\d+[.)]\s+|>\s*)/;

/** A Markdown line as the editor shows its text. */
function plainLine(line: string): string {
  return line
    .replace(MARKDOWN_PREFIX, "")
    .replace(/[*_`~]/g, "")
    .trim();
}

/**
 * Where a proposed edit lands in the note as it is now: the lines of `before` it changes or
 * removes (the new text is not in the note yet). A pure addition marks the line it follows.
 */
export function changedLines(before: string, after: string): string[] {
  const kept = new Set(after.split("\n").map((l) => l.trim()));
  const lines = before.split("\n").map((l) => l.trim());
  const changed = lines.filter((l) => l && !kept.has(l)).map(plainLine);
  const found = changed.filter((l) => l.length > 2);
  if (found.length > 0) return found.slice(0, 6);
  const last = [...lines].reverse().find((l) => plainLine(l).length > 2);
  return last ? [plainLine(last)] : [];
}

/** The marker's target for a tool step on a note, or undefined when the step is elsewhere. */
export function focusOfTool(action: AiAction): Focus | undefined {
  const args = argsOf(action);
  const noteId = str(args.note_id);
  if (!noteId) return undefined;
  switch (action.tool) {
    case "get_note":
    case "get_transcript":
    case "get_note_history":
      return { noteId, turnId: action.turn_id, mode: "reading", phrases: [] };
    case "edit_note": {
      const edits = Array.isArray(args.edits) ? (args.edits as Record<string, unknown>[]) : [];
      const phrases = edits
        .map((e) => str(e?.find))
        .flatMap((find) => find.split("\n"))
        .map(plainLine)
        .filter((l) => l.length > 2);
      return { noteId, turnId: action.turn_id, mode: "editing", phrases };
    }
    case "rename_note":
    case "move_note":
    case "label_speaker":
      return { noteId, turnId: action.turn_id, mode: "editing", phrases: [] };
    default:
      return undefined;
  }
}

export function focusOfProposal(proposal: Proposal, turnId: string): Focus | undefined {
  if (proposal.kind.type !== "edit_note") return undefined;
  return {
    noteId: proposal.kind.note.id,
    turnId,
    mode: "suggesting",
    phrases: changedLines(proposal.kind.before_md, proposal.kind.after_md),
    proposalId: proposal.id,
  };
}

/** The pure fold behind the store (tested on its own). */
export function applyActivity(
  state: Pick<ActivityState, "runs" | "turnThreads" | "focus">,
  event: AssistantEvent,
  now: number,
): Pick<ActivityState, "runs" | "turnThreads" | "focus"> {
  const { runs, turnThreads, focus } = state;
  switch (event.type) {
    case "turn_started":
      return {
        runs: {
          ...runs,
          [event.thread_id]: {
            threadId: event.thread_id,
            turnId: event.turn_id,
            startedAt: now,
            step: null,
            touched: [],
          },
        },
        turnThreads: { ...turnThreads, [event.turn_id]: event.thread_id },
        focus,
      };
    case "tool": {
      const run = runs[event.thread_id];
      const nextFocus = focusOfTool(event.action);
      return {
        runs: run
          ? {
              ...runs,
              [event.thread_id]: {
                ...run,
                step: event.action,
                touched: addTouch(run.touched, touchOf(event.action)),
              },
            }
          : runs,
        turnThreads,
        // A step elsewhere (a search, a list) leaves the marker where it was.
        focus: nextFocus ?? focus,
      };
    }
    case "proposal":
      return {
        runs,
        turnThreads,
        focus: focusOfProposal(event.proposal, event.turn_id) ?? focus,
      };
    case "turn_finished": {
      const { [event.thread_id]: _done, ...rest } = runs;
      return {
        runs: runs[event.thread_id]?.turnId === event.turn_id ? rest : runs,
        turnThreads,
        focus: focus?.turnId === event.turn_id && focus.mode !== "suggesting" ? null : focus,
      };
    }
    case "text_delta":
      return state;
  }
}

export const useCilicaActivity = create<ActivityState>()((set) => ({
  runs: {},
  turnThreads: {},
  focus: null,
  apply: (event, now) => set((s) => applyActivity(s, event, now)),
}));
