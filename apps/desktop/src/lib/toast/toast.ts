/**
 * The toast API (spec.md › Toasts), backed by one module-level store.
 *
 *   toast("Saved")
 *   toast.success("Summary ready", { description: "Acme kickoff" })
 *   toast.error("Mic unavailable", { description: "Choose another input in Settings." })
 *   toast.action("Zoom meeting detected", { action: { label: "Record", onClick } })
 *   toast.undo("Moved 3 notes to Client work", { onUndo })
 *   toast.promise(summarise(), { loading: "Summarising…", success: "Summary ready", error: "Summary failed" })
 */
import { useSyncExternalStore } from "react";
import { createToastStore, type ToastButton, type ToastInput, type ToastStore } from "./store";

/** The store behind `toast()` and `<Toaster />`. */
export const toastStore: ToastStore = createToastStore();

export type ToastOptions = Omit<ToastInput, "title" | "variant" | "onUndo">;

export interface ToastContent {
  title: string;
  description?: string;
}

/** A message for a settled promise: a title, a title and description, or a function of the value. */
export type ToastMessage<T> = string | ToastContent | ((value: T) => string | ToastContent);

function content<T>(message: ToastMessage<T>, value: T): ToastContent {
  const resolved = typeof message === "function" ? message(value) : message;
  return typeof resolved === "string" ? { title: resolved } : resolved;
}

function show(variant: ToastInput["variant"]) {
  return (title: string, options: ToastOptions = {}) =>
    toastStore.add({ ...options, title, variant });
}

function promise<T>(
  task: Promise<T>,
  messages: {
    loading: string | ToastContent;
    success: ToastMessage<T>;
    error: ToastMessage<unknown>;
  },
  options: Omit<ToastOptions, "description" | "action"> = {},
): Promise<T> {
  const loading =
    typeof messages.loading === "string" ? { title: messages.loading } : messages.loading;
  const id = toastStore.add({ ...options, ...loading, variant: "loading" });
  task.then(
    (value) => {
      toastStore.add({ ...options, id, variant: "success", ...content(messages.success, value) });
    },
    (error: unknown) => {
      toastStore.add({ ...options, id, variant: "error", ...content(messages.error, error) });
    },
  );
  return task;
}

export const toast = Object.assign(show("default"), {
  info: show("default"),
  success: show("success"),
  error: show("error"),
  action: (title: string, options: ToastOptions & { action: ToastButton }) =>
    toastStore.add({ ...options, title, variant: "action" }),
  undo: (title: string, options: Omit<ToastOptions, "action"> & { onUndo: () => void }) =>
    toastStore.add({ ...options, title, variant: "undo" }),
  promise,
  /** Dismiss one toast, or all of them without an id. */
  dismiss: (id?: string) => toastStore.dismiss(id),
});

/** For the app's Esc handling: dismisses the newest toast; false when none is visible. */
export const dismissNewestToast = (): boolean => toastStore.dismissNewest();
/** For ⌘Z: runs the newest visible undo toast; false when there is none. */
export const triggerLatestUndo = (): boolean => toastStore.triggerLatestUndo();
export const hasVisibleToasts = (): boolean => toastStore.hasVisible();
export const pauseToasts = (reason?: string) => toastStore.pause(reason);
export const resumeToasts = (reason?: string) => toastStore.resume(reason);

/** React view of a toast store (the default one unless given). */
export function useToasts(store: ToastStore = toastStore) {
  return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}
