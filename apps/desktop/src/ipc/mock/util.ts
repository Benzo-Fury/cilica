/** Small helpers shared by the mock backend. */
import type { AppError, ErrorCode } from "@/ipc/types";

let counter = 0;

/** A short unique id with a readable prefix ("note_12_k3f9"). */
export function newId(prefix: string): string {
  counter += 1;
  return `${prefix}_${counter}_${Math.random().toString(36).slice(2, 6)}`;
}

/** Deep copy for JSON-safe data, so callers can never mutate store state. */
export function clone<T>(value: T): T {
  return value === undefined ? value : (JSON.parse(JSON.stringify(value)) as T);
}

export function iso(ms: number = Date.now()): string {
  return new Date(ms).toISOString();
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** An ISO timestamp some time before `now`. */
export function ago(
  now: number,
  parts: { days?: number; hours?: number; minutes?: number },
): string {
  return iso(
    now - (parts.days ?? 0) * DAY - (parts.hours ?? 0) * HOUR - (parts.minutes ?? 0) * MINUTE,
  );
}

export const time = { MINUTE, HOUR, DAY };

/** Throwable AppError (a plain object, exactly what Tauri rejects with). */
export function appError(code: ErrorCode, message: string): AppError {
  return { code, message };
}

export function notFound(
  what:
    | "note"
    | "category"
    | "version"
    | "person"
    | "speaker"
    | "thread"
    | "turn"
    | "proposal"
    | "action"
    | "task"
    | "task run"
    | "model",
): AppError {
  const messages: Record<typeof what, string> = {
    note: "That note no longer exists. It may have been deleted.",
    category: "That category no longer exists. It may have been deleted.",
    version: "That version is no longer available. Reopen History and pick another.",
    person: "That person is no longer saved. Refresh Saved people and try again.",
    speaker: "That speaker is no longer in this transcript. Reopen the note and try again.",
    thread: "That conversation no longer exists. Start a new one.",
    turn: "That reply has already finished.",
    proposal: "That suggestion is no longer available.",
    action: "That action is no longer in the activity log.",
    task: "That scheduled task no longer exists.",
    "task run": "That task run is no longer available.",
    model: "That speech model is not in the catalogue. Pick another one.",
  };
  return appError("not_found", messages[what]);
}

/** `"14:05"` style wall-clock time. */
export function clock(ms: number): string {
  const d = new Date(ms);
  return `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** Strip Markdown to plain text (for snippets and search). */
export function stripMarkdown(md: string): string {
  return md
    .replace(/```[^\n]*\n([\s\S]*?)```/g, "$1")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/^\s{0,3}#{1,6}\s+/gm, "")
    .replace(/^\s*>\s?/gm, "")
    .replace(/^\s*[-*+]\s+\[[ xX]\]\s+/gm, "")
    .replace(/^\s*[-*+]\s+/gm, "")
    .replace(/^\s*\d+\.\s+/gm, "")
    .replace(/^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$/gm, "")
    .replace(/\|/g, " ")
    .replace(/(\*\*|__)(.*?)\1/g, "$2")
    .replace(/(\*|_)(.*?)\1/g, "$2")
    .replace(/`([^`]*)`/g, "$1")
    .replace(/[ \t]+/g, " ")
    .replace(/\n{2,}/g, "\n")
    .trim();
}

/** First non-heading line of text, as plain text, for the sidebar snippet. */
export function snippetFrom(md: string): string {
  for (const raw of md.split("\n")) {
    const line = raw.trim();
    if (!line || /^#{1,6}\s/.test(line) || /^```/.test(line) || /^\|?\s*:?-{3,}/.test(line))
      continue;
    const text = stripMarkdown(line);
    if (text) return text.length > 120 ? `${text.slice(0, 119).trimEnd()}…` : text;
  }
  return "";
}

export function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** Split text into word chunks that keep their trailing whitespace, for streaming. */
export function wordChunks(text: string, wordsPerChunk = 1): string[] {
  const tokens = text.match(/\S+\s*|\s+/g) ?? [];
  const out: string[] = [];
  for (let i = 0; i < tokens.length; i += wordsPerChunk) {
    out.push(tokens.slice(i, i + wordsPerChunk).join(""));
  }
  return out;
}
