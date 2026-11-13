/**
 * The assistant (spec › Right sidebar, AssistantThread / Composer docs), as the right-hand panel
 * or inside the floating bar (`variant="bar"`, see CilicaBar): the context line,
 * prompt chips when the thread is empty, the conversation (your bubbles, Cilica's plain-text
 * replies streaming in, one-line tool steps, diff and confirm cards) and the composer.
 * When no model is ready it says why and links to Settings › Cilica.
 */
import {
  ChevronDown,
  ChevronLeft,
  CircleAlert,
  KeyRound,
  PanelBottom,
  PanelRight,
  Settings2,
  Square,
  SquarePen,
  Trash2,
} from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { type ReactNode, useEffect, useMemo, useRef } from "react";
import { useAction, useShortcut } from "@/app/keybinds";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { isMac } from "@/app/platform";
import { openSettings } from "@/app/router";
import { useNotes } from "@/features/notes/hooks";
import { useProposal } from "@/features/proposals/hooks";
import { ProposalCard } from "@/features/proposals/ProposalCard";
import { toolIcon, toolName } from "@/features/proposals/tools";
import { useSettings, useUpdateSettings } from "@/features/settings/hooks";
import { usePanels } from "@/features/shell/panels";
import { isTauri } from "@/ipc/client";
import type { Message, MessagePart, Proposal, Thread } from "@/ipc/types";
import {
  AiText,
  Bubble,
  Button,
  ChipButton,
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
  ThreadItem,
  ThreadNotice,
  TitleBand,
  ToolStep,
  WindowControls,
} from "@/ui";
import { useWindowControls } from "../shell/useWindowControls";
import { conversationMeta, groupConversations, retentionLine } from "./conversations";
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

export function AssistantPanel({ variant = "panel" }: { variant?: "panel" | "bar" }) {
  const bar = variant === "bar";
  const open = usePanels((s) => s.assistantOpen);
  const { patch } = useUpdateSettings();
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
  const { data: settings } = useSettings();
  const retentionDays = settings?.ai.conversation_retention_days ?? 7;
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const reduced = useReducedMotion();
  const ready = status?.ready ?? false;
  const working = state.sending || state.activeTurn !== null;
  const empty = state.messages.length === 0;
  const thread = threads.find((t) => t.id === threadId);
  const chips = usePromptChips(context, open && empty && ready);
  const notReady =
    !statusLoading && !ready ? (status?.problem ?? "Choose a model for Cilica in Settings.") : null;
  /** No conversation open: your earlier conversations show, and the first send starts a new one. */
  const listing = threadId === null && empty && !working;
  const hasList = listing && listed.length > 0;
  /** Bar only, with nothing to list: suggestions sit above the composer instead of a thread. */
  const compact = bar && listing && !hasList;

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
    if (empty && ready) void refetchChips();
    return () => clearTimeout(t);
  }, [open, focusTick]);

  const submit = (text = draft) => {
    const trimmed = text.trim();
    if (!trimmed || working) return;
    setDraft("");
    void send(trimmed, context);
  };

  const newThread = () => {
    if (working) void stop();
    setThreadId(null);
    composerRef.current?.focus();
  };
  useAction("assistant.newThread", () => {
    usePanels.getState().setAssistantOpen(true);
    newThread();
  });

  const Head = bar ? BarHead : TitleBand;
  const notice = notReady !== null && (
    <div className="nt-cilica-notice" role="status">
      <Icon icon={KeyRound} size="sm" />
      <span>{notReady}</span>
      <Button size="sm" variant="secondary" onClick={() => openSettings("cilica")}>
        Set up
      </Button>
    </div>
  );
  const composer = (
    <Composer
      ref={composerRef}
      value={draft}
      onChange={setDraft}
      onSubmit={() => submit()}
      onStop={() => void stop()}
      working={working}
      disabled={!ready && !statusLoading}
      placeholder={
        ready || statusLoading
          ? "Ask about your notes, or tell Cilica what to change"
          : "Set up Cilica in Settings first"
      }
    />
  );
  return (
    <div
      className={cn("nt-assistant", bar && "nt-assistant-bar")}
      data-compact={compact || undefined}
      data-working={working || undefined}
    >
      <span className="nt-cilica-glow" aria-hidden />
      <Head className="nt-assistant-head">
        {!listing && (
          <IconButton
            icon={ChevronLeft}
            label="All conversations"
            onClick={newThread}
            className="nt-assistant-back"
          />
        )}
        <strong className="nt-assistant-title">
          {listing ? "Cilica" : thread?.title || "New conversation"}
        </strong>
        {thread && !listing && (
          <IconButton
            icon={Trash2}
            iconMotion="trash"
            label="Delete this conversation"
            onClick={() => deleteThread(thread)}
          />
        )}
        <IconButton icon={SquarePen} label="New conversation" onClick={newThread} />
        {bar && (
          <>
            <IconButton
              icon={PanelRight}
              label="Open in side panel"
              onClick={() => patch("appearance", { cilica_placement: "side_panel" })}
            />
            <IconButton
              icon={ChevronDown}
              label="Collapse"
              onClick={() => usePanels.getState().setAssistantOpen(false)}
            />
          </>
        )}
        {!bar && (
          <IconButton
            icon={PanelBottom}
            label="Show as floating bar"
            onClick={() => patch("appearance", { cilica_placement: "floating_bar" })}
          />
        )}
        {!bar && (
          <IconButton
            icon={PanelRight}
            label="Hide Cilica"
            shortcut={toggleKeys}
            onClick={() => usePanels.getState().setAssistantOpen(false)}
          />
        )}
        {!bar && !isMac && <WindowControls {...controls} />}
      </Head>
      <AnimatePresence mode="popLayout" initial={false} custom={listing ? -1 : 1}>
        <motion.div
          key={listing ? "home" : "thread"}
          className="nt-assistant-view"
          custom={listing ? -1 : 1}
          variants={viewVariants(reduced ?? false)}
          initial="enter"
          animate="center"
          exit="exit"
        >
          {hasList && !bar ? (
            // No conversation open in the panel: earlier conversations scroll top down; the intro
            // and suggestions stay just above the composer, as in a new conversation.
            <>
              <ConversationList
                bar={false}
                threads={listed}
                retentionDays={retentionDays}
                onOpen={setThreadId}
                onDelete={deleteThread}
              />
              {notReady === null && (
                <div className="nt-assistant-home-intro">
                  <AssistantIntro
                    chips={chips.data ?? []}
                    chipsLoading={chips.isLoading}
                    onChip={(prompt) => submit(prompt)}
                  />
                </div>
              )}
            </>
          ) : hasList ? (
            <ConversationList
              bar={bar}
              threads={listed}
              retentionDays={retentionDays}
              onOpen={setThreadId}
              onDelete={deleteThread}
            />
          ) : compact ? (
            notReady === null &&
            (chips.data ?? []).length > 0 && (
              <div className="nt-cilicabar-chips">
                {(chips.data ?? []).slice(0, 3).map((chip) => (
                  <ChipButton key={chip.label} onClick={() => submit(chip.prompt)}>
                    {chip.label}
                  </ChipButton>
                ))}
              </div>
            )
          ) : (
            <ThreadView
              state={state}
              loading={loading}
              notReady={notReady}
              empty={empty}
              chips={chips.data ?? []}
              chipsLoading={chips.isLoading}
              onChip={(prompt) => submit(prompt)}
            />
          )}
        </motion.div>
      </AnimatePresence>
      {notice}
      {composer}
      <RetentionNote days={retentionDays} />
    </div>
  );
}

/**
 * List ⇄ conversation: the view-change slide (motion.md › Notes ⇄ Transcript). Opening a
 * conversation comes in from the right; going back to the list comes in from the left.
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

/** Earlier conversations: grouped by day in the panel, the 5 most recent in the bar. */
function ConversationList({
  bar,
  threads,
  retentionDays,
  onOpen,
  onDelete,
}: {
  bar: boolean;
  threads: Thread[];
  retentionDays: number;
  onOpen: (id: string) => void;
  onDelete: (thread: Thread) => void;
}) {
  const row = (t: Thread) => (
    <ConversationRow
      key={t.id}
      inline={bar}
      title={t.title || "New conversation"}
      meta={conversationMeta(t, retentionDays)}
      onOpen={() => onOpen(t.id)}
      onDelete={() => onDelete(t)}
    />
  );
  const groups = bar
    ? [{ label: "Recent", threads: threads.slice(0, 5) }]
    : groupConversations(threads);
  return (
    <div className="nt-conv-list">
      {groups.map((g) => (
        <ConversationGroup key={g.label} label={g.label}>
          {g.threads.map(row)}
        </ConversationGroup>
      ))}
    </div>
  );
}

/** What Cilica is for, with suggested prompts: shown whenever no conversation has started. */
function AssistantIntro({
  chips,
  chipsLoading,
  onChip,
}: {
  chips: { label: string; prompt: string }[];
  chipsLoading: boolean;
  onChip: (prompt: string) => void;
}) {
  return (
    <div className="nt-assistant-empty">
      <h3 className="nt-assistant-empty-title">Ask Cilica</h3>
      <p className="nt-assistant-empty-text">
        About this note, a meeting, or everything you have written. Cilica can also tidy, file and
        edit notes, and asks before it changes anything that matters.
      </p>
      <div className="nt-chip-list" aria-busy={chipsLoading || undefined}>
        {chipsLoading && chips.length === 0
          ? [0, 1, 2].map((i) => <span key={i} className="nt-skel nt-chip-skel" />)
          : chips.map((chip) => (
              <ChipButton
                key={chip.label}
                className="nt-prompt-chip"
                onClick={() => onChip(chip.prompt)}
              >
                {chip.label}
              </ChipButton>
            ))}
      </div>
    </div>
  );
}

function RetentionNote({ days }: { days: number }) {
  return (
    <ConversationFootnote>
      {retentionLine(days)}{" "}
      <button type="button" className="nt-cite" onClick={() => openSettings("cilica")}>
        Change in Settings
      </button>
    </ConversationFootnote>
  );
}

/** The bar's header row: the same controls as the panel's title band, without window chrome. */
function BarHead({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={cn("nt-cilicabar-head", className)}>{children}</div>;
}

function ThreadView({
  state,
  loading,
  notReady,
  empty,
  chips,
  chipsLoading,
  onChip,
}: {
  state: ThreadState;
  loading: boolean;
  notReady: string | null;
  empty: boolean;
  chips: { label: string; prompt: string }[];
  chipsLoading: boolean;
  onChip: (prompt: string) => void;
}) {
  const streaming = streamingPart(state);
  const lastLength = useMemo(() => {
    const last = state.messages[state.messages.length - 1];
    return last ? JSON.stringify(last.parts).length : 0;
  }, [state.messages]);
  // Follow new words until you scroll up.
  const scroll = useStickToBottom(`${state.messages.length}:${lastLength}`);

  // No model yet: the notice above the composer says so; the conversation area stays empty.
  if (notReady) return <div className="nt-assistant-body" />;
  if (loading) {
    return (
      <div className="nt-assistant-body">
        <SkeletonText lines={4} />
      </div>
    );
  }
  if (empty) {
    return (
      <div className="nt-assistant-body">
        <AssistantIntro chips={chips} chipsLoading={chipsLoading} onChip={onChip} />
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
      </div>
      <PinnedProposals state={state} />
    </div>
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
      <ThreadItem className={isPending(message) ? "nt-msg-pending" : undefined}>
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
