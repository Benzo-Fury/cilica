/** Home's facts from written notes: a rough copy of `crates/store/src/written.rs`. */
import type { HomeGlance, NoteLine as NoteLineIn, WritingHours } from "@/ipc/types";
import { addedWords, endsMidSentence, spellFixes } from "./faces";
import type { MockState } from "./state";

/** A line as the backend sends it: `said_by` is always present. */
type NoteLine = NoteLineIn & { said_by: string | null };

const LINE_LIMIT = 12;
const MIN_SAVES = 6;
const CUES = [
  "i think",
  "i feel",
  "i like",
  "i love",
  "i hate",
  "i want",
  "i wonder",
  "i believe",
  "i'd",
  "i'm sure",
  "we should",
  "we need",
  "we decided",
  "decided",
  "the problem",
  "the real",
  "the point",
  "the trick",
  "what if",
  "maybe",
  "probably",
  "should",
  "must",
  "never",
  "always",
  "because",
  "turns out",
  "honestly",
  "the best",
  "the worst",
  "matters",
];

/** A Markdown line as plain prose, or null for checkboxes, code, tables and links. */
export function plain(line: string): string | null {
  let t = line.trim();
  if (!t || t.startsWith("```") || t.startsWith("|") || /^[-*] \[/.test(t) || t.includes("://")) {
    return null;
  }
  t = t
    .replace(/^[#> ]+/, "")
    .replace(/^[-*+] /, "")
    .replace(/^\d+\. /, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/`[^`]*`/g, "")
    .replace(/\*/g, "")
    .replace(/(^|[^\p{L}\p{N}])_+|_+(?=[^\p{L}\p{N}]|$)/gu, "$1")
    .trim();
  return t || null;
}

/** Prose lines of a body as plain text, skipping fenced code blocks. */
function prose(body: string): string[] {
  let fenced = false;
  const out: string[] = [];
  for (const line of body.split("\n")) {
    if (/^\s*(```|~~~)/.test(line)) {
      fenced = !fenced;
      continue;
    }
    const text = fenced ? null : plain(line);
    if (text) out.push(text);
  }
  return out;
}

const sentences = (line: string) =>
  (line.match(/[^.!?]+(?:[.!?]+(?=\s|$)|$)/g) ?? []).map((s) => s.trim()).filter(Boolean);

const OPEN_QUOTES = '"“';
const CLOSE_QUOTES = '"”';
const BYLINE = /^(?:—|–|--)\s*(.+)$/;
const NOT_NAMES = new Set([
  "i",
  "me",
  "you",
  "we",
  "us",
  "note",
  "notes",
  "quote",
  "idea",
  "decision",
  "update",
  "todo",
  "q",
  "a",
  "re",
]);

/** One to three capitalised words that read as a person's name, not you and not a label. */
function personName(text: string): string | null {
  const name = text
    .trim()
    .replace(/[.,:!]+$/, "")
    .trim();
  const words = name.split(/\s+/).filter(Boolean);
  const shaped =
    words.length >= 1 && words.length <= 3 && words.every((w) => /^\p{Lu}[\p{L}'’.-]*$/u.test(w));
  const named = !(words.length === 1 && NOT_NAMES.has(name.toLowerCase()));
  return shaped && named ? name : null;
}

/** `said Sam`, `Sam said`, `says Sam` or `Sam says`: the name either side of the verb. */
function saidBy(text: string): string | null {
  const m = text.match(/^(?:said|says) (.+)$/) ?? text.match(/^(.+) (?:said|says)$/);
  return m?.[1] ? personName(m[1]) : null;
}

/** A line that is only a byline, `— Sam` or `-- Sam Lee`: the name it gives. */
export function byline(line: string): string | null {
  const m = line.trim().match(BYLINE);
  return m?.[1] ? personName(m[1]) : null;
}

/** A line quoting someone else: what they said and who said it (see `Attribution` in written.rs). */
export function attribution(line: string): { quote: string; who: string } | null {
  const open = [...line].findIndex((c) => OPEN_QUOTES.includes(c));
  if (open >= 0) {
    const chars = [...line];
    let close = -1;
    for (let i = chars.length - 1; i > open; i--)
      if (CLOSE_QUOTES.includes(chars[i] ?? "")) {
        close = i;
        break;
      }
    if (close < 0) return null;
    const quote = chars
      .slice(open + 1, close)
      .join("")
      .trim()
      .replace(/,$/, "")
      .trim();
    const before = chars.slice(0, open).join("").trim();
    const after = chars
      .slice(close + 1)
      .join("")
      .trim();
    // After the quote: a dash byline, or "said Sam" / "Sam said"
    const dashed = after.match(/^(?:—|–|--)\s*(.+)$/);
    let who = dashed?.[1]
      ? personName(dashed[1])
      : saidBy(
          after
            .replace(/^[, ]+/, "")
            .replace(/[.!]+$/, "")
            .trim(),
        );
    // Before the quote: "Sam said" or a speaker label "Sam:"
    if (!who) {
      const rest = before.replace(/[:,\s]+$/, "");
      who = saidBy(rest) ?? (before.endsWith(":") ? personName(rest) : null);
    }
    return who && quote ? { quote, who } : null;
  }
  // No quote marks: a sentence signed off with a dash
  const m = line.match(/^(.*) (?:—|–) ([^—–]+)$/);
  const who = m?.[2] ? personName(m[2]) : null;
  return m?.[1] && who ? { quote: m[1].trim(), who } : null;
}

const unquote = (line: string) =>
  line
    .trim()
    .replace(/^["“]+/, "")
    .replace(/["”]+$/, "")
    .trim();

/** Sentences worth quoting, with their score and who said them (null for your own words). */
export function ownLines(body: string): [string, number, string | null][] {
  const out: [string, number, string | null][] = [];
  const lines = prose(body);
  lines.forEach((line, i) => {
    // A byline only names the speaker of the line above it
    if (byline(line)) return;
    const quoted = attribution(line);
    const below = quoted ? null : byline(lines[i + 1] ?? "");
    const text = quoted ? quoted.quote : below ? unquote(line) : line;
    const who = quoted?.who ?? below ?? null;
    for (const s of sentences(text)) {
      const n = s.split(/\s+/).length;
      if (s.endsWith("?") || n < 5 || n > 30) continue;
      const padded = ` ${s
        .toLowerCase()
        .replace(/’/g, "'")
        .replace(/[^\p{L}\p{N}']+/gu, " ")
        .trim()} `;
      const score = CUES.filter((c) => padded.includes(` ${c} `)).length;
      if (score > 0) out.push([s, score, who]);
    }
  });
  return out;
}

export function questions(body: string): string[] {
  return prose(body)
    .flatMap(sentences)
    .filter((s) => s.endsWith("?") && s.split(/\s+/).length >= 3);
}

/** Saves per local hour, or null with too few to say anything. */
export function writingHours(times: Date[]): WritingHours | null {
  if (times.length < MIN_SAVES) return null;
  const hours = Array.from({ length: 24 }, () => 0);
  for (const t of times) hours[t.getHours()] = (hours[t.getHours()] ?? 0) + 1;
  let peak = 23;
  for (let h = 23; h >= 0; h--) if ((hours[h] ?? 0) > (hours[peak] ?? 0)) peak = h;
  return { hours, saves: times.length, peak_hour: peak };
}

export function writtenGlance(state: MockState) {
  const notes = [...state.notes.values()]
    .filter((n) => !n.deleted_at && n.kind === "manual")
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  const ranked: [number, number, NoteLine][] = [];
  const asked: NoteLine[] = [];
  const times: Date[] = [];
  notes.forEach((n, order) => {
    times.push(new Date(n.created_at));
    if (n.updated_at > n.created_at) times.push(new Date(n.updated_at));
    const line = (text: string, said_by: string | null = null): NoteLine => ({
      note_id: n.id,
      note_title: n.title,
      written_at: n.updated_at,
      text,
      said_by,
    });
    for (const [text, score, who] of ownLines(n.body_md)) {
      ranked.push([score, order, line(text, who)]);
    }
    for (const text of questions(n.body_md)) asked.push(line(text));
  });
  const manual = new Set(notes.map((n) => n.id));
  for (const v of state.versions) {
    if (v.author === "user" && manual.has(v.note_id)) times.push(new Date(v.created_at));
  }
  ranked.sort((a, b) => b[0] - a[0] || a[1] - b[1]);

  // A title with nothing under it (a day old at least), and notes that stop mid-sentence.
  const day = 24 * 3600_000;
  const bare = notes
    .filter(
      (n) => !n.body_md.trim() && n.title.trim() && Date.now() - Date.parse(n.created_at) >= day,
    )
    .sort((a, b) => a.created_at.localeCompare(b.created_at))[0];
  const cutOff = notes.filter((n) => endsMidSentence(n.body_md)).length;

  // Spelling fixes and who wrote what, walking each note's saves oldest first. A version holds the
  // body before a write by its author; the current body follows the last one.
  const fixes = new Map<string, number>();
  let byYou = 0;
  let byCilica = 0;
  for (const n of notes) {
    const vs = state.versions
      .filter((v) => v.note_id === n.id)
      .sort((a, b) => a.created_at.localeCompare(b.created_at));
    const chain: ["user" | "ai", string][] = vs.map((v, i) => [
      i === 0 ? "user" : (vs[i - 1]?.author ?? "user"),
      v.body_md,
    ]);
    chain.push([vs[vs.length - 1]?.author ?? "user", n.body_md]);
    spellFixes(
      chain.map(([, b]) => b),
      fixes,
    );
    const [you, margin] = addedWords(chain);
    byYou += you;
    byCilica += margin;
  }
  const topFix = [...fixes].filter(([, c]) => c >= 2).sort((a, b) => b[1] - a[1])[0];
  const total = byYou + byCilica;
  const pct = total >= 200 ? Math.floor((byCilica * 100) / total) : 0;

  return {
    own_lines: ranked.slice(0, LINE_LIMIT).map(([, , l]) => l),
    writing: writingHours(times),
    questions: asked.slice(0, LINE_LIMIT),
    bare_note: bare
      ? { note_id: bare.id, title: bare.title.trim(), created_at: bare.created_at }
      : null,
    mid_sentence: cutOff >= 2 ? cutOff : null,
    spelling: topFix
      ? {
          word: topFix[0].split("|")[0] ?? "",
          wrong: topFix[0].split("|")[1] ?? "",
          count: topFix[1],
        }
      : null,
    ghostwriter: pct >= 5 ? { cilica_pct: pct, words: total } : null,
  } satisfies Pick<
    HomeGlance,
    | "own_lines"
    | "writing"
    | "questions"
    | "bare_note"
    | "mid_sentence"
    | "spelling"
    | "ghostwriter"
  >;
}
