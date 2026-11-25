import { describe, expect, it } from "vitest";
import { addTab, type Bar, closeTab, direction, follow, keep, MAX_TABS, prune } from "./model";

const bar = (tabs: string[], preview: string | null = null): Bar => ({ tabs, preview });

describe("follow, looking at notes", () => {
  it("opens a note you look at in a preview tab beside the one you are on", () => {
    expect(follow(bar(["a", "b"]), "a", "x")).toEqual(bar(["a", "x", "b"], "x"));
  });

  it("swaps the preview tab for the next note you look at", () => {
    expect(follow(bar(["a", "x"], "x"), "x", "y")).toEqual(bar(["a", "y"], "y"));
  });

  it("keeps only one preview tab, moving it beside where you are", () => {
    expect(follow(bar(["a", "b", "p"], "p"), "a", "x")).toEqual(bar(["a", "x", "b"], "x"));
  });

  it("opens at the end when you arrive from somewhere that is not a tab", () => {
    expect(follow(bar(["a"]), null, "x")).toEqual(bar(["a", "x"], "x"));
    expect(follow(bar(["a", "p"], "p"), null, "x")).toEqual(bar(["a", "x"], "x"));
    expect(follow(bar([]), null, "x")).toEqual(bar(["x"], "x"));
  });

  it("selects a note that already has a tab", () => {
    expect(follow(bar(["a", "b"], "b"), "a", "b")).toEqual(bar(["a", "b"], "b"));
  });

  it("leaves the tabs alone when you leave notes", () => {
    expect(follow(bar(["a"], "a"), "a", null)).toEqual(bar(["a"], "a"));
  });

  it("drops the leftmost tab when there are too many", () => {
    const full = Array.from({ length: MAX_TABS }, (_, i) => `n${i}`);
    const next = follow(bar(full), null, "x");
    expect(next.tabs).toHaveLength(MAX_TABS);
    expect(next.tabs[0]).toBe("n1");
    expect(next.tabs.at(-1)).toBe("x");
  });
});

describe("follow, working on notes", () => {
  it("gives a new note a tab of its own, even from the preview tab", () => {
    expect(follow(bar(["a", "p"], "p"), "p", "x", "work")).toEqual(bar(["a", "p", "x"], "p"));
  });

  it("keeps the tab of an open note you start working on", () => {
    expect(follow(bar(["a", "p"], "p"), "a", "p", "work")).toEqual(bar(["a", "p"], null));
  });
});

describe("keep", () => {
  it("turns the preview tab into a kept one", () => {
    expect(keep(bar(["a", "p"], "p"), "p")).toEqual(bar(["a", "p"], null));
  });

  it("leaves the preview alone for any other note", () => {
    expect(keep(bar(["a", "p"], "p"), "a")).toEqual(bar(["a", "p"], "p"));
  });

  it("means the next note you look at gets a tab of its own", () => {
    const kept = keep(bar(["a", "p"], "p"), "p");
    expect(follow(kept, "p", "x")).toEqual(bar(["a", "p", "x"], "x"));
  });
});

describe("addTab", () => {
  it("opens the note right after the current tab, kept", () => {
    expect(addTab(bar(["a", "b", "c"]), "a", "x")).toEqual(bar(["a", "x", "b", "c"]));
  });

  it("leaves the preview tab where it is", () => {
    expect(addTab(bar(["a", "p"], "p"), "a", "x")).toEqual(bar(["a", "x", "p"], "p"));
  });

  it("opens at the end when no tab is on screen", () => {
    expect(addTab(bar(["a", "b"]), null, "x")).toEqual(bar(["a", "b", "x"]));
  });

  it("keeps a note that is already open in the preview tab", () => {
    expect(addTab(bar(["a", "b"], "b"), "a", "b")).toEqual(bar(["a", "b"]));
  });

  it("never drops the tab on screen to make room", () => {
    const full = Array.from({ length: MAX_TABS }, (_, i) => `n${i}`);
    const next = addTab(bar(full), "n0", "x");
    expect(next.tabs).toContain("n0");
    expect(next.tabs).toContain("x");
    expect(next.tabs).toHaveLength(MAX_TABS);
  });
});

describe("closeTab", () => {
  it("shows the tab to the right after closing the one on screen", () => {
    expect(closeTab(bar(["a", "b", "c"]), "b", "b")).toEqual({ bar: bar(["a", "c"]), next: "c" });
  });

  it("falls back to the tab on the left at the end of the bar", () => {
    expect(closeTab(bar(["a", "b"]), "b", "b")).toEqual({ bar: bar(["a"]), next: "a" });
  });

  it("goes nowhere when the last tab closes", () => {
    expect(closeTab(bar(["a"], "a"), "a", "a")).toEqual({ bar: bar([]), next: null });
  });

  it("stays put when closing a tab that is not on screen", () => {
    expect(closeTab(bar(["a", "b"], "b"), "b", "a")).toEqual({ bar: bar(["a"]), next: "a" });
  });
});

describe("direction", () => {
  it("says which way the selection moved", () => {
    expect(direction(["a", "b", "c"], "a", "c")).toBe(1);
    expect(direction(["a", "b", "c"], "c", "b")).toBe(-1);
    expect(direction(["a", "b"], null, "b")).toBe(0);
    expect(direction(["a", "b"], "a", "x")).toBe(0);
  });
});

describe("prune", () => {
  it("drops tabs for notes that are gone, and the preview with them", () => {
    expect(prune(bar(["a", "b", "c"], "b"), (id) => id !== "b")).toEqual(bar(["a", "c"]));
  });
});
