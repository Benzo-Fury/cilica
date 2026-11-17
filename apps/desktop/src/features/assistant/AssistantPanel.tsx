/**
 * Cilica's side panel (spec › Right sidebar, AssistantThread / Composer docs).
 *
 * At rest (no conversation open) it is calm: one line on what Cilica can see, up to 3 suggestions
 * for the open note, then the conversations, any still running first. "See all" opens the full
 * list. A conversation shows your bubbles, Cilica's replies, one line per step as it happens, what
 * the turn is touching while it runs, and "What changed" with Undo once it is done.
 *
 * Leaving a conversation never stops it: the turn keeps going in the background, its row shows it
 * working, and a toast says when it is done or needs your OK (feed.ts).
 */
import {
  ChevronLeft,
  CircleAlert,
  CircleCheck,
  Eye,
  KeyRound,
  PanelRight,
  Settings2,
  Square,
  SquarePen,
  Trash2,
  Undo2,
} from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { forgetAiUndo } from "@/app/aiUndo";
import { runToastAction } from "@/app/backendEvents";
import { useAction, useShortcut } from "@/app/keybinds";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { isMac } from "@/app/platform";
import { openNote, openSettings } from "@/app/router";
import { useNotes } from "@/features/notes/hooks";
import { useProposal } from "@/features/proposals/hooks";
import { ProposalCard } from "@/features/proposals/ProposalCard";
import { toolIcon, toolName } from "@/features/proposals/tools";
import { useSettings } from "@/features/settings/hooks";
import { usePanels } from "@/features/shell/panels";
import { isTauri } from "@/ipc/client";
import type { AiAction, Message, MessagePart, Proposal, Thread } from "@/ipc/types";
import {
  AiText,
  Bubble,
  Button,
  ChangeCard,
  type ChangeRow,
  Composer,
  ConversationFootnote,
  ConversationGroup,
  ConversationRow,
  cn,
  Icon,
  IconButton,
  Markdown,
  Shimmer,
  SkeletonText,
  SuggestionList,
  SuggestionRow,
  SuggestionSkeleton,
  ThreadItem,
  ThreadNotice,
  TitleBand,
  ToolStep,
  TouchList,
  WindowControls,
} from "@/ui";
import { useWindowControls } from "../shell/useWindowControls";
import { type Run, useCilicaActivity } from "./activity";
import { conversationMeta, groupConversations, retentionLine } from "./conversations";
import { MAX_SUGGESTIONS, restingConversations, seeLine, suggestionIcon } from "./home";
import {
  useDeleteThread,
  useLiveThread,
  usePromptChips,
  useProviderStatus,
  useThreads,
  useTurnActions,
  useViewContext,
  useVisibleThreads,
} from "./hooks";
import { Receipt } from "./Receipt";
import { isComposing, workingLine } from "./status";
import { useStickToBottom } from "./stickToBottom";
import { useAssistantStore } from "./store";
import { isPending, isToolWorking, streamingPart, type ThreadState } from "./thread";

type Screen = "home" | "all" | "thread";

export function AssistantPanel() {
  const open = usePanels((s) => s.assistantOpen);
  const toggleKeys = useShortcut("assistant.toggle");
  const controls = useWindowControls();
  const context = useViewContext();
  const { data: status, isLoading: statusLoading, refetch: refetchStatus } = useProviderStatus();
  const { data: threads = [] } = useThreads();
  const threadId = useAssistantStore((s) => s.threadId);
  const setThreadId = useAssistantStore((s) => s.setThreadId);
  const draft = useAssistantStore((s) => s.draft);
  const setDraft = useAssistantStore((s) => s.setDraft);
  const focusTick = useAssistantStore((s) => s.focusTick);
  const { state, dispatch, loading } = useLiveThread(threadId);
  const { send, stop } = useTurnActions(dispatch, state.activeTurn);
  const deleteThread = useDeleteThread();
  const { threads: listed } = useVisibleThreads();
  const runs = useCilicaActivity((s) => s.runs);
  const { data: settings } = useSettings();
  const retentionDays = settings?.ai.conversation_retention_days ?? 7;
  const suggestionsOn = settings?.ai.suggestions ?? true;
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const reduced = useReducedMotion() ?? false;
  const [showAll, setShowAll] = useState(false);
  const ready = status?.ready ?? false;
  const working = state.sending || state.activeTurn !== null;
  const empty = state.messages.length === 0;
  const thread = threads.find((t) => t.id === threadId);
  const listing = threadId === null && empty && !working;
  const screen: Screen = !listing ? "thread" : showAll ? "all" : "home";
  const chips = usePromptChips(context, open && listing && ready && suggestionsOn);
  const notReady =
    !statusLoading && !ready ? (status?.problem ?? "Choose a model for Cilica in Settings.") : null;

  // A remembered thread that no longer exists: start fresh.
  useEffect(() => {
    if (threadId && threads.length > 0 && !threads.some((t) => t.id === threadId))
      setThreadId(null);
  }, [threadId, threads, setThreadId]);

  // Opening the panel (⌘J, Ask Cilica) puts the caret in the composer and asks for fresh chips.
  // A "not ready" answer is asked again: a key saved in another window (onboarding) only
  // refreshed that window's cache, and this panel stays mounted while closed.
  const { refetch: refetchChips } = chips;
  // biome-ignore lint/correctness/useExhaustiveDependencies: on open and on request only
  useEffect(() => {
    if (!open) return;
    const t = setTimeout(() => composerRef.current?.focus(), 60);
    if (!ready && !statusLoading) void refetchStatus();
    if (empty && ready && suggestionsOn) void refetchChips();
    return () => clearTimeout(t);
  }, [open, focusTick]);

  const submit = (text = draft) => {
    const trimmed = text.trim();
    if (!trimmed || working) return;
    setDraft("");
    setShowAll(false);
    void send(trimmed, context);
  };

  /** Back to rest. A turn still running carries on in the background. */
  const goHome = () => {
    setShowAll(false);
    setThreadId(null);
    composerRef.current?.focus();
  };
  const openThread = (id: string) => {
    setShowAll(false);
    setThreadId(id);
  };
  useAction("assistant.newThread", () => {
    usePanels.getState().setAssistantOpen(true);
    goHome();
  });

  const notice = notReady !== null && (
    <div className="nt-cilica-notice" role="status">
      <Icon icon={KeyRound} size="sm" />
      <span>{notReady}</span>
      <Button size="sm" variant="secondary" onClick={() => openSettings("cilica")}>
        Set up
      </Button>
    </div>
  );
  const run = threadId ? runs[threadId] : undefined;
  return (
    <div className="nt-assistant" data-working={working || undefined}>
      <span className="nt-cilica-glow" aria-hidden />
      <TitleBand className="nt-assistant-head">
        {screen !== "home" && (
          <IconButton
            icon={ChevronLeft}
            label={screen === "all" ? "Back" : "All conversations"}
            onClick={goHome}
            className="nt-assistant-back"
          />
        )}
        <strong className="nt-assistant-title">
          {screen === "home"
            ? "Cilica"
            : screen === "all"
              ? "Conversations"
              : thread?.title || "New conversation"}
        </strong>
        {screen === "thread" && working && <WorkingClock run={run} />}
        {screen === "thread" && thread && !working && (
          <IconButton
            icon={Trash2}
            iconMotion="trash"
            label="Delete this conversation"
            onClick={() => deleteThread(thread)}
          />
        )}
        <IconButton icon={SquarePen} label="New conversation" onClick={goHome} />
        <IconButton
          icon={PanelRight}
          label="Hide Cilica"
          shortcut={toggleKeys}
          onClick={() => usePanels.getState().setAssistantOpen(false)}
        />
        {!isMac && <WindowControls {...controls} />}
      </TitleBand>
      <AnimatePresence mode="popLayout" initial={false} custom={screen === "thread" ? 1 : -1}>
        <motion.div
          key={screen}
          className="nt-assistant-view"
          custom={screen === "thread" ? 1 : -1}
          variants={viewVariants(reduced)}
          initial="enter"
          animate="center"
          exit="exit"
        >
          {screen === "home" ? (
            <RestingHome
              seeing={seeLine(context)}
              notReady={notReady !== null}
              suggestionsOn={suggestionsOn}
              chips={(chips.data ?? []).slice(0, MAX_SUGGESTIONS)}
              chipsLoading={chips.isLoading}
              onChip={(prompt) => submit(prompt)}
              threads={listed}
              runs={runs}
              retentionDays={retentionDays}
              onOpen={openThread}
              onDelete={deleteThread}
              onShowAll={() => setShowAll(true)}
            />
          ) : screen === "all" ? (
            <AllConversations
              threads={listed}
              runs={runs}
              retentionDays={retentionDays}
              onOpen={openThread}
              onDelete={deleteThread}
            />
          ) : (
            <ThreadView state={state} loading={loading} notReady={notReady} />
          )}
        </motion.div>
      </AnimatePresence>
      {screen === "thread" && working && <WorkFooter run={run} />}
      {notice}
      <Composer
        ref={composerRef}
        value={draft}
        onChange={setDraft}
        onSubmit={() => submit()}
        onStop={() => void stop()}
        working={screen === "thread" && working}
        disabled={!ready && !statusLoading}
        placeholder={
          !ready && !statusLoading
            ? "Set up Cilica in Settings first"
            : screen === "thread" && working
              ? "Cilica is working on this…"
              : "Ask, or tell Cilica what to do"
        }
      />
    </div>
  );
}

/**
 * Home ⇄ list ⇄ conversation: the view-change slide (motion.md › Notes ⇄ Transcript). Going
 * deeper comes in from the right; going back comes in from the left.
 */
function viewVariants(reduced: boolean) {
  const shift = (direction: number) => (reduced ? 0 : 8 * direction);
  return {
    enter: (direction: number) => ({ opacity: 0, x: shift(direction) }),
    center: { opacity: 1, x: 0, transition: reduced ? reducedTransition : springs.gentle },
    exit: (direction: number) => ({
      opacity: 0,
      x: -shift(direction),
      transition: reduced ? reducedTransition : tweens.exit,
    }),
  };
}

/** "Working · 8 s", counting while the turn runs. */
function WorkingClock({ run }: { run: Run | undefined }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  const seconds = run ? Math.max(0, Math.floor((now - run.startedAt) / 1000)) : 0;
  return (
    <span className="nt-assistant-clock" aria-live="off">
      {run && seconds > 0 ? `Working · ${formatSeconds(seconds)}` : "Working"}
    </span>
  );
}

export function formatSeconds(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  const m = Math.floor(seconds / 60);
  return `${m} min ${seconds % 60} s`;
}

/** A running conversation's line in the list: its newest step. */
function runLine(run: Run, titleOf: (noteId: string) => string | undefined): string {
  const step = run.step;
  if (!step) return "Working…";
  if (step.status === "running") {
    const line = workingLine(step, titleOf) || step.summary || toolName(step.tool);
    return line.endsWith("…") ? line : `${line}…`;
  }
  return step.summary || "Working…";
}

function useNoteTitle() {
  const { data: notes = [] } = useNotes();
  return useMemo(() => {
    const byId = new Map(notes.map((n) => [n.id, n.title.trim() || "Untitled"]));
    return (noteId: string) => byId.get(noteId);
  }, [notes]);
}

function ConversationRows({
  threads,
  runs,
  retentionDays,
  onOpen,
  onDelete,
}: {
  threads: Thread[];
  runs: Record<string, Run>;
  retentionDays: number;
  onOpen: (id: string) => void;
  onDelete: (thread: Thread) => void;
}) {
  const titleOf = useNoteTitle();
  return (
    <>
      {threads.map((t) => {
        const run = runs[t.id];
        return (
          <ConversationRow
            key={t.id}
            inline={!run}
            live={Boolean(run)}
            title={t.title || "New conversation"}
            meta={run ? runLine(run, titleOf) : conversationMeta(t, retentionDays)}
            onOpen={() => onOpen(t.id)}
            onDelete={() => onDelete(t)}
          />
        );
      })}
    </>
  );
}

/** The panel at rest. */
function RestingHome({
  seeing,
  notReady,
  suggestionsOn,
  chips,
  chipsLoading,
  onChip,
  threads,
  runs,
  retentionDays,
  onOpen,
  onDelete,
  onShowAll,
}: {
  seeing: string;
  notReady: boolean;
  suggestionsOn: boolean;
  chips: { label: string; prompt: string }[];
  chipsLoading: boolean;
  onChip: (prompt: string) => void;
  threads: Thread[];
  runs: Record<string, Run>;
  retentionDays: number;
  onOpen: (id: string) => void;
  onDelete: (thread: Thread) => void;
  onShowAll: () => void;
}) {
  const running = useMemo(() => new Set(Object.keys(runs)), [runs]);
  const { shown, more } = restingConversations(threads, running);
  const showSuggestions = !notReady && suggestionsOn && (chipsLoading || chips.length > 0);
  return (
    <div className="nt-assistant-body nt-assistant-home">
      <header className="nt-assistant-intro">
        <h3 className="nt-assistant-empty-title">Ready when you are</h3>
        <p className="nt-assistant-empty-text">{seeing}</p>
      </header>
      {showSuggestions && (
        <SuggestionList label="Suggested" busy={chipsLoading}>
          {chipsLoading && chips.length === 0 ? (
            <SuggestionSkeleton />
          ) : (
            chips.map((chip) => (
              <SuggestionRow
                key={chip.label}
                icon={suggestionIcon(chip)}
                title={chip.label}
                detail={chip.prompt}
                onClick={() => onChip(chip.prompt)}
              />
            ))
          )}
        </SuggestionList>
      )}
      {shown.length > 0 && (
        <ConversationGroup label="Conversations">
          <ConversationRows
            threads={shown}
            runs={runs}
            retentionDays={retentionDays}
            onOpen={onOpen}
            onDelete={onDelete}
          />
          {more && (
            <button type="button" className="nt-conv-more" onClick={onShowAll}>
              See all {threads.length} conversations
            </button>
          )}
        </ConversationGroup>
      )}
    </div>
  );
}

/** Every conversation, grouped by day, with the retention line under them. */
function AllConversations({
  threads,
  runs,
  retentionDays,
  onOpen,
  onDelete,
}: {
  threads: Thread[];
  runs: Record<string, Run>;
  retentionDays: number;
  onOpen: (id: string) => void;
  onDelete: (thread: Thread) => void;
}) {
  return (
    <div className="nt-conv-list">
      {groupConversations(threads).map((g) => (
        <ConversationGroup key={g.label} label={g.label}>
          <ConversationRows
            threads={g.threads}
            runs={runs}
            retentionDays={retentionDays}
            onOpen={onOpen}
            onDelete={onDelete}
          />
        </ConversationGroup>
      ))}
      <ConversationFootnote>
        {retentionLine(retentionDays)}{" "}
        <button type="button" className="nt-cite" onClick={() => openSettings("cilica")}>
          Change in Settings
        </button>
      </ConversationFootnote>
    </div>
  );
}

/** Under a running conversation: what it is touching, and that you can leave. */
function WorkFooter({ run }: { run: Run | undefined }) {
  const titleOf = useNoteTitle();
  const items = (run?.touched ?? []).map((t) => ({
    key: t.key,
    label: t.label ?? (t.noteId ? (titleOf(t.noteId) ?? "A note") : "A note"),
    verb: t.verb,
    live: t.live,
  }));
  return (
    <div className="nt-assistant-work">
      <TouchList items={items} />
      <p className="nt-assistant-leave">
        You can leave this. Cilica keeps going and tells you when it's done.
      </p>
    </div>
  );
}

function ThreadView({
  state,
  loading,
  notReady,
}: {
  state: ThreadState;
  loading: boolean;
  notReady: string | null;
}) {
  const streaming = streamingPart(state);
  const lastLength = useMemo(() => {
    const last = state.messages[state.messages.length - 1];
    return last ? JSON.stringify(last.parts).length : 0;
  }, [state.messages]);
  // Follow new words until you scroll up.
  const scroll = useStickToBottom(`${state.messages.length}:${lastLength}`);

  // No model yet: the notice above the composer says so; the conversation area stays empty.
  if (notReady && state.messages.length === 0) return <div className="nt-assistant-body" />;
  if (loading) {
    return (
      <div className="nt-assistant-body">
        <SkeletonText lines={4} />
      </div>
    );
  }
  const lastUser = [...state.messages].reverse().find((m) => m.role === "user");
  const waiting =
    (state.sending || state.activeTurn !== null) &&
    !state.messages.some(
      (m) => m.role === "assistant" && m.turn_id === state.activeTurn && m.parts.length > 0,
    );
  return (
    <div
      className="nt-assistant-body nt-assistant-scroll"
      ref={scroll.ref}
      onScroll={scroll.onScroll}
    >
      <div className="nt-thread">
        {state.messages.map((message) => (
          <MessageView key={message.id} message={message} state={state} streaming={streaming} />
        ))}
        <AnimatePresence>
          {waiting && lastUser && (
            <ThreadItem key="thinking">
              <div className="nt-step">
                <Shimmer>Reading your question…</Shimmer>
              </div>
            </ThreadItem>
          )}
          {isComposing(state) && (
            <ThreadItem key="composing">
              <div className="nt-step">
                <Shimmer>Writing it down…</Shimmer>
              </div>
            </ThreadItem>
          )}
        </AnimatePresence>
        <TurnChanges state={state} />
      </div>
      <PinnedProposals state={state} />
    </div>
  );
}

/** The newest finished turn's writes, and the notes it read. */
export function turnChanges(
  state: Pick<ThreadState, "messages" | "activeTurn">,
): { turnId: string; writes: AiAction[]; readNotes: number } | null {
  const last = [...state.messages].reverse().find((m) => m.role === "assistant" && m.turn_id);
  if (!last || last.turn_id === state.activeTurn) return null;
  const actions = state.messages
    .filter((m) => m.turn_id === last.turn_id)
    .flatMap((m) => m.parts.flatMap((p) => (p.type === "tool" ? [p.action] : [])));
  const writes = actions.filter((a) => a.tier !== "read" && a.status === "applied");
  if (writes.length === 0) return null;
  const read = new Set(
    actions
      .filter((a) => a.tier === "read" && a.status === "applied")
      .map((a) => noteIdOf(a))
      .filter((id): id is string => Boolean(id)),
  );
  return { turnId: last.turn_id, writes, readNotes: read.size };
}

function noteIdOf(action: AiAction): string | null {
  try {
    const args = JSON.parse(action.args_json) as { note_id?: unknown };
    return typeof args.note_id === "string" ? args.note_id : null;
  } catch {
    return null;
  }
}

/** "What changed": every write of the turn just done, with one Undo for all of them. */
function TurnChanges({ state }: { state: ThreadState }) {
  const changes = turnChanges(state);
  const undoKeys = useShortcut("edit.undo");
  const [undone, setUndone] = useState<string | null>(null);
  if (!changes) return null;
  const rows: ChangeRow[] = changes.writes.map((a) => {
    const noteId = noteIdOf(a);
    return {
      key: a.id,
      icon: toolIcon(a.tool),
      label: a.summary || toolName(a.tool),
      onOpen: noteId ? () => openNote(noteId) : undefined,
    };
  });
  if (changes.readNotes > 0)
    rows.push({
      key: "read",
      icon: Eye,
      label: changes.readNotes === 1 ? "Read 1 note" : `Read ${changes.readNotes} notes`,
      quiet: true,
    });
  const isUndone = undone === changes.turnId;
  return (
    <ThreadItem key={changes.turnId}>
      <ChangeCard
        title={isUndone ? "Changes undone" : "What changed"}
        icon={isUndone ? Undo2 : CircleCheck}
        rows={rows}
        footer={
          !isUndone && (
            <Button
              size="sm"
              variant="secondary"
              shortcut={undoKeys}
              onClick={() => {
                forgetAiUndo();
                setUndone(changes.turnId);
                void runToastAction({ type: "undo_turn", turn_id: changes.turnId });
              }}
            >
              Undo all
            </Button>
          )
        }
      />
    </ThreadItem>
  );
}

function MessageView({
  message,
  state,
  streaming,
}: {
  message: Message;
  state: ThreadState;
  streaming: { messageId: string; index: number } | null;
}) {
  if (message.role === "user") {
    const text = message.parts.map((p) => (p.type === "text" ? p.text : "")).join("");
    return (
      <ThreadItem className={cn(isPending(message) && "nt-msg-pending")}>
        <div className="nt-msg-user">
          <Bubble>{text}</Bubble>
        </div>
      </ThreadItem>
    );
  }
  const end = state.ends[message.turn_id];
  return (
    <>
      {message.parts.map((part, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: parts only append within a message
        <PartItem key={i} part={part} state={state}>
          <PartView
            part={part}
            state={state}
            streaming={streaming?.messageId === message.id && streaming.index === i}
          />
        </PartItem>
      ))}
      {end?.type === "cancelled" && (
        <ThreadItem>
          <ThreadNotice icon={Square}>Stopped. Ask again to carry on.</ThreadNotice>
        </ThreadItem>
      )}
    </>
  );
}

/**
 * A part's row in the thread. A proposal waiting on Accept or Reject is shown in
 * `PinnedProposals` instead, until it is answered.
 */
function PartItem({
  part,
  state,
  children,
}: {
  part: MessagePart;
  state: ThreadState;
  children: ReactNode;
}) {
  if (part.type !== "proposal") return <ThreadItem>{children}</ThreadItem>;
  return (
    <PendingProposalItem proposalId={part.proposal_id} initial={state.proposals[part.proposal_id]}>
      {children}
    </PendingProposalItem>
  );
}

/** Inline in the thread once answered; while pending it lives in the pinned footer instead. */
function PendingProposalItem({
  proposalId,
  initial,
  children,
}: {
  proposalId: string;
  initial: Proposal | undefined;
  children: ReactNode;
}) {
  const { data } = useProposal(proposalId, initial);
  if ((data ?? initial)?.status === "pending") return null;
  return <ThreadItem>{children}</ThreadItem>;
}

/**
 * Proposals still waiting on Accept or Reject, pinned to the bottom of the conversation so replies
 * arriving after them never push them out of view.
 */
function PinnedProposals({ state }: { state: ThreadState }) {
  const ids = state.messages.flatMap((m) =>
    m.parts.flatMap((p) => (p.type === "proposal" ? [p.proposal_id] : [])),
  );
  if (ids.length === 0) return null;
  return (
    <div className="nt-thread-pinned">
      <AnimatePresence initial={false}>
        {ids.map((id) => (
          <PinnedProposal key={id} proposalId={id} initial={state.proposals[id]} />
        ))}
      </AnimatePresence>
    </div>
  );
}

function PinnedProposal({ proposalId, initial }: { proposalId: string; initial?: Proposal }) {
  const { data } = useProposal(proposalId, initial);
  if ((data ?? initial)?.status !== "pending") return null;
  return (
    <ThreadItem>
      <ProposalCard proposalId={proposalId} initial={initial} />
    </ThreadItem>
  );
}

function PartView({
  part,
  state,
  streaming,
}: {
  part: MessagePart;
  state: ThreadState;
  streaming: boolean;
}) {
  const { data: notes = [] } = useNotes();
  switch (part.type) {
    case "text":
      return (
        <AiText>
          <Markdown
            source={part.text}
            streaming={streaming}
            onLink={(href) => {
              if (!isTauri) window.open(href, "_blank", "noopener");
              else void import("@tauri-apps/plugin-opener").then((m) => m.openUrl(href));
            }}
            renderCitation={(title, time, key) => {
              const note = notes.find(
                (n) => n.title.trim().toLowerCase() === title.trim().toLowerCase(),
              );
              if (!note)
                return (
                  <span key={key} className="nt-faint">
                    (“{title}”{time ? `, ${time}` : ""})
                  </span>
                );
              return <Receipt key={key} noteId={note.id} title={title} time={time} />;
            }}
          />
        </AiText>
      );
    case "tool": {
      const a = part.action;
      const working = isToolWorking(state, a);
      const details = formatArgs(a.args_json, a.error);
      const label =
        (working && workingLine(a, (id) => notes.find((n) => n.id === id)?.title)) ||
        a.summary ||
        toolName(a.tool);
      return (
        <ToolStep
          icon={toolIcon(a.tool)}
          label={working && !label.endsWith("…") ? `${label}…` : label}
          working={working}
          failed={a.status === "failed"}
          details={details}
        />
      );
    }
    case "proposal":
      return (
        <ProposalCard proposalId={part.proposal_id} initial={state.proposals[part.proposal_id]} />
      );
    case "error":
      return (
        <ThreadNotice
          icon={CircleAlert}
          tone="danger"
          action={
            /key|setting|model|provider/i.test(part.message) && (
              <button type="button" className="nt-cite" onClick={() => openSettings("cilica")}>
                <Icon icon={Settings2} size="sm" /> Open Settings › Cilica
              </button>
            )
          }
        >
          {part.message}
        </ThreadNotice>
      );
  }
}

/** Tool arguments as readable lines ("query: onboarding"), plus any error. */
export function formatArgs(argsJson: string, error: string | null): string | undefined {
  let lines: string[] = [];
  try {
    const args = JSON.parse(argsJson) as Record<string, unknown>;
    lines = Object.entries(args).map(
      ([k, v]) => `${k.replace(/_/g, " ")}: ${typeof v === "string" ? v : JSON.stringify(v)}`,
    );
  } catch {
    if (argsJson.trim() && argsJson !== "{}") lines = [argsJson];
  }
  if (error) lines.push(`Error: ${error}`);
  return lines.length ? lines.join("\n") : undefined;
}
