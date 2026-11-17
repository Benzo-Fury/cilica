import { Mail, MessageSquare, Search } from "lucide-react";
import { describe, expect, it } from "vitest";
import type { Thread, ViewContext } from "@/ipc/types";
import { restingConversations, seeLine, suggestionIcon } from "./home";

const thread = (id: string): Thread => ({
  id,
  title: id,
  created_at: "2026-11-17T10:00:00Z",
  updated_at: "2026-11-17T10:00:00Z",
});

const view = (patch: Partial<ViewContext>): ViewContext => ({
  note_id: null,
  note_title: null,
  panel: null,
  selected_text: null,
  category_id: null,
  search_query: null,
  is_recording: false,
  ...patch,
});

describe("restingConversations", () => {
  it("lists running conversations first and keeps to the limit", () => {
    const threads = ["a", "b", "c", "d", "e", "f"].map(thread);
    const { shown, more } = restingConversations(threads, new Set(["e"]), 4);
    expect(shown.map((t) => t.id)).toEqual(["e", "a", "b", "c"]);
    expect(more).toBe(true);
  });

  it("says there is no more when everything fits", () => {
    expect(restingConversations([thread("a")], new Set()).more).toBe(false);
  });
});

describe("suggestionIcon", () => {
  it("picks an icon from what the suggestion does", () => {
    expect(suggestionIcon({ label: "Draft a follow-up email", prompt: "" })).toBe(Mail);
    expect(suggestionIcon({ label: "Find related notes", prompt: "" })).toBe(Search);
    expect(suggestionIcon({ label: "Something else", prompt: "" })).toBe(MessageSquare);
  });
});

describe("seeLine", () => {
  it("says what Cilica can see", () => {
    expect(seeLine(view({ note_id: "n", panel: "transcript" }))).toBe(
      "Cilica can see this note and its transcript.",
    );
    expect(seeLine(view({ note_id: "n", selected_text: "hi" }))).toMatch(/what you selected/);
    expect(seeLine(view({}))).toBe("Cilica can read every note and transcript you have.");
  });
});
