/** From the backend's link graph to what the canvas draws: filtered, coloured, re-indexed. */
import type { Category, LinkGraph } from "@/ipc/types";
import type { GraphViewEdge, GraphViewNode } from "@/ui";

export interface GraphFilter {
  people: boolean;
  tags: boolean;
  missing: boolean;
  colorBy: "type" | "category";
}

export const SHOW_ALL: GraphFilter = { people: true, tags: true, missing: true, colorBy: "type" };

/** A category's colour: its place in the sidebar, cycling through the eight speaker colours. */
export function categoryColor(categories: readonly Category[], id: string | null): number | null {
  if (!id) return null;
  const at = categories.findIndex((c) => c.id === id);
  return at < 0 ? null : (at % 8) + 1;
}

export function toView(
  graph: LinkGraph,
  filter: GraphFilter = SHOW_ALL,
  categories: readonly Category[] = [],
  /** Always kept, whatever the filter (the node the view was opened on). */
  keep: string | null = null,
): { nodes: GraphViewNode[]; edges: GraphViewEdge[] } {
  const index = new Map<number, number>();
  const nodes: GraphViewNode[] = [];
  graph.nodes.forEach((n, i) => {
    const shown =
      n.key === keep ||
      n.kind === "note" ||
      (n.kind === "person" && filter.people) ||
      (n.kind === "tag" && filter.tags) ||
      (n.kind === "missing" && filter.missing);
    if (!shown) return;
    index.set(i, nodes.length);
    const group =
      filter.colorBy === "category" && n.kind === "note"
        ? categoryColor(categories, n.category_id)
        : null;
    nodes.push({
      key: n.key,
      label: n.label || "Untitled",
      degree: n.degree,
      tone: group ? "group" : n.kind,
      colorIndex: group ?? n.color_index,
    });
  });
  const edges: GraphViewEdge[] = [];
  for (const e of graph.edges) {
    const source = index.get(e.source);
    const target = index.get(e.target);
    if (source === undefined || target === undefined) continue;
    edges.push({ source, target, dotted: e.kind === "transcript" });
  }
  return { nodes, edges };
}

/** Keys of the nodes whose label contains every word of the query. */
export function searchNodes(nodes: readonly GraphViewNode[], query: string): Set<string> | null {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return null;
  return new Set(
    nodes.filter((n) => words.every((w) => n.label.toLowerCase().includes(w))).map((n) => n.key),
  );
}

/**
 * The order the graph grew in, for the replay: notes by when they were created (notes created at
 * the same moment, as in an import, appear together), and every other node (a person, a tag, a
 * note not created yet) with the first note that links to it. `when[step]` is the creation time
 * of that step's notes.
 */
export function growthOrder(
  graph: LinkGraph,
  view: { nodes: readonly GraphViewNode[]; edges: readonly GraphViewEdge[] },
): { ranks: number[]; when: string[] } {
  const created = new Map<string, string>();
  for (const n of graph.nodes) if (n.created_at) created.set(n.key, n.created_at);
  const times = [
    ...new Set(view.nodes.map((n) => created.get(n.key)).filter((t): t is string => !!t)),
  ].sort((a, b) => Date.parse(a) - Date.parse(b) || a.localeCompare(b));
  const step = new Map(times.map((t, i) => [t, i]));
  const ranks = view.nodes.map((n) => {
    const t = created.get(n.key);
    return t === undefined ? Number.POSITIVE_INFINITY : (step.get(t) ?? 0);
  });
  // Everything else arrives with its earliest neighbouring note.
  const earliest = ranks.map(() => Number.POSITIVE_INFINITY);
  for (const e of view.edges) {
    const a = ranks[e.source] ?? Number.POSITIVE_INFINITY;
    const b = ranks[e.target] ?? Number.POSITIVE_INFINITY;
    earliest[e.source] = Math.min(earliest[e.source] ?? Number.POSITIVE_INFINITY, b);
    earliest[e.target] = Math.min(earliest[e.target] ?? Number.POSITIVE_INFINITY, a);
  }
  const last = Math.max(0, times.length - 1);
  return {
    ranks: ranks.map((r, i) => {
      if (Number.isFinite(r)) return r;
      const near = earliest[i] ?? Number.POSITIVE_INFINITY;
      return Number.isFinite(near) ? near : last;
    }),
    when: times,
  };
}
