import {
  applyClick,
  clickKind,
  countNotes,
  emptySelection,
  extendSelection,
  inOrder,
  pruneSelection,
  rangeIds,
  visibleOrder,
} from "./selection";

// Sidebar order: uncategorised a, b; then a category with c, d; then one with e.
const order = ["a", "b", "c", "d", "e"];

describe("clickKind", () => {
  const event = { metaKey: false, ctrlKey: false, shiftKey: false };
  it("uses ⌘ on macOS and Ctrl elsewhere", () => {
    expect(clickKind({ ...event, metaKey: true }, true)).toBe("toggle");
    expect(clickKind({ ...event, ctrlKey: true }, true)).toBe("plain");
    expect(clickKind({ ...event, ctrlKey: true }, false)).toBe("toggle");
    expect(clickKind({ ...event, metaKey: true }, false)).toBe("plain");
  });
  it("treats Shift as a range, even with ⌘", () => {
    expect(clickKind({ ...event, shiftKey: true, metaKey: true }, true)).toBe("range");
  });
});

describe("rangeIds", () => {
  it("runs either way, across categories", () => {
    expect(rangeIds(order, "b", "d")).toEqual(["b", "c", "d"]);
    expect(rangeIds(order, "e", "b")).toEqual(["b", "c", "d", "e"]);
    expect(rangeIds(order, "c", "c")).toEqual(["c"]);
  });
  it("falls back to the clicked note when the anchor is not shown", () => {
    expect(rangeIds(order, "folded", "c")).toEqual(["c"]);
    expect(rangeIds(order, null, "c")).toEqual(["c"]);
    expect(rangeIds(order, "a", "gone")).toEqual([]);
  });
});

describe("applyClick", () => {
  it("clears the selection on a plain click, keeping the clicked note as the anchor", () => {
    const s = applyClick({ ids: ["a", "b"], anchor: "a" }, "d", "plain", order, null);
    expect(s).toEqual({ ids: [], anchor: "d" });
    expect(applyClick(s, "b", "range", order, null).ids).toEqual(["b", "c", "d"]);
  });

  it("toggles notes one at a time, leaving the open note out", () => {
    let s = applyClick(emptySelection, "c", "toggle", order, "a");
    expect(s).toEqual({ ids: ["c"], anchor: "c" });
    s = applyClick(s, "e", "toggle", order, "a");
    expect(s.ids).toEqual(["c", "e"]);
    s = applyClick(s, "c", "toggle", order, "a");
    expect(s.ids).toEqual(["e"]);
    s = applyClick(s, "e", "toggle", order, "a");
    expect(s.ids).toEqual([]);
  });

  it("selects a range from the anchor, or from the open note", () => {
    expect(applyClick({ ids: ["b"], anchor: "b" }, "d", "range", order, null).ids).toEqual([
      "b",
      "c",
      "d",
    ]);
    expect(applyClick(emptySelection, "c", "range", order, "e")).toEqual({
      ids: ["c", "d", "e"],
      anchor: "e",
    });
    // The anchor stays, so a second Shift-click redraws the range from it.
    const first = applyClick({ ids: ["c"], anchor: "c" }, "e", "range", order, null);
    expect(applyClick(first, "a", "range", order, null).ids).toEqual(["a", "b", "c"]);
  });
});

describe("extendSelection", () => {
  it("grows and shrinks from the anchor with Shift+arrows", () => {
    const one = extendSelection({ ids: ["b"], anchor: "b" }, "b", 1, order);
    expect(one).toEqual({ selection: { ids: ["b", "c"], anchor: "b" }, focus: "c" });
    const two = extendSelection(one?.selection ?? emptySelection, "c", 1, order);
    expect(two?.selection.ids).toEqual(["b", "c", "d"]);
    const back = extendSelection(two?.selection ?? emptySelection, "d", -1, order);
    expect(back?.selection.ids).toEqual(["b", "c"]);
  });
  it("starts from the focused note when it is not selected", () => {
    const s = extendSelection({ ids: ["a"], anchor: "a" }, "c", 1, order);
    expect(s?.selection).toEqual({ ids: ["c", "d"], anchor: "c" });
  });
  it("stops at either end", () => {
    expect(extendSelection(emptySelection, "a", -1, order)).toBeNull();
    expect(extendSelection(emptySelection, "e", 1, order)).toBeNull();
  });
});

describe("pruneSelection", () => {
  it("drops notes that are gone and keeps the same object when nothing changed", () => {
    const s = { ids: ["a", "c"], anchor: "c" };
    expect(pruneSelection(s, new Set(order))).toBe(s);
    expect(pruneSelection(s, new Set(["a"]))).toEqual({ ids: ["a"], anchor: null });
  });
});

describe("inOrder", () => {
  it("sorts by sidebar order, keeping notes in folded categories last", () => {
    expect(inOrder(["e", "folded", "a", "c"], order)).toEqual(["a", "c", "e", "folded"]);
  });
});

it("counts notes", () => {
  expect(countNotes(1)).toBe("1 note");
  expect(countNotes(3)).toBe("3 notes");
});

describe("visibleOrder", () => {
  it("lists uncategorised notes, then each open category in order, skipping folded ones", () => {
    const grouped = {
      uncategorised: [{ id: "a" }],
      byCategory: new Map([
        ["work", [{ id: "w1" }, { id: "w2" }]],
        ["home", [{ id: "h1" }]],
        ["old", [{ id: "o1" }]],
      ]),
    };
    const categories = [
      { id: "home", collapsed: false },
      { id: "old", collapsed: true },
      { id: "work", collapsed: false },
    ];
    expect(visibleOrder(grouped, categories)).toEqual(["a", "h1", "w1", "w2"]);
  });
});
