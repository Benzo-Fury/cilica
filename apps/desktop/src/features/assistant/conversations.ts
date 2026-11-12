/**
 * The conversation list shown when no conversation is open: day groups, each row's meta line,
 * and the retention line under the list (Settings › Margin › Delete conversations after).
 */
import type { Thread } from "@/ipc/types";
import { relativeDate } from "@/lib/format";

const DAY = 24 * 60 * 60 * 1000;

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

/** Calendar days from `now` to `iso`: 0 today, -1 yesterday, 1 tomorrow. */
function dayOffset(iso: string | number, now: Date): number {
  return Math.round((startOfDay(new Date(iso)) - startOfDay(now)) / DAY);
}

export interface ConversationGroup {
  label: "Today" | "Yesterday" | "Earlier";
  threads: Thread[];
}

/** Threads (newest first) under Today, Yesterday and Earlier; empty groups are left out. */
export function groupConversations(threads: Thread[], now: Date = new Date()): ConversationGroup[] {
  const groups: ConversationGroup[] = [
    { label: "Today", threads: [] },
    { label: "Yesterday", threads: [] },
    { label: "Earlier", threads: [] },
  ];
  for (const thread of threads) {
    const offset = dayOffset(thread.updated_at, now);
    const group = offset >= 0 ? groups[0] : offset === -1 ? groups[1] : groups[2];
    group?.threads.push(thread);
  }
  return groups.filter((g) => g.threads.length > 0);
}

/**
 * The quiet line beside a conversation: when it was last active, or, when the retention setting
 * removes it today or tomorrow, that instead.
 */
export function conversationMeta(thread: Thread, retentionDays: number, now: Date = new Date()) {
  if (retentionDays > 0) {
    const expires = new Date(thread.updated_at).getTime() + retentionDays * DAY;
    const offset = dayOffset(expires, now);
    if (offset <= 0) return "Deleted today";
    if (offset === 1) return "Deleted tomorrow";
  }
  return relativeDate(thread.updated_at, now);
}

/** The line under the list. */
export function retentionLine(retentionDays: number): string {
  if (retentionDays <= 0) return "Conversations are kept until you delete them.";
  return `Conversations are deleted after ${retentionDays} ${retentionDays === 1 ? "day" : "days"}.`;
}
