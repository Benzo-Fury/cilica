/**
 * The one subscription to backend events. It keeps the query cache fresh (`data_changed`,
 * `settings_changed`, `recording_state_changed`), renders backend toasts through the toast
 * library, and re-publishes every event on a local bus so features can react
 * (`useBackendEvent("note_body_stream", …)`).
 */
import { type QueryClient, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { api, errorMessage, onBackendEvent } from "@/ipc/client";
import type {
  BackendEvent,
  BackendEventOf,
  BackendEventType,
  ToastAction,
  ToastRequest,
} from "@/ipc/types";
import { toast } from "@/lib/toast";
import { forgetAiUndo, rememberAiUndo } from "./aiUndo";
import { keysForScope, qk } from "./queryKeys";
import type { SettingsPageId } from "./router";
import { navigate, openNote } from "./router";

type Listener = (event: BackendEvent) => void;
const listeners = new Map<BackendEventType, Set<Listener>>();

/** Publish an event to feature listeners (also used by tests and the dev mock). */
export function publish(event: BackendEvent) {
  for (const listener of listeners.get(event.type) ?? []) listener(event);
}

/** React to one kind of backend event while mounted. */
export function useBackendEvent<T extends BackendEventType>(
  type: T,
  handler: (event: BackendEventOf<T>) => void,
) {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    const listener: Listener = (event) => ref.current(event as BackendEventOf<T>);
    const set = listeners.get(type) ?? new Set();
    set.add(listener);
    listeners.set(type, set);
    return () => {
      set.delete(listener);
    };
  }, [type]);
}

const SETTINGS_PAGES: readonly string[] = [
  "general",
  "appearance",
  "keybinds",
  "recording",
  "transcription",
  "cilica",
  "sounds",
  "swear_jar",
  "import",
];

/** What a backend toast's button does. */
export function runToastAction(action: ToastAction): Promise<unknown> | undefined {
  const fail = (error: unknown) =>
    toast.error("That did not work", { description: errorMessage(error) });
  switch (action.type) {
    case "start_recording":
      return api
        .startRecording({ note_id: null, meeting_app: action.meeting_app, trigger: "detected" })
        .catch(fail);
    case "stop_recording":
      return api.stopRecording().catch(fail);
    case "keep_recording":
      return api.keepRecording().catch(fail);
    case "open_note":
      openNote(action.note_id);
      return;
    case "open_task_run":
      navigate({ name: "tasks", taskRunId: action.task_run_id });
      return;
    case "undo_ai_action":
      return api
        .undoAiAction(action.action_id)
        .then((undone) => toast(`Undid: ${undone.summary}`))
        .catch(fail);
    case "undo_turn":
      return api
        .undoTurn(action.turn_id)
        .then((count) =>
          count > 0
            ? toast(count === 1 ? "Undid Cilica's change" : `Undid Cilica's ${count} changes`)
            : toast("Nothing left to undo", { description: "Those changes were already undone." }),
        )
        .catch(fail);
    case "regenerate_summary":
      return api.regenerateSummary(action.note_id).catch(fail);
    case "open_settings": {
      const page = SETTINGS_PAGES.includes(action.page)
        ? (action.page as SettingsPageId)
        : "general";
      navigate({ name: "settings", page });
      return;
    }
  }
}

/** Render a backend `ToastRequest` with the in-house toast library. */
export function showBackendToast(request: ToastRequest) {
  const description = request.description ?? undefined;
  const action = request.action;
  switch (request.kind) {
    case "success":
      if (action) {
        return toast.success(request.title, {
          description,
          duration: 6000,
          action: { label: request.action_label ?? "Open", onClick: () => runToastAction(action) },
        });
      }
      return toast.success(request.title, { description });
    case "error":
      return toast.error(request.title, { description });
    case "undo":
      if (action) {
        const run = () => runToastAction(action);
        // ⌘Z after the toast has gone still undoes Cilica's last turn (see aiUndo.ts).
        if (action.type === "undo_turn" || action.type === "undo_ai_action") {
          rememberAiUndo(request.title, run);
        }
        return toast.undo(request.title, {
          description,
          onUndo: () => {
            forgetAiUndo(run);
            void run();
          },
        });
      }
      return toast(request.title, { description });
    case "action":
      if (action) {
        return toast.action(request.title, {
          description,
          action: { label: request.action_label ?? "Open", onClick: () => runToastAction(action) },
        });
      }
      return toast(request.title, { description });
    default:
      return toast(request.title, { description });
  }
}

function handle(queryClient: QueryClient, event: BackendEvent) {
  switch (event.type) {
    case "data_changed": {
      for (const scope of event.scopes) {
        for (const queryKey of keysForScope(scope, event.note_id)) {
          void queryClient.invalidateQueries({ queryKey });
        }
      }
      break;
    }
    case "settings_changed":
      queryClient.setQueryData(qk.settings, event.settings);
      break;
    case "recording_state_changed":
      queryClient.setQueryData(qk.recording, event.state);
      break;
    case "task_run_finished":
      void queryClient.invalidateQueries({ queryKey: qk.tasks });
      break;
    case "toast":
      showBackendToast(event.toast);
      break;
    case "open_note":
      // Setup finished: the main window opens on the welcome note.
      openNote(event.note_id);
      break;
    default:
      break;
  }
  publish(event);
}

/** Install the backend event subscription once (from the app providers). */
export function useBackendEventBridge() {
  const queryClient = useQueryClient();
  useEffect(() => onBackendEvent((event) => handle(queryClient, event)), [queryClient]);
}
