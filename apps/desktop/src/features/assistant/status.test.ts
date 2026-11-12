import { describe, expect, it } from "vitest";
import { isComposing, workingLine } from "./status";
import { emptyThread, type ThreadState } from "./thread";

const titles: Record<string, string> = { n1: "Q3 planning sync" };
const titleOf = (id: string) => titles[id];

describe("workingLine", () => {
  it("names what Margin is touching", () => {
    expect(workingLine({ tool: "get_note", args_json: '{"note_id":"n1"}' }, titleOf)).toBe(
      "Reading “Q3 planning sync”",
    );
    expect(workingLine({ tool: "get_transcript", args_json: '{"note_id":"n1"}' }, titleOf)).toBe(
      "Rereading the transcript of “Q3 planning sync”",
    );
    expect(workingLine({ tool: "search_notes", args_json: '{"query":" pricing "}' }, titleOf)).toBe(
      "Searching your notes for “pricing”",
    );
    expect(workingLine({ tool: "search_notes", args_json: "{}" }, titleOf)).toBe(
      "Looking through your recent notes",
    );
  });

  it("falls back when it knows nothing specific", () => {
    expect(workingLine({ tool: "get_note", args_json: '{"note_id":"gone"}' }, titleOf)).toBeNull();
    expect(workingLine({ tool: "get_note", args_json: "not json" }, titleOf)).toBeNull();
    expect(workingLine({ tool: "move_note", args_json: "{}" }, titleOf)).toBeNull();
  });

  it("shortens long titles", () => {
    titles.n2 = "A very long meeting title that keeps going well past what fits on one line";
    const line = workingLine({ tool: "get_note", args_json: '{"note_id":"n2"}' }, titleOf) ?? "";
    expect(line.endsWith("…”")).toBe(true);
    expect(line.length).toBeLessThan(60);
  });
});

describe("isComposing", () => {
  const tool = (status: "running" | "applied") =>
    ({ type: "tool", action: { id: "a", turn_id: "t", status } }) as never;
  const thread = (parts: unknown[]): ThreadState => ({
    ...emptyThread,
    threadId: "th",
    activeTurn: "t",
    messages: [
      {
        id: "m",
        thread_id: "th",
        turn_id: "t",
        role: "assistant",
        parts: parts as never,
        created_at: "",
      },
    ],
  });

  it("is true between the last finished step and the first words", () => {
    expect(isComposing(thread([tool("applied")]))).toBe(true);
    expect(isComposing(thread([tool("running")]))).toBe(false);
    expect(isComposing(thread([tool("applied"), { type: "text", text: "Two" }]))).toBe(false);
    expect(isComposing({ ...thread([tool("applied")]), activeTurn: null })).toBe(false);
  });
});
