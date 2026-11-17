/**
 * The panel at rest (no conversation open): one line on what Cilica can see, up to 3 suggestions
 * with an icon each, and the conversations, running ones first.
 */
import {
  CalendarCheck,
  FileText,
  ListChecks,
  type LucideIcon,
  Mail,
  MessageSquare,
  PencilLine,
  Search,
  Users,
} from "lucide-react";
import type { PromptChip, Thread, ViewContext } from "@/ipc/types";

/** Suggestions shown at rest. More than this and the panel stops being calm. */
export const MAX_SUGGESTIONS = 3;
/** Conversations listed at rest before "See all". */
export const RECENT_LIMIT = 4;

/** "Cilica can see this note and its transcript." */
export function seeLine(context: ViewContext): string {
  if (context.selected_text?.trim()) return "Cilica can see what you selected in this note.";
  if (context.note_id)
    return context.panel === "transcript"
      ? "Cilica can see this note and its transcript."
      : "Cilica can see this note, and opens others when it needs them.";
  if (context.search_query) return `Cilica can see your search for “${context.search_query}”.`;
  if (context.category_id) return "Cilica can see this category.";
  return "Cilica can read every note and transcript you have.";
}

const ICONS: [RegExp, LucideIcon][] = [
  [/\b(e-?mail|follow-?up|reply|message)\b/i, Mail],
  [/\b(action items?|to-?dos?|tasks?|checklist|open items?)\b/i, ListChecks],
  [/\b(remind|schedule|every|weekly|daily|deadline|due)\b/i, CalendarCheck],
  [/\b(who|people|person|commit(ted|ments?)?|owners?)\b/i, Users],
  [/\b(find|search|related|duplicates?|similar|asked)\b/i, Search],
  [/\b(tighten|rewrite|edit|shorter|clearer|tidy|fix)\b/i, PencilLine],
  [/\b(summar(y|ise|ize)|digest|recap|overview)\b/i, FileText],
];

/** An icon that says what kind of thing a suggestion does. */
export function suggestionIcon(chip: PromptChip): LucideIcon {
  const text = `${chip.label} ${chip.prompt}`;
  return ICONS.find(([pattern]) => pattern.test(text))?.[1] ?? MessageSquare;
}

/** The conversations at rest: running ones first, then the newest, up to `limit` in all. */
export function restingConversations(
  threads: Thread[],
  running: ReadonlySet<string>,
  limit: number = RECENT_LIMIT,
): { shown: Thread[]; more: boolean } {
  const live = threads.filter((t) => running.has(t.id));
  const rest = threads.filter((t) => !running.has(t.id));
  const shown = [...live, ...rest.slice(0, Math.max(0, limit - live.length))];
  return { shown, more: threads.length > shown.length };
}
