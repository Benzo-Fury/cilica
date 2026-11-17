/**
 * The one listener for Cilica's work (mounted in the shell): it feeds `useCilicaActivity`, keeps
 * every remembered conversation current while another is on screen, and tells you when a turn you
 * are not watching is done or needs your OK, once per turn. Nothing is said while it runs.
 */
import { type QueryClient, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { setTurnToastHandler, useBackendEvent } from "@/app/backendEvents";
import { qk } from "@/app/queryKeys";
import { usePanels } from "@/features/shell/panels";
import type { Proposal, Thread } from "@/ipc/types";
import { toast } from "@/lib/toast";
import { useCilicaActivity } from "./activity";
import { useAssistantStore } from "./store";
import { rememberThread, resumeThread, threadReducer } from "./thread";

/** Plain toast for a finished turn waits this long for the turn's Undo toast to replace it. */
const UNDO_TOAST_GRACE = 700;

/** True while the conversation is on screen in the open panel. */
export function isWatching(threadId: string): boolean {
  return (
    usePanels.getState().assistantOpen &&
    useAssistantStore.getState().threadId === threadId &&
    document.visibilityState === "visible"
  );
}

/** Open the panel on a conversation. */
export function showThread(threadId: string) {
  useAssistantStore.getState().setThreadId(threadId);
  usePanels.getState().setAssistantOpen(true);
}

function titleOf(queryClient: QueryClient, threadId: string): string {
  const thread = queryClient.getQueryData<Thread[]>(qk.threads)?.find((t) => t.id === threadId);
  return thread?.title.trim() ?? "";
}

/** "Cilica finished “Tasks from Acme kickoff”", or a plain line for an untitled conversation. */
export function doneTitle(title: string): string {
  return title ? `Cilica finished “${title}”` : "Cilica finished your request";
}

/** One line on what a proposal waits for. */
export function proposalLine(proposal: Proposal): string {
  const kind = proposal.kind;
  switch (kind.type) {
    case "edit_note":
      return `Review an edit to “${kind.note.title || "Untitled"}”`;
    case "rename_note":
      return `Rename “${kind.note.title || "Untitled"}” to “${kind.new_title}”`;
    case "trash_notes":
      return kind.notes.length === 1
        ? `Move “${kind.notes[0]?.title || "Untitled"}” to Trash`
        : `Move ${kind.notes.length} notes to Trash`;
    case "merge_notes":
      return `Merge ${kind.sources.length} notes into “${kind.merged_title}”`;
    case "bulk_move":
      return `Move ${kind.notes.length} notes to ${kind.category_name}`;
    default:
      return proposal.reason;
  }
}

export function useCilicaFeed() {
  const queryClient = useQueryClient();

  useEffect(() => {
    const pending = new Map<string, ReturnType<typeof setTimeout>>();
    const stop = setTurnToastHandler((request, turnId, undo) => {
      const threadId = useCilicaActivity.getState().turnThreads[turnId];
      if (!threadId) return false;
      const timer = pending.get(turnId);
      if (timer) clearTimeout(timer);
      pending.delete(turnId);
      // On screen, the "What changed" card under the reply carries Undo (and ⌘Z still works).
      // A turn that already asked for your OK said so once; that is its one notification.
      if (isWatching(threadId) || askedTurns.has(turnId)) return true;
      toast.undo(doneTitle(titleOf(queryClient, threadId)), {
        description: request.title,
        action: { label: "Show", onClick: () => showThread(threadId) },
        onUndo: undo,
      });
      return true;
    });
    feedPending = pending;
    return () => {
      stop();
      for (const timer of pending.values()) clearTimeout(timer);
      feedPending = null;
    };
  }, [queryClient]);

  useBackendEvent("assistant", ({ event }) => {
    useCilicaActivity.getState().apply(event, Date.now());

    // Keep a conversation that is not on screen current, so opening it shows every step.
    const remembered = resumeThread(event.thread_id);
    if (
      remembered.threadId === event.thread_id &&
      useAssistantStore.getState().threadId !== event.thread_id
    ) {
      rememberThread(threadReducer(remembered, { type: "event", event }));
    }

    if (isWatching(event.thread_id)) return;
    const show = { label: "Show", onClick: () => showThread(event.thread_id) };
    if (event.type === "proposal" && event.proposal.status === "pending") {
      // One per turn: a turn that proposes several changes asks once.
      if (askedTurns.has(event.turn_id)) return;
      askedTurns.add(event.turn_id);
      toast.action("Cilica needs your OK", {
        description: proposalLine(event.proposal),
        action: { label: "Review", onClick: () => showThread(event.thread_id) },
        duration: 12_000,
      });
    } else if (event.type === "turn_finished") {
      const title = titleOf(queryClient, event.thread_id);
      if (event.end.type === "error") {
        toast.error(title ? `Cilica stopped working on “${title}”` : "Cilica stopped working", {
          description: event.end.message,
          action: show,
        });
      } else if (event.end.type === "completed" && !askedTurns.has(event.turn_id)) {
        // A turn that changed notes raises an Undo toast right after; that one says it instead.
        const turnId = event.turn_id;
        feedPending?.set(
          turnId,
          setTimeout(() => {
            feedPending?.delete(turnId);
            if (!isWatching(event.thread_id))
              toast.success(doneTitle(titleOf(queryClient, event.thread_id)), {
                action: show,
                duration: 6000,
              });
          }, UNDO_TOAST_GRACE),
        );
      }
    }
  });
}

let feedPending: Map<string, ReturnType<typeof setTimeout>> | null = null;
/** Turns that raised "Cilica needs your OK": they get no other notification. */
const askedTurns = new Set<string>();
