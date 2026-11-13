/**
 * Structured search, facets and thread search: the mock side of crates/store/src/search.rs.
 * Same rules as the store: every part of a `NoteQuery` has to match, words rank title first,
 * details-only searches list the newest first by the date the query is about.
 */
import type {
  DateField,
  Message,
  Note,
  NoteQuery,
  SearchFacets,
  SearchHit,
  SearchSource,
  ThreadHit,
} from "@/ipc/types";
import { linkKey, parseRefs } from "@/lib/links";
import { makeExcerpt, summaryOf } from "./notes";
import type { MockContext, MockState } from "./state";
import { escapeHtml, stripMarkdown } from "./util";

const MAX_RESULTS = 200;
const SOURCE_RANK: Record<SearchSource, number> = { title: 0, body: 1, transcript: 2, details: 3 };

export function searchCommands({ state }: MockContext) {
  return {
    findNotes(query: NoteQuery, limit: number): SearchHit[] {
      return findNotes(state, query, limit);
    },
    searchFacets(): SearchFacets {
      return facets(state);
    },
    searchThreads(query: string, limit: number): ThreadHit[] {
      return searchThreads(state, query, limit);
    },
  };
}

// ───────── Notes ─────────

function words(text: string): string[] {
  return text.toLowerCase().split(/\s+/).filter(Boolean);
}

function hasDetails(q: NoteQuery): boolean {
  return Boolean(
    q.after ||
      q.before ||
      q.time_of_day ||
      q.kind ||
      q.category_ids?.length ||
      q.uncategorized ||
      q.person_ids?.length ||
      q.tags?.length ||
      q.links_to?.length ||
      q.meeting_app ||
      q.min_duration_ms != null ||
      q.max_duration_ms != null ||
      q.has_recording != null ||
      q.has_quotes != null ||
      q.import_provider ||
      q.trash === "only" ||
      q.exclude?.length ||
      q.date_field === "deleted",
  );
}

/** The dates of a note a field looks at, in order of preference. */
function dates(note: Note, field: DateField): (string | null)[] {
  switch (field) {
    case "created":
      return [note.created_at];
    case "edited":
      return [note.updated_at];
    case "met":
      return [note.started_at];
    case "deleted":
      return [note.deleted_at];
    default:
      return [note.started_at ?? note.created_at, note.updated_at];
  }
}

function sortDate(note: Note, field: DateField): string {
  switch (field) {
    case "created":
      return note.created_at;
    case "edited":
      return note.updated_at;
    case "deleted":
      return note.deleted_at ?? note.updated_at;
    default:
      return note.started_at ?? note.updated_at;
  }
}

function inRange(q: NoteQuery, ts: string | null): boolean {
  if (!ts) return false;
  const t = Date.parse(ts);
  if (q.after && t < Date.parse(q.after)) return false;
  if (q.before && t >= Date.parse(q.before)) return false;
  const window = q.time_of_day;
  if (window) {
    const minute = (((Math.floor(t / 60_000) + (q.utc_offset_minutes ?? 0)) % 1440) + 1440) % 1440;
    const inside =
      window.start <= window.end
        ? minute >= window.start && minute < window.end
        : minute >= window.start || minute < window.end;
    if (!inside) return false;
  }
  return true;
}

/** The first of the field's dates in range, or null when none is (the note does not match). */
function matchedAt(state: MockState, q: NoteQuery, note: Note): string | null {
  const field = q.date_field ?? "any";
  const hit = dates(note, field).find((d) => inRange(q, d));
  if (hit) return hit;
  if (field === "edited") {
    const versions = state.versions
      .filter((v) => v.note_id === note.id && inRange(q, v.created_at))
      .map((v) => v.created_at)
      .sort();
    return versions.at(-1) ?? null;
  }
  return null;
}

function tagsOf(note: Note): Set<string> {
  return new Set(
    parseRefs(note.body_md)
      .filter((r) => r.kind === "tag")
      .map((r) => r.key),
  );
}

function linksOf(note: Note): Set<string> {
  return new Set(
    parseRefs(note.body_md)
      .filter((r) => r.kind !== "tag")
      .map((r) => r.key),
  );
}

function passesDetails(state: MockState, q: NoteQuery, note: Note): boolean {
  if (q.kind && note.kind !== q.kind) return false;
  const categories = q.category_ids ?? [];
  if (categories.length || q.uncategorized) {
    const inCategory = note.category_id !== null && categories.includes(note.category_id);
    if (!inCategory && !(q.uncategorized && note.category_id === null)) return false;
  }
  const transcript = state.transcripts.get(note.id);
  for (const person of q.person_ids ?? []) {
    if (!transcript?.speakers.some((s) => s.person_id === person)) return false;
  }
  if (q.tags?.length) {
    const tags = tagsOf(note);
    if (!q.tags.every((t) => tags.has(linkKey(t.replace(/^#/, ""))))) return false;
  }
  if (q.links_to?.length) {
    const links = linksOf(note);
    if (!q.links_to.every((t) => links.has(linkKey(t)))) return false;
  }
  if (q.meeting_app?.trim()) {
    const app = note.meeting_app?.toLowerCase() ?? "";
    if (!app.includes(q.meeting_app.trim().toLowerCase())) return false;
  }
  if (q.min_duration_ms != null || q.max_duration_ms != null) {
    const duration =
      transcript?.duration_ms ??
      (note.started_at && note.ended_at
        ? Date.parse(note.ended_at) - Date.parse(note.started_at)
        : null);
    if (duration == null) return false;
    if (q.min_duration_ms != null && duration < q.min_duration_ms) return false;
    if (q.max_duration_ms != null && duration > q.max_duration_ms) return false;
  }
  if (q.has_recording != null && Boolean(transcript?.has_audio) !== q.has_recording) return false;
  if (q.has_quotes != null && (transcript?.quotes.length ?? 0) > 0 !== q.has_quotes) return false;
  // The mock keeps no import source on notes: nothing matches one.
  if (q.import_provider) return false;
  return true;
}

function textOf(state: MockState, note: Note) {
  const body = stripMarkdown(note.body_md).replace(/\n/g, " ");
  const segments = state.transcripts.get(note.id)?.segments ?? [];
  const transcript = segments.map((s) => s.text).join(" ");
  return { body, segments, transcript, all: `${note.title}\n${body}\n${transcript}`.toLowerCase() };
}

export function findNotes(state: MockState, q: NoteQuery, limit: number): SearchHit[] {
  const terms = [
    ...(q.phrases ?? []).map((p) => p.toLowerCase().trim()),
    ...words(q.text ?? ""),
  ].filter(Boolean);
  if ((!terms.length && !hasDetails(q)) || limit <= 0) return [];
  const dated = Boolean(q.after || q.before || q.time_of_day);
  const field = q.date_field ?? "any";
  const trash = q.trash ?? "exclude";

  const hits: (SearchHit & { rank: number; sort: string })[] = [];
  for (const note of state.notes.values()) {
    // Trash
    if (trash === "only" && !note.deleted_at) continue;
    if (trash === "exclude" && field !== "deleted" && note.deleted_at) continue;
    if (!passesDetails(state, q, note)) continue;
    const at = dated ? matchedAt(state, q, note) : null;
    if (dated && !at) continue;

    // Words
    const text = textOf(state, note);
    if ((q.exclude ?? []).some((w) => w.trim() && text.all.includes(w.toLowerCase().trim())))
      continue;
    if (!terms.every((t) => text.all.includes(t))) continue;
    const first = terms[0];
    let source: SearchSource = "details";
    let excerpt = escapeHtml(note.snippet);
    let atMs: number | null = null;
    if (first) {
      if (note.title.toLowerCase().includes(first)) {
        source = "title";
        excerpt = makeExcerpt(text.body || note.title, terms);
      } else if (text.body.toLowerCase().includes(first)) {
        source = "body";
        excerpt = makeExcerpt(text.body, terms);
      } else {
        source = "transcript";
        const segment = text.segments.find((s) => s.text.toLowerCase().includes(first));
        excerpt = makeExcerpt(segment?.text ?? text.transcript, terms);
        atMs = segment?.start_ms ?? null;
      }
    }
    hits.push({
      note: summaryOf(note),
      source,
      excerpt,
      at_ms: atMs,
      matched_at: at,
      rank: SOURCE_RANK[source],
      sort: at ?? sortDate(note, field),
    });
  }
  hits.sort((a, b) =>
    terms.length
      ? a.rank - b.rank || b.note.updated_at.localeCompare(a.note.updated_at)
      : b.sort.localeCompare(a.sort),
  );
  return hits
    .slice(0, Math.min(limit, MAX_RESULTS))
    .map(({ rank: _rank, sort: _sort, ...hit }) => hit);
}

// ───────── Facets ─────────

function byCount(values: string[]): string[] {
  const counts = new Map<string, { value: string; count: number }>();
  for (const value of values) {
    const key = value.toLowerCase();
    const had = counts.get(key);
    counts.set(key, { value: had?.value ?? value, count: (had?.count ?? 0) + 1 });
  }
  return [...counts.values()]
    .sort((a, b) => b.count - a.count || a.value.localeCompare(b.value))
    .map((c) => c.value);
}

function facets(state: MockState): SearchFacets {
  const live = [...state.notes.values()].filter((n) => !n.deleted_at);
  const tags = live.flatMap((n) =>
    parseRefs(n.body_md)
      .filter((r) => r.kind === "tag")
      .map((r) => r.target),
  );
  const apps = live.flatMap((n) => (n.meeting_app ? [n.meeting_app] : []));
  return { tags: byCount(tags), meeting_apps: byCount(apps), import_providers: [] };
}

// ───────── Threads ─────────

function messageText(message: Message): string {
  return message.parts.flatMap((p) => (p.type === "text" ? [p.text] : [])).join("\n");
}

function searchThreads(state: MockState, query: string, limit: number): ThreadHit[] {
  const terms = words(query);
  const first = terms[0];
  if (!first) return [];
  const hits: ThreadHit[] = [];
  const threads = [...state.threads].sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  for (const thread of threads) {
    if (hits.length >= limit) break;
    const texts = state.messages
      .filter((m) => m.thread_id === thread.id)
      .map(messageText)
      .filter(Boolean);
    const all = `${thread.title}\n${texts.join("\n")}`.toLowerCase();
    if (!terms.every((t) => all.includes(t))) continue;
    const source = texts.find((t) => t.toLowerCase().includes(first)) ?? thread.title;
    hits.push({ thread, excerpt: makeExcerpt(source.replace(/\s+/g, " "), terms) });
  }
  return hits;
}
