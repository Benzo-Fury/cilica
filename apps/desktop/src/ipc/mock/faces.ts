/**
 * Home's newer faces, worked out the way `crates/store/src/home.rs` and `written.rs` do: filler
 * phrases, overtime, slow decisions, monologues, bare and cut-off notes, spelling fixes, Margin's
 * share of your writing and undone edits. A rough copy, tuned for the small seed.
 */
import type { HomeGlance } from "@/ipc/types";

type Glance = HomeGlance;
type Tics = NonNullable<Glance["tics"]>;
type TicCount = Tics["you"];
type MeetingDecisions = NonNullable<Glance["slow_decision"]>["slowest"];

// ---------- Verbal tics ---------- //

export const FILLERS = [
  "basically",
  "to be fair",
  "to be honest",
  "sounds good",
  "you know",
  "i mean",
  "kind of",
  "sort of",
  "literally",
  "actually",
  "honestly",
  "obviously",
  "essentially",
  "at the end of the day",
  "circle back",
  "no worries",
  "makes sense",
  "for sure",
  "like i said",
  "per se",
];
const MIN_TICS = 3;

const lineWords = (text: string) =>
  text
    .toLowerCase()
    .replace(/’/g, "'")
    .split(/[^\p{L}\p{N}']+/u)
    .filter(Boolean);

/** Consecutive-word matches of a phrase, back-to-back repeats included. */
export function phraseCount(words: string[], phrase: string): number {
  const want = phrase.split(" ");
  let n = 0;
  for (let i = 0; i + want.length <= words.length; i++) {
    if (want.every((w, j) => words[i + j] === w)) n++;
  }
  return n;
}

/** Filler counts per person; `result` gives yours and up to two others'. */
export class TicCounter {
  private people = new Map<
    string,
    { name: string; mine: boolean; color: number; counts: Map<string, number> }
  >();

  add(name: string, mine: boolean, color: number, text: string) {
    const key = mine ? "\u0000you" : name;
    const entry = this.people.get(key) ?? { name, mine, color, counts: new Map() };
    const words = lineWords(text);
    for (const p of FILLERS) {
      const n = phraseCount(words, p);
      if (n) entry.counts.set(p, (entry.counts.get(p) ?? 0) + n);
    }
    this.people.set(key, entry);
  }

  result(): Tics | null {
    const top = (counts: Map<string, number>) => {
      let best: [string, number] | null = null;
      for (const p of FILLERS) {
        const n = counts.get(p) ?? 0;
        if (n >= MIN_TICS && (!best || n > best[1])) best = [p, n];
      }
      return best;
    };
    let you: TicCount | null = null;
    const others: TicCount[] = [];
    for (const e of this.people.values()) {
      const t = top(e.counts);
      if (!t) continue;
      if (e.mine) you = { phrase: t[0], count: t[1], name: null, color_index: null };
      else others.push({ phrase: t[0], count: t[1], name: e.name, color_index: e.color });
    }
    others.sort((a, b) => b.count - a.count);
    return you ? { you, others: others.slice(0, 2) } : null;
  }
}

// ---------- Decisions, turns, overtime ---------- //

/** Bullets under a "Decisions" heading, and the `[mm:ss]` each one was made at. */
export function decisions(body: string): { count: number; at: number[] } {
  let inside = false;
  let count = 0;
  const at: number[] = [];
  for (const raw of body.split("\n")) {
    const t = raw.trim();
    if (t.startsWith("#")) {
      inside = t.replace(/^#+/, "").trim().toLowerCase() === "decisions";
      continue;
    }
    const item = /^[-*] (.*)$/.exec(t)?.[1];
    if (!inside || item === undefined || item.trim().toLowerCase().startsWith("none")) continue;
    count++;
    const stamp = /\[(\d+):(\d+)(?::(\d+))?\]\s*$/.exec(item);
    if (stamp) {
      const [a, b, c] = stamp.slice(1).map((x) => (x === undefined ? undefined : Number(x)));
      at.push(
        (c === undefined ? (a ?? 0) * 60 + (b ?? 0) : (a ?? 0) * 3600 + (b ?? 0) * 60 + c) * 1000,
      );
    }
  }
  return { count, at: at.sort((x, y) => x - y) };
}

/** The slowest meeting per decision (15 min or more each), and the quickest other one. */
export function slowDecision(meetings: MeetingDecisions[]): Glance["slow_decision"] {
  const per = (m: MeetingDecisions) => m.duration_ms / Math.max(1, m.decisions);
  const slowest = [...meetings].sort((a, b) => per(b) - per(a))[0];
  if (!slowest || per(slowest) < 15 * 60_000) return null;
  const fastest =
    meetings
      .filter((m) => m.note_id !== slowest.note_id && per(m) < per(slowest))
      .sort((a, b) => per(a) - per(b))[0] ?? null;
  return { slowest, fastest };
}

/** Lines by the same person within two seconds of each other, merged into turns. */
export function turns(
  lines: { name: string; mine: boolean; start: number; end: number }[],
): { name: string; mine: boolean; start: number; end: number }[] {
  const out: { name: string; mine: boolean; start: number; end: number }[] = [];
  for (const l of lines) {
    const last = out[out.length - 1];
    if (last && last.name === l.name && last.mine === l.mine && l.start <= last.end + 2000) {
      last.end = Math.max(last.end, l.end);
    } else out.push({ ...l });
  }
  return out;
}

/** The meeting since `since` that ran furthest past its title's usual (median) length. */
export function overtime(
  history: { note_id: string; title: string; held: number; ms: number }[],
  since: number,
): Glance["overtime"] {
  const key = (t: string) => t.trim().replace(/\s+/g, " ").toLowerCase();
  let best: { over: number; o: NonNullable<Glance["overtime"]> } | null = null;
  history.forEach((m, i) => {
    if (m.held < since || !m.title.trim()) return;
    const earlier = history
      .slice(0, i)
      .filter((h) => key(h.title) === key(m.title))
      .map((h) => h.ms);
    if (earlier.length < 2) return;
    const recentSix = earlier.slice(-6).sort((a, b) => a - b);
    const usual = recentSix[Math.floor((recentSix.length - 1) / 2)] ?? 0;
    const over = m.ms - usual;
    if (over < 5 * 60_000 || m.ms * 100 < usual * 115 || (best && over <= best.over)) return;
    const prev = earlier[earlier.length - 1] ?? 0;
    best = {
      over,
      o: {
        note_id: m.note_id,
        title: m.title,
        usual_ms: usual,
        actual_ms: m.ms,
        recent_ms: [...earlier.slice(-3), m.ms],
        again: prev >= usual + 5 * 60_000,
      },
    };
  });
  return (best as { o: NonNullable<Glance["overtime"]> } | null)?.o ?? null;
}

// ---------- Written notes ---------- //

/** Prose whose last line stops without punctuation, long enough to have needed some. */
export function endsMidSentence(body: string): boolean {
  const last = body
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean)
    .pop();
  if (!last || /^(#|-|\*|\+|>|\||```|~~~|\d+\. )/.test(last)) return false;
  if (last.split(/\s+/).length < 4) return false;
  return /[\p{L}\p{N}]$/u.test(last);
}

export const SPELLINGS: [string, string[]][] = [
  ["definitely", ["definately", "definatly", "definetly", "definitly"]],
  ["separate", ["seperate"]],
  ["receive", ["recieve"]],
  ["necessary", ["neccessary", "necessery", "neccesary"]],
  ["accommodate", ["accomodate", "acommodate"]],
  ["calendar", ["calender"]],
  ["tomorrow", ["tommorow", "tomorow", "tommorrow"]],
  ["until", ["untill"]],
  ["weird", ["wierd"]],
  ["beginning", ["begining"]],
  ["believe", ["beleive"]],
  ["occurred", ["occured"]],
];

/** Misspellings that went down while the right spelling went up, across saves (oldest first). */
export function spellFixes(saves: string[], into: Map<string, number>) {
  const counts = saves.map((b) => {
    const words = b.toLowerCase().split(/[^\p{L}]+/u);
    const n = (w: string) => words.filter((x) => x === w).length;
    const m = new Map<string, [number, number]>();
    for (const [right, wrongs] of SPELLINGS) {
      for (const wrong of wrongs) m.set(`${right}|${wrong}`, [n(wrong), n(right)]);
    }
    return m;
  });
  for (let i = 1; i < counts.length; i++) {
    for (const [k, [wb, rb]] of counts[i - 1] ?? new Map<string, [number, number]>()) {
      const [wa, ra] = counts[i]?.get(k) ?? [0, 0];
      const fixed = Math.min(Math.max(0, wb - wa), Math.max(0, ra - rb));
      if (fixed > 0) into.set(k, (into.get(k) ?? 0) + fixed);
    }
  }
}

/** Words added by you and by Margin across saves: (author of the change, body after it). */
export function addedWords(chain: ["user" | "ai", string][]): [number, number] {
  const n = (b: string) => b.split(/\s+/).filter(Boolean).length;
  let you = chain[0] ? n(chain[0][1]) : 0;
  let margin = 0;
  for (let i = 1; i < chain.length; i++) {
    const grew = Math.max(0, n(chain[i]?.[1] ?? "") - n(chain[i - 1]?.[1] ?? ""));
    if (chain[i]?.[0] === "ai") margin += grew;
    else you += grew;
  }
  return [you, margin];
}

/** The first line of `a` that `b` lacks, without Markdown markers. */
export function firstNewLine(a: string, b: string): string | null {
  const have = new Set(b.split("\n").map((l) => l.trim()));
  const line = a
    .split("\n")
    .map((l) => l.trim())
    .find((l) => l && !l.startsWith("```") && !have.has(l));
  const clean = line?.replace(/^[#>\-* ]+/, "").trim();
  return clean || null;
}
