/** Links, the graph and backlinks: the mock side of crates/store/src/links.rs. */
import type {
  Backlinks,
  GraphEdgeKind,
  GraphNode,
  LinkGraph,
  LinkSuggestion,
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
import {
  ALIASES,
  bodyOf,
  joinProperties,
  linkTarget,
  listProperty,
  RELATED,
  setListProperty,
  splitProperties,
  UNRELATED,
} from "@/lib/properties";
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
    created_at: null,
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
      created_at: n.created_at,
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

/**
 * `?graph=<count>` in the browser preview pads the global graph with that many made-up notes,
 * in loose clusters with a few people and tags, so a big graph can be previewed. They open
 * nothing when clicked. Their creation dates spread over the year before the seed notes, so the
 * graph's replay has something to show.
 */
function demoNodeCount(): number {
  if (typeof window === "undefined") return 0;
  const raw = new URLSearchParams(window.location.search).get("graph");
  if (raw === null) return 0;
  const n = Number.parseInt(raw, 10);
  return Number.isFinite(n) ? Math.min(Math.max(n, 0), 2000) : 300;
}

const DEMO_WORDS = [
  "Roadmap",
  "Pricing",
  "Hiring",
  "Onboarding",
  "Research",
  "Launch",
  "Retro",
  "Budget",
  "Design",
  "Support",
  "Interview",
  "Strategy",
  "Metrics",
  "Partners",
  "Offsite",
  "Release",
];

export function padGraph(graph: LinkGraph, count: number): LinkGraph {
  if (count <= 0) return graph;
  // A small deterministic generator, so the layout is the same on every load.
  let seed = 7;
  const rand = () => {
    seed = (seed * 1_103_515_245 + 12_345) % 2_147_483_648;
    return seed / 2_147_483_648;
  };
  const nodes = [...graph.nodes];
  const edges = [...graph.edges];
  const blank = {
    note_id: null,
    note_kind: null,
    category_id: null,
    person_id: null,
    color_index: null,
    created_at: null,
  };
  const add = (node: Omit<GraphNode, "degree">) => nodes.push({ ...node, degree: 0 }) - 1;
  const link = (a: number, b: number, kind: GraphEdgeKind) => {
    if (a !== b) edges.push({ source: Math.min(a, b), target: Math.max(a, b), kind });
  };
  // Created over the year before the seed notes, in the order they are made up here.
  const end = Date.parse("2026-10-01T00:00:00Z");
  const span = 365 * 24 * 3600 * 1000;
  const createdAt = (fraction: number) => new Date(end - span * (1 - fraction)).toISOString();
  const clusters = Math.max(3, Math.round(count / 25));
  const hubs: number[] = [];
  const people: number[] = [];
  const tags: number[] = [];
  for (let c = 0; c < clusters; c++) {
    const word = DEMO_WORDS[c % DEMO_WORDS.length] as string;
    hubs.push(
      add({
        ...blank,
        key: `demo:hub:${c}`,
        kind: "note",
        label: `${word} hub ${c + 1}`,
        created_at: createdAt((c / clusters) * 0.85),
      }),
    );
    if (c % 3 === 0) {
      const label = `Person ${people.length + 1}`;
      people.push(
        add({ ...blank, key: `demo:p:${c}`, kind: "person", label, color_index: (c % 8) + 1 }),
      );
    }
    if (c % 4 === 1)
      tags.push(
        add({ ...blank, key: `demo:t:${c}`, kind: "tag", label: `#${word.toLowerCase()}` }),
      );
  }
  const notes: number[] = [];
  const pick = (from: number[]) => from[Math.floor(rand() * from.length)] as number;
  for (let i = 0; notes.length + hubs.length < count; i++) {
    const c = Math.floor(rand() * clusters);
    const word = DEMO_WORDS[c % DEMO_WORDS.length] as string;
    const kind = rand() < 0.04 ? "missing" : "note";
    const at = add({
      ...blank,
      key: `demo:n:${i}`,
      kind,
      label: `${word} note ${i + 1}`,
      created_at: kind === "note" ? createdAt((notes.length + hubs.length) / count) : null,
    });
    link(at, hubs[c] as number, "link");
    // A few links between notes, the odd meeting with a person, and some tags.
    if (notes.length > 0 && rand() < 0.45) link(at, pick(notes), "link");
    if (people.length > 0 && rand() < 0.12) link(at, pick(people), "transcript");
    if (tags.length > 0 && rand() < 0.18) link(at, pick(tags), "tag");
    notes.push(at);
  }
  // Join the hubs in a loose ring, and tie the first to the real notes.
  for (let c = 0; c < clusters; c++)
    link(hubs[c] as number, hubs[(c + 1) % clusters] as number, "link");
  if (graph.nodes.length > 0) link(hubs[0] as number, 0, "link");
  for (const n of nodes) n.degree = 0;
  for (const e of edges) {
    (nodes[e.source] as GraphNode).degree++;
    (nodes[e.target] as GraphNode).degree++;
  }
  return { ...graph, nodes, edges };
}

export function linksCommands({ state }: MockContext) {
  const suggestionTitle = (noteId: string, targetNoteId: string) => {
    if (noteId === targetNoteId) throw appError("invalid", "A note cannot link to itself.");
    const title = requireNote(state, targetNoteId).title.trim();
    if (!title)
      throw appError(
        "invalid",
        "That note has no title yet, so there is nothing to link to. Give it a title first.",
      );
    return title;
  };
  const editProperties = (noteId: string, edit: (yaml: string) => string): NoteSummary => {
    const note = requireLiveNote(state, noteId);
    const { yaml, body } = splitProperties(note.body_md);
    const next = joinProperties(edit(yaml ?? ""), body);
    note.body_md = splitProperties(next).yaml?.trim() ? next : body;
    note.updated_at = iso();
    return summaryOf(note);
  };
  return {
    getLinkGraph(focus: string | null, depth: number): LinkGraph {
      const graph = buildGraph(state);
      if (focus === null) return padGraph(graph, demoNodeCount());
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

    getLinkSuggestions(noteId: string): LinkSuggestion[] {
      const note = requireNote(state, noteId);
      const notes = liveNewestFirst(state);
      const ownKey = linkKey(note.title);
      const owners = new Map<string, Note>();
      for (const n of notes)
        if (linkKey(n.title) && !owners.has(linkKey(n.title))) owners.set(linkKey(n.title), n);
      const refs = parseRefs(note.body_md).filter((r) => r.kind !== "tag");
      const skip = new Set([ownKey, ...refs.map((r) => r.key)]);
      const yaml = splitProperties(note.body_md).yaml ?? "";
      for (const v of listProperty(yaml, UNRELATED)) skip.add(linkKey(linkTarget(v)));
      const open = (n: Note) => {
        const key = linkKey(n.title);
        return n.id !== noteId && !skip.has(key) && owners.get(key)?.id === n.id;
      };
      const inbound = new Map<string, number>();
      for (const n of notes)
        for (const r of parseRefs(n.body_md))
          if (r.kind !== "tag") inbound.set(r.key, (inbound.get(r.key) ?? 0) + 1);
      const hub = (n: Note) => Math.min(inbound.get(linkKey(n.title)) ?? 0, 20);
      const found = new Map<string, LinkSuggestion & { score: number }>();
      const offer = (s: LinkSuggestion & { score: number }) => {
        const had = found.get(s.note.id);
        if (!had || had.score < s.score) found.set(s.note.id, s);
      };
      const body = bodyOf(note.body_md);
      for (const n of notes) {
        if (!open(n)) continue;
        const nYaml = splitProperties(n.body_md).yaml ?? "";
        for (const name of [n.title, ...listProperty(nYaml, ALIASES)]) {
          if (name.trim().length < 3) continue;
          const hit = findUnlinked(body, name.trim());
          if (hit) {
            offer({
              note: { id: n.id, title: n.title },
              reason: "mentioned",
              excerpt: excerpt(hit.line),
              shared: [],
              score: 100 + hub(n),
            });
            break;
          }
        }
      }
      if (owners.get(ownKey)?.id === noteId) {
        for (const n of notes) {
          if (!open(n) || !parseRefs(n.body_md).some((r) => r.kind !== "tag" && r.key === ownKey))
            continue;
          offer({
            note: { id: n.id, title: n.title },
            reason: "links_here",
            excerpt: excerpt(linkingLine(n.body_md, ownKey) ?? ""),
            shared: [],
            score: 50 + hub(n),
          });
        }
      }
      const mine = new Set(refs.map((r) => r.key));
      for (const n of notes) {
        if (!open(n)) continue;
        const shared = parseRefs(n.body_md).filter((r) => r.kind !== "tag" && mine.has(r.key));
        if (shared.length < 2) continue;
        offer({
          note: { id: n.id, title: n.title },
          reason: "shares_links",
          excerpt: "",
          shared: shared.slice(0, 3).map((r) => r.target),
          score: 20 + 5 * Math.min(shared.length, 10) + hub(n),
        });
      }
      return [...found.values()]
        .sort((a, b) => b.score - a.score || a.note.title.localeCompare(b.note.title))
        .slice(0, 3)
        .map(({ score: _score, ...s }) => s);
    },

    acceptLinkSuggestion(noteId: string, targetNoteId: string): NoteSummary {
      const title = suggestionTitle(noteId, targetNoteId);
      return editProperties(noteId, (yaml) => {
        const key = linkKey(title);
        const related = listProperty(yaml, RELATED);
        if (!related.some((v) => linkKey(linkTarget(v)) === key)) related.push(`[[${title}]]`);
        const unrelated = listProperty(yaml, UNRELATED).filter(
          (v) => linkKey(linkTarget(v)) !== key,
        );
        return setListProperty(setListProperty(yaml, RELATED, related), UNRELATED, unrelated);
      });
    },

    dismissLinkSuggestion(noteId: string, targetNoteId: string): NoteSummary {
      const title = suggestionTitle(noteId, targetNoteId);
      return editProperties(noteId, (yaml) => {
        const unrelated = listProperty(yaml, UNRELATED);
        if (!unrelated.some((v) => linkKey(linkTarget(v)) === linkKey(title)))
          unrelated.push(title);
        return setListProperty(yaml, UNRELATED, unrelated);
      });
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
