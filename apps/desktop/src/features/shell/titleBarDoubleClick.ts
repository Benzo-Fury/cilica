/**
 * Double-click on the title band (macOS).
 *
 * The webview covers the native title bar, so AppKit never sees the double-click, and Tauri's
 * drag-region script answers it with its own maximise: no system animation, and the user's
 * "Double-click a window's title bar to" setting ignored. This catches the double-click first and
 * asks the backend to do exactly what a native title bar would (fill, zoom, minimise or nothing).
 * Single clicks and drags still go to Tauri's script.
 */
import { useEffect } from "react";
import { isMac } from "@/app/platform";
import { api, isTauri } from "@/ipc/client";

const DRAG_ATTR = "data-tauri-drag-region";
const CLICKABLE_TAGS = new Set(["A", "BUTTON", "INPUT", "SELECT", "TEXTAREA", "LABEL", "SUMMARY"]);
const INTERACTIVE_ROLES = new Set([
  "button",
  "link",
  "menuitem",
  "tab",
  "checkbox",
  "radio",
  "switch",
  "option",
]);

function isClickable(el: Element): boolean {
  const editable = el.getAttribute("contenteditable");
  const tabindex = el.getAttribute("tabindex");
  return (
    CLICKABLE_TAGS.has(el.tagName) ||
    (editable !== null && editable !== "false") ||
    (tabindex !== null && tabindex !== "-1") ||
    INTERACTIVE_ROLES.has(el.getAttribute("role") ?? "")
  );
}

/**
 * True when an event with this composed path lands on a drag region. Same rules as Tauri's drag
 * script, so the double-click is caught exactly where a drag would start: a bare attribute counts
 * only for direct hits, "deep" for its whole subtree, "false" and clickable elements block.
 */
export function isDragRegion(path: readonly EventTarget[]): boolean {
  for (const target of path) {
    if (typeof Element === "undefined" || !(target instanceof Element)) continue;
    const attr = target.getAttribute(DRAG_ATTR);
    if (attr === null) {
      if (isClickable(target)) return false;
      continue;
    }
    if (attr === "false") return false;
    if (attr === "deep") return true;
    if (attr === "" || attr === "true") return target === path[0];
  }
  return false;
}

/** Install the title-band double-click handler for the main window (macOS only). */
export function useTitleBarDoubleClick() {
  useEffect(() => {
    if (!isTauri || !isMac) return;
    // Like AppKit, act on the second click's mouse-up, and not if the pointer moved in between.
    let down: { x: number; y: number } | null = null;
    const onDown = (e: MouseEvent) => {
      down =
        e.button === 0 && e.detail === 2 && isDragRegion(e.composedPath())
          ? { x: e.clientX, y: e.clientY }
          : null;
    };
    const onUp = (e: MouseEvent) => {
      if (e.button !== 0 || e.detail !== 2 || !down) return;
      const still = e.clientX === down.x && e.clientY === down.y;
      down = null;
      if (!still || !isDragRegion(e.composedPath())) return;
      // Keep Tauri's drag script (a document listener) from also maximising.
      e.stopImmediatePropagation();
      void api.titleBarDoubleClick().catch(() => undefined);
    };
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("mouseup", onUp, true);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("mouseup", onUp, true);
    };
  }, []);
}
