/**
 * Saved people logic without React: who to show first, how well Cilica knows a voice, and the
 * words for each.
 */
import type { Person, PersonMeeting, PersonVoice, VoiceSampleShape } from "@/ipc/types";
import { plural } from "@/lib/format";

/** Samples a person keeps (the store's `MAX_SAMPLES`). */
export const MAX_SAMPLES = 30;
/** A voice is steady from this consistency on, with at least `STEADY_SAMPLES` samples. */
export const STEADY = 0.85;
export const STEADY_SAMPLES = 3;
/** A sample matching the voiceprint less than this sounds unlike the others. */
export const ODD = 0.78;
/** Meetings shown on a person's page before "Show all". */
export const SHOWN_MEETINGS = 8;

const DAY = 86_400_000;

/** "18 min", or "under 1 min". */
export function speechLength(ms: number): string {
  const m = Math.round(ms / 60_000);
  if (m < 1) return "under 1 min";
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  return m % 60 ? `${h} h ${m % 60} min` : `${h} h`;
}

/** "Today", "Yesterday" or "3 Nov". */
export function heardOn(iso: string, now: Date = new Date()): string {
  const at = new Date(iso);
  const days = Math.round((startOfDay(now).getTime() - startOfDay(at).getTime()) / DAY);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  return at.toLocaleDateString("en-GB", {
    day: "numeric",
    month: "short",
    ...(at.getFullYear() !== now.getFullYear() && { year: "numeric" }),
  });
}

function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

/** "14:30". */
export function timeOfDay(iso: string): string {
  return new Date(iso).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
}

/** "last met today" or "last met on 3 Nov". */
export function lastMet(iso: string, now: Date = new Date()): string {
  const day = heardOn(iso, now);
  return day === "Today" || day === "Yesterday"
    ? `last met ${day.toLowerCase()}`
    : `last met on ${day}`;
}

/** Under a name in the list: "9 meetings · last met today". */
export function metLine(person: Person, now: Date = new Date()): string {
  if (person.meeting_count === 0) return "Not in a meeting yet";
  const met = person.last_met ? ` · ${lastMet(person.last_met, now)}` : "";
  return `${plural(person.meeting_count, "meeting")}${met}`;
}

/** Their first name ("Sam" for "Sam Lee"), for buttons and short lines. */
export function firstName(person: Person): string {
  return person.name.trim().split(/\s+/)[0] || person.name;
}

/** You first, then whoever you met most recently; people never met last, by name. */
export function sortPeople(people: readonly Person[]): Person[] {
  return [...people].sort(
    (a, b) =>
      Number(b.is_self) - Number(a.is_self) ||
      (b.last_met ?? "").localeCompare(a.last_met ?? "") ||
      a.name.localeCompare(b.name),
  );
}

const fold = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase().trim();

/** Whether a person matches what was typed in Find: any word of their name starts with it. */
export function matchesName(person: Person, query: string): boolean {
  const q = fold(query);
  if (!q) return true;
  const name = fold(person.name);
  return name.startsWith(q) || name.split(/\s+/).some((w) => w.startsWith(q));
}

/**
 * Others who may be this same person saved twice: the same first name ("Sam" and "Sam Lee"),
 * or one name that starts another. Never You.
 */
export function likelyDuplicates(person: Person, people: readonly Person[]): Person[] {
  if (person.is_self) return [];
  const mine = fold(person.name);
  const first = mine.split(/\s+/)[0] ?? mine;
  return people.filter((p) => {
    if (p.id === person.id || p.is_self) return false;
    const theirs = fold(p.name);
    const theirFirst = theirs.split(/\s+/)[0] ?? theirs;
    const short = mine.length < theirs.length ? mine : theirs;
    const long = mine.length < theirs.length ? theirs : mine;
    return (
      long.startsWith(`${short} `) ||
      (first === theirFirst && (mine === first || theirs === theirFirst))
    );
  });
}

/** Whether a sample sounds unlike the others (only said once there are enough to compare). */
export function isOdd(sample: VoiceSampleShape, count: number): boolean {
  return count >= STEADY_SAMPLES && (sample.similarity ?? 0) < ODD;
}

/** How well Cilica knows a voice. */
export type VoiceState = "known" | "learning" | "unknown";

export function voiceState(person: Person, voice?: PersonVoice): VoiceState {
  if (person.sample_count === 0) return "unknown";
  if (!voice) return person.sample_count < STEADY_SAMPLES ? "learning" : "known";
  return learningReason(person, voice) ? "learning" : "known";
}

/** The short label beside a person in the list. */
export const VOICE_STATE_LABEL: Record<VoiceState, string> = {
  known: "Recognised",
  learning: "Still learning",
  unknown: "Voice not learnt",
};

/** The headline of a person's voice card. */
export function voiceHeadline(person: Person, state: VoiceState): string {
  const whose = person.is_self ? "your" : `${firstName(person)}'s`;
  switch (state) {
    case "known":
      return `Cilica recognises ${whose} voice`;
    case "learning":
      return `Cilica is still learning ${whose} voice`;
    case "unknown":
      return `Cilica doesn't know ${whose} voice yet`;
  }
}

/** The sentence under the headline: how its naming went, why it is still learning, or how to teach it. */
export function voiceBody(person: Person, state: VoiceState, voice?: PersonVoice): string {
  const them = person.is_self ? "you" : "them";
  if (state === "unknown") {
    return person.is_self
      ? "In a meeting transcript, pick You for the speaker that's you. Cilica learns your voice from that meeting and names you from then on."
      : `In a meeting transcript, pick ${person.name} for their speaker. Cilica learns their voice from that meeting and names them when it's sure.`;
  }
  if (state === "learning" && voice) return learningReason(person, voice) ?? "";
  const named = voice?.named ?? 0;
  if (!named) return `It names ${them} in the next meeting where it hears ${them} clearly.`;
  const corrected = voice?.corrected ?? 0;
  const fixes = corrected
    ? `, and you corrected it ${corrected === 1 ? "once" : `${corrected} times`}`
    : ", and you never had to correct it";
  return `It named ${them} on its own in ${plural(named, "meeting")}${fixes}.`;
}

/**
 * Why Cilica is still learning this voice, or null once it is steady. Explains itself on the
 * voice card.
 */
export function learningReason(person: Person, voice: PersonVoice): string | null {
  const samples = voice.shape?.samples ?? [];
  const n = samples.length;
  if (n === 0) return null;
  if (n >= STEADY_SAMPLES && (voice.consistency ?? 0) >= STEADY) return null;
  const who = person.is_self ? "you" : person.name;
  const them = person.is_self ? "you" : "them";
  const why =
    n < STEADY_SAMPLES
      ? `Cilica has heard ${who} in ${n === 1 ? "only 1 meeting" : `only ${n} meetings`} so far, so it may not recognise ${them} every time.`
      : `${person.is_self ? "Your" : `${person.name}'s`} meetings don't sound alike yet, often from different microphones or rooms, so Cilica may not recognise ${them} every time.`;
  const odd = samples.some((s) => isOdd(s, n))
    ? " One meeting it learnt from sounds unlike the others. Check it below, and stop learning from it if it isn't them."
    : "";
  return `${why}${odd} Each meeting where you confirm ${person.is_self ? "yourself" : "them"} makes it steadier.`;
}

/** How much of a meeting they spoke, 0..1, when the meeting's length is known. */
export function talkShare(meeting: PersonMeeting): number | null {
  if (!meeting.duration_ms) return null;
  return Math.max(0, Math.min(1, meeting.speech_ms / meeting.duration_ms));
}

/** Under a meeting's title: "Spoke 4 min of 42 min · 18 lines". */
export function meetingLine(meeting: PersonMeeting): string {
  if (meeting.lines === 0) return "Named, with no lines of their own";
  const of = meeting.duration_ms ? ` of ${speechLength(meeting.duration_ms)}` : "";
  return `Spoke ${speechLength(meeting.speech_ms)}${of} · ${plural(meeting.lines, "line")}`;
}

/** Everything they said across their meetings. */
export function totalSpeech(meetings: readonly PersonMeeting[]): number {
  return meetings.reduce((a, m) => a + m.speech_ms, 0);
}

/** The sample Cilica learnt from in each meeting, by meeting id. */
export function samplesByMeeting(voice?: PersonVoice): Map<string, VoiceSampleShape> {
  const map = new Map<string, VoiceSampleShape>();
  for (const s of voice?.shape?.samples ?? []) {
    if (s.note_id && !map.has(s.note_id)) map.set(s.note_id, s);
  }
  return map;
}

/** The forget-voice confirmation, for you or someone else. */
export function forgetCopy(person: Person): { title: string; body: string } {
  const n = person.sample_count;
  const meetings = n === 1 ? "the 1 meeting" : `the ${n} meetings`;
  return person.is_self
    ? {
        title: "Forget your voice?",
        body: `Cilica deletes what it learnt from ${meetings} it knows your voice from and stops recognising you. Your meetings and transcripts stay. Pick You for a speaker to start again.`,
      }
    : {
        title: `Forget ${person.name}'s voice?`,
        body: `Cilica deletes what it learnt from ${meetings} it knows their voice from and stops recognising them. Their meetings and transcripts stay. Pick ${person.name} for a speaker to start again.`,
      };
}

/** The delete confirmation. */
export function deleteCopy(person: Person): { title: string; body: string } {
  const meetings = person.meeting_count;
  return {
    title: `Delete ${person.name}?`,
    body: `${person.name} and their voice are deleted.${meetings > 0 ? ` In ${plural(meetings, "meeting")} they go back to being Person 1, 2 and so on.` : ""} The transcripts stay. This can't be undone.`,
  };
}

/** The merge confirmation. */
export function mergeCopy(source: Person, target: Person): { title: string; body: string } {
  const n = source.sample_count;
  const voice =
    n === 0
      ? "nothing is learnt from them"
      : `what Cilica learnt of their voice joins ${target.name}'s`;
  return {
    title: `Merge ${source.name} into ${target.name}?`,
    body: `Everything ${source.name} said, in ${plural(source.meeting_count, "meeting")}, becomes ${target.name}'s, and ${voice}. Use this when they are the same person saved twice.`,
  };
}

/** The forget-every-voice confirmation. */
export function forgetAllCopy(known: number): { title: string; body: string } {
  return {
    title: "Forget every voice?",
    body: `Cilica deletes what it learnt about ${plural(known, "voice")}, yours included, and stops recognising anyone. People, meetings and transcripts stay. This can't be undone.`,
  };
}

/** The confirmation for no longer learning from one meeting. */
export function removeSampleCopy(
  person: Person,
  title: string,
  last: boolean,
): { title: string; body: string } {
  const whose = person.is_self ? "your" : `${person.name}'s`;
  const only = last
    ? ` It's the only meeting Cilica knows ${person.is_self ? "you" : "them"} from, so it stops recognising ${person.is_self ? "you" : "them"}.`
    : "";
  return {
    title: `Stop learning ${whose} voice from “${title}”?`,
    body: `Cilica forgets what it learnt from this meeting and won't learn from it again.${only} The meeting and its transcript stay, and ${person.is_self ? "you are" : `${person.name} is`} still named in it.`,
  };
}
