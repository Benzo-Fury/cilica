import { describe, expect, it } from "vitest";
import type { AiAction, AssistantEvent, Message, Proposal } from "@/ipc/types";
import {
  emptyThread,
  forgetThread,
  isToolWorking,
  rememberThread,
  resumeThread,
  streamingPart,
  type ThreadState,
  threadReducer,
} from "./thread";

const T = "thread_1";
const TURN = "turn_1";
const MSG = "msg_reply";

const user: Message = {
  id: "msg_user",
  thread_id: T,
  turn_id: TURN,
  role: "user",
  parts: [{ type: "text", text: "Tidy this note" }],
  created_at: "2026-11-11T10:00:00Z",
};

const action = (status: AiAction["status"], summary: string): AiAction => ({
  id: "act_1",
  thread_id: T,
  turn_id: TURN,
  tool: "search_notes",
  tier: "read",
  args_json: "{}",
  summary,
  status,
  error: null,
  can_undo: false,
  proposal_id: null,
  created_at: "2026-11-11T10:00:01Z",
});

const proposal: Proposal = {
  id: "prop_1",
  source: "chat",
  thread_id: T,
  task_run_id: null,
  tier: "edit",
  kind: { type: "rename_note", note: { id: "n", title: "Old" }, new_title: "New" },
  reason: "Clearer",
  status: "pending",
  created_at: "2026-11-11T10:00:02Z",
  resolved_at: null,
};

const ev = (event: AssistantEvent) => ({ type: "event" as const, event });
const run = (state: ThreadState, ...actions: Parameters<typeof threadReducer>[1][]) =>
  actions.reduce(threadReducer, state);

describe("threadReducer", () => {
  it("builds a turn from the event stream in display order", () => {
    let s = run(emptyThread, { type: "load", threadId: T, messages: [] });
    s = run(s, { type: "send", threadId: T, text: "Tidy this note", now: user.created_at });
    expect(s.messages).toHaveLength(1);
    expect(s.sending).toBe(true);
    s = run(
      s,
      ev({ type: "turn_started", thread_id: T, turn_id: TURN, user_message: user }),
      ev({ type: "text_delta", thread_id: T, turn_id: TURN, message_id: MSG, delta: "Let me " }),
      ev({ type: "text_delta", thread_id: T, turn_id: TURN, message_id: MSG, delta: "look. " }),
      ev({
        type: "tool",
        thread_id: T,
        turn_id: TURN,
        message_id: MSG,
        action: action("running", "Searching"),
      }),
    );
    // The optimistic bubble was replaced by the real message.
    expect(s.messages.map((m) => m.id)).toEqual(["msg_user", MSG]);
    expect(s.activeTurn).toBe(TURN);
    const tool = s.messages[1]?.parts[1];
    expect(tool?.type === "tool" && isToolWorking(s, tool.action)).toBe(true);

    s = run(
      s,
      ev({
        type: "tool",
        thread_id: T,
        turn_id: TURN,
        message_id: MSG,
        action: action("applied", "Searched 3 notes"),
      }),
      ev({
        type: "text_delta",
        thread_id: T,
        turn_id: TURN,
        message_id: MSG,
        delta: "Here is a rename.",
      }),
      ev({ type: "proposal", thread_id: T, turn_id: TURN, message_id: MSG, proposal }),
      ev({ type: "proposal", thread_id: T, turn_id: TURN, message_id: MSG, proposal }),
    );
    const parts = s.messages[1]?.parts ?? [];
    expect(parts.map((p) => p.type)).toEqual(["text", "tool", "text", "proposal"]);
    expect(parts[0]).toEqual({ type: "text", text: "Let me look. " });
    expect(parts[1]).toMatchObject({ action: { summary: "Searched 3 notes", status: "applied" } });
    expect(s.proposals.prop_1?.kind.type).toBe("rename_note");
    expect(streamingPart(s)).toBeNull(); // the last part is a card, not text

    s = run(
      s,
      ev({ type: "turn_finished", thread_id: T, turn_id: TURN, end: { type: "completed" } }),
    );
    expect(s.activeTurn).toBeNull();
    expect(s.ends[TURN]).toEqual({ type: "completed" });
  });

  it("shows an error under the reply and ignores other threads", () => {
    let s = run(emptyThread, { type: "load", threadId: T, messages: [user] });
    s = run(
      s,
      ev({ type: "text_delta", thread_id: "other", turn_id: "x", message_id: "y", delta: "nope" }),
      ev({
        type: "turn_finished",
        thread_id: T,
        turn_id: TURN,
        end: { type: "error", message: "The key was rejected." },
      }),
      ev({
        type: "turn_finished",
        thread_id: T,
        turn_id: TURN,
        end: { type: "error", message: "The key was rejected." },
      }),
    );
    expect(s.messages).toHaveLength(2);
    expect(s.messages[1]?.parts).toEqual([{ type: "error", message: "The key was rejected." }]);
  });

  it("marks the streaming text part and stops running rows when cancelled", () => {
    let s = run(emptyThread, { type: "load", threadId: T, messages: [] });
    s = run(
      s,
      ev({ type: "turn_started", thread_id: T, turn_id: TURN, user_message: user }),
      ev({
        type: "tool",
        thread_id: T,
        turn_id: TURN,
        message_id: MSG,
        action: action("running", "Searching"),
      }),
      ev({ type: "text_delta", thread_id: T, turn_id: TURN, message_id: MSG, delta: "Wor" }),
    );
    expect(streamingPart(s)).toEqual({ messageId: MSG, index: 1 });
    s = run(
      s,
      ev({ type: "turn_finished", thread_id: T, turn_id: TURN, end: { type: "cancelled" } }),
    );
    const tool = s.messages[1]?.parts[0];
    expect(tool?.type === "tool" && isToolWorking(s, tool.action)).toBe(false);
    expect(s.ends[TURN]?.type).toBe("cancelled");
  });

  it("keeps a streaming turn when the same thread's history loads", () => {
    let s = run(emptyThread, { type: "load", threadId: T, messages: [] });
    s = run(s, { type: "send", threadId: T, text: "Hi", now: user.created_at });
    s = run(s, { type: "load", threadId: T, messages: [] });
    expect(s.messages).toHaveLength(1);
    s = run(s, { type: "load", threadId: "other", messages: [] });
    expect(s.messages).toHaveLength(0);
  });

  it("turns a failed send into an error line", () => {
    let s = run(emptyThread, { type: "load", threadId: T, messages: [] });
    s = run(
      s,
      { type: "send", threadId: T, text: "Hi", now: user.created_at },
      { type: "send_failed", message: "Add a key." },
    );
    expect(s.sending).toBe(false);
    expect(s.messages.map((m) => m.role)).toEqual(["assistant"]);
  });
});

describe("handing a thread between views", () => {
  it("resumes the other view's live state, mid-reply included", () => {
    const live = run(
      emptyThread,
      { type: "load", threadId: T, messages: [] },
      ev({ type: "turn_started", thread_id: T, turn_id: TURN, user_message: user }),
      ev({ type: "text_delta", thread_id: T, turn_id: TURN, message_id: MSG, delta: "Half a" }),
    );
    rememberThread(live);
    const resumed = resumeThread(T);
    expect(resumed.activeTurn).toBe(TURN);
    expect(resumed.messages).toHaveLength(2);

    // Stale history (cached before the turn) must not wipe the reply that is still streaming.
    const merged = threadReducer(resumed, { type: "load", threadId: T, messages: [] });
    expect(streamingPart(merged)).toEqual({ messageId: MSG, index: 0 });
  });

  it("starts empty for a thread it has not seen or has forgotten", () => {
    expect(resumeThread(null)).toBe(emptyThread);
    expect(resumeThread("thread_unknown")).toBe(emptyThread);
    rememberThread({ ...emptyThread, threadId: "thread_gone" });
    forgetThread("thread_gone");
    expect(resumeThread("thread_gone")).toBe(emptyThread);
  });
});
