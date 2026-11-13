import { createMockBackend, type MockBackend } from "./index";

let backend: MockBackend;

beforeEach(() => {
  backend = createMockBackend({ latencyMs: 0 });
});

afterEach(() => {
  backend.dispose();
});

const api = () => backend.commands;
const titles = (hits: { note: { title: string } }[]) => hits.map((h) => h.note.title);

describe("findNotes", () => {
  it("finds nothing when nothing is asked", async () => {
    expect(await api().findNotes({}, 20)).toEqual([]);
    expect(await api().findNotes({ trash: "include" }, 20)).toEqual([]);
  });

  it("matches words like searchNotes", async () => {
    const hits = await api().findNotes({ text: "pricing" }, 20);
    expect(hits.length).toBeGreaterThan(0);
    expect(hits.every((h) => h.matched_at === null)).toBe(true);
  });

  it("filters meetings by app and duration, newest first", async () => {
    const zoom = await api().findNotes({ kind: "meeting", meeting_app: "zoom" }, 20);
    expect(titles(zoom)).toContain("Q3 planning sync");
    expect(zoom.every((h) => h.note.meeting_app === "Zoom")).toBe(true);
    expect(zoom[0]?.source).toBe("details");

    const long = await api().findNotes({ min_duration_ms: 40 * 60_000 }, 20);
    expect(titles(long)).toContain("Q3 planning sync");
    expect(titles(long)).not.toContain("Pricing interview, Marco");
  });

  it("filters by date and says which date matched", async () => {
    const after = new Date(Date.now() - 24 * 3_600_000).toISOString();
    const hits = await api().findNotes({ after, date_field: "created" }, 50);
    expect(hits.length).toBeGreaterThan(0);
    for (const hit of hits) {
      expect(hit.matched_at).not.toBeNull();
      expect(Date.parse(hit.matched_at ?? "")).toBeGreaterThanOrEqual(Date.parse(after));
    }
  });

  it("searches Trash only when asked", async () => {
    const trash = await api().listTrash();
    const trashed = trash[0];
    expect(trashed).toBeDefined();
    const word = trashed?.title.split(/\s+/)[0] ?? "";
    const only = await api().findNotes({ text: word, trash: "only" }, 20);
    expect(only.every((h) => h.note.deleted_at)).toBe(true);
    expect(only.length).toBeGreaterThan(0);
    const live = await api().findNotes({ text: word }, 20);
    expect(live.every((h) => !h.note.deleted_at)).toBe(true);
  });

  it("filters by tags in note bodies", async () => {
    const note = await api().createNote(null);
    await api().updateNoteBody(note.id, "About #launch-plan here");
    const hits = await api().findNotes({ tags: ["#launch-plan"] }, 20);
    expect(hits.map((h) => h.note.id)).toEqual([note.id]);
    expect((await api().searchFacets()).tags).toContain("launch-plan");
  });
});

describe("searchFacets", () => {
  it("lists the apps meetings were recorded in", async () => {
    const facets = await api().searchFacets();
    expect(facets.meeting_apps).toEqual(expect.arrayContaining(["Zoom", "Google Meet"]));
  });
});

describe("searchThreads", () => {
  it("finds conversations by their messages", async () => {
    const hits = await api().searchThreads("onboarding owns", 5);
    expect(hits.map((h) => h.thread.title)).toEqual(["Onboarding decisions"]);
    expect(hits[0]?.excerpt).toContain("<mark>");
    expect(await api().searchThreads("nothing like this", 5)).toEqual([]);
  });
});
