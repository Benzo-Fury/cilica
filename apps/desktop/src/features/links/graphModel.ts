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
