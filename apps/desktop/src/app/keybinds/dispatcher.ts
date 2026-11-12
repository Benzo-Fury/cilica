/**
 * Runs registered action handlers when their shortcut is pressed, and lets the palette run
 * actions by id. Components register with `useAction(id, handler)`; the most recently mounted
 * handler for an id wins (so a dialog can take over a shortcut while it is open).
 */
import { useEffect, useRef } from "react";
import { isTauri } from "@/ipc/client";
import { dismissNewestToast, triggerLatestUndo } from "@/lib/toast";
import { undoLastAiChange } from "../aiUndo";
import { closeTopLayer } from "../layers";
import { isMac } from "../platform";
import { type ActionId, eventToAccelerator, getAction, matchAction } from "./registry";
import { useKeybindStore } from "./store";

type Handler = (event?: KeyboardEvent) => void;
interface Entry {
  handler: { current: Handler };
  enabled: { current: boolean };
}

const handlers = new Map<string, Entry[]>();

function activeEntry(id: string): Entry | undefined {
  const list = handlers.get(id);
  if (!list) return undefined;
  for (let i = list.length - 1; i >= 0; i--) {
    const entry = list[i];
    if (entry?.enabled.current) return entry;
  }
  return undefined;
}

/** Is any enabled handler registered for this action right now? */
export function canRunAction(id: ActionId): boolean {
  return Boolean(activeEntry(id));
}

/** Run an action by id (palette, menus). Returns false when nothing handles it. */
export function runAction(id: ActionId): boolean {
  const entry = activeEntry(id);
  if (!entry) return false;
  entry.handler.current();
  return true;
}

/** Handle a registry action while mounted (and `enabled`). */
export function useAction(id: ActionId, handler: Handler, enabled = true) {
  const handlerRef = useRef(handler);
  const enabledRef = useRef(enabled);
  handlerRef.current = handler;
  enabledRef.current = enabled;
  useEffect(() => {
    const entry: Entry = { handler: handlerRef, enabled: enabledRef };
    const list = handlers.get(id) ?? [];
    list.push(entry);
    handlers.set(id, list);
    return () => {
      const current = handlers.get(id) ?? [];
      handlers.set(
        id,
        current.filter((e) => e !== entry),
      );
    };
  }, [id]);
}

export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag === "INPUT") {
    const type = (target as HTMLInputElement).type;
    return !["checkbox", "radio", "button", "submit", "range", "color"].includes(type);
  }
  return false;
}

/** Capture phase: shortcuts run before the editor or the browser see them. */
function onKeyDownCapture(event: KeyboardEvent) {
  if (event.isComposing || event.repeat) return;
  const { overrides, capturing } = useKeybindStore.getState();
  if (capturing) return;
  if (event.key === "Escape") return; // handled in the bubble phase, after Radix layers
  const accelerator = eventToAccelerator(event, isMac);
  if (!accelerator) return;
  const action = matchAction(accelerator, overrides);
  if (!action || action.scope === "editor") return;
  // The backend owns global hotkeys inside Tauri; in a browser preview we handle it here.
  if (action.scope === "global" && isTauri) return;
  if (!action.allowInInputs && isEditableTarget(event.target)) return;
  const entry = activeEntry(action.id);
  if (!entry) return;
  event.preventDefault();
  event.stopPropagation();
  entry.handler.current(event);
}

/** Bubble phase: Esc closes the topmost layer; ⌘Z runs a visible Undo toast, else Margin's last change. */
function onKeyDown(event: KeyboardEvent) {
  if (event.defaultPrevented || event.isComposing) return;
  if (useKeybindStore.getState().capturing) return;
  if (event.key === "Escape") {
    if (closeTopLayer() || dismissNewestToast()) event.preventDefault();
    return;
  }
  const accelerator = eventToAccelerator(event, isMac);
  if (accelerator === "Mod+Z" && !isEditableTarget(event.target)) {
    if (triggerLatestUndo() || undoLastAiChange()) event.preventDefault();
  }
}

/** Install the global key listeners once (from the app providers). */
export function useKeybindDispatcher() {
  useEffect(() => {
    window.addEventListener("keydown", onKeyDownCapture, true);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDownCapture, true);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);
}

/** The label for an action id (palette, menus, tooltips). */
export function actionLabel(id: ActionId): string {
  return getAction(id)?.label ?? id;
}
