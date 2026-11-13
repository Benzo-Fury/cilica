/**
 * The assistant thread as a reducer: seeded from `getThreadMessages`, then advanced by the
 * `assistant` event stream exactly as `AssistantEvent` documents it:
 *
 * - `turn_started`: the user's message is in (replacing the optimistic bubble); the turn is active.
 * - `text_delta`: append to the assistant message `message_id`, creating it on first sight; a
 *   delta after a tool row starts a new text part, so parts stay in display order.
 * - `tool`: upsert the tool row by `action.id` (running → its past-tense summary).
 * - `proposal`: add a card for the proposal (kept by id for its current status).
 * - `turn_finished`: the turn ends `completed`, `cancelled` or with an `error`.
 */
import type {
  AiAction,
  AssistantEvent,
  Message,
  MessagePart,
  Proposal,
  TurnEnd,
} from "@/ipc/types";

export interface ThreadState {
  threadId: string | null;
  messages: Message[];
  /** Proposals seen in this thread's events, by id (cards look up their live status). */
  proposals: Record<string, Proposal>;
  /** The turn Cilica is working on, if any. */
  activeTurn: string | null;
  /** Set between Send and `turn_started`. */
  sending: boolean;
  /** How turns ended this session (cancelled and errors are shown under the reply). */
  ends: Record<string, TurnEnd>;
}

export type ThreadAction =
  | { type: "load"; threadId: string | null; messages: Message[] }
  | { type: "event"; event: AssistantEvent }
  /** Show the user's words at once while the turn starts. */
  | { type: "send"; threadId: string; text: string; now: string }
  | { type: "turn_id"; turnId: string }
  | { type: "send_failed"; message: string };

export const emptyThread: ThreadState = {
  threadId: null,
  messages: [],
  proposals: {},
  activeTurn: null,
  sending: false,
  ends: {},
};

/**
 * The last live state of each thread this session. The side panel and the floating bar are two
 * views of one conversation, and only one is mounted at a time: when the placement changes, the
 * view that mounts resumes from here, so the conversation (and a reply still streaming) carries on.
 */
const liveThreads = new Map<string, ThreadState>();

/** Where a view of `threadId` starts: its last live state, or empty until history loads. */
export function resumeThread(threadId: string | null): ThreadState {
  return (threadId ? liveThreads.get(threadId) : undefined) ?? emptyThread;
}

export function rememberThread(state: ThreadState): void {
  if (state.threadId) liveThreads.set(state.threadId, state);
}

export function forgetThread(threadId: string): void {
  liveThreads.delete(threadId);
}

const PENDING = "pending:";

export function isPending(message: Message): boolean {
  return message.id.startsWith(PENDING);
}

function withMessage(
  messages: Message[],
  id: string,
  create: () => Message,
  update: (m: Message) => Message,
): Message[] {
  const index = messages.findIndex((m) => m.id === id);
  if (index < 0) return [...messages, update(create())];
  const next = messages.slice();
  next[index] = update(messages[index] as Message);
  return next;
}

function appendText(parts: MessagePart[], delta: string): MessagePart[] {
  const last = parts[parts.length - 1];
  if (last?.type === "text")
    return [...parts.slice(0, -1), { type: "text", text: last.text + delta }];
  return [...parts, { type: "text", text: delta }];
}

function upsertTool(parts: MessagePart[], action: AiAction): MessagePart[] {
  const index = parts.findIndex((p) => p.type === "tool" && p.action.id === action.id);
  if (index < 0) return [...parts, { type: "tool", action }];
  const next = parts.slice();
  next[index] = { type: "tool", action };
  return next;
}

export function threadReducer(state: ThreadState, action: ThreadAction): ThreadState {
  switch (action.type) {
    case "load": {
      if (state.threadId !== action.threadId) {
        return { ...emptyThread, threadId: action.threadId, messages: action.messages };
      }
      // The same thread again (it was just created, or refetched): keep what is still streaming.
      const known = new Set(action.messages.map((m) => m.id));
      const live = state.messages.filter(
        (m) =>
          !known.has(m.id) &&
          (isPending(m) || (state.activeTurn !== null && m.turn_id === state.activeTurn)),
      );
      return { ...state, messages: [...action.messages, ...live] };
    }
    case "send":
      return {
        ...state,
        threadId: action.threadId,
        sending: true,
        messages: [
          ...state.messages,
          {
            id: `${PENDING}${state.messages.length}`,
            thread_id: action.threadId,
            turn_id: "",
            role: "user",
            parts: [{ type: "text", text: action.text }],
            created_at: action.now,
          },
        ],
      };
    case "turn_id":
      return state.activeTurn ? state : { ...state, activeTurn: action.turnId };
    case "send_failed":
      return {
        ...state,
        sending: false,
        activeTurn: null,
        messages: [
          ...state.messages.filter((m) => !isPending(m)),
          {
            id: `error:${state.messages.length}`,
            thread_id: state.threadId ?? "",
            turn_id: "",
            role: "assistant",
            parts: [{ type: "error", message: action.message }],
            created_at: new Date(0).toISOString(),
          },
        ],
      };
    case "event":
      return applyEvent(state, action.event);
  }
}

function applyEvent(state: ThreadState, event: AssistantEvent): ThreadState {
  if (event.thread_id !== state.threadId) return state;
  const now = new Date().toISOString();
  const assistantMessage = (id: string) => () =>
    ({
      id,
      thread_id: event.thread_id,
      turn_id: event.turn_id,
      role: "assistant",
      parts: [],
      created_at: now,
    }) satisfies Message;

  switch (event.type) {
    case "turn_started": {
      const rest = state.messages.filter((m) => !isPending(m) && m.id !== event.user_message.id);
      return {
        ...state,
        sending: false,
        activeTurn: event.turn_id,
        messages: [...rest, event.user_message],
      };
    }
    case "text_delta":
      return {
        ...state,
        messages: withMessage(
          state.messages,
          event.message_id,
          assistantMessage(event.message_id),
          (m) => ({
            ...m,
            parts: appendText(m.parts, event.delta),
          }),
        ),
      };
    case "tool":
      return {
        ...state,
        messages: withMessage(
          state.messages,
          event.message_id,
          assistantMessage(event.message_id),
          (m) => ({
            ...m,
            parts: upsertTool(m.parts, event.action),
          }),
        ),
      };
    case "proposal":
      return {
        ...state,
        proposals: { ...state.proposals, [event.proposal.id]: event.proposal },
        messages: withMessage(
          state.messages,
          event.message_id,
          assistantMessage(event.message_id),
          (m) =>
            m.parts.some((p) => p.type === "proposal" && p.proposal_id === event.proposal.id)
              ? m
              : { ...m, parts: [...m.parts, { type: "proposal", proposal_id: event.proposal.id }] },
        ),
      };
    case "turn_finished": {
      let messages = state.messages;
      if (event.end.type === "error") {
        const error = event.end.message;
        const last = [...messages]
          .reverse()
          .find((m) => m.turn_id === event.turn_id && m.role === "assistant");
        const id = last?.id ?? `error:${event.turn_id}`;
        messages = withMessage(messages, id, assistantMessage(id), (m) =>
          m.parts.some((p) => p.type === "error" && p.message === error)
            ? m
            : { ...m, parts: [...m.parts, { type: "error", message: error }] },
        );
      }
      return {
        ...state,
        messages,
        sending: false,
        activeTurn: state.activeTurn === event.turn_id ? null : state.activeTurn,
        ends: { ...state.ends, [event.turn_id]: event.end },
      };
    }
  }
}

/** Is a tool row still working? Only while its turn is active. */
export function isToolWorking(state: ThreadState, action: AiAction): boolean {
  return action.status === "running" && state.activeTurn === action.turn_id;
}

/** The text part that is streaming right now (its words fade in). */
export function streamingPart(state: ThreadState): { messageId: string; index: number } | null {
  if (!state.activeTurn) return null;
  const message = [...state.messages]
    .reverse()
    .find((m) => m.turn_id === state.activeTurn && m.role === "assistant");
  if (!message) return null;
  const index = message.parts.length - 1;
  return message.parts[index]?.type === "text" ? { messageId: message.id, index } : null;
}
