/** Close matches for a search nobody matched: a copy of `crates/store/src/near.rs`. */
import type { NearWord } from "@/ipc/types";
import type { MockContext } from "./state";

const RESULT_LIMIT = 3;
const MIN_QUERY = 4;

const fold = (s: string) =>
  s
    .toLowerCase()
    .split("")
    .filter((c) => /[\p{L}\p{N}]/u.test(c))
    .join("");

const tokens = (text: string) =>
  text
    .toLowerCase()
    .split(/[^\p{L}\p{N}'’]+/u)
    .map((w) => w.replace(/^['’]+|['’]+$/g, ""))
    .filter(Boolean);

/** Edits between two strings: insert, delete, replace, or swap two neighbours. */
export function distance(a: string, b: string): number {
  const d = Array.from({ length: a.length + 1 }, (_, i) =>
    Array.from({ length: b.length + 1 }, (_, j) => (i === 0 ? j : j === 0 ? i : 0)),
  );
  const at = (i: number, j: number) => d[i]?.[j] ?? 0;
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      let best = Math.min(
        at(i - 1, j) + 1,
        at(i, j - 1) + 1,
        at(i - 1, j - 1) + (a[i - 1] === b[j - 1] ? 0 : 1),
      );
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) {
        best = Math.min(best, at(i - 2, j - 2) + 1);
      }
      const row = d[i];
      if (row) row[j] = best;
    }
  }
  return at(a.length, b.length);
}

/** How close a one- or two-word phrase is to the query, or null when it is not close. */
export function closeness(phrase: string, query: string): number | null {
  const parts = query.split(/\s+/).filter(Boolean);
  if (parts.length === 0 || parts.length > 2) return null;
  if (phrase === parts.join(" ").toLowerCase()) return null;
  const q = fold(query);
  const p = fold(phrase);
  if (p === q) return 0;
  if (phrase.includes(" ") && parts.length === 1) return null;
  const max = q.length >= 7 ? 2 : 1;
  if (Math.abs(p.length - q.length) > max) return null;
  const d = distance(p, q);
  return d <= max ? d : null;
}

interface Tally {
  closeness: number;
  count: number;
  notes: Set<string>;
  first: { noteId: string; title: string; atMs: number };
  voices: Map<string, { count: number; mine: boolean }>;
}

export function nearCommands({ state }: MockContext) {
  return {
    nearWords(query: string): NearWord[] {
      if (fold(query).length < MIN_QUERY || query.split(/\s+/).filter(Boolean).length > 2) {
        return [];
      }
      const meetings = [...state.notes.values()]
        .filter((n) => !n.deleted_at && n.kind === "meeting")
        .sort((a, b) => (b.started_at ?? b.created_at).localeCompare(a.started_at ?? a.created_at));
      const found = new Map<string, Tally>();
      for (const note of meetings) {
        const t = state.transcripts.get(note.id);
        if (!t) continue;
        for (const s of t.segments.filter((x) => x.is_final)) {
          const sp = t.speakers.find((x) => x.id === s.speaker_id);
          const mine = sp ? sp.is_self : s.channel === "mic";
          const person = sp && state.people.find((p) => p.id === sp.person_id);
          const name = mine ? "You" : (person?.name ?? sp?.label ?? null);
          const toks = tokens(s.text);
          const seen = (phrase: string, close: number) => {
            const tally = found.get(phrase) ?? {
              closeness: close,
              count: 0,
              notes: new Set<string>(),
              first: { noteId: note.id, title: note.title, atMs: s.start_ms },
              voices: new Map(),
            };
            tally.count += 1;
            tally.notes.add(note.id);
            if (name && name !== "Others") {
              const v = tally.voices.get(name) ?? { count: 0, mine };
              v.count += 1;
              tally.voices.set(name, v);
            }
            found.set(phrase, tally);
          };
          toks.forEach((w, i) => {
            const one = closeness(w, query);
            if (one !== null) seen(w, one);
            const next = toks[i + 1];
            if (next) {
              const two = closeness(`${w} ${next}`, query);
              if (two !== null) seen(`${w} ${next}`, two);
            }
          });
        }
      }
      return [...found.entries()]
        .sort(
          ([pa, a], [pb, b]) =>
            a.closeness - b.closeness || b.count - a.count || pa.localeCompare(pb),
        )
        .slice(0, RESULT_LIMIT)
        .map(([phrase, t]) => {
          const top = [...t.voices.entries()].sort(
            ([na, a], [nb, b]) => b.count - a.count || na.localeCompare(nb),
          )[0];
          return {
            phrase,
            count: t.count,
            meetings: t.notes.size,
            speaker: top?.[0] ?? null,
            is_self: top?.[1].mine ?? false,
            note_id: t.first.noteId,
            note_title: t.first.title,
            at_ms: t.first.atMs,
          };
        });
    },
  };
}
