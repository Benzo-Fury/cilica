/**
 * What a `[[link]]` in the editor points at, from data the UI already has (the notes list and
 * Saved people), so drawing a link never calls the backend. Resolution matches the store's: the
 * newest note with that title (case-insensitive), else a saved person with that name, else a note
 * not created yet.
 */
import { create } from "zustand";
import type { NoteSummary, Person } from "@/ipc/types";
import { linkKey, wikiPage } from "@/lib/links";

export type Resolved =
  | { kind: "note"; noteId: string; title: string; snippet: string }
  | { kind: "person"; personId: string; name: string; color: number }
  | { kind: "missing"; title: string };

interface ResolverState {
  notes: Map<string, NoteSummary>;
  people: Map<string, Person>;
  set: (notes: readonly NoteSummary[], people: readonly Person[]) => void;
}

export const useLinkResolver = create<ResolverState>()((set) => ({
  notes: new Map(),
  people: new Map(),
  set: (notes, people) => {
    const byTitle = new Map<string, NoteSummary>();
    const newest = [...notes].sort((a, b) => b.updated_at.localeCompare(a.updated_at));
    for (const n of newest) {
      const key = linkKey(n.title);
      if (key && !byTitle.has(key)) byTitle.set(key, n);
    }
    const byName = new Map<string, Person>();
    for (const p of people) {
      const key = linkKey(p.name);
      if (key && !byName.has(key)) byName.set(key, p);
    }
    set({ notes: byTitle, people: byName });
  },
}));

/** Resolve a page name (no heading or alias) or a wiki link's inside (`Title#Heading|shown`). */
export function resolveLink(target: string): Resolved {
  const page = wikiPage(target) ?? target.trim();
  const key = linkKey(page);
  const { notes, people } = useLinkResolver.getState();
  const note = notes.get(key);
  if (note) return { kind: "note", noteId: note.id, title: note.title, snippet: note.snippet };
  const person = people.get(key);
  if (person) {
    return { kind: "person", personId: person.id, name: person.name, color: person.color_index };
  }
  return { kind: "missing", title: page };
}

/** Note titles for `[[` suggestions, best matches first. */
export function suggestNotes(query: string, limit = 8): NoteSummary[] {
  const q = linkKey(query);
  const all = [...useLinkResolver.getState().notes.values()];
  if (!q) return all.slice(0, limit);
  const scored: { note: NoteSummary; score: number }[] = [];
  for (const note of all) {
    const t = linkKey(note.title);
    const at = t.indexOf(q);
    if (at < 0) continue;
    // Whole title, then start of title, then start of a word, then anywhere.
    const score = t === q ? 0 : at === 0 ? 1 : /[\s\-_/:(]/.test(t[at - 1] ?? "") ? 2 : 3;
    scored.push({ note, score });
  }
  return scored
    .sort((a, b) => a.score - b.score || a.note.title.length - b.note.title.length)
    .slice(0, limit)
    .map((s) => s.note);
}
