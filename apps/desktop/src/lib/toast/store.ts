/**
 * Framework-free toast store: a queue with at most `maxVisible` toasts on screen, one timer per
 * visible toast, and a global pause (hovering the stack, window blur) that keeps remaining time.
 * Snapshots are immutable so React can read them with `useSyncExternalStore`.
 */
import type { ReactNode } from "react";

export type ToastVariant = "default" | "success" | "error" | "action" | "undo" | "loading";

export interface ToastButton {
  label: string;
  onClick: () => void;
}

export interface ToastInput {
  /** Pass an existing id to replace that toast's content in place. */
  id?: string;
  variant?: ToastVariant;
  title: string;
  description?: string;
  action?: ToastButton;
  onUndo?: () => void;
  onDismiss?: () => void;
  /** Overrides the variant's default; `null` keeps the toast until it is dismissed. */
  duration?: number | null;
  icon?: ReactNode;
}

export interface ToastRecord {
  id: string;
  variant: ToastVariant;
  title: string;
  description?: string;
  action?: ToastButton;
  onUndo?: () => void;
  onDismiss?: () => void;
  icon?: ReactNode;
  /** Full timer length in ms, `null` for no timer. */
  duration: number | null;
  status: "visible" | "queued";
  /** Ms left on the timer when it last stopped (or the full duration before it starts). */
  remaining: number | null;
  /** When the current run of the timer started; `null` while it is not running. */
  runningSince: number | null;
  createdAt: number;
  /** Bumps whenever the content is replaced; renderers key timer visuals on it. */
  version: number;
}

export interface ToastSnapshot {
  /** Oldest first. Visible and queued toasts together; filter on `status`. */
  toasts: readonly ToastRecord[];
  paused: boolean;
}

export const DEFAULT_DURATIONS: Record<ToastVariant, number | null> = {
  default: 4000,
  success: 4000,
  error: 8000,
  action: 6000,
  undo: 5000,
  loading: null,
};

type TimerHandle = unknown;

export interface ToastStoreOptions {
  maxVisible?: number;
  setTimeout?: (fn: () => void, ms: number) => TimerHandle;
  clearTimeout?: (handle: TimerHandle) => void;
  now?: () => number;
}

export interface ToastStore {
  subscribe(listener: () => void): () => void;
  getSnapshot(): ToastSnapshot;
  /** Adds a toast, or replaces the content of the toast with `input.id` if it exists. */
  add(input: ToastInput): string;
  /** Merges fields into an existing toast. Changing `variant` or `duration` restarts its timer. */
  update(id: string, patch: Partial<Omit<ToastInput, "id">>): void;
  /** Removes one toast (or every toast without an id) and promotes queued ones. */
  dismiss(id?: string): void;
  pause(reason?: string): void;
  resume(reason?: string): void;
  /** Dismisses the newest visible toast. False when nothing is visible. */
  dismissNewest(): boolean;
  /** Runs the newest visible undo toast's `onUndo` and dismisses it. False when there is none. */
  triggerLatestUndo(): boolean;
  hasVisible(): boolean;
  /** Ms of the timer already used up, for countdown visuals. */
  elapsed(toast: ToastRecord): number;
}

export function createToastStore(options: ToastStoreOptions = {}): ToastStore {
  const maxVisible = options.maxVisible ?? 3;
  const setTimer =
    options.setTimeout ?? ((fn: () => void, ms: number) => globalThis.setTimeout(fn, ms));
  const clearTimer =
    options.clearTimeout ??
    ((handle: TimerHandle) => globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>));
  const now = options.now ?? (() => Date.now());

  let toasts: ToastRecord[] = [];
  let snapshot: ToastSnapshot = { toasts, paused: false };
  let seq = 0;
  const pauseReasons = new Set<string>();
  const handles = new Map<string, TimerHandle>();
  const listeners = new Set<() => void>();

  const isPaused = () => pauseReasons.size > 0;

  function emit() {
    snapshot = { toasts, paused: isPaused() };
    for (const listener of listeners) listener();
  }

  function stopTimer(id: string) {
    const handle = handles.get(id);
    if (handle !== undefined) {
      clearTimer(handle);
      handles.delete(id);
    }
  }

  /** Starts the timer of a visible toast unless paused. Returns the updated record. */
  function startTimer(toast: ToastRecord): ToastRecord {
    stopTimer(toast.id);
    if (toast.status !== "visible" || toast.remaining === null || isPaused()) {
      return { ...toast, runningSince: null };
    }
    const id = toast.id;
    handles.set(
      id,
      setTimer(() => {
        handles.delete(id);
        dismiss(id);
      }, toast.remaining),
    );
    return { ...toast, runningSince: now() };
  }

  /** Stops a running timer and banks the time used. */
  function freeze(toast: ToastRecord): ToastRecord {
    stopTimer(toast.id);
    if (toast.runningSince === null || toast.remaining === null) return toast;
    const used = now() - toast.runningSince;
    return { ...toast, remaining: Math.max(0, toast.remaining - used), runningSince: null };
  }

  function promote() {
    let visible = toasts.filter((t) => t.status === "visible").length;
    if (visible >= maxVisible) return;
    toasts = toasts.map((t) => {
      if (t.status !== "queued" || visible >= maxVisible) return t;
      visible += 1;
      return startTimer({ ...t, status: "visible" });
    });
  }

  function replace(id: string, next: ToastRecord) {
    toasts = toasts.map((t) => (t.id === id ? next : t));
  }

  function add(input: ToastInput): string {
    const existing = input.id ? toasts.find((t) => t.id === input.id) : undefined;
    if (existing) {
      const { id: _id, ...rest } = input;
      update(existing.id, {
        description: undefined,
        action: undefined,
        onUndo: undefined,
        icon: undefined,
        ...rest,
        variant: input.variant ?? "default",
      });
      return existing.id;
    }
    seq += 1;
    const id = input.id ?? `toast-${seq}`;
    const variant = input.variant ?? "default";
    const duration = input.duration !== undefined ? input.duration : DEFAULT_DURATIONS[variant];
    const record: ToastRecord = {
      id,
      variant,
      title: input.title,
      description: input.description,
      action: input.action,
      onUndo: input.onUndo,
      onDismiss: input.onDismiss,
      icon: input.icon,
      duration,
      status: "queued",
      remaining: duration,
      runningSince: null,
      createdAt: now(),
      version: 0,
    };
    toasts = [...toasts, record];
    promote();
    emit();
    return id;
  }

  function update(id: string, patch: Partial<Omit<ToastInput, "id">>) {
    const current = toasts.find((t) => t.id === id);
    if (!current) return;
    const restart = "variant" in patch || "duration" in patch;
    let next: ToastRecord = { ...current, ...patch, version: current.version + 1 } as ToastRecord;
    if (restart) {
      const variant = next.variant;
      const duration = patch.duration !== undefined ? patch.duration : DEFAULT_DURATIONS[variant];
      next = startTimer({ ...next, duration, remaining: duration, runningSince: null });
    }
    replace(id, next);
    emit();
  }

  function dismiss(id?: string) {
    if (id === undefined) {
      const all = toasts;
      for (const t of all) stopTimer(t.id);
      toasts = [];
      emit();
      for (const t of all) t.onDismiss?.();
      return;
    }
    const target = toasts.find((t) => t.id === id);
    if (!target) return;
    stopTimer(id);
    toasts = toasts.filter((t) => t.id !== id);
    promote();
    emit();
    target.onDismiss?.();
  }

  function pause(reason = "manual") {
    if (pauseReasons.has(reason)) return;
    const wasPaused = isPaused();
    pauseReasons.add(reason);
    if (!wasPaused) toasts = toasts.map(freeze);
    emit();
  }

  function resume(reason = "manual") {
    if (!pauseReasons.delete(reason)) return;
    if (!isPaused()) toasts = toasts.map((t) => (t.status === "visible" ? startTimer(t) : t));
    emit();
  }

  function newestVisible(predicate: (t: ToastRecord) => boolean = () => true) {
    for (let i = toasts.length - 1; i >= 0; i -= 1) {
      const t = toasts[i];
      if (t && t.status === "visible" && predicate(t)) return t;
    }
    return undefined;
  }

  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    getSnapshot: () => snapshot,
    add,
    update,
    dismiss,
    pause,
    resume,
    dismissNewest() {
      const t = newestVisible();
      if (!t) return false;
      dismiss(t.id);
      return true;
    },
    triggerLatestUndo() {
      const t = newestVisible((x) => x.variant === "undo");
      if (!t) return false;
      t.onUndo?.();
      dismiss(t.id);
      return true;
    },
    hasVisible: () => toasts.some((t) => t.status === "visible"),
    elapsed(toast) {
      if (toast.duration === null || toast.remaining === null) return 0;
      const running = toast.runningSince === null ? 0 : now() - toast.runningSince;
      return Math.min(toast.duration, Math.max(0, toast.duration - toast.remaining + running));
    },
  };
}
