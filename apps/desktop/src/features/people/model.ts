/** Saved people logic without React: how a person's voice is described. */
import type { Person } from "@/ipc/types";
import { plural } from "@/lib/format";

/** "4 voice samples · 12 meetings", or that Cilica has not learnt the voice yet. */
export function voiceLine(person: Person): string {
  const voice =
    person.sample_count > 0
      ? plural(person.sample_count, "voice sample")
      : person.is_self
        ? "Cilica doesn't know your voice yet"
        : "Voice not learnt yet";
  return `${voice} · ${plural(person.meeting_count, "meeting")}`;
}

/** The forget-voice confirmation, for you or someone else. */
export function forgetCopy(person: Person): { title: string; body: string } {
  return person.is_self
    ? {
        title: "Forget your voice?",
        body: "Cilica stops recognising you until you pick You for a speaker again. Your meetings stay as they are.",
      }
    : {
        title: `Forget ${person.name}'s voice?`,
        body: `Cilica stops recognising ${person.name} until you pick them for a speaker again. ${person.name} and their meetings stay.`,
      };
}
