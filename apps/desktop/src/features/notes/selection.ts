/**
 * Selecting notes in the sidebar and in Trash. ⌘-click (Ctrl on Windows and Linux) adds or removes
 * a note, Shift-click selects everything between the anchor and the clicked note in the order the
 * list shows them, across categories. A plain click opens a note and clears the selection, and so
 * does clicking anywhere outside the selected notes. The open note is not part of the selection.
 *
 * The pure functions here hold the rules (and are tested); `useNoteSelection` holds the state.
 */
import { useEffect, useRef } from "react";
import { create } from "zustand";

export interface Selection {
  /** Selected note ids, in the order they were added. */
  ids: readonly string[];
  /** Where a Shift-click range starts: the last note clicked without Shift. */
  anchor: string | null;
}

export const emptySelection: Selection = { ids: [], anchor: null };

export type ClickKind = "plain" | "toggle" | "range";

/** Which kind of click a pointer or keyboard event is. ⌘ on macOS, Ctrl elsewhere. */
export function clickKind(
  event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean },
  mac: boolean,
): ClickKind {
  if (event.shiftKey) return "range";
  if (mac ? event.metaKey : event.ctrlKey) return "toggle";
  return "plain";
}

/** Ids from `from` to `to` inclusive, in `order`. Just `to` when `from` is not shown. */
export function rangeIds(order: readonly string[], from: string | null, to: string): string[] {
  const end = order.indexOf(to);
  if (end < 0) return [];
  const start = from === null ? -1 : order.indexOf(from);
  if (start < 0) return [to];
  const [lo, hi] = start <= end ? [start, end] : [end, start];
  return order.slice(lo, hi + 1);
}

/**
 * The selection after clicking `id`. A plain click clears it but sets the anchor, so a following
 * Shift-click selects from there. `current` is the open note: a range with no anchor starts from it.
 */
export function applyClick(
  selection: Selection,
  id: string,
  kind: ClickKind,
  order: readonly string[],
  current: string | null,
): Selection {
  if (kind === "plain") return { ids: [], anchor: id };
  if (kind === "toggle") {
    const ids = selection.ids.includes(id)
      ? selection.ids.filter((x) => x !== id)
      : [...selection.ids, id];
    return { ids, anchor: id };
  }
  const anchor = selection.anchor ?? current;
  return { ids: rangeIds(order, anchor, id), anchor: anchor ?? id };
}

/** Shift+↑/↓ from the focused note: the range from the anchor to the focused note's neighbour. */
export function extendSelection(
  selection: Selection,
  focusedId: string,
  direction: -1 | 1,
  order: readonly string[],
): { selection: Selection; focus: string } | null {
  const next = order[order.indexOf(focusedId) + direction];
  if (next === undefined || order.indexOf(focusedId) < 0) return null;
  // From a note outside the selection, start afresh from it.
  const anchor = selection.ids.includes(focusedId) ? (selection.anchor ?? focusedId) : focusedId;
  return { selection: { ids: rangeIds(order, anchor, next), anchor }, focus: next };
}

/** Drop ids that are no longer live notes (trashed by Margin, removed in another window). */
export function pruneSelection(selection: Selection, live: ReadonlySet<string>): Selection {
  const ids = selection.ids.filter((id) => live.has(id));
  if (ids.length === selection.ids.length) return selection;
  const anchor = selection.anchor && live.has(selection.anchor) ? selection.anchor : null;
  return { ids, anchor };
}

/** The selected ids in sidebar order (what bulk actions act on, so blocks keep their order). */
export function inOrder(ids: readonly string[], order: readonly string[]): string[] {
  const chosen = new Set(ids);
  const shown = order.filter((id) => chosen.has(id));
  // Notes in a folded category are not in `order`; keep them, after the shown ones.
  return [...shown, ...ids.filter((id) => !shown.includes(id))];
}

/** "1 note", "3 notes". */
export function countNotes(count: number): string {
  return `${count} ${count === 1 ? "note" : "notes"}`;
}

interface NoteSelectionState extends Selection {
  set: (selection: Selection) => void;
  clear: () => void;
}

/** The sidebar's note selection. Selected notes take the group's actions (menu, ⌘⌫, drag). */
export const useNoteSelection = create<NoteSelectionState>()((set) => ({
  ...emptySelection,
  set: ({ ids, anchor }) => set({ ids, anchor }),
  clear: () => set(emptySelection),
}));

/** True while any note is selected (⌘⌫ then acts on the selection, not the open note). */
export const useHasSelection = () => useNoteSelection((s) => s.ids.length > 0);

/**
 * Call `onAway` when the pointer goes down anywhere outside `keep` (a CSS selector) while `active`.
 * Menus, dialogs and toasts are always kept, so acting on a selection does not clear it first.
 */
export function useClickAway(active: boolean, keep: string, onAway: () => void) {
  const handler = useRef(onAway);
  handler.current = onAway;
  useEffect(() => {
    if (!active) return;
    const always = '[role="menu"], [role="dialog"], [role="alertdialog"], [data-keeps-selection]';
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      if (target.closest(`${keep}, ${always}`)) return;
      handler.current();
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    return () => document.removeEventListener("pointerdown", onPointerDown, true);
  }, [active, keep]);
}

/**
 * Note ids in the order the sidebar shows them: uncategorised notes, then each open category's.
 * Notes in a folded category are hidden, so a Shift range skips them, as Finder skips the
 * contents of a closed folder.
 */
export function visibleOrder(
  grouped: {
    uncategorised: readonly { id: string }[];
    byCategory: ReadonlyMap<string, readonly { id: string }[]>;
  },
  categories: readonly { id: string; collapsed: boolean }[],
): string[] {
  const ids = grouped.uncategorised.map((n) => n.id);
  for (const category of categories) {
    if (category.collapsed) continue;
    for (const note of grouped.byCategory.get(category.id) ?? []) ids.push(note.id);
  }
  return ids;
}
