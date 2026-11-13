/**
 * The assistant: threads, a fake streamed turn, prompt chips, proposals, the audit log and providers.
 *
 * A turn plays out over about two seconds: turn_started, a read tool row (running → applied),
 * text deltas word by word, then what the words asked for, then turn_finished:
 *
 * - "edit", "change", "rewrite", "make" with a note open → an edit_note proposal (several hunks)
 * - "rename" → rename_note; "merge" → merge_notes; "trash", "delete" or "clean up" → trash_notes
 * - "move" or "file" → an additive move applied at once, then one Undo toast for the turn
 * - "new note" or "create" → an additive create_note applied at once, with the Undo toast
 * - "fail" or "error" → the turn ends with an error
 *
 * Applying an edit whose note changed since it was proposed rejects with `conflict`.
 */
import type {
  AiAction,
  ClaudeCodeStatus,
  DataScope,
  LocalLlmModel,
  Message,
  MessagePart,
  OllamaStatus,
  PromptChip,
  Proposal,
  ProposalStatus,
  ProviderKind,
  ProviderStatus,
  Thread,
  ViewContext,
} from "@/ipc/types";
import { requireLiveNote, snapshot, writeBody } from "./notes";
import type { MockContext } from "./state";
import { Timers } from "./timers";
import { appError, iso, newId, notFound, wordChunks } from "./util";

const EDIT_WORDS = /\b(edit|change|rewrite|make|tighten|clearer)\b/i;
const RENAME_WORDS = /\brename\b/i;
const MERGE_WORDS = /\bmerge\b/i;
const TRASH_WORDS = /\b(trash|delete|clean up|remove)\b/i;
const MOVE_WORDS = /\b(move|file)\b/i;
const CREATE_WORDS = /\b(new note|create)\b/i;
const FAIL_WORDS = /\b(fail|error)\b/i;

type Intent = "edit" | "rename" | "merge" | "trash" | "move" | "create" | "fail" | "answer";

function intentOf(text: string, hasNote: boolean): Intent {
  if (FAIL_WORDS.test(text)) return "fail";
  if (MERGE_WORDS.test(text)) return "merge";
  if (RENAME_WORDS.test(text) && hasNote) return "rename";
  if (TRASH_WORDS.test(text)) return "trash";
  if (MOVE_WORDS.test(text)) return "move";
  if (CREATE_WORDS.test(text)) return "create";
  if (EDIT_WORDS.test(text) && hasNote) return "edit";
  return "answer";
}

/** Past tense, as the agent writes it once a proposal is applied. */
function appliedSummary(kind: Proposal["kind"]): string {
  switch (kind.type) {
    case "edit_note":
      return `Edited ${kind.note.title || "Untitled"}`;
    case "rename_note":
      return `Renamed ${kind.note.title || "Untitled"} to ${kind.new_title}`;
    case "trash_notes":
      return kind.notes.length === 1
        ? `Moved ${kind.notes[0]?.title} to Trash`
        : `Moved ${kind.notes.length} notes to Trash`;
    case "merge_notes":
      return `Merged ${kind.sources.length} notes into ${kind.merged_title}`;
    case "delete_category":
      return `Deleted ${kind.name}`;
    case "delete_person":
      return `Deleted ${kind.name}`;
    case "bulk_move":
      return `Moved ${kind.notes.length} notes to ${kind.category_name}`;
    case "move_groups":
      return `Moved ${kind.groups.reduce((n, g) => n + g.notes.length, 0)} notes`;
    case "create_note":
      return `Created ${kind.title}`;
  }
}

export function assistantCommands({ state, emit }: MockContext) {
  const ollama = {
    running: false,
    pulls: new Map<string, ReturnType<typeof setInterval>>(),
    models: [
      {
        id: "qwen3:4b",
        name: "Qwen3 4B",
        description: "Fast and light. Fine for titles and short summaries.",
        size_mb: 2600,
        min_ram_gb: 8,
        recommended: false,
        downloaded: false,
      },
      {
        id: "qwen3:8b",
        name: "Qwen3 8B",
        description: "The best balance for most Macs.",
        size_mb: 5200,
        min_ram_gb: 16,
        recommended: true,
        downloaded: false,
      },
      {
        id: "qwen3:14b",
        name: "Qwen3 14B",
        description: "Sharper summaries and edits. Slower.",
        size_mb: 9300,
        min_ram_gb: 24,
        recommended: false,
        downloaded: false,
      },
      {
        id: "gpt-oss:20b",
        name: "gpt-oss 20B",
        description: "Strongest reasoning here. Needs a lot of memory.",
        size_mb: 14000,
        min_ram_gb: 32,
        recommended: false,
        downloaded: false,
      },
    ] as LocalLlmModel[],
  };
  const turns = new Map<string, { threadId: string; timers: Timers }>();

  const requireThread = (threadId: string): Thread => {
    const thread = state.threads.find((t) => t.id === threadId);
    if (!thread) throw notFound("thread");
    return thread;
  };
  const requireProposal = (proposalId: string): Proposal => {
    const proposal = state.proposals.find((p) => p.id === proposalId);
    if (!proposal) throw notFound("proposal");
    return proposal;
  };
  const requireAction = (actionId: string): AiAction => {
    const action = state.aiActions.find((a) => a.id === actionId);
    if (!action) throw notFound("action");
    return action;
  };

  const providerStatus = (kind: ProviderKind): ProviderStatus => {
    const hasKey = state.providerKeys.includes(kind);
    switch (kind) {
      case "anthropic":
        return {
          kind,
          has_key: hasKey,
          ready: hasKey,
          problem: hasKey ? null : "Add an Anthropic API key in Settings › Cilica.",
        };
      case "openai_compat":
        return {
          kind,
          has_key: hasKey,
          ready: hasKey,
          problem: hasKey ? null : "Add a base URL and an API key in Settings › Cilica.",
        };
      case "ollama": {
        const hasModel = ollama.models.some(
          (m) => m.downloaded && m.id === state.settings.ai.model,
        );
        return {
          kind,
          has_key: false,
          ready: ollama.running && hasModel,
          problem: !ollama.running
            ? "Ollama is not running yet. Set it up below."
            : hasModel
              ? null
              : "Download a local model below, then choose it.",
        };
      }
      case "claude_code":
        return { kind, has_key: false, ready: true, problem: null };
    }
  };

  const replyFor = (text: string, view: ViewContext): string => {
    const note = view.note_id ? state.notes.get(view.note_id) : undefined;
    const live = [...state.notes.values()].filter((n) => !n.deleted_at);
    const title = note?.title || "this note";
    switch (intentOf(text, Boolean(note))) {
      case "edit":
        return `I drafted changes to **${title}**: clearer owners on the action items and one new next step. Review them below and keep the ones that read right.`;
      case "rename":
        return `A shorter title would make ${title} easier to find. Here is a suggestion.`;
      case "merge":
        return 'Two notes cover the onboarding rollout ("1:1 with Priya" and "Q3 planning sync"). Merging them keeps one source of truth.';
      case "trash":
        return '"Reading list" has not changed in weeks. I can move it to Trash; it stays there for 30 days.';
      case "move":
        return `I moved ${note ? title : "the pricing interview"} into Product, next to the other planning notes.`;
      case "create":
        return "I started a note for the launch checklist with the open items from this week.";
      case "fail":
        return "Let me look that up.";
      case "answer":
        break;
    }
    if (note) {
      const open = (note.body_md.match(/^\s*- \[ \]/gm) ?? []).length;
      const items = open === 1 ? "1 open action item" : `${open} open action items`;
      // Meetings come with a receipt: the note's title and the moment in its transcript.
      const receipt =
        note.kind === "meeting" && note.title
          ? ` The turning point: ("${note.title}", 14:22).`
          : "";
      return `${note.title || "This note"} has ${items}. The main decisions are under Decisions, and the open questions at the end still need an owner.${receipt} I can draft a follow-up or turn the open items into a checklist.`;
    }
    const recent = live
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at))
      .slice(0, 2)
      .map((n) => n.title || "Untitled");
    return `I looked through your ${live.length} notes. The most active ones this week are ${recent.join(" and ") || "none yet"}. Onboarding comes up most often, so that is a good place to start.`;
  };

  /** An edit with several hunks: a sharper first action item, a sentence tightened, a new section. */
  const proposedBody = (body: string): string => {
    const line = "- [ ] Confirm owners and due dates for every open item (You, Friday)";
    if (!body.trim()) return `## Next steps\n\n${line}\n`;
    let next = body.replace(/^(\s*- \[ \] )(.+)$/m, (_m, marker: string, item: string) =>
      /\(.+\)\s*$/.test(item)
        ? `${marker}${item.replace(/\s*\(.+\)\s*$/, "")} (owner confirmed, due Thursday)`
        : `${marker}${item} (You, Thursday)`,
    );
    next = next.replace(
      /^([A-Z][^\n#*-][^\n]{30,}?)\. ([^\n]+)$/m,
      (_m, first: string) => `${first}.`,
    );
    return `${next.trimEnd()}\n\n## Next steps\n\n${line}\n`;
  };

  const ref = (id: string) => {
    const n = state.notes.get(id);
    return n && !n.deleted_at ? { id: n.id, title: n.title } : null;
  };

  /** A proposal and its audit row, as the agent records them. */
  const propose = (
    threadId: string,
    turnId: string,
    tier: Proposal["tier"],
    kind: Proposal["kind"],
    reason: string,
    summary: string,
  ): Proposal => {
    const proposal: Proposal = {
      id: newId("prop"),
      source: "chat",
      thread_id: threadId,
      task_run_id: null,
      tier,
      kind,
      reason,
      status: "pending",
      created_at: iso(),
      resolved_at: null,
    };
    state.proposals.unshift(proposal);
    state.aiActions.unshift({
      id: newId("act"),
      thread_id: threadId,
      turn_id: turnId,
      tool: kind.type === "trash_notes" ? "trash_note" : kind.type,
      tier,
      args_json: JSON.stringify(
        kind.type === "edit_note" ? { note_id: kind.note.id } : { proposal: kind.type },
      ),
      summary,
      status: "proposed",
      error: null,
      can_undo: false,
      proposal_id: proposal.id,
      created_at: iso(),
    });
    return proposal;
  };

  const finishTurn = (
    turnId: string,
    threadId: string,
    end: { type: "completed" } | { type: "cancelled" },
  ) => {
    turns.get(turnId)?.timers.clear();
    turns.delete(turnId);
    emit({
      type: "assistant",
      event: { type: "turn_finished", thread_id: threadId, turn_id: turnId, end },
    });
  };

  return {
    listThreads(): Thread[] {
      return [...state.threads].sort((a, b) => b.updated_at.localeCompare(a.updated_at));
    },

    createThread(): Thread {
      const now = iso();
      const thread: Thread = {
        id: newId("thread"),
        title: "New chat",
        created_at: now,
        updated_at: now,
      };
      state.threads.unshift(thread);
      return thread;
    },

    deleteThread(threadId: string): null {
      requireThread(threadId);
      state.threads = state.threads.filter((t) => t.id !== threadId);
      state.messages = state.messages.filter((m) => m.thread_id !== threadId);
      return null;
    },

    getThreadMessages(threadId: string): Message[] {
      requireThread(threadId);
      return state.messages
        .filter((m) => m.thread_id === threadId)
        .sort((a, b) => a.created_at.localeCompare(b.created_at));
    },

    sendMessage(threadId: string, text: string, view: ViewContext): string {
      const thread = requireThread(threadId);
      const trimmed = text.trim();
      if (!trimmed) throw appError("invalid", "Type a question or an instruction first.");
      const turnId = newId("turn");
      const timers = new Timers();
      turns.set(turnId, { threadId, timers });
      const now = iso();
      const userMessage: Message = {
        id: newId("msg"),
        thread_id: threadId,
        turn_id: turnId,
        role: "user",
        parts: [{ type: "text", text: trimmed }],
        created_at: now,
      };
      state.messages.push(userMessage);
      if (thread.title === "New chat")
        thread.title = trimmed.length > 48 ? `${trimmed.slice(0, 47)}…` : trimmed;
      thread.updated_at = now;

      const reply: Message = {
        id: newId("msg"),
        thread_id: threadId,
        turn_id: turnId,
        role: "assistant",
        parts: [],
        created_at: now,
      };
      const ids = { thread_id: threadId, turn_id: turnId, message_id: reply.id };
      const action: AiAction = {
        id: newId("act"),
        thread_id: threadId,
        turn_id: turnId,
        tool: "search_notes",
        tier: "read",
        args_json: JSON.stringify({ query: trimmed.slice(0, 60) }),
        summary: "Searching your notes",
        status: "running",
        error: null,
        can_undo: false,
        proposal_id: null,
        created_at: now,
      };
      const toolPart: MessagePart = { type: "tool", action };
      const textPart = { type: "text" as const, text: "" };
      const chunks = wordChunks(replyFor(trimmed, view), 1);
      const intent = intentOf(trimmed, Boolean(view.note_id));

      timers.later(50, () => {
        emit({
          type: "assistant",
          event: {
            type: "turn_started",
            thread_id: threadId,
            turn_id: turnId,
            user_message: { ...userMessage },
          },
        });
      });
      timers.later(250, () => {
        state.messages.push(reply);
        reply.parts.push(toolPart);
        state.aiActions.unshift(action);
        emit({ type: "assistant", event: { type: "tool", ...ids, action: { ...action } } });
      });
      timers.later(750, () => {
        action.status = "applied";
        action.summary = `Searched ${Math.min(3, state.notes.size)} notes`;
        emit({ type: "assistant", event: { type: "tool", ...ids, action: { ...action } } });
        reply.parts.push(textPart);
      });
      chunks.forEach((chunk, i) => {
        timers.later(850 + i * 50, () => {
          textPart.text += chunk;
          emit({ type: "assistant", event: { type: "text_delta", ...ids, delta: chunk } });
        });
      });
      const afterText = 850 + chunks.length * 50;
      timers.later(afterText + 100, () => {
        const note = view.note_id ? state.notes.get(view.note_id) : undefined;
        const live = note && !note.deleted_at ? note : undefined;
        const emitProposal = (proposal: Proposal) => {
          reply.parts.push({ type: "proposal", proposal_id: proposal.id });
          emit({
            type: "assistant",
            event: { type: "proposal", ...ids, proposal: { ...proposal } },
          });
        };
        /** An additive write: applied at once, shown as a tool row, undone with the turn. */
        const applied = (tool: string, summary: string, args: object, apply: () => void) => {
          apply();
          const row: AiAction = {
            id: newId("act"),
            thread_id: threadId,
            turn_id: turnId,
            tool,
            tier: "additive",
            args_json: JSON.stringify(args),
            summary,
            status: "applied",
            error: null,
            can_undo: true,
            proposal_id: null,
            created_at: iso(),
          };
          state.aiActions.unshift(row);
          reply.parts.push({ type: "tool", action: row });
          emit({ type: "assistant", event: { type: "tool", ...ids, action: { ...row } } });
          emit({ type: "data_changed", scopes: ["notes", "ai_actions"], note_id: null });
        };
        switch (intent) {
          case "edit":
            if (live) {
              emitProposal(
                propose(
                  threadId,
                  turnId,
                  "edit",
                  {
                    type: "edit_note",
                    note: { id: live.id, title: live.title },
                    before_md: live.body_md,
                    after_md: proposedBody(live.body_md),
                  },
                  "You asked for clearer action items; this names owners and dates and adds one next step.",
                  `Proposed an edit to ${live.title || "Untitled"}`,
                ),
              );
            }
            break;
          case "rename":
            if (live) {
              const short = (live.title || "Untitled").replace(/\s*[·,(].*$/, "").trim();
              emitProposal(
                propose(
                  threadId,
                  turnId,
                  "edit",
                  {
                    type: "rename_note",
                    note: { id: live.id, title: live.title },
                    new_title: `${short} (notes)`,
                  },
                  "A shorter title that still says what it is.",
                  `Proposed renaming ${live.title}`,
                ),
              );
            }
            break;
          case "merge": {
            const sources = [ref("note_priya"), ref("note_q3")].filter(
              (r): r is { id: string; title: string } => r !== null,
            );
            if (sources.length === 2) {
              emitProposal(
                propose(
                  threadId,
                  turnId,
                  "destructive",
                  {
                    type: "merge_notes",
                    sources,
                    merged_title: "Onboarding rollout",
                    merged_md:
                      "# Onboarding rollout\n\n- Ship the three-step onboarding before the pricing change.\n- Priya owns the rollout plan, due Thursday.\n- Marco books five usability sessions for step 3.\n",
                  },
                  "Both notes track the onboarding rollout and repeat the same owners and dates.",
                  "Proposed merging 2 notes",
                ),
              );
            }
            break;
          }
          case "trash": {
            const target =
              ref("note_reading") ?? (live ? { id: live.id, title: live.title } : null);
            if (target) {
              emitProposal(
                propose(
                  threadId,
                  turnId,
                  "destructive",
                  { type: "trash_notes", notes: [target] },
                  "It has not changed in weeks and nothing links to it.",
                  `Proposed moving ${target.title} to Trash`,
                ),
              );
            }
            break;
          }
          case "move": {
            const target = live ?? state.notes.get("note_pricing");
            const category =
              state.categories.get("cat_product") ?? [...state.categories.values()][0];
            if (target && category) {
              const before = target.category_id;
              applied(
                "move_note",
                `Moved ${target.title || "Untitled"} to ${category.name}`,
                { note_id: target.id, category_id: category.id, from: before },
                () => {
                  target.category_id = category.id;
                  target.updated_at = iso();
                },
              );
            }
            break;
          }
          case "create": {
            const id = newId("note");
            applied("create_note", "Created Launch checklist", { note_id: id }, () => {
              state.notes.set(id, {
                id,
                title: "Launch checklist",
                body_md:
                  "- [ ] Freeze features on Friday\n- [ ] Send the launch email\n- [ ] Invite the waitlist in batches\n",
                snippet: "Freeze features on Friday",
                category_id: null,
                kind: "manual",
                meeting_app: null,
                started_at: null,
                ended_at: null,
                position: -100,
                created_at: iso(),
                updated_at: iso(),
                deleted_at: null,
              });
            });
            break;
          }
          case "fail":
            timers.later(100, () => {
              turns.get(turnId)?.timers.clear();
              turns.delete(turnId);
              emit({
                type: "assistant",
                event: {
                  type: "turn_finished",
                  thread_id: threadId,
                  turn_id: turnId,
                  end: {
                    type: "error",
                    message:
                      "Anthropic did not answer in time. Check your connection, then ask again.",
                  },
                },
              });
            });
            return;
          case "answer":
            break;
        }
        // One Undo for everything the turn applied (as the agent does after a turn).
        const undoable = reply.parts.filter(
          (p) => p.type === "tool" && p.action.can_undo && p.action.status === "applied",
        );
        if (undoable.length) {
          const first = undoable[0];
          emit({
            type: "toast",
            toast: {
              kind: "undo",
              title:
                undoable.length === 1 && first?.type === "tool"
                  ? first.action.summary
                  : `Cilica made ${undoable.length} changes`,
              description: null,
              action_label: "Undo",
              action: { type: "undo_turn", turn_id: turnId },
            },
          });
        }
        timers.later(100, () => finishTurn(turnId, threadId, { type: "completed" }));
      });
      return turnId;
    },

    cancelTurn(turnId: string): null {
      const turn = turns.get(turnId);
      if (!turn) throw notFound("turn");
      finishTurn(turnId, turn.threadId, { type: "cancelled" });
      return null;
    },

    getWeekLine(): string | null {
      const week = Date.now() - 7 * 24 * 3600_000;
      const recent = [...state.notes.values()].filter(
        (n) => !n.deleted_at && new Date(n.updated_at).getTime() >= week,
      );
      return recent.length >= 3 ? "Seven meetings, one decision, and a very patient Marco." : null;
    },

    getPromptChips(view: ViewContext): PromptChip[] {
      const note = view.note_id ? state.notes.get(view.note_id) : undefined;
      if (note?.kind === "meeting") {
        return [
          {
            label: "List action items",
            prompt: "List every action item from this meeting with its owner and due date.",
          },
          {
            label: "Draft a follow-up email",
            prompt: "Draft a short follow-up email to the people in this meeting.",
          },
          {
            label: "Who committed to what",
            prompt: "For each person in this meeting, list what they committed to.",
          },
        ];
      }
      if (note) {
        return [
          { label: "Summarise this note", prompt: "Summarise this note in three bullet points." },
          {
            label: "Tighten the wording",
            prompt: "Make this note shorter and clearer without losing anything.",
          },
          {
            label: "Find related notes",
            prompt: "Which of my other notes cover the same topic as this one?",
          },
        ];
      }
      return [
        { label: "Weekly digest", prompt: "Summarise what changed in my notes this week." },
        {
          label: "Find duplicates",
          prompt: "Find notes that cover the same topic and could be merged.",
        },
        { label: "What is still open", prompt: "List every open action item across my notes." },
        {
          label: "Prepare for tomorrow",
          prompt: "What do I need to prepare for my meetings tomorrow?",
        },
      ];
    },

    // ── Proposals ──

    listProposals(status: ProposalStatus | null): Proposal[] {
      return state.proposals
        .filter((p) => status === null || p.status === status)
        .sort((a, b) => b.created_at.localeCompare(a.created_at));
    },

    getProposal(proposalId: string): Proposal {
      return requireProposal(proposalId);
    },

    applyProposal(proposalId: string, finalMd: string | null): Proposal {
      const proposal = requireProposal(proposalId);
      if (proposal.status !== "pending") {
        throw appError("conflict", "That suggestion was already applied or dismissed.");
      }
      const linked = state.aiActions.find((a) => a.proposal_id === proposalId);
      const actionId = linked?.id ?? null;
      const scopes = new Set<DataScope>(["proposals", "ai_actions", "tasks"]);
      let noteId: string | null = null;
      const kind = proposal.kind;
      const now = iso();
      switch (kind.type) {
        case "edit_note": {
          const note = requireLiveNote(state, kind.note.id);
          if (note.body_md.trim() !== kind.before_md.trim()) {
            throw appError(
              "conflict",
              "This note changed after Cilica suggested the edit. Ask Cilica again for a fresh suggestion.",
            );
          }
          writeBody(state, note, finalMd ?? kind.after_md, actionId);
          noteId = note.id;
          scopes.add("notes").add("versions");
          break;
        }
        case "rename_note": {
          const note = requireLiveNote(state, kind.note.id);
          snapshot(state, note, "user");
          note.title = kind.new_title;
          note.updated_at = now;
          noteId = note.id;
          scopes.add("notes");
          break;
        }
        case "trash_notes": {
          for (const ref of kind.notes) {
            const note = state.notes.get(ref.id);
            if (note) note.deleted_at = now;
          }
          scopes.add("notes").add("trash");
          break;
        }
        case "merge_notes": {
          const first = state.notes.get(kind.sources[0]?.id ?? "");
          const categoryId = first?.category_id ?? null;
          const positions = [...state.notes.values()]
            .filter((n) => !n.deleted_at && n.category_id === categoryId)
            .map((n) => n.position ?? 0);
          const merged = {
            id: newId("note"),
            title: kind.merged_title,
            body_md: kind.merged_md,
            snippet:
              kind.merged_md
                .split("\n")
                .find((l) => l.trim() && !l.startsWith("#"))
                ?.replace(/^[-*]\s+/, "") ?? "",
            category_id: categoryId,
            kind: "manual" as const,
            meeting_app: null,
            started_at: null,
            ended_at: null,
            position: positions.length ? Math.min(...positions) - 1 : 1,
            created_at: now,
            updated_at: now,
            deleted_at: null,
          };
          state.notes.set(merged.id, merged);
          for (const ref of kind.sources) {
            const note = state.notes.get(ref.id);
            if (note) note.deleted_at = now;
          }
          noteId = merged.id;
          scopes.add("notes").add("trash");
          break;
        }
        case "delete_category": {
          for (const note of state.notes.values()) {
            if (note.category_id === kind.category_id) note.category_id = null;
          }
          state.categories.delete(kind.category_id);
          scopes.add("notes").add("categories");
          break;
        }
        case "delete_person": {
          state.people = state.people.filter((p) => p.id !== kind.person_id);
          for (const t of state.transcripts.values()) {
            for (const s of t.speakers) if (s.person_id === kind.person_id) s.person_id = null;
          }
          scopes.add("people").add("transcript");
          break;
        }
        case "create_note": {
          const created = {
            id: newId("note"),
            title: kind.title,
            body_md: kind.body_md,
            snippet:
              kind.body_md
                .split("\n")
                .find((l) => l.trim() && !l.startsWith("#"))
                ?.replace(/^[-*]\s+(\[.\]\s+)?/, "") ?? "",
            category_id: kind.category_id,
            kind: "manual" as const,
            meeting_app: null,
            started_at: null,
            ended_at: null,
            position: -100,
            created_at: now,
            updated_at: now,
            deleted_at: null,
          };
          state.notes.set(created.id, created);
          noteId = created.id;
          scopes.add("notes");
          break;
        }
        case "bulk_move": {
          for (const ref of kind.notes) {
            const note = state.notes.get(ref.id);
            if (note) note.category_id = kind.category_id;
          }
          scopes.add("notes");
          break;
        }
        case "move_groups": {
          for (const group of kind.groups) {
            for (const ref of group.notes) {
              const note = state.notes.get(ref.id);
              if (note) note.category_id = group.category_id;
            }
          }
          scopes.add("notes");
          break;
        }
      }
      proposal.status = "applied";
      proposal.resolved_at = now;
      if (linked) {
        linked.summary = appliedSummary(kind);
        linked.status = "applied";
        linked.can_undo = kind.type === "edit_note" || kind.type === "rename_note";
      }
      emit({ type: "data_changed", scopes: [...scopes], note_id: noteId });
      return proposal;
    },

    dismissProposal(proposalId: string): Proposal {
      const proposal = requireProposal(proposalId);
      if (proposal.status !== "pending") {
        throw appError("conflict", "That suggestion was already applied or dismissed.");
      }
      proposal.status = "dismissed";
      proposal.resolved_at = iso();
      const linked = state.aiActions.find((a) => a.proposal_id === proposalId);
      if (linked) linked.status = "rejected";
      return proposal;
    },

    // ── Audit log ──

    listAiActions(limit: number, offset: number): AiAction[] {
      return [...state.aiActions]
        .sort((a, b) => b.created_at.localeCompare(a.created_at))
        .slice(offset, offset + limit);
    },

    undoAiAction(actionId: string): AiAction {
      const action = requireAction(actionId);
      if (!action.can_undo || action.status !== "applied") {
        throw appError("invalid", "This action can't be undone.");
      }
      undo(action);
      emit({
        type: "data_changed",
        scopes: ["notes", "versions", "ai_actions"],
        note_id: noteIdOf(action),
      });
      return action;
    },

    undoTurn(turnId: string): number {
      const actions = state.aiActions.filter(
        (a) => a.turn_id === turnId && a.can_undo && a.status === "applied",
      );
      for (const action of actions) undo(action);
      if (actions.length)
        emit({
          type: "data_changed",
          scopes: ["notes", "versions", "ai_actions", "trash"],
          note_id: null,
        });
      return actions.length;
    },

    // ── Providers ──

    getProviderStatus(): ProviderStatus {
      return providerStatus(state.settings.ai.provider);
    },

    /** Keys starting "sk-" verify; anything else is stored but rejected, like a revoked key. */
    setProviderKey(kind: ProviderKind, key: string): ProviderStatus {
      if (key.trim().length < 8) {
        throw appError("invalid", "That key looks too short. Check it and paste it again.");
      }
      if (!state.providerKeys.includes(kind)) state.providerKeys.push(kind);
      if (!key.trim().startsWith("sk-")) {
        state.providerKeys = state.providerKeys.filter((k) => k !== kind);
        return {
          kind,
          has_key: false,
          ready: false,
          problem: "That key was rejected by the provider. Check it and paste it again.",
        };
      }
      return providerStatus(kind);
    },

    clearProviderKey(kind: ProviderKind): null {
      state.providerKeys = state.providerKeys.filter((k) => k !== kind);
      return null;
    },

    detectClaudeCode(): ClaudeCodeStatus {
      return { installed: true, version: "2.4.0", signed_in: true };
    },

    // ── Local models (Ollama) ──

    getOllamaStatus(): OllamaStatus {
      return ollama.running
        ? { installed: true, running: true, version: "0.13.0" }
        : { installed: false, running: false, version: null };
    },

    listLocalModels(): LocalLlmModel[] {
      return ollama.models;
    },

    setUpOllama(): null {
      // Installs and starts Ollama only; models are downloaded one by one when asked.
      const total = 229 * 1024 * 1024;
      let step = 0;
      const handle = setInterval(() => {
        step += 1;
        const done = step >= 8;
        if (done) {
          clearInterval(handle);
          ollama.running = true;
        }
        emit({
          type: "model_download",
          progress: {
            model_id: "ollama:app",
            downloaded_bytes: done ? 0 : Math.round((total * step) / 8),
            total_bytes: done ? 0 : total,
            done,
            error: null,
          },
        });
      }, 250);
      return null;
    },

    pullLocalModel(modelId: string): null {
      ollama.running = true;
      pull(modelId);
      return null;
    },

    cancelLocalModelPull(modelId: string): null {
      const handle = ollama.pulls.get(modelId);
      if (handle) clearInterval(handle);
      ollama.pulls.delete(modelId);
      emit({
        type: "model_download",
        progress: {
          model_id: `ollama:${modelId}`,
          downloaded_bytes: 0,
          total_bytes: 0,
          done: false,
          error: "Download cancelled.",
        },
      });
      return null;
    },

    deleteLocalModel(modelId: string): null {
      const m = ollama.models.find((x) => x.id === modelId);
      if (m) m.downloaded = false;
      return null;
    },

    selectLocalModel(modelId: string): null {
      chooseLocal(modelId);
      return null;
    },

    dispose(): void {
      for (const turn of turns.values()) turn.timers.clear();
      turns.clear();
    },
  };

  function chooseLocal(modelId: string) {
    state.settings.ai = {
      ...state.settings.ai,
      provider: "ollama",
      model: modelId,
      fast_model: modelId,
    };
    emit({ type: "settings_changed", settings: JSON.parse(JSON.stringify(state.settings)) });
  }

  function pull(modelId: string) {
    const model = ollama.models.find((m) => m.id === modelId);
    if (!model || model.downloaded || ollama.pulls.has(modelId)) return;
    const total = model.size_mb * 1024 * 1024;
    let step = 0;
    const handle = setInterval(() => {
      step += 1;
      const done = step >= 16;
      emit({
        type: "model_download",
        progress: {
          model_id: `ollama:${modelId}`,
          downloaded_bytes: Math.round((total * step) / 16),
          total_bytes: total,
          done,
          error: null,
        },
      });
      if (done) {
        clearInterval(handle);
        ollama.pulls.delete(modelId);
        model.downloaded = true;
      }
    }, 250);
    ollama.pulls.set(modelId, handle);
  }

  function noteIdOf(action: AiAction): string | null {
    try {
      const args = JSON.parse(action.args_json) as { note_id?: unknown };
      return typeof args.note_id === "string" ? args.note_id : null;
    } catch {
      return null;
    }
  }

  /** Revert an applied action: for note edits, restore the body from just before it. */
  function undo(action: AiAction): void {
    const noteId = noteIdOf(action);
    const note = noteId ? state.notes.get(noteId) : undefined;
    if (note && action.tool === "edit_note") {
      const versions = state.versions
        .filter((v) => v.note_id === note.id)
        .sort((a, b) => a.created_at.localeCompare(b.created_at));
      const aiIndex = versions.findIndex((v) => v.ai_action_id === action.id);
      const before = aiIndex > 0 ? versions[aiIndex - 1] : undefined;
      if (before) {
        snapshot(state, note, "ai", action.id);
        note.body_md = before.body_md;
        note.updated_at = iso();
      }
    }
    if (action.tool === "move_note") {
      const args = JSON.parse(action.args_json) as { note_id?: string; from?: string | null };
      const moved = args.note_id ? state.notes.get(args.note_id) : undefined;
      if (moved) moved.category_id = args.from ?? null;
    }
    if (action.tool === "create_note" && note) note.deleted_at = iso();
    action.status = "undone";
    action.can_undo = false;
  }
}
