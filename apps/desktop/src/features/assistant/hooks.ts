/**
 * The assistant's data: provider readiness, threads, the live thread (history plus the event
 * stream through `threadReducer`), sending and stopping turns, and prompt chips.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";
import { useBackendEvent } from "@/app/backendEvents";
import { qk } from "@/app/queryKeys";
import { useRouter } from "@/app/router";
import { useNoteViewState } from "@/features/note/viewState";
import { useNote } from "@/features/notes/hooks";
import { useRecordingState } from "@/features/recorder/hooks";
import { useSettings } from "@/features/settings/hooks";
import { api, errorMessage } from "@/ipc/client";
import type { Thread, ViewContext } from "@/ipc/types";
import { toast } from "@/lib/toast";
import { buildViewContext } from "./context";
import { useAssistantStore } from "./store";
import { forgetThread, rememberThread, resumeThread, threadReducer } from "./thread";

export function useProviderStatus() {
  const { data: settings } = useSettings();
  const provider = settings?.ai.provider;
  return useQuery({
    queryKey: [...qk.providerStatus, provider ?? "", settings?.ai.base_url ?? ""],
    queryFn: () => api.getProviderStatus(),
    enabled: Boolean(settings),
    staleTime: 60_000,
  });
}

/** The current ViewContext, rebuilt whenever what you are looking at changes. */
export function useViewContext(): ViewContext {
  const view = useRouter((s) => s.view);
  const noteId = view.name === "note" ? view.noteId : null;
  const { data: note } = useNote(noteId);
  const panelState = useNoteViewState();
  const recording = useRecordingState();
  const onThisNote = panelState.noteId === noteId;
  return useMemo(
    () =>
      buildViewContext({
        view,
        note: note ?? null,
        panel: onThisNote ? panelState.panel : "notes",
        selectedText: onThisNote ? panelState.selectedText : "",
        searchQuery: null,
        recording,
      }),
    [view, note, onThisNote, panelState.panel, panelState.selectedText, recording],
  );
}

export function useThreads() {
  return useQuery({ queryKey: qk.threads, queryFn: () => api.listThreads() });
}

/**
 * Delete a conversation with Undo: it leaves the list at once and is deleted for good when the
 * toast closes without Undo.
 */
export function useDeleteThread() {
  const queryClient = useQueryClient();
  return useCallback(
    (thread: Thread) => {
      const store = useAssistantStore.getState();
      store.hide(thread.id);
      if (store.threadId === thread.id) store.setThreadId(null);
      let undone = false;
      toast.undo(`Deleted “${thread.title || "New conversation"}”`, {
        onUndo: () => {
          undone = true;
          useAssistantStore.getState().unhide(thread.id);
        },
        onDismiss: () => {
          if (undone) return;
          api
            .deleteThread(thread.id)
            .then(() => {
              forgetThread(thread.id);
              queryClient.setQueryData<Thread[]>(qk.threads, (list) =>
                list?.filter((t) => t.id !== thread.id),
              );
            })
            .catch((error: unknown) => {
              toast.error("The conversation was not deleted", { description: errorMessage(error) });
            })
            .finally(() => useAssistantStore.getState().unhide(thread.id));
        },
      });
    },
    [queryClient],
  );
}

/** Conversations to list, newest first, without the ones waiting on an Undo toast. */
export function useVisibleThreads() {
  const { data: threads = [], isLoading } = useThreads();
  const hidden = useAssistantStore((s) => s.hidden);
  const visible = useMemo(() => threads.filter((t) => !hidden.includes(t.id)), [threads, hidden]);
  return { threads: visible, isLoading };
}

/** The open thread: history, then live events folded in. */
export function useLiveThread(threadId: string | null) {
  const queryClient = useQueryClient();
  const [state, dispatch] = useReducer(threadReducer, threadId, resumeThread);
  useEffect(() => rememberThread(state), [state]);
  const { data: history, isLoading } = useQuery({
    queryKey: qk.threadMessages(threadId ?? ""),
    queryFn: () => api.getThreadMessages(threadId as string),
    enabled: Boolean(threadId),
    staleTime: Number.POSITIVE_INFINITY,
  });
  useEffect(() => {
    if (!threadId) dispatch({ type: "load", threadId: null, messages: [] });
    else if (history) dispatch({ type: "load", threadId, messages: history });
  }, [threadId, history]);
  useBackendEvent("assistant", (event) => {
    dispatch({ type: "event", event: event.event });
    // The cached history stops at the last load; once a reply is stored, fetch it again so a
    // view that opens later (the other placement, or this one reopened) starts complete.
    if (event.event.type === "turn_finished")
      void queryClient.invalidateQueries({ queryKey: qk.threadMessages(event.event.thread_id) });
  });
  return { state, dispatch, loading: Boolean(threadId) && isLoading };
}

/** Send a message (creating the thread on first send) and stop the running turn. */
export function useTurnActions(
  dispatch: ReturnType<typeof useLiveThread>["dispatch"],
  activeTurn: string | null,
) {
  const queryClient = useQueryClient();
  const turnRef = useRef(activeTurn);
  turnRef.current = activeTurn;

  const send = useCallback(
    async (text: string, view: ViewContext) => {
      const store = useAssistantStore.getState();
      let threadId = store.threadId;
      try {
        if (!threadId) {
          const thread = await api.createThread();
          queryClient.setQueryData<Thread[]>(qk.threads, (list) => [thread, ...(list ?? [])]);
          // The new thread has no history; seed an empty one so the reducer accepts its events.
          queryClient.setQueryData(qk.threadMessages(thread.id), []);
          threadId = thread.id;
          store.setThreadId(threadId);
        }
        dispatch({ type: "send", threadId, text, now: new Date().toISOString() });
        const turnId = await api.sendMessage(threadId, text, view);
        dispatch({ type: "turn_id", turnId });
        void queryClient.invalidateQueries({ queryKey: qk.threads });
      } catch (error) {
        dispatch({ type: "send_failed", message: errorMessage(error) });
      }
    },
    [queryClient, dispatch],
  );

  const stop = useCallback(async () => {
    const turnId = turnRef.current;
    if (!turnId) return;
    try {
      await api.cancelTurn(turnId);
    } catch {
      // The turn already finished; its turn_finished event settles the UI.
    }
  }, []);

  return { send, stop };
}

/**
 * Prompt chips for the empty state. Chips the backend has cached come back at once and fresh
 * ones a little later, so ask again after a few seconds and whenever the panel reopens.
 */
export function usePromptChips(view: ViewContext, enabled: boolean) {
  const key = `${view.note_id ?? ""}:${view.panel ?? ""}:${view.category_id ?? ""}:${view.search_query ?? ""}`;
  const query = useQuery({
    queryKey: qk.promptChips(key),
    queryFn: () => api.getPromptChips(view),
    enabled,
    staleTime: 0,
  });
  const { refetch } = query;
  useEffect(() => {
    if (!enabled) return;
    const t = setTimeout(() => void refetch(), 4000);
    return () => clearTimeout(t);
  }, [enabled, refetch]);
  return query;
}
