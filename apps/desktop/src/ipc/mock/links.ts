/** Links, the graph and backlinks: the mock side of crates/store/src/links.rs. */
import type {
  Backlinks,
  GraphEdgeKind,
  GraphNode,
  LinkGraph,
  Mention,
  Note,
  NoteSummary,
} from "@/ipc/types";
import {
  findUnlinked,
  linkingLine,
  linkKey,
  parseRefs,
  plainLine,
  rewriteWikiLinks,
} from "@/lib/links";
import { requireLiveNote, requireNote, summaryOf } from "./notes";
import type { MockContext, MockState } from "./state";
import { appError, iso } from "./util";

const MAX_DEPTH = 3;
const EXCERPT_CHARS = 180;

function excerpt(line: string): string {
  const plain = plainLine(line);
  if (plain.length <= EXCERPT_CHARS) return plain;
  const cut = plain.slice(0, EXCERPT_CHARS - 1);
  return `${cut.slice(0, cut.lastIndexOf(" ")).trimEnd()}…`;
}

/** Live notes, newest first (a title used twice resolves to the newest). */
function liveNewestFirst(state: MockState): Note[] {
  return [...state.notes.values()]
    .filter((n) => !n.deleted_at)
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
}

export function buildGraph(state: MockState): LinkGraph {
  const nodes: GraphNode[] = [];
  const index = new Map<string, number>();
  const edges = new Map<string, { a: number; b: number; kind: GraphEdgeKind }>();
  const node = (key: string, make: () => Omit<GraphNode, "key" | "degree">) => {
    const at = index.get(key);
    if (at !== undefined) return at;
    nodes.push({ ...make(), key, degree: 0 });
    index.set(key, nodes.length - 1);
    return nodes.length - 1;
  };
  const edge = (x: number, y: number, kind: GraphEdgeKind) => {
    if (x === y) return;
    const [a, b] = x < y ? [x, y] : [y, x];
    const id = `${a}:${b}`;
    const had = edges.get(id);
    if (!had || had.kind === "transcript") edges.set(id, { a, b, kind });
  };
  const blank = {
    note_id: null,
    note_kind: null,
    category_id: null,
    person_id: null,
    color_index: null,
  };

  const notes = liveNewestFirst(state);
  const byTitle = new Map<string, number>();
  for (const n of notes) {
    const i = node(`n:${n.id}`, () => ({
      ...blank,
      kind: "note",
      label: n.title,
      note_id: n.id,
      note_kind: n.kind,
      category_id: n.category_id,
    }));
    const key = linkKey(n.title);
    if (key && !byTitle.has(key)) byTitle.set(key, i);
  }
  const personNode = (id: string) => {
    const p = state.people.find((x) => x.id === id);
    if (!p) return null;
    return node(`p:${p.id}`, () => ({
      ...blank,
      kind: "person",
      label: p.name,
      person_id: p.id,
      color_index: p.color_index,
    }));
  };
  for (const n of notes) {
    const from = index.get(`n:${n.id}`) ?? 0;
    for (const r of parseRefs(n.body_md)) {
      if (r.kind === "tag") {
        edge(
          from,
          node(`t:${r.key}`, () => ({ ...blank, kind: "tag", label: `#${r.target}` })),
          "tag",
        );
        continue;
      }
      const kind: GraphEdgeKind = r.kind === "embed" ? "embed" : "link";
      const note = byTitle.get(r.key);
      const person = state.people.find((p) => linkKey(p.name) === r.key);
      const to =
        note ??
        (person ? personNode(person.id) : null) ??
        node(`m:${r.key}`, () => ({ ...blank, kind: "missing", label: r.target }));
      edge(from, to, kind);
    }
    for (const s of state.transcripts.get(n.id)?.speakers ?? []) {
      if (!s.person_id || s.is_self) continue;
      const to = personNode(s.person_id);
      if (to !== null) edge(from, to, "transcript");
    }
  }
  const list = [...edges.values()]
    .sort((x, y) => x.a - y.a || x.b - y.b)
    .map((e) => ({ source: e.a, target: e.b, kind: e.kind }));
  for (const e of list) {
    const s = nodes[e.source];
    const t = nodes[e.target];
    if (s) s.degree++;
    if (t) t.degree++;
  }
  return { nodes, edges: list, focus: null };
}

function neighbourhood(graph: LinkGraph, start: number, depth: number): LinkGraph {
  const adjacent: number[][] = graph.nodes.map(() => []);
  for (const e of graph.edges) {
    adjacent[e.source]?.push(e.target);
    adjacent[e.target]?.push(e.source);
  }
  const level = new Map<number, number>([[start, 0]]);
  const queue = [start];
  while (queue.length) {
    const at = queue.shift() as number;
    const d = level.get(at) ?? 0;
    if (d >= depth || (at !== start && graph.nodes[at]?.kind === "tag")) continue;
    for (const next of adjacent[at] ?? []) {
      if (!level.has(next)) {
        level.set(next, d + 1);
        queue.push(next);
      }
    }
  }
  const keep = [...level.keys()].sort((a, b) => a - b);
  const remap = new Map(keep.map((old, i) => [old, i]));
  return {
    nodes: keep.map((i) => graph.nodes[i] as GraphNode),
    edges: graph.edges.flatMap((e) => {
      const source = remap.get(e.source);
      const target = remap.get(e.target);
      return source === undefined || target === undefined ? [] : [{ ...e, source, target }];
    }),
    focus: remap.get(start) ?? null,
  };
}

/** After a rename, point links to the old title at the new one (unless another note keeps it). */
export function relinkAfterRename(
  state: MockState,
  noteId: string,
  oldTitle: string,
  newTitle: string,
) {
  const oldKey = linkKey(oldTitle);
  if (!oldKey || oldKey === linkKey(newTitle) || !newTitle.trim()) return 0;
  if (liveNewestFirst(state).some((n) => n.id !== noteId && linkKey(n.title) === oldKey)) return 0;
  let changed = 0;
  for (const n of liveNewestFirst(state)) {
    const next = rewriteWikiLinks(n.body_md, oldKey, newTitle.trim());
    if (next !== null) {
      n.body_md = next;
      n.updated_at = iso();
      changed++;
    }
  }
  return changed;
}

export function linksCommands({ state }: MockContext) {
  return {
    getLinkGraph(focus: string | null, depth: number): LinkGraph {
      const graph = buildGraph(state);
      if (focus === null) return graph;
      const start = graph.nodes.findIndex((n) => n.note_id === focus);
      if (start < 0) throw appError("not_found", "That note was not found.");
      return neighbourhood(graph, start, Math.min(Math.max(depth, 1), MAX_DEPTH));
    },

    getBacklinks(noteId: string): Backlinks {
      const note = requireNote(state, noteId);
      const key = linkKey(note.title);
      if (!key) return { linked: [], unlinked: [] };
      const notes = liveNewestFirst(state);
      const owner = notes.find((n) => linkKey(n.title) === key);
      const linked: Mention[] = [];
      const seen = new Set([noteId]);
      if (owner?.id === noteId) {
        for (const n of notes) {
          if (n.id === noteId) continue;
          if (!parseRefs(n.body_md).some((r) => r.kind !== "tag" && r.key === key)) continue;
          seen.add(n.id);
          linked.push({
            note: { id: n.id, title: n.title },
            excerpt: excerpt(linkingLine(n.body_md, key) ?? ""),
          });
        }
      }
      const unlinked: Mention[] = [];
      if (key.length >= 3) {
        for (const n of notes) {
          if (seen.has(n.id)) continue;
          const hit = findUnlinked(n.body_md, note.title);
          if (hit)
            unlinked.push({ note: { id: n.id, title: n.title }, excerpt: excerpt(hit.line) });
        }
      }
      return { linked: linked.slice(0, 50), unlinked: unlinked.slice(0, 50) };
    },

    linkMention(sourceNoteId: string, targetNoteId: string): NoteSummary {
      const title = requireNote(state, targetNoteId).title;
      const source = requireLiveNote(state, sourceNoteId);
      const hit = findUnlinked(source.body_md, title);
      if (!hit) throw appError("invalid", "That mention is no longer in the note.");
      const written = source.body_md.slice(hit.start, hit.end);
      const link = written === title ? `[[${title}]]` : `[[${title}|${written}]]`;
      source.body_md = source.body_md.slice(0, hit.start) + link + source.body_md.slice(hit.end);
      source.updated_at = iso();
      return summaryOf(source);
    },
  };
}
