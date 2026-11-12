import { describe, expect, it } from "vitest";
import { buildViewContext, type ContextInputs, contextLabel, scopeLabel } from "./context";

const base: ContextInputs = {
  view: { name: "note", noteId: "n1" },
  note: { id: "n1", title: "Acme kickoff", category_id: "c1" },
  panel: "transcript",
  selectedText: "",
  searchQuery: null,
  recording: { phase: "idle" },
};

describe("buildViewContext", () => {
  it("describes the open note and panel", () => {
    const ctx = buildViewContext(base);
    expect(ctx).toEqual({
      note_id: "n1",
      note_title: "Acme kickoff",
      panel: "transcript",
      selected_text: null,
      category_id: "c1",
      search_query: null,
      is_recording: false,
    });
    expect(contextLabel(ctx, base.view)).toBe("Viewing: Acme kickoff · Transcript");
  });

  it("includes selected text only on the Notes panel", () => {
    const notes = buildViewContext({ ...base, panel: "notes", selectedText: "  ship the beta  " });
    expect(notes.selected_text).toBe("ship the beta");
    expect(contextLabel(notes, base.view)).toBe("Viewing: Acme kickoff · Notes · 3 words selected");
    expect(scopeLabel(notes, null)).toBe("Selection");
    expect(buildViewContext({ ...base, selectedText: "x" }).selected_text).toBeNull();
  });

  it("ignores a stale note and falls back to the view", () => {
    const ctx = buildViewContext({
      ...base,
      view: { name: "people" },
      searchQuery: "  ",
      recording: { phase: "recording" },
    });
    expect(ctx.note_id).toBeNull();
    expect(ctx.panel).toBeNull();
    expect(ctx.search_query).toBeNull();
    expect(ctx.is_recording).toBe(true);
    expect(contextLabel(ctx, { name: "people" })).toBe("Viewing: Saved people · Recording");
    expect(scopeLabel(ctx, null)).toBe("All notes");
  });

  it("names a search and untitled notes", () => {
    const search = buildViewContext({
      ...base,
      view: { name: "home" },
      note: null,
      searchQuery: "pricing",
    });
    expect(contextLabel(search, { name: "home" })).toBe("Viewing: Search for “pricing”");
    const untitled = buildViewContext({
      ...base,
      note: { id: "n1", title: " ", category_id: null },
    });
    expect(untitled.note_title).toBe("Untitled");
    expect(scopeLabel(untitled, null)).toBe("This note");
  });
});
