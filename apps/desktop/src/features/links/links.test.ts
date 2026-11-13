import { createMockBackend } from "@/ipc/mock";
import type { LinkGraph } from "@/ipc/types";
import { growthOrder, searchNodes, toView } from "./graphModel";

describe("links in the mock backend", () => {
  it("builds the graph from the seed notes", async () => {
    const mock = createMockBackend({ latencyMs: 0 });
    const graph = await mock.commands.getLinkGraph(null, 1);
    const q3 = graph.nodes.find((n) => n.label === "Q3 planning sync");
    expect(q3?.kind).toBe("note");
    expect(q3?.degree).toBeGreaterThan(3);
    expect(graph.nodes.find((n) => n.label === "Rollout plan")?.kind).toBe("missing");
    expect(graph.nodes.find((n) => n.label === "#roadmap")?.kind).toBe("tag");
    expect(graph.nodes.find((n) => n.label === "Marco Diaz")?.kind).toBe("person");
    // People heard in a meeting join it with a dotted edge.
    expect(graph.edges.some((e) => e.kind === "transcript")).toBe(true);
    // Notes in Trash are not in the graph.
    expect(graph.nodes.some((n) => n.label === "Roadmap draft (old)")).toBe(false);
    for (const e of graph.edges) {
      expect(e.source).toBeLessThan(graph.nodes.length);
      expect(e.target).toBeLessThan(graph.nodes.length);
    }
    mock.dispose();
  });

  it("returns a local graph around a note", async () => {
    const mock = createMockBackend({ latencyMs: 0 });
    const one = await mock.commands.getLinkGraph("note_pricing", 1);
    const two = await mock.commands.getLinkGraph("note_pricing", 2);
    expect(one.nodes[one.focus ?? -1]?.note_id).toBe("note_pricing");
    expect(two.nodes.length).toBeGreaterThan(one.nodes.length);
    mock.dispose();
  });

  it("lists backlinks and links an unlinked mention", async () => {
    const mock = createMockBackend({ latencyMs: 0 });
    const before = await mock.commands.getBacklinks("note_onboarding");
    expect(before.linked.map((m) => m.note.title).sort()).toEqual([
      "1:1 with Priya",
      "Q3 planning sync",
    ]);
    expect(before.unlinked.map((m) => m.note.id)).toContain("note_search");
    await mock.commands.linkMention("note_search", "note_onboarding");
    const body = (await mock.commands.getNote("note_search")).body_md;
    expect(body).toContain("Compare with the [[Onboarding review|onboarding review]] numbers");
    const after = await mock.commands.getBacklinks("note_onboarding");
    expect(after.linked.map((m) => m.note.id)).toContain("note_search");
    mock.dispose();
  });

  it("follows a rename into the notes that link to it", async () => {
    const mock = createMockBackend({ latencyMs: 0 });
    await mock.commands.renameNote("note_onboarding", "Onboarding funnel");
    const q3 = await mock.commands.getNote("note_q3");
    expect(q3.body_md).toContain("[[Onboarding funnel]]");
    expect(q3.body_md).not.toContain("[[Onboarding review]]");
    mock.dispose();
  });
});

describe("graph view model", () => {
  it("filters kinds and re-indexes edges", async () => {
    const mock = createMockBackend({ latencyMs: 0 });
    const graph = await mock.commands.getLinkGraph(null, 1);
    const all = toView(graph);
    const notesOnly = toView(graph, {
      people: false,
      tags: false,
      missing: false,
      colorBy: "type",
    });
    expect(notesOnly.nodes.every((n) => n.tone === "note")).toBe(true);
    expect(notesOnly.nodes.length).toBeLessThan(all.nodes.length);
    for (const e of notesOnly.edges) {
      expect(notesOnly.nodes[e.source]).toBeDefined();
      expect(notesOnly.nodes[e.target]).toBeDefined();
    }
    // The node a view was opened on stays, whatever the filter.
    const kept = toView(
      graph,
      { people: false, tags: false, missing: false, colorBy: "type" },
      [],
      "t:roadmap",
    );
    expect(kept.nodes.some((n) => n.key === "t:roadmap")).toBe(true);
    const cats = await mock.commands.listCategories();
    const byCat = toView(
      graph,
      { people: true, tags: true, missing: true, colorBy: "category" },
      cats,
    );
    expect(byCat.nodes.some((n) => n.tone === "group" && n.colorIndex)).toBe(true);
    expect(searchNodes(all.nodes, "pricing interview")?.size).toBe(1);
    expect(searchNodes(all.nodes, "  ")).toBeNull();
    mock.dispose();
  });
});

describe("growth order", () => {
  const blank = {
    note_id: null,
    note_kind: null,
    category_id: null,
    person_id: null,
    color_index: null,
    degree: 1,
  };
  const note = (key: string, created_at: string) => ({
    ...blank,
    key,
    kind: "note" as const,
    label: key,
    created_at,
  });

  it("orders notes by when they were created, others with their first note", () => {
    const graph: LinkGraph = {
      nodes: [
        note("n:b", "2026-03-02T00:00:00.000000Z"),
        note("n:a", "2026-01-01T00:00:00.000000Z"),
        { ...blank, key: "p:sam", kind: "person", label: "Sam", created_at: null },
        note("n:c", "2026-03-02T00:00:00.000000Z"),
        { ...blank, key: "m:x", kind: "missing", label: "X", created_at: null },
      ],
      edges: [
        { source: 0, target: 2, kind: "link" },
        { source: 1, target: 2, kind: "transcript" },
        { source: 3, target: 4, kind: "link" },
      ],
      focus: null,
    };
    const view = toView(graph);
    const { ranks, when } = growthOrder(graph, view);
    const rankOf = (key: string) => ranks[view.nodes.findIndex((n) => n.key === key)];
    expect(when).toEqual(["2026-01-01T00:00:00.000000Z", "2026-03-02T00:00:00.000000Z"]);
    expect(rankOf("n:a")).toBe(0);
    // Created at the same moment: together.
    expect(rankOf("n:b")).toBe(1);
    expect(rankOf("n:c")).toBe(1);
    // A person arrives with the earliest note that reaches it.
    expect(rankOf("p:sam")).toBe(0);
    expect(rankOf("m:x")).toBe(1);
  });

  it("puts a node no note reaches at the end", () => {
    const graph: LinkGraph = {
      nodes: [
        note("n:a", "2026-01-01T00:00:00.000000Z"),
        note("n:b", "2026-02-01T00:00:00.000000Z"),
        { ...blank, key: "t:lonely", kind: "tag", label: "#lonely", created_at: null },
      ],
      edges: [],
      focus: null,
    };
    const view = toView(graph);
    expect(growthOrder(graph, view).ranks).toEqual([0, 1, 1]);
  });

  it("orders the mock graph with every node placed", async () => {
    const mock = createMockBackend({ latencyMs: 0 });
    const graph = await mock.commands.getLinkGraph(null, 1);
    const view = toView(graph);
    const { ranks, when } = growthOrder(graph, view);
    expect(ranks).toHaveLength(view.nodes.length);
    for (const r of ranks) {
      expect(r).toBeGreaterThanOrEqual(0);
      expect(r).toBeLessThan(when.length);
    }
    mock.dispose();
  });
});
