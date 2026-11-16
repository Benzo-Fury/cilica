/**
 * Suggested links, as pills in the note's header: notes this one names, notes that link here, and
 * notes that link to the same things. Approving one adds it to the note's hidden `related`
 * property (the text is untouched); dismissing one means it is not suggested again.
 */
import { Link2 } from "lucide-react";
import { AnimatePresence } from "motion/react";
import type { LinkSuggestion } from "@/ipc/types";
import { displayTitle } from "@/lib/format";
import { SuggestionChip } from "@/ui";
import { useAnswerLinkSuggestion, useLinkSuggestions } from "./hooks";

/** Why it is suggested, in a sentence. */
export function suggestionHint(s: LinkSuggestion): string {
  switch (s.reason) {
    case "mentioned":
      return s.excerpt ? `Named in this note: “${s.excerpt}”` : "Named in this note";
    case "links_here":
      return s.excerpt ? `Links to this note: “${s.excerpt}”` : "Links to this note";
    case "shares_links":
      return `Also links to ${listOf(s.shared)}`;
  }
}

function listOf(items: string[]): string {
  if (items.length <= 1) return items[0] ?? "the same notes";
  return `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}

export function LinkSuggestions({ noteId, enabled }: { noteId: string; enabled: boolean }) {
  const { data } = useLinkSuggestions(noteId, enabled);
  const answer = useAnswerLinkSuggestion(noteId);
  const list = enabled ? (data ?? []) : [];
  return (
    <AnimatePresence initial={false} mode="popLayout">
      {list.map((s) => {
        const title = displayTitle(s.note.title);
        return (
          <SuggestionChip
            key={s.note.id}
            icon={Link2}
            hint={suggestionHint(s)}
            acceptLabel={`Link to ${title}`}
            dismissLabel="Not related"
            onAccept={() => answer.mutate({ targetId: s.note.id, accept: true })}
            onDismiss={() => answer.mutate({ targetId: s.note.id, accept: false })}
          >
            {title}
          </SuggestionChip>
        );
      })}
    </AnimatePresence>
  );
}
