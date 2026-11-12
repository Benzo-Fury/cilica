/**
 * Which face Home shows, and the data behind it. Each visit picks one at random from the faces
 * that have something to show, avoiding the one shown last time when there is a choice.
 */
import { useQuery } from "@tanstack/react-query";
import { qk } from "@/app/queryKeys";
import { api } from "@/ipc/client";
import type { HomeFace, HomeGlance, Pace, TalkShare } from "@/ipc/types";

/** Every face Home can show: each kept face in Settings, without "random". */
export type GlanceFace = Exclude<HomeFace, "random">;

export function useHomeGlance(enabled: boolean) {
  return useQuery({
    queryKey: qk.homeGlance,
    enabled,
    staleTime: 60_000,
    queryFn: () => api.getHomeGlance(),
  });
}

/**
 * The faces this data can fill. The doodle needs nothing, so it is the fallback: offered only
 * when nothing else can be shown (it still shows when kept in Settings, see `openingFace`).
 */
export function availableFaces(glance: HomeGlance | undefined): GlanceFace[] {
  const out: GlanceFace[] = [];
  if (glance?.tics) out.push("tics");
  if (glance?.quotes.length) out.push("overheard");
  if (glance?.talk) out.push("talk");
  if (glance?.pace) out.push("pace");
  if (glance?.silence) out.push("silence");
  if (glance?.own_lines.length) out.push("own_words");
  if (glance?.writing) out.push("writing");
  if (glance?.questions.length) out.push("questions");
  if (glance?.overtime) out.push("overtime");
  if (glance?.slow_decision) out.push("slow_decision");
  if (glance?.monologue) out.push("monologue");
  if (glance?.bare_note) out.push("bare_note");
  if (glance?.mid_sentence) out.push("mid_sentence");
  if (glance?.spelling) out.push("spelling");
  if (glance?.ghostwriter) out.push("ghostwriter");
  if (glance?.undos) out.push("undos");
  for (const face of JAR_FACES) if (jarFaceReady(face, glance?.swear_jar)) out.push(face);
  if (out.length === 0) out.push("doodle");
  return out;
}

/** The faces drawn from the swear jar. */
export const JAR_FACES = ["swear_jar", "whos_paying", "clean_week"] as const;
export type JarFace = (typeof JAR_FACES)[number];

/** Thursday to Sunday: late enough in the week for a clean jar to mean something. */
const LATE_WEEK = new Set([4, 5, 6, 0]);

/**
 * Whether the jar has enough for a face. The jar needs three coins; Who's paying needs a meeting
 * with someone else in it; Clean week needs an empty jar after a week with coins, and (unless
 * `anyDay`, for previews) waits until Thursday.
 */
export function jarFaceReady(
  face: JarFace,
  jar: HomeGlance["swear_jar"] | undefined,
  { now = new Date(), anyDay = false }: { now?: Date; anyDay?: boolean } = {},
): boolean {
  if (!jar) return false;
  switch (face) {
    case "swear_jar":
      return jar.count >= 3;
    case "whos_paying":
      return jar.has_meetings && jar.count >= 3 && jar.people.some((p) => p.name !== null);
    case "clean_week":
      return jar.count === 0 && jar.last_week > 0 && (anyDay || LATE_WEEK.has(now.getDay()));
  }
}

/**
 * The first name, for a joke that needs to sound like you know them. Labels for people nobody has
 * named yet ("Person 2", "Speaker 3") stay whole.
 */
export function firstName(name: string): string {
  const trimmed = name.trim();
  if (/^(person|speaker)\s+\d+$/i.test(trimmed)) return trimmed;
  return trimmed.split(/\s+/)[0] ?? trimmed;
}

/** A random face, not `last` unless it is the only one. `roll` is in [0, 1). */
export function pickFace(faces: GlanceFace[], last: GlanceFace | null, roll: number): GlanceFace {
  const pool = faces.length > 1 ? faces.filter((f) => f !== last) : faces;
  return pool[Math.min(pool.length - 1, Math.floor(roll * pool.length))] ?? "doodle";
}

/**
 * The face to open with. The doodle when it is kept; any other kept face when its data is there,
 * otherwise a random face stands in until there is something to count; Random picks as
 * `pickFace` does.
 */
export function openingFace(
  faces: GlanceFace[],
  kept: HomeFace,
  last: GlanceFace | null,
  roll: number,
): GlanceFace {
  if (kept === "doodle") return "doodle";
  if (kept !== "random" && faces.includes(kept)) return kept;
  return pickFace(faces, last, roll);
}

/** "9 pm", "12 am", "12 pm" for an hour of the day. */
export function hourLabel(hour: number): string {
  const h = ((hour % 24) + 24) % 24;
  const twelve = h % 12 === 0 ? 12 : h % 12;
  return `${twelve} ${h < 12 ? "am" : "pm"}`;
}

/**
 * The longest run of hours with nothing written, going round midnight, as "Quiet from 1 am to
 * 7 am". Nothing to say when the quiet is under four hours or the whole day.
 */
export function quietStretch(hours: readonly number[]): string | null {
  let best = { start: 0, length: 0 };
  for (let start = 0; start < 24; start++) {
    if (hours[start] || !hours[(start + 23) % 24]) continue;
    let length = 0;
    while (length < 24 && !hours[(start + length) % 24]) length++;
    if (length > best.length) best = { start, length };
  }
  if (best.length < 4 || best.length >= 24) return null;
  return `Quiet from ${hourLabel(best.start)} to ${hourLabel(best.start + best.length)}.`;
}

/** The tallest column the hour-dots chart draws before each dot stands for more than one save. */
export const MAX_DOTS = 10;

/** How many saves one dot stands for, so the busiest hour fits in {@link MAX_DOTS} dots. */
export function savesPerDot(hours: readonly number[]): number {
  return Math.max(1, Math.ceil(Math.max(0, ...hours) / MAX_DOTS));
}

/** "14:22" or "1:02:05" for a position in a recording. */
export function timecode(ms: number): string {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** "4 h 12 min", "58 min" or "45 s". */
export function duration(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} s`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  return m % 60 ? `${h} h ${m % 60} min` : `${h} h`;
}

/** The headline for the talk bar: your share, or whoever did most of it when you said nothing. */
export function talkHeadline(talk: TalkShare): { lead: string; strong: string; tail: string } {
  const you = talk.slices.find((s) => s.is_self);
  if (you) {
    const pct = Math.round((you.ms / talk.total_ms) * 100);
    return { lead: "You did", strong: `${pct}%`, tail: "of the talking this week" };
  }
  const top = talk.slices.find((s) => s.name);
  return { lead: "", strong: top?.name ?? "Others", tail: "did most of the talking this week" };
}

/** "14 faster than last week", "the same as last week", or nothing without a last week. */
export function paceChange(pace: Pace): string | null {
  if (pace.last_week_wpm === null) return null;
  const d = pace.you_wpm - pace.last_week_wpm;
  if (Math.abs(d) < 3) return "about the same as last week";
  return `${Math.abs(d)} ${d > 0 ? "faster" : "slower"} than last week`;
}

/** Shorten a line to a sentence that fits under the silence. */
export function clip(text: string, max = 110): string {
  if (text.length <= max) return text;
  const cut = text.slice(0, max);
  return `${cut.slice(0, Math.max(cut.lastIndexOf(" "), max - 20)).trimEnd()}…`;
}

/** "4 min 12 s", "6 min", "45 s" for a stretch of talking. */
export function minSec(ms: number): string {
  const s = Math.round(ms / 1000);
  const m = Math.floor(s / 60);
  if (!m) return `${s} s`;
  return s % 60 ? `${m} min ${s % 60} s` : `${m} min`;
}

/** Whole minutes, rounded. */
export const minutes = (ms: number) => Math.round(ms / 60_000);

/** "Wednesday" for a timestamp, in local time. */
export function weekday(iso: string): string {
  return new Date(iso).toLocaleDateString("en-GB", { weekday: "long" });
}

const SMALL = [
  "zero",
  "one",
  "two",
  "three",
  "four",
  "five",
  "six",
  "seven",
  "eight",
  "nine",
  "ten",
  "eleven",
  "twelve",
  "thirteen",
  "fourteen",
  "fifteen",
  "sixteen",
  "seventeen",
  "eighteen",
  "nineteen",
  "twenty",
];

/** "twelve" up to twenty, digits after. */
export function numberWord(n: number): string {
  return SMALL[n] ?? String(n);
}

/** "Marco's", "James'". */
export function possessive(name: string): string {
  return name.endsWith("s") ? `${name}'` : `${name}'s`;
}

/** "3, 41 and 47" for a list of numbers. */
export function andList(items: (string | number)[]): string {
  const xs = items.map(String);
  return xs.length < 2 ? (xs[0] ?? "") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`;
}
