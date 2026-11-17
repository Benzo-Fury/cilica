import { describe, expect, it } from "vitest";
import type { AiAction, AssistantEvent, Proposal } from "@/ipc/types";
import { addTouch, applyActivity, changedLines, focusOfTool, touchOf } from "./activity";

const T = "thread_1";
const TURN = "turn_1";

const tool = (tool: string, args: object, status: AiAction["status"] = "running"): AiAction => ({
  id: `act_${tool}`,
  thread_id: T,
  turn_id: TURN,
  tool,
  tier: "read",
  args_json: JSON.stringify(args),
  summary: "",
  status,
  error: null,
  can_undo: false,
  proposal_id: null,
  created_at: "2026-11-17T10:00:00Z",
});

const empty = { runs: {}, turnThreads: {}, focus: null };
const started: AssistantEvent = {
  type: "turn_started",
  thread_id: T,
  turn_id: TURN,
  user_message: {
    id: "m",
    thread_id: T,
    turn_id: TURN,
    role: "user",
    parts: [],
    created_at: "2026-11-17T10:00:00Z",
  },
};

describe("touchOf", () => {
  it("names the note a step works on, in the tense of its status", () => {
    expect(touchOf(tool("get_note", { note_id: "n1" }))).toMatchObject({
      noteId: "n1",
      verb: "reading",
      live: true,
    });
    expect(touchOf(tool("get_note", { note_id: "n1" }, "applied"))?.verb).toBe("read");
    expect(touchOf(tool("edit_note", { note_id: "n1" }, "proposed"))?.verb).toBe("waiting on you");
    expect(touchOf(tool("create_task", {}))).toMatchObject({ label: "Scheduled tasks" });
    expect(touchOf(tool("search_notes", { query: "x" }))).toBeNull();
  });

  it("keeps one slot per place, with its newest verb", () => {
    const a = addTouch([], touchOf(tool("get_note", { note_id: "n1" })));
    const b = addTouch(a, touchOf(tool("edit_note", { note_id: "n1" })));
    expect(b).toHaveLength(1);
    expect(b[0]?.verb).toBe("editing");
  });
});

describe("focus", () => {
  it("sweeps a note being read and jumps to the text an edit finds", () => {
    expect(focusOfTool(tool("get_note", { note_id: "n1" }))).toMatchObject({
      mode: "reading",
      phrases: [],
    });
    const edit = focusOfTool(
      tool("edit_note", { note_id: "n1", edits: [{ find: "- Follow up on SSO", replace: "x" }] }),
    );
    expect(edit).toMatchObject({ mode: "editing", phrases: ["Follow up on SSO"] });
    expect(focusOfTool(tool("search_notes", { query: "x" }))).toBeUndefined();
  });

  it("finds where a proposed edit lands in the note as it is now", () => {
    expect(changedLines("# A\n- one\n- **two**", "# A\n- one\n- two by Friday")).toEqual(["two"]);
    // A pure addition marks the line it follows.
    expect(changedLines("Hello", "Hello\nSam owns SSO")).toEqual(["Hello"]);
  });
});

describe("applyActivity", () => {
  it("tracks a run from start to finish and clears its marker", () => {
    let s = applyActivity(empty, started, 1000);
    expect(s.runs[T]?.startedAt).toBe(1000);
    expect(s.turnThreads[TURN]).toBe(T);
    s = applyActivity(
      s,
      {
        type: "tool",
        thread_id: T,
        turn_id: TURN,
        message_id: "r",
        action: tool("get_note", { note_id: "n1" }),
      },
      1100,
    );
    expect(s.runs[T]?.touched).toHaveLength(1);
    expect(s.focus?.noteId).toBe("n1");
    // A search elsewhere leaves the marker where it was.
    s = applyActivity(
      s,
      {
        type: "tool",
        thread_id: T,
        turn_id: TURN,
        message_id: "r",
        action: tool("search_notes", {}),
      },
      1200,
    );
    expect(s.focus?.noteId).toBe("n1");
    s = applyActivity(
      s,
      { type: "turn_finished", thread_id: T, turn_id: TURN, end: { type: "completed" } },
      1300,
    );
    expect(s.runs[T]).toBeUndefined();
    expect(s.focus).toBeNull();
    expect(s.turnThreads[TURN]).toBe(T);
  });

  it("points the marker at a proposed edit", () => {
    const proposal: Proposal = {
      id: "p",
      source: "chat",
      thread_id: T,
      task_run_id: null,
      tier: "edit",
      kind: {
        type: "edit_note",
        note: { id: "n2", title: "Acme" },
        before_md: "Hello",
        after_md: "Hello\nSam owns SSO",
      },
      reason: "",
      status: "pending",
      created_at: "2026-11-17T10:00:00Z",
      resolved_at: null,
    };
    const s = applyActivity(
      empty,
      { type: "proposal", thread_id: T, turn_id: TURN, message_id: "r", proposal },
      0,
    );
    expect(s.focus).toMatchObject({ noteId: "n2", mode: "suggesting", phrases: ["Hello"] });
    // The suggestion stays marked after the turn ends, until it is answered.
    const after = applyActivity(
      s,
      { type: "turn_finished", thread_id: T, turn_id: TURN, end: { type: "completed" } },
      1,
    );
    expect(after.focus?.proposalId).toBe("p");
  });
});
